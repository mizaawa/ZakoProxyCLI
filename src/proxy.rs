use crate::{
    Shared, profiles,
    store::{self, Account, ApiKey, Config, Group, RequestLog, Settings, Usage},
};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashSet},
    time::{Duration, Instant},
};

const MAX_EVENT_BYTES: usize = 256 * 1024;
const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
const MAX_TRACKED_CHAT_CHOICES: u64 = 1024;
const MAX_ERROR_BYTES: usize = 2048;

mod cyber;

fn route_for(protocol: &str) -> &'static str {
    match protocol {
        "compact" => "responses/compact",
        "anthropic" => "messages",
        "chat" => "chat/completions",
        _ => "responses",
    }
}

fn error_summary(message: &str, secrets: &[String]) -> String {
    let mut message = message.to_owned();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        message = message.replace(secret, "[redacted]");
        if let Ok(encoded) = serde_json::to_string(secret) {
            message = message.replace(&encoded[1..encoded.len() - 1], "[redacted]");
        }
    }
    if message.len() > MAX_ERROR_BYTES {
        let mut end = MAX_ERROR_BYTES - '…'.len_utf8();
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push('…');
    }
    message
}

/// Endpoint roots may include /v1, but must not contain credentials or fragments.
pub fn upstream_url(endpoint: &str, route: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(endpoint.trim()).map_err(|_| "Invalid upstream endpoint")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err(
            "Endpoint must be an HTTP(S) URL without credentials, query, or fragment".into(),
        );
    }
    let root = url.path().trim_end_matches('/');
    let root = if root.ends_with("/v1") {
        root.to_owned()
    } else {
        format!("{root}/v1")
    };
    url.set_path(&format!(
        "{root}/{}",
        route.trim_start_matches('/').trim_start_matches("v1/")
    ));
    Ok(url)
}

fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({"error":{"message":message,"type":"proxy_error"}})),
    )
        .into_response()
}

fn authorize(
    state: &Shared,
    headers: &HeaderMap,
) -> Result<(Config, ApiKey, Group), (StatusCode, &'static str)> {
    let raw = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, value)| value)
        .filter(|v| !v.is_empty())
        .or_else(|| {
            headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.is_empty())
        })
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "A valid proxy API key is required",
        ))?;
    let config = state.store.config().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Configuration unavailable",
        )
    })?;
    let hash = store::digest(raw);
    let key = config
        .keys
        .iter()
        .find(|key| key.enabled && key.hash == hash)
        .cloned()
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "A valid proxy API key is required",
        ))?;
    let group = config
        .groups
        .iter()
        .find(|group| group.id == key.group_id)
        .cloned()
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "The API key has no available group",
        ))?;
    Ok((config, key, group))
}

pub async fn models(State(state): State<Shared>, headers: HeaderMap) -> Response {
    let (config, _, group) = match authorize(&state, &headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    let models: BTreeSet<_> = config
        .accounts
        .iter()
        .filter(|account| {
            account.enabled && account.provider_available() && account.group_ids.contains(&group.id)
        })
        .flat_map(|account| {
            account
                .models
                .iter()
                .chain(account.model_mappings.keys())
                .filter(|requested| account.actual_model(requested).is_some())
                .cloned()
        })
        .collect();
    Json(json!({"object":"list", "data":models.into_iter().map(|id| json!({"id":id,"object":"model","owned_by":"upstream"})).collect::<Vec<_>>()})).into_response()
}

pub async fn responses(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    forward(state, headers, body, "responses").await
}

pub async fn compact(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    forward(state, headers, body, "compact").await
}

pub async fn chat_completions(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    forward(state, headers, body, "chat").await
}

pub async fn messages(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    forward(state, headers, body, "anthropic").await
}

fn apply_system(body: &mut Value, group: &Group, model: &str) -> Result<(), &'static str> {
    let Some(instruction) = profiles::instruction(group, model) else {
        return Ok(());
    };
    let system = match body.get("system") {
        None => json!(instruction),
        Some(Value::String(existing)) if existing.is_empty() => json!(instruction),
        Some(Value::String(existing)) => json!(format!("{instruction}\n\n{existing}")),
        Some(Value::Array(blocks)) => {
            let mut blocks = blocks.clone();
            blocks.insert(0, json!({"type":"text", "text":instruction}));
            json!(blocks)
        }
        Some(_) => {
            return Err("system must be a string or an array when a group instruction is applied");
        }
    };
    body["system"] = system;
    Ok(())
}

fn apply_instruction(body: &mut Value, group: &Group, model: &str) -> Result<(), &'static str> {
    let Some(instruction) = profiles::instruction(group, model) else {
        return Ok(());
    };
    let instructions = match body.get("instructions") {
        None => instruction.to_owned(),
        Some(Value::String(existing)) if existing.is_empty() => instruction.to_owned(),
        Some(Value::String(existing)) => format!("{instruction}\n\n{existing}"),
        Some(_) => return Err("instructions must be a string when a group instruction is applied"),
    };
    body["instructions"] = Value::String(instructions);
    Ok(())
}

fn apply_chat_instruction(
    body: &mut Value,
    group: &Group,
    model: &str,
) -> Result<(), &'static str> {
    let Some(instruction) = profiles::instruction(group, model) else {
        return Ok(());
    };
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return Err("messages must be an array when a group instruction is applied");
    };
    messages.insert(0, json!({"role":"system", "content":instruction}));
    Ok(())
}

struct Admission {
    state: Shared,
    account_id: String,
}
impl Drop for Admission {
    fn drop(&mut self) {
        if let Ok(mut runtime) = self.state.runtime.lock()
            && let Some(account) = runtime.get_mut(&self.account_id)
        {
            account.active = account.active.saturating_sub(1);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AdmissionError {
    Cooldown(u64),
    Concurrency,
    Rpm(u64),
    Unavailable,
    Exhausted,
}
impl AdmissionError {
    fn retry_after(self) -> u64 {
        match self {
            Self::Cooldown(seconds) | Self::Rpm(seconds) => seconds.max(1),
            _ => 1,
        }
    }
    fn response(self) -> Response {
        let (status, code, message) = match self {
            Self::Cooldown(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "upstream_cooldown",
                "Upstream accounts are temporarily cooling down after failures",
            ),
            Self::Concurrency => (
                StatusCode::TOO_MANY_REQUESTS,
                "concurrency_limit_exceeded",
                "Account concurrency limit reached",
            ),
            Self::Rpm(_) => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_exceeded",
                "Account requests-per-minute limit reached",
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "admission_unavailable",
                "Account admission is temporarily unavailable",
            ),
            Self::Exhausted => (
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "Upstream request failed; no untried account is available",
            ),
        };
        let mut response = (
            status,
            Json(json!({"error":{"message":message,"type":"proxy_error","code":code}})),
        )
            .into_response();
        response.headers_mut().insert(
            "retry-after",
            HeaderValue::from_str(&self.retry_after().to_string()).unwrap(),
        );
        response
            .headers_mut()
            .insert("x-zako-error-source", HeaderValue::from_static("local"));
        response
    }
}

fn select_account<'a>(
    state: &Shared,
    accounts: &'a [Account],
    tried: &HashSet<String>,
) -> Result<(&'a Account, Admission), AdmissionError> {
    release_cooldowns_if_pool_blocked(state, accounts)?;
    let mut blocked = None;
    for account in accounts
        .iter()
        .filter(|account| !tried.contains(&account.id))
    {
        match admit(state, account) {
            Ok(admission) => return Ok((account, admission)),
            Err(reason) => {
                if blocked
                    .is_none_or(|saved: AdmissionError| reason.retry_after() < saved.retry_after())
                {
                    blocked = Some(reason);
                }
            }
        }
    }
    Err(blocked.unwrap_or(AdmissionError::Exhausted))
}

fn release_cooldowns_if_pool_blocked(
    state: &Shared,
    accounts: &[Account],
) -> Result<(), AdmissionError> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| AdmissionError::Unavailable)?;
    let now = store::now();
    if accounts.len() == 1 {
        let account = runtime.entry(accounts[0].id.clone()).or_default();
        if account.cooldown_until != 0 {
            account.cooldown_until = 0;
            account.failures = 0;
        }
    } else if !accounts.is_empty()
        && accounts.iter().all(|account| {
            runtime
                .get(&account.id)
                .is_some_and(|status| status.cooldown_until > now)
        })
    {
        for account in accounts {
            let status = runtime.get_mut(&account.id).unwrap();
            status.cooldown_until = 0;
            status.failures = 0;
        }
    }
    Ok(())
}

fn admit(state: &Shared, account: &Account) -> Result<Admission, AdmissionError> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| AdmissionError::Unavailable)?;
    let runtime = runtime.entry(account.id.clone()).or_default();
    let now = store::now();
    if runtime.cooldown_until > now {
        return Err(AdmissionError::Cooldown(
            (runtime.cooldown_until - now) as u64,
        ));
    }
    if runtime.cooldown_until != 0 {
        runtime.cooldown_until = 0;
        runtime.failures = 0;
    }
    if runtime.window != now / 60 {
        runtime.window = now / 60;
        runtime.requests = 0;
    }
    if account.rpm > 0 && runtime.requests >= account.rpm {
        return Err(AdmissionError::Rpm((60 - now.rem_euclid(60)) as u64));
    }
    if account.concurrency > 0 && runtime.active >= account.concurrency {
        return Err(AdmissionError::Concurrency);
    }
    runtime.active = runtime.active.saturating_add(1);
    runtime.requests = runtime.requests.saturating_add(1);
    Ok(Admission {
        state: state.clone(),
        account_id: account.id.clone(),
    })
}

fn health(
    state: &Shared,
    account_id: &str,
    settings: &Settings,
    success: bool,
    cooldown_enabled: bool,
) {
    if let Ok(mut runtime) = state.runtime.lock() {
        let runtime = runtime.entry(account_id.to_owned()).or_default();
        if success {
            runtime.failures = 0;
            runtime.cooldown_until = 0;
        } else {
            runtime.failures = runtime.failures.saturating_add(1);
            if cooldown_enabled && runtime.failures >= settings.failure_threshold.max(1) {
                runtime.cooldown_until = store::now() + i64::from(settings.cooldown_seconds);
            } else if !cooldown_enabled {
                runtime.cooldown_until = 0;
            }
        }
    }
}

struct LogGuard {
    state: Shared,
    entry: RequestLog,
    started: Instant,
    finished: Option<Instant>,
    retention: u32,
    secrets: Vec<String>,
}
impl LogGuard {
    fn new(state: &Shared, key: &ApiKey, model: &str, protocol: &str, retention: u32) -> Self {
        Self {
            state: state.clone(),
            started: Instant::now(),
            finished: None,
            retention,
            secrets: Vec::new(),
            entry: RequestLog {
                id: store::id(),
                timestamp: store::now(),
                key_id: key.id.clone(),
                model: model.into(),
                protocol: protocol.into(),
                request_path: format!("/v1/{}", route_for(protocol)),
                status: 499,
                ..Default::default()
            },
        }
    }
    fn finish(&mut self) {
        self.finished.get_or_insert_with(Instant::now);
    }
    fn set_error(&mut self, message: &str) {
        self.entry.error = Some(error_summary(message, &self.secrets));
    }
}
impl Drop for LogGuard {
    fn drop(&mut self) {
        self.entry.latency_ms = self
            .finished
            .unwrap_or_else(Instant::now)
            .duration_since(self.started)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        if self.entry.status >= 400 && self.entry.error.is_none() {
            self.set_error(if self.entry.status == 499 {
                "Request cancelled before the upstream response completed"
            } else {
                "Upstream request failed without an error detail"
            });
        }
        if let Some(message) = &self.entry.error {
            self.entry.error = Some(error_summary(message, &self.secrets));
        }
        if let Err(error) = self.state.store.log(&self.entry, self.retention) {
            tracing::error!(request_id = %self.entry.id, %error, "Could not record proxy request");
        }
    }
}

async fn forward(state: Shared, headers: HeaderMap, body: Value, protocol: &str) -> Response {
    let (config, key, group) = match authorize(&state, &headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    // Routing belongs to the group; instruction selection belongs only to this key.
    let policy = Group {
        profile_ids: key.profile_ids.clone(),
        default_strategy_enabled: key.default_strategy_enabled,
        ..Default::default()
    };
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut log = LogGuard::new(
        &state,
        &key,
        &model,
        protocol,
        config.settings.retention_days,
    );
    for name in ["authorization", "x-api-key"] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            let value = if name == "authorization" {
                value
                    .split_once(' ')
                    .map(|(_, token)| token)
                    .unwrap_or(value)
            } else {
                value
            };
            log.secrets.push(value.to_owned());
        }
    }
    if !body.is_object() || model.is_empty() {
        log.entry.status = 400;
        log.set_error("A JSON object with a nonempty model is required");
        return error(
            StatusCode::BAD_REQUEST,
            "A JSON object with a nonempty model is required",
        );
    }
    let mut accounts: Vec<_> = config
        .accounts
        .iter()
        .filter(|account| {
            account.enabled
                && account.provider_available()
                && account.group_ids.contains(&group.id)
                && account.protocols.iter().any(|item| item == protocol)
                && account.actual_model(&model).is_some()
        })
        .cloned()
        .collect();
    accounts.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    if accounts.is_empty() {
        log.entry.status = 503;
        log.set_error("No account supports this group, model, and protocol");
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "No account supports this group, model, and protocol",
        );
    }
    let cooldown_enabled = accounts.len() > 1;
    let mut tried = HashSet::new();
    let mut reserved = None;
    let attempts = config.settings.max_retries.saturating_add(1);
    for _ in 0..attempts {
        let selected = reserved
            .take()
            .map(Ok)
            .unwrap_or_else(|| select_account(&state, &accounts, &tried));
        let (account, admission) = match selected {
            Ok(selected) => selected,
            Err(reason) => {
                let mut response = reason.response();
                log.entry.status = response.status().as_u16();
                if !matches!(reason, AdmissionError::Exhausted) || log.entry.error.is_none() {
                    log.set_error(match reason {
                        AdmissionError::Cooldown(_) => {
                            "Upstream accounts are temporarily cooling down after failures"
                        }
                        AdmissionError::Concurrency => "Account concurrency limit reached",
                        AdmissionError::Rpm(_) => "Account requests-per-minute limit reached",
                        AdmissionError::Unavailable => {
                            "Account admission is temporarily unavailable"
                        }
                        AdmissionError::Exhausted => {
                            "Upstream request failed; no untried account is available"
                        }
                    });
                }
                response.headers_mut().insert(
                    "x-request-id",
                    HeaderValue::from_str(&log.entry.id).unwrap(),
                );
                return response;
            }
        };
        let actual_model = account.actual_model(&model).unwrap();
        // Rebuild each attempt from the original request: account mappings can differ on failover.
        let mut upstream_body = body.clone();
        upstream_body["model"] = json!(actual_model);
        let instruction_result = match protocol {
            "anthropic" => apply_system(&mut upstream_body, &policy, actual_model),
            "chat" => apply_chat_instruction(&mut upstream_body, &policy, actual_model),
            _ => apply_instruction(&mut upstream_body, &policy, actual_model),
        };
        if let Err(message) = instruction_result {
            log.entry.status = 400;
            log.set_error(message);
            return error(StatusCode::BAD_REQUEST, message);
        }
        tried.insert(account.id.clone());
        log.entry.attempts += 1;
        log.entry.account_id = account.id.clone();
        log.entry.status = 499;
        log.entry.error = None;
        let route = route_for(protocol);
        let url = match upstream_url(&account.endpoint, route) {
            Ok(url) => url,
            Err(_) => {
                log.entry.status = 502;
                log.set_error("Invalid upstream endpoint configuration");
                health(
                    &state,
                    &account.id,
                    &config.settings,
                    false,
                    cooldown_enabled,
                );
                continue;
            }
        };
        let secret = match state.store.unseal(&account.api_key) {
            Ok(secret) => secret,
            Err(_) => {
                log.entry.status = 502;
                log.set_error("Could not decrypt the upstream API key");
                health(
                    &state,
                    &account.id,
                    &config.settings,
                    false,
                    cooldown_enabled,
                );
                continue;
            }
        };
        log.secrets.push(secret.clone());
        let client = match crate::network::account_client(&state, account) {
            Ok(client) => client,
            Err(_) => {
                log.entry.status = 502;
                log.set_error("Could not configure the upstream network client");
                health(
                    &state,
                    &account.id,
                    &config.settings,
                    false,
                    cooldown_enabled,
                );
                continue;
            }
        };
        let mut request = client
            .post(url)
            .json(&upstream_body)
            .timeout(Duration::from_secs(u64::from(
                config.settings.request_timeout_seconds.max(1),
            )));
        request = if protocol == "anthropic" {
            request.header("x-api-key", &secret).header(
                "anthropic-version",
                headers
                    .get("anthropic-version")
                    .cloned()
                    .unwrap_or_else(|| HeaderValue::from_static("2023-06-01")),
            )
        } else {
            request.bearer_auth(&secret)
        };
        if protocol == "anthropic"
            && let Some(value) = headers.get("anthropic-beta")
        {
            request = request.header("anthropic-beta", value);
        }
        for name in [
            "accept",
            "openai-beta",
            "idempotency-key",
            "x-client-request-id",
        ] {
            if let Some(value) = headers.get(name) {
                request = request.header(name, value);
            }
        }
        let upstream = match request.send().await {
            Ok(response) => response,
            Err(cause) => {
                log.entry.status = 502;
                log.set_error(if cause.is_timeout() {
                    "Upstream request timed out before response headers"
                } else if cause.is_connect() {
                    "Could not connect to the upstream server"
                } else {
                    "Upstream request transport failed before response headers"
                });
                health(
                    &state,
                    &account.id,
                    &config.settings,
                    false,
                    cooldown_enabled,
                );
                continue;
            }
        };
        let status = upstream.status();
        let retryable = matches!(
            status,
            StatusCode::UNAUTHORIZED
                | StatusCode::FORBIDDEN
                | StatusCode::REQUEST_TIMEOUT
                | StatusCode::TOO_MANY_REQUESTS
        ) || status.is_server_error();
        if retryable {
            health(
                &state,
                &account.id,
                &config.settings,
                false,
                cooldown_enabled,
            );
            if log.entry.attempts < attempts {
                reserved = select_account(&state, &accounts, &tried).ok();
                if reserved.is_some() {
                    continue;
                }
            }
        }
        let mut response_headers = HeaderMap::new();
        if !status.is_success() {
            response_headers.insert("x-zako-error-source", HeaderValue::from_static("upstream"));
        }
        for name in ["content-type", "x-request-id", "request-id", "retry-after"] {
            if let Some(value) = upstream.headers().get(name) {
                // An upstream must not reflect its credential through response headers.
                if secret.is_empty()
                    || !value
                        .as_bytes()
                        .windows(secret.len())
                        .any(|part| part == secret.as_bytes())
                {
                    response_headers.insert(name, value.clone());
                }
            }
        }
        if !response_headers.contains_key("x-request-id")
            && let Ok(request_id) = HeaderValue::from_str(&log.entry.id)
        {
            response_headers.insert("x-request-id", request_id);
        }
        let sse = upstream
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_ascii_lowercase().starts_with("text/event-stream"));
        let settings = config.settings.clone();
        let account_id = account.id.clone();
        let stream_state = state.clone();
        log.entry.status = 499;
        let chat_choices =
            (protocol == "chat").then(|| body.get("n").and_then(Value::as_u64).unwrap_or(1).max(1));
        let output = async_stream::stream! {
            let mut admission = Some(admission);
            let mut log = log;
            let mut observer = Observer::new(sse, chat_choices);
            observer.secrets = log.secrets.clone();
            observer.http_error = !status.is_success();
            let mut redactor = SecretFilter::new(secret);
            let mut stream = upstream.bytes_stream();
            let mut transport_failed = false;
            let mut health_recorded = None;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(chunk) => {
                        observer.feed(&chunk);
                        log.entry.usage = observer.usage.clone();
                        log.entry.cyber = observer.cyber;
                        if observer.content_seen && log.entry.first_token_ms.is_none() {
                            log.entry.first_token_ms = Some(log.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                        }
                        if let Some(message) = &observer.error {
                            log.set_error(message);
                        }
                        if status.is_success() && (observer.completed || observer.failed) {
                            log.entry.status = if observer.failed { 502 } else { status.as_u16() };
                            let healthy = !observer.failed;
                            if health_recorded.is_none() || (health_recorded == Some(true) && !healthy) {
                                health(&stream_state, &account_id, &settings, healthy, cooldown_enabled);
                                health_recorded = Some(healthy);
                            }
                        }
                        if observer.completed || observer.failed {
                            // A terminal protocol event ends model work even if the relay keeps HTTP open.
                            if !status.is_success() {
                                log.entry.status = status.as_u16();
                            }
                            log.finish();
                            admission.take();
                        }
                        let chunk = redactor.feed(&chunk);
                        if !chunk.is_empty() { yield Ok::<Bytes, std::io::Error>(chunk); }
                    }
                    Err(cause) => {
                        transport_failed = true;
                        if !(sse && (observer.completed || observer.failed)) {
                            log.set_error(if cause.is_timeout() { "Upstream response timed out" } else { "Upstream response transport failed" });
                        }
                        break;
                    }
                }
            }
            observer.finish();
            log.finish();
            if observer.content_seen && log.entry.first_token_ms.is_none() {
                log.entry.first_token_ms = Some(log.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
            }
            if let Some(message) = &observer.error {
                log.set_error(message);
            }
            log.entry.usage = observer.usage;
            log.entry.cyber = observer.cyber;
            let transport_failed = transport_failed && !(sse && (observer.completed || observer.failed));
            let incomplete = sse && status.is_success() && !observer.completed && !observer.failed;
            log.entry.status = if transport_failed || incomplete || (status.is_success() && observer.failed) { 502 } else { status.as_u16() };
            if incomplete {
                log.set_error("Upstream event stream ended before a terminal event");
            } else if observer.failed && log.entry.error.is_none() {
                log.set_error("Upstream reported a failed response without an error detail");
            } else if !status.is_success() && log.entry.error.is_none() {
                log.set_error(&format!("Upstream HTTP {} {}", status.as_u16(), status.canonical_reason().unwrap_or("error")));
            }
            let healthy = log.entry.status < 400;
            if status.is_success() && (health_recorded.is_none() || (health_recorded == Some(true) && !healthy)) {
                health(&stream_state, &account_id, &settings, healthy, cooldown_enabled);
            }
            admission.take();
            let tail = redactor.finish();
            if !tail.is_empty() { yield Ok(tail); }
            if transport_failed || incomplete {
                yield Err(std::io::Error::other("Upstream response ended before completion"));
            }
        };
        let mut response = Response::new(Body::from_stream(output));
        *response.status_mut() = status;
        *response.headers_mut() = response_headers;
        return response;
    }
    log.entry.status = 502;
    if log.entry.error.is_none() {
        log.set_error("Upstream request failed; no successful account attempt");
    }
    error(StatusCode::BAD_GATEWAY, "Upstream request failed")
}

fn has_content_delta(value: &Value, event_name: &str) -> bool {
    let nonempty = |value: Option<&Value>| {
        value
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
    };
    match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(event_name)
    {
        "response.output_text.delta"
        | "response.reasoning_text.delta"
        | "response.reasoning_summary_text.delta"
        | "response.function_call_arguments.delta"
        | "response.refusal.delta" => {
            return nonempty(value.get("delta"));
        }
        "content_block_delta" => {
            let delta = value.get("delta");
            return ["text", "thinking", "partial_json"]
                .iter()
                .any(|key| nonempty(delta.and_then(|delta| delta.get(*key))));
        }
        _ => {}
    }
    value
        .get("choices")
        .and_then(Value::as_array)
        .is_some_and(|choices| {
            choices.iter().any(|choice| {
                let Some(delta) = choice.get("delta") else {
                    return false;
                };
                if ["content", "reasoning", "reasoning_content", "refusal"]
                    .iter()
                    .any(|key| nonempty(delta.get(*key)))
                {
                    return true;
                }
                if delta
                    .get("content")
                    .and_then(Value::as_array)
                    .is_some_and(|blocks| blocks.iter().any(|block| nonempty(block.get("text"))))
                {
                    return true;
                }
                let function_has_content = |function: &Value| {
                    nonempty(function.get("name")) || nonempty(function.get("arguments"))
                };
                delta.get("function_call").is_some_and(function_has_content)
                    || delta
                        .get("tool_calls")
                        .and_then(Value::as_array)
                        .is_some_and(|tools| {
                            tools
                                .iter()
                                .any(|tool| tool.get("function").is_some_and(function_has_content))
                        })
            })
        })
}

// Retain only explicit error fields, never the whole request or successful response body.
fn response_error(value: &Value, event_name: &str, http_error: bool) -> Option<String> {
    let response = value.get("response").unwrap_or(value);
    let error = value
        .get("error")
        .filter(|error| !error.is_null())
        .or_else(|| response.get("error").filter(|error| !error.is_null()))
        .or_else(|| {
            (http_error
                || event_name == "error"
                || value.get("type").and_then(Value::as_str) == Some("error"))
            .then_some(value)
        })?;
    if let Some(message) = error.as_str().filter(|message| !message.trim().is_empty()) {
        return Some(message.to_owned());
    }
    let scalar = |key: &str| {
        error.get(key).and_then(|value| match value {
            Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    };
    let code = scalar("code").or_else(|| scalar("type"));
    let message = scalar("message");
    match (code, message) {
        (Some(code), Some(message)) => Some(format!("{code}: {message}")),
        (Some(code), None) => Some(code),
        (None, message) => message,
    }
}

// Keep bounded parser buffers and per-channel marker state, never response content in logs.
struct Observer {
    sse: bool,
    line: Vec<u8>,
    event_name: String,
    data: Vec<u8>,
    oversized: bool,
    discard_line: bool,
    json: Vec<u8>,
    json_oversized: bool,
    marker: cyber::CyberDetector,
    cyber: bool,
    usage: Usage,
    completed: bool,
    failed: bool,
    content_seen: bool,
    error: Option<String>,
    secrets: Vec<String>,
    http_error: bool,
    chat_choices: Option<u64>,
    finished_choices: BTreeSet<u64>,
}
impl Observer {
    fn new(sse: bool, chat_choices: Option<u64>) -> Self {
        Self {
            sse,
            line: Vec::new(),
            event_name: String::new(),
            data: Vec::new(),
            oversized: false,
            discard_line: false,
            json: Vec::new(),
            json_oversized: false,
            marker: cyber::CyberDetector::default(),
            cyber: false,
            usage: Usage::default(),
            completed: false,
            failed: false,
            content_seen: false,
            error: None,
            secrets: Vec::new(),
            http_error: false,
            // Larger requests still use [DONE]; avoid unbounded state from arbitrary n values.
            chat_choices: chat_choices.filter(|expected| *expected <= MAX_TRACKED_CHAT_CHOICES),
            finished_choices: BTreeSet::new(),
        }
    }
    fn feed(&mut self, bytes: &[u8]) {
        if !self.sse {
            if !self.json_oversized && self.json.len().saturating_add(bytes.len()) <= MAX_JSON_BYTES
            {
                self.json.extend_from_slice(bytes);
            } else {
                self.json.clear();
                self.json_oversized = true;
            }
            return;
        }
        for byte in bytes {
            if *byte == b'\n' {
                if self.discard_line {
                    self.line.clear();
                    self.discard_line = false;
                } else {
                    self.line();
                }
            } else if !self.discard_line {
                if self.line.len() < MAX_EVENT_BYTES {
                    self.line.push(*byte);
                } else {
                    self.line.clear();
                    self.discard_line = true;
                    self.oversized = true;
                }
            }
        }
    }
    fn line(&mut self) {
        let line = std::mem::take(&mut self.line);
        let line = line.strip_suffix(b"\r").unwrap_or(&line);
        if line.is_empty() {
            match self.event_name.as_str() {
                "response.completed" | "message_stop" => self.completed = true,
                "response.failed" | "error" => self.failed = true,
                _ => {}
            }
            let event_name = std::mem::take(&mut self.event_name);
            if !self.oversized {
                let data = std::mem::take(&mut self.data);
                self.event(&data, &event_name);
            } else if event_name == "response.incomplete" {
                self.failed = true;
            }
            self.data.clear();
            self.oversized = false;
        } else if let Some(name) = line.strip_prefix(b"event:") {
            if name.len() < 128 {
                self.event_name = String::from_utf8_lossy(name).trim().to_owned();
            }
        } else if let Some(data) = line.strip_prefix(b"data:") {
            let data = data.strip_prefix(b" ").unwrap_or(data);
            if self.data.len().saturating_add(data.len()).saturating_add(1) <= MAX_EVENT_BYTES {
                self.data.extend_from_slice(data);
                self.data.push(b'\n');
            } else {
                self.data.clear();
                self.oversized = true;
            }
        }
    }
    fn event(&mut self, data: &[u8], event_name: &str) {
        if data.trim_ascii() == b"[DONE]" {
            self.completed = true;
            return;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(data) {
            if !self.cyber && !self.http_error {
                self.cyber = self.marker.observe(&value, event_name, self.sse);
            }
            let usage = store::usage(&value);
            self.usage.input_tokens = self.usage.input_tokens.max(usage.input_tokens);
            self.usage.output_tokens = self.usage.output_tokens.max(usage.output_tokens);
            self.usage.cached_tokens = self.usage.cached_tokens.max(usage.cached_tokens);
            self.usage.cache_creation_input_tokens = self
                .usage
                .cache_creation_input_tokens
                .max(usage.cache_creation_input_tokens);
            self.usage.cache_read_input_tokens = self
                .usage
                .cache_read_input_tokens
                .max(usage.cache_read_input_tokens);
            if self.sse && has_content_delta(&value, event_name) {
                self.content_seen = true;
            }
            if let Some(message) = response_error(&value, event_name, self.http_error) {
                self.error = Some(error_summary(&message, &self.secrets));
            }
            match value.get("type").and_then(Value::as_str) {
                Some("response.completed" | "message_stop") => self.completed = true,
                Some("response.failed" | "error") => self.failed = true,
                _ => {}
            }
            let response = value.get("response").unwrap_or(&value);
            let incomplete = event_name == "response.incomplete"
                || value.get("type").and_then(Value::as_str) == Some("response.incomplete")
                || response.get("status").and_then(Value::as_str) == Some("incomplete");
            if incomplete {
                // Token limits and filtering terminate this response, not the account's health.
                if matches!(
                    response
                        .pointer("/incomplete_details/reason")
                        .and_then(Value::as_str),
                    Some("max_output_tokens" | "content_filter")
                ) {
                    self.completed = true;
                } else {
                    self.failed = true;
                }
            }
            if value.get("error").is_some_and(|error| !error.is_null())
                || response.get("error").is_some_and(|error| !error.is_null())
                || response.get("status").and_then(Value::as_str) == Some("failed")
            {
                self.failed = true;
            }
            if let Some(expected) = self.chat_choices
                && let Some(choices) = value.get("choices").and_then(Value::as_array)
            {
                for choice in choices {
                    if let Some(index) = choice.get("index").and_then(Value::as_u64)
                        && index < expected
                        && choice
                            .get("finish_reason")
                            .and_then(Value::as_str)
                            .is_some_and(|reason| !reason.is_empty())
                    {
                        self.finished_choices.insert(index);
                    }
                }
                if self.finished_choices.len() as u64 == expected {
                    self.completed = true;
                }
            }
        } else if event_name == "response.incomplete" {
            // A malformed terminal event must not hold a concurrency slot indefinitely.
            self.failed = true;
        }
    }
    fn finish(&mut self) {
        if self.sse {
            if !self.line.is_empty() && !self.discard_line {
                self.line();
            }
            if !self.oversized {
                let data = std::mem::take(&mut self.data);
                let event_name = std::mem::take(&mut self.event_name);
                self.event(&data, &event_name);
            }
        } else if !self.json_oversized {
            let data = std::mem::take(&mut self.json);
            self.event(&data, "");
        }
    }
}

#[cfg(test)]
#[path = "proxy/cyber_tests.rs"]
mod cyber_tests;

// A rolling suffix prevents credentials split between network chunks from leaking.
struct SecretFilter {
    secret: Vec<u8>,
    pending: Vec<u8>,
}
impl SecretFilter {
    fn new(secret: String) -> Self {
        Self {
            secret: secret.into_bytes(),
            pending: Vec::new(),
        }
    }
    fn feed(&mut self, bytes: &[u8]) -> Bytes {
        if self.secret.is_empty() {
            return Bytes::copy_from_slice(bytes);
        }
        self.pending.extend_from_slice(bytes);
        let mut output = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            let remaining = &self.pending[index..];
            if remaining.starts_with(&self.secret) {
                output.extend_from_slice(b"[redacted]");
                index += self.secret.len();
            } else if self.secret.starts_with(remaining) {
                // Retain only a suffix that could actually be a split credential.
                break;
            } else {
                output.push(self.pending[index]);
                index += 1;
            }
        }
        self.pending.drain(..index);
        Bytes::from(output)
    }
    fn finish(&mut self) -> Bytes {
        Bytes::from(std::mem::take(&mut self.pending))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_normalization_and_validation() {
        assert_eq!(
            upstream_url("https://example.com", "responses")
                .unwrap()
                .as_str(),
            "https://example.com/v1/responses"
        );
        assert_eq!(
            upstream_url("https://example.com/v1/", "responses/compact")
                .unwrap()
                .as_str(),
            "https://example.com/v1/responses/compact"
        );
        assert_eq!(
            upstream_url("http://localhost:1999/proxy", "/v1/models")
                .unwrap()
                .as_str(),
            "http://localhost:1999/proxy/v1/models"
        );
        for invalid in [
            "file:///etc/passwd",
            "https://user:pass@example.com",
            "https://example.com?token=x",
            "https://example.com/#x",
        ] {
            assert!(upstream_url(invalid, "models").is_err());
        }
    }
    #[test]
    fn instruction_changes_only_selected_model_and_preserves_fields() {
        let profile = &profiles::all()[0];
        let group = Group {
            profile_ids: vec![profile.id.clone()],
            ..Default::default()
        };
        let original = json!({"model":"other","instructions":"Existing","input":[{"role":"user","content":"hello"}],"tools":[{"type":"function"}],"store":false});
        let mut body = original.clone();
        apply_instruction(&mut body, &group, "other").unwrap();
        assert_eq!(body, original);
        apply_instruction(&mut body, &group, &profile.models[0]).unwrap();
        assert_eq!(
            body["instructions"],
            format!("{}\n\nExisting", profile.instruction)
        );
        assert_eq!(body["input"], original["input"]);
        assert_eq!(body["tools"], original["tools"]);
        assert_eq!(body["store"], original["store"]);
        assert!(
            apply_instruction(&mut json!({"instructions":[]}), &group, &profile.models[0]).is_err()
        );
    }
    #[test]
    fn selected_profiles_apply_one_matching_model_and_preserve_fields() {
        let group = Group {
            profile_ids: profiles::all()
                .iter()
                .map(|profile| profile.id.clone())
                .collect(),
            ..Default::default()
        };
        for model in ["gpt-5.6-sol", "gpt-6-astra", "gpt-6.1-sol"] {
            let expected = profiles::all()
                .iter()
                .find(|profile| profile.models.iter().any(|selected| selected == model))
                .unwrap();
            let original = json!({"model":model,"instructions":"Existing instructions","input":[{"role":"user","content":"hello"}],"tools":[{"type":"function","name":"tool"}],"metadata":{"custom":true},"store":false});
            let mut body = original.clone();
            apply_instruction(&mut body, &group, model).unwrap();
            assert_eq!(
                body["instructions"],
                format!("{}\n\nExisting instructions", expected.instruction)
            );
            body["instructions"] = original["instructions"].clone();
            assert_eq!(body, original);
        }
        for model in [
            "gpt-5.6",
            "gpt-6",
            "gpt-6-sol",
            "gpt-6.1",
            "gpt-6.10-sol",
            "gpt-60",
            "unknown",
        ] {
            let original = json!({"model":model,"instructions":["unchanged"]});
            let mut body = original.clone();
            apply_instruction(&mut body, &group, model).unwrap();
            assert_eq!(body, original);
        }
        let mut custom = json!({"instructions":"Existing"});
        apply_instruction(&mut custom, &group, "custom").unwrap();
        assert_eq!(custom["instructions"], "Existing");
    }
    #[test]
    fn unselected_profiles_never_leak_through_legacy_builtin_text() {
        let six = profiles::all()
            .iter()
            .find(|profile| profile.model_family == "gpt-6")
            .unwrap();
        let group: Group = serde_json::from_value(json!({
            "name":"Only six", "profile_ids":[six.id],
            "models":["gpt-5.6-sol", "gpt-6.1-sol"], "instruction":six.instruction
        }))
        .unwrap();
        for model in ["gpt-5.6-sol", "gpt-6.1-sol"] {
            let original = json!({"model":model,"input":"hello","instructions":"Keep me"});
            let mut body = original.clone();
            apply_instruction(&mut body, &group, model).unwrap();
            assert_eq!(body, original);
        }
        let mut absent = json!({"model":"gpt-6-astra"});
        apply_instruction(&mut absent, &group, "gpt-6-astra").unwrap();
        assert_eq!(absent["instructions"], six.instruction);
        let mut empty = json!({"instructions":""});
        apply_instruction(&mut empty, &group, "gpt-6-astra").unwrap();
        assert_eq!(empty["instructions"], six.instruction);
        assert!(apply_instruction(&mut json!({"instructions":[]}), &group, "gpt-6-astra").is_err());
        let legacy_custom: Group = serde_json::from_value(json!({
            "name":"Custom", "models":["gpt-6.1-sol"],
            "instruction":"Old custom text must not bypass the unchecked model"
        }))
        .unwrap();
        let mut unchecked = json!({"instructions":"Existing"});
        apply_instruction(&mut unchecked, &legacy_custom, "gpt-6.1-sol").unwrap();
        assert_eq!(unchecked["instructions"], "Existing");
    }
    #[test]
    fn first_token_ignores_metadata_and_detects_content_across_chunks() {
        let metadata = b"data: {\"type\":\"response.created\"}\n\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\ndata: {\"usage\":{\"output_tokens\":4}}\n\n: heartbeat\n\n";
        for content in [
            json!({"type":"response.output_text.delta","delta":"hello"}),
            json!({"type":"response.reasoning_text.delta","delta":"think"}),
            json!({"type":"response.reasoning_summary_text.delta","delta":"think"}),
            json!({"type":"response.function_call_arguments.delta","delta":"{}"}),
            json!({"choices":[{"delta":{"content":"hello"}}]}),
            json!({"choices":[{"delta":{"reasoning_content":"think"}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"function":{"arguments":"{}"}}]}}]}),
            json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"hello"}}),
            json!({"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"think"}}),
            json!({"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{}"}}),
        ] {
            let content = format!("data: {content}\n\n");
            for split in 0..=content.len() {
                let mut observer = Observer::new(true, None);
                observer.feed(metadata);
                assert!(!observer.content_seen);
                observer.feed(&content.as_bytes()[..split]);
                observer.feed(&content.as_bytes()[split..]);
                assert!(observer.content_seen, "{content} at {split}");
            }
        }
        for empty in [
            json!({"type":"response.output_text.delta","delta":""}),
            json!({"choices":[{"delta":{"role":"assistant","content":""}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","type":"function"}]}}]}),
            json!({"type":"content_block_delta","delta":{"type":"signature_delta","signature":"signature"}}),
        ] {
            assert!(!has_content_delta(&empty, ""));
        }
    }

    #[test]
    fn cache_usage_merges_across_anthropic_message_events() {
        let mut observer = Observer::new(true, None);
        observer.feed(b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":10,\"cache_creation_input_tokens\":40,\"cache_read_input_tokens\":20}}}\n\n");
        observer.feed(b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7}}\n\n");
        observer.feed(b"data: {\"type\":\"message_stop\"}\n\n");
        assert_eq!(observer.usage.input_tokens, 70);
        assert_eq!(observer.usage.output_tokens, 7);
        assert_eq!(observer.usage.cached_tokens, 20);
        assert_eq!(observer.usage.cache_read_input_tokens, 20);
        assert_eq!(observer.usage.cache_creation_input_tokens, 40);
        assert!(observer.completed);
    }

    #[test]
    fn json_and_sse_errors_are_bounded_and_redact_decoded_credentials() {
        for sse in [false, true] {
            let mut observer = Observer::new(sse, None);
            observer.secrets = vec!["upstream-secret".into(), "proxy-key".into()];
            let error = r#"{"error":{"code":"invalid_api_key","message":"upstream-\u0073ecret and proxy-\u006bey rejected"}}"#;
            let bytes = if sse {
                format!("event: error\ndata: {error}\n\n")
            } else {
                error.to_owned()
            };
            for chunk in bytes.as_bytes().chunks(3) {
                observer.feed(chunk);
            }
            observer.finish();
            assert_eq!(
                observer.error.as_deref(),
                Some("invalid_api_key: [redacted] and [redacted] rejected")
            );
            assert!(observer.failed);
        }
        assert_eq!(
            response_error(
                &json!({"error":{"code":429,"message":"Rate limited"}}),
                "",
                false
            )
            .as_deref(),
            Some("429: Rate limited")
        );
        assert_eq!(
            response_error(&json!({"message":"Bad input"}), "", true).as_deref(),
            Some("Bad input")
        );
        assert!(response_error(&json!({"message":"success output"}), "", false).is_none());
        let bounded = error_summary(&"错误".repeat(MAX_ERROR_BYTES), &[]);
        assert!(bounded.len() <= MAX_ERROR_BYTES);
        assert!(bounded.ends_with('…'));
    }

    #[test]
    fn sse_usage_survives_every_byte_boundary() {
        let bytes = b"event: response.output_text.delta\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"CYBER\"}\r\n\r\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":100,\"output_tokens\":8,\"input_tokens_details\":{\"cached_tokens\":60}}}}\n\n";
        for split in 0..=bytes.len() {
            let mut observer = Observer::new(true, None);
            observer.feed(&bytes[..split]);
            observer.feed(&bytes[split..]);
            observer.finish();
            assert_eq!(observer.usage.input_tokens, 100);
            assert_eq!(observer.usage.cached_tokens, 60);
            assert!(observer.completed);
            assert!(observer.cyber);
        }
        let mut observer = Observer::new(true, None);
        for byte in bytes {
            observer.feed(&[*byte]);
        }
        observer.finish();
        assert_eq!(observer.usage.output_tokens, 8);
    }
    #[test]
    fn oversized_sse_events_are_bounded_and_next_event_works() {
        let mut observer = Observer::new(true, None);
        observer.feed(b"data: ");
        observer.feed(&vec![b'x'; MAX_EVENT_BYTES + 50]);
        observer.feed(b"\n\ndata: {\"type\":\"response.completed\"}\n\n");
        assert!(observer.completed);
        assert!(observer.line.len() <= MAX_EVENT_BYTES);
        assert!(observer.data.len() <= MAX_EVENT_BYTES);
    }
    #[test]
    fn credentials_are_redacted_across_all_boundaries() {
        let bytes = b"before sk-private after sk-private";
        for split in 0..=bytes.len() {
            let mut filter = SecretFilter::new("sk-private".into());
            let output = [
                filter.feed(&bytes[..split]),
                filter.feed(&bytes[split..]),
                filter.finish(),
            ]
            .concat();
            assert_eq!(output, b"before [redacted] after [redacted]");
        }
    }
    #[test]
    fn terminal_bytes_are_not_buffered_behind_an_unrelated_secret() {
        let mut filter = SecretFilter::new("upstream-secret".into());
        let terminal = b"data: [DONE]\n\n";
        assert_eq!(filter.feed(terminal).as_ref(), terminal);
        assert!(filter.finish().is_empty());
        assert_eq!(filter.feed(b"before upstream-").as_ref(), b"before ");
        assert_eq!(filter.feed(b"secret after").as_ref(), b"[redacted] after");
    }

    #[test]
    fn normal_incomplete_response_variants_are_terminal_but_errors_still_fail() {
        for sse in [false, true] {
            for reason in ["max_output_tokens", "content_filter", "unknown"] {
                let mut observer = Observer::new(sse, None);
                let response =
                    json!({"status":"incomplete","incomplete_details":{"reason":reason}});
                let bytes = if sse {
                    format!(
                        "event: response.incomplete\ndata: {}\n\n",
                        json!({"response":response})
                    )
                    .into_bytes()
                } else {
                    serde_json::to_vec(&response).unwrap()
                };
                observer.feed(&bytes);
                observer.finish();
                assert_eq!(observer.completed, reason != "unknown");
                assert_eq!(observer.failed, reason == "unknown");
            }
        }
        let mut observer = Observer::new(true, None);
        observer.feed(b"event: response.incomplete\ndata: {\"response\":{\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"},\"error\":{\"code\":\"server_error\"}}}\n\n");
        assert!(observer.failed);
    }

    #[test]
    fn malformed_incomplete_event_is_terminal_and_choice_tracking_is_bounded() {
        for data in ["", "not-json"] {
            let mut observer = Observer::new(true, None);
            observer.feed(format!("event: response.incomplete\ndata: {data}\n\n").as_bytes());
            assert!(observer.failed);
        }
        let mut observer = Observer::new(true, Some(u64::MAX));
        observer.feed(b"data: {\"choices\":[{\"index\":0,\"finish_reason\":\"stop\"}]}\n\n");
        assert!(observer.finished_choices.is_empty());
        assert!(!observer.completed);
        observer.feed(b"data: [DONE]\n\n");
        assert!(observer.completed);
    }

    #[test]
    fn admission_is_atomic_and_drop_releases_concurrency() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::new_app(dir.path(), dir.path().into()).unwrap();
        let account = Account {
            id: "a".into(),
            concurrency: 1,
            rpm: 2,
            ..Default::default()
        };
        let guard = admit(&state, &account).unwrap();
        assert!(admit(&state, &account).is_err());
        drop(guard);
        let guard = admit(&state, &account).unwrap();
        drop(guard);
        assert!(admit(&state, &account).is_err());
        assert_eq!(state.runtime.lock().unwrap()["a"].active, 0);
        health(
            &state,
            "a",
            &Settings {
                failure_threshold: 1,
                ..Default::default()
            },
            false,
            true,
        );
        assert!(state.runtime.lock().unwrap()["a"].cooldown_until > store::now());
    }

    #[test]
    fn single_account_pool_clears_cooldown_and_never_reenters_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::new_app(dir.path(), dir.path().into()).unwrap();
        let account = Account {
            id: "only".into(),
            ..Default::default()
        };
        let settings = Settings {
            failure_threshold: 1,
            ..Default::default()
        };

        health(&state, "only", &settings, false, false);
        assert_eq!(state.runtime.lock().unwrap()["only"].cooldown_until, 0);

        {
            let mut runtime = state.runtime.lock().unwrap();
            runtime.get_mut("only").unwrap().cooldown_until = store::now() + 60;
        }
        let accounts = [account];
        let (selected, admission) = select_account(&state, &accounts, &HashSet::new()).unwrap();
        assert_eq!(selected.id, "only");
        let runtime = state.runtime.lock().unwrap();
        assert_eq!(runtime["only"].cooldown_until, 0);
        assert_eq!(runtime["only"].failures, 0);
        drop(runtime);
        drop(admission);
    }

    #[test]
    fn all_cooled_accounts_are_released_as_a_pool() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::new_app(dir.path(), dir.path().into()).unwrap();
        let accounts = [
            Account {
                id: "a".into(),
                ..Default::default()
            },
            Account {
                id: "b".into(),
                ..Default::default()
            },
        ];
        {
            let mut runtime = state.runtime.lock().unwrap();
            for id in ["a", "b"] {
                let status = runtime.entry(id.into()).or_default();
                status.failures = 3;
                status.cooldown_until = store::now() + 60;
            }
        }

        let (selected, admission) = select_account(&state, &accounts, &HashSet::new()).unwrap();
        assert_eq!(selected.id, "a");
        let runtime = state.runtime.lock().unwrap();
        for id in ["a", "b"] {
            assert_eq!(runtime[id].cooldown_until, 0);
            assert_eq!(runtime[id].failures, 0);
        }
        drop(runtime);
        drop(admission);
    }

    #[test]
    fn a_partially_cooled_pool_keeps_cooldown_and_selects_available_account() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::new_app(dir.path(), dir.path().into()).unwrap();
        let accounts = [
            Account {
                id: "a".into(),
                ..Default::default()
            },
            Account {
                id: "b".into(),
                ..Default::default()
            },
        ];
        state.runtime.lock().unwrap().insert(
            "a".into(),
            crate::Runtime {
                failures: 2,
                cooldown_until: store::now() + 60,
                ..Default::default()
            },
        );

        let (selected, admission) = select_account(&state, &accounts, &HashSet::new()).unwrap();
        assert_eq!(selected.id, "b");
        assert!(state.runtime.lock().unwrap()["a"].cooldown_until > store::now());
        drop(admission);
    }

    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    async fn serve(router: axum::Router) -> (String, Server) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (endpoint, Server(task))
    }
    fn fixture(endpoints: &[String]) -> (tempfile::TempDir, Shared) {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::new_app(dir.path(), dir.path().into()).unwrap();
        let secret = state.store.seal("upstream-secret").unwrap();
        state
            .store
            .update(|config| {
                config.groups.push(Group {
                    id: "group".into(),
                    ..Default::default()
                });
                config.keys.push(ApiKey {
                    id: "key".into(),
                    group_id: "group".into(),
                    hash: store::digest("proxy-key"),
                    enabled: true,
                    ..Default::default()
                });
                config.settings.failure_threshold = 1;
                for (index, endpoint) in endpoints.iter().enumerate() {
                    config.accounts.push(Account {
                        id: format!("account-{index}"),
                        endpoint: endpoint.clone(),
                        api_key: secret.clone(),
                        group_ids: vec!["group".into()],
                        models: vec!["chosen".into()],
                        protocols: vec!["responses".into(), "compact".into()],
                        enabled: true,
                        priority: index as i32,
                        ..Default::default()
                    });
                }
                Ok(())
            })
            .unwrap();
        (dir, state)
    }
    async fn call(state: Shared, path: &str, body: Value) -> Response {
        use tower::ServiceExt;
        crate::app(state)
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("authorization", "Bearer proxy-key")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn local_and_transport_failures_have_safe_reasons_and_request_paths() {
        let (_dir, state) = fixture(&["http://127.0.0.1:1".into()]);
        let response = call(
            state.clone(),
            "/v1/responses/compact",
            json!({"model":"chosen","input":"private prompt"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].request_path, "/v1/responses/compact");
        let reason = logs[0].error.as_deref().unwrap();
        assert!(reason.contains("connect") || reason.contains("transport"));
        assert!(!reason.contains("private prompt"));
        assert!(logs[0].first_token_ms.is_none());
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"input":"private prompt"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        let local = logs.iter().find(|log| log.status == 400).unwrap();
        assert_eq!(
            local.error.as_deref(),
            Some("A JSON object with a nonempty model is required")
        );
    }

    #[tokio::test]
    async fn streamed_first_token_excludes_metadata_and_terminal_freezes_duration() {
        use axum::routing::post;
        let (endpoint, _server) = serve(axum::Router::new().route("/v1/responses", post(|| async {
            let output = async_stream::stream! {
                yield Ok::<_, std::io::Error>(Bytes::from_static(b"data: {\"type\":\"response.created\"}\n\n"));
                tokio::time::sleep(Duration::from_millis(50)).await;
                yield Ok(Bytes::from_static(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n"));
                yield Ok(Bytes::from_static(b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"output_tokens\":1}}}\n\n"));
                tokio::time::sleep(Duration::from_secs(300)).await;
            };
            ([("content-type", "text/event-stream")], Body::from_stream(output))
        }))).await;
        let (_dir, state) = fixture(&[endpoint]);
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","stream":true}),
        )
        .await;
        let mut stream = response.into_body().into_data_stream();
        let mut received = Vec::new();
        while let Some(chunk) = stream.next().await {
            received.extend_from_slice(&chunk.unwrap());
            if String::from_utf8_lossy(&received).contains("response.completed") {
                break;
            }
        }
        let completed_at = Instant::now();
        tokio::time::sleep(Duration::from_millis(80)).await;
        drop(stream);
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        let log = &logs[0];
        assert_eq!(log.status, 200);
        assert!(log.error.is_none());
        assert!(log.first_token_ms.unwrap() >= 40);
        assert!(log.latency_ms >= log.first_token_ms.unwrap());
        assert!(
            log.latency_ms
                < log.first_token_ms.unwrap() + completed_at.elapsed().as_millis() as u64
        );
    }

    #[tokio::test]
    async fn no_selected_models_never_routes_as_wildcard() {
        let (_dir, state) = fixture(&["http://127.0.0.1:1".into()]);
        state
            .store
            .update(|config| {
                config.accounts[0].models.clear();
                config.accounts[0]
                    .model_mappings
                    .insert("chosen".into(), "unselected".into());
                Ok(())
            })
            .unwrap();
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","input":"hello"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(state.runtime.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn every_strategy_is_really_forwarded_without_a_passphrase() {
        use axum::{http::Uri, routing::post};
        fn stream_fixture(path: &str) -> &'static str {
            match path {
                "/v1/messages" => {
                    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
                }
                "/v1/chat/completions" => {
                    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n"
                }
                _ => {
                    "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\"}\n\n"
                }
            }
        }
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
        let observed = captured.clone();
        let (endpoint, _server) = serve(axum::Router::new().fallback(post(
            move |uri: Uri, Json(body): Json<Value>| {
                let streaming = body["stream"] == true;
                let fixture = stream_fixture(uri.path());
                *observed.lock().unwrap() = Some((uri.path().to_owned(), body));
                async move {
                    if streaming {
                        ([("content-type", "text/event-stream")], fixture).into_response()
                    } else {
                        Json(json!({"id":"ok","usage":{"input_tokens":1,"output_tokens":1}}))
                            .into_response()
                    }
                }
            },
        )))
        .await;
        let (_dir, state) = fixture(&[endpoint]);
        let mut cases: Vec<_> = profiles::all()
            .iter()
            .flat_map(|profile| {
                profile
                    .models
                    .iter()
                    .map(|model| (Some(profile.id.clone()), model.clone(), false))
            })
            .collect();
        for template in profiles::templates() {
            for model in ["provider/custom-model", "deepseek-test"] {
                cases.push((
                    Some(format!("coldcoffee@{}@{}", template.id, hex::encode(model))),
                    model.into(),
                    false,
                ));
            }
        }
        for model in [
            "gpt-6.2-custom",
            "provider/Claude-New",
            "MiniMax-M3.1",
            "deepseek-4.1-flash",
        ] {
            cases.push((
                Some(profiles::model_profile(model, &[]).unwrap().id),
                model.into(),
                false,
            ));
        }
        cases.push((None, "gpt-6.2-astra".into(), false));
        cases.push((None, "unknown-model".into(), true));
        for (selected, model, fallback) in cases {
            state
                .store
                .update(|config| {
                    config.keys[0].profile_ids = selected.iter().cloned().collect();
                    config.keys[0].default_strategy_enabled = fallback;
                    config.accounts[0].models = vec![model.clone()];
                    config.accounts[0]
                        .model_mappings
                        .insert("chosen".into(), model.clone());
                    config.accounts[0].protocols = vec![
                        "responses".into(),
                        "compact".into(),
                        "chat".into(),
                        "anthropic".into(),
                    ];
                    Ok(())
                })
                .unwrap();
            let expected_instruction = selected
                .as_ref()
                .map(|id| profiles::find(id).unwrap().instruction.clone())
                .or_else(|| fallback.then(|| profiles::DEFAULT_INSTRUCTION.to_owned()));
            for (path, streaming) in [
                ("/v1/responses", false),
                ("/v1/responses/compact", false),
                ("/v1/chat/completions", false),
                ("/v1/messages", false),
                ("/v1/responses", true),
                ("/v1/chat/completions", true),
                ("/v1/messages", true),
            ] {
                let messages = json!([
                    {"role":"system","content":"Keep client system"},
                    {"role":"user","content":[{"type":"text","text":"hello"},{"type":"image_url","image_url":{"url":"data:image/png;base64,fixture"}}]}
                ]);
                let mut original = json!({"model":"chosen","metadata":{"keep":true},"tools":[],"stream":streaming});
                match path {
                    "/v1/messages" => {
                        original["system"] = json!([{"type":"text","text":"Keep system","cache_control":{"type":"ephemeral"}}]);
                        original["messages"] = messages;
                        original["max_tokens"] = json!(16);
                    }
                    "/v1/chat/completions" => original["messages"] = messages,
                    _ => {
                        original["instructions"] = json!("Keep instructions");
                        original["input"] = messages;
                    }
                }
                let response = call(state.clone(), path, original.clone()).await;
                let status = response.status();
                let bytes = axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap();
                assert_eq!(
                    status,
                    StatusCode::OK,
                    "{selected:?} {model} {path}: {}",
                    String::from_utf8_lossy(&bytes)
                );
                if streaming {
                    assert_eq!(
                        bytes.as_ref(),
                        stream_fixture(path).as_bytes(),
                        "{selected:?} {path}"
                    );
                }
                let (actual_path, actual) = captured.lock().unwrap().take().unwrap();
                assert_eq!(actual_path, path);
                let mut expected = original;
                expected["model"] = json!(model);
                if let Some(text) = &expected_instruction {
                    match path {
                        "/v1/messages" => expected["system"]
                            .as_array_mut()
                            .unwrap()
                            .insert(0, json!({"type":"text","text":text})),
                        "/v1/chat/completions" => expected["messages"]
                            .as_array_mut()
                            .unwrap()
                            .insert(0, json!({"role":"system","content":text})),
                        _ => {
                            expected["instructions"] = json!(format!("{text}\n\nKeep instructions"))
                        }
                    }
                    assert!(!text.trim().is_empty(), "{selected:?}");
                    for marker in ["冷咖啡已激活", "QQ一群", "QQ四群", "冰美式", "Boot latch"]
                    {
                        assert!(!text.contains(marker), "{selected:?}: {marker}");
                    }
                }
                assert_eq!(actual, expected, "{selected:?} {model} {path}");
            }
        }
    }

    #[tokio::test]
    async fn mapped_failover_rebuilds_actual_model_and_instruction_for_each_account() {
        use axum::routing::post;
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed_first = captured.clone();
        let (first, _first_server) = serve(axum::Router::new().route(
            "/v1/responses/compact",
            post(move |Json(body): Json<Value>| {
                observed_first.lock().unwrap().push(body);
                async { StatusCode::SERVICE_UNAVAILABLE }
            }),
        ))
        .await;
        let observed_second = captured.clone();
        let (second, _second_server) = serve(axum::Router::new().route(
            "/v1/responses/compact",
            post(move |Json(body): Json<Value>| {
                observed_second.lock().unwrap().push(body);
                async { Json(json!({"id":"success"})) }
            }),
        ))
        .await;
        let (_dir, state) = fixture(&[first, second]);
        state
            .store
            .update(|config| {
                config.keys[0].profile_ids = profiles::all().iter().map(|p| p.id.clone()).collect();
                for (account, actual) in config
                    .accounts
                    .iter_mut()
                    .zip(["gpt-5.6-sol", "gpt-6-astra"])
                {
                    account.models = vec![actual.into()];
                    account
                        .model_mappings
                        .insert("chosen".into(), actual.into());
                }
                Ok(())
            })
            .unwrap();
        let original = json!({"model":"chosen","instructions":"Original","input":"hello","tools":[],"metadata":{"keep":true}});
        let response = call(state.clone(), "/v1/responses/compact", original.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 2);
        for (body, actual) in captured.iter().zip(["gpt-5.6-sol", "gpt-6-astra"]) {
            let profile = profiles::all()
                .iter()
                .find(|p| p.models.contains(&actual.to_owned()))
                .unwrap();
            let mut expected = original.clone();
            expected["model"] = json!(actual);
            expected["instructions"] = json!(format!("{}\n\nOriginal", profile.instruction));
            assert_eq!(*body, expected);
        }
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs[0].model, "chosen");
        assert_eq!(logs[0].attempts, 2);
    }

    #[tokio::test]
    async fn mock_upstream_failover_preserves_compact_payload_and_logs_usage() {
        use axum::{http::Uri, routing::post};
        let (first, _first_server) = serve(axum::Router::new().route(
            "/v1/responses/compact",
            post(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        ))
        .await;
        let received = std::sync::Arc::new(std::sync::Mutex::new(None));
        let observed = received.clone();
        let (second, _second_server) = serve(axum::Router::new().route("/v1/responses/compact", post(move |uri: Uri, headers: HeaderMap, Json(body): Json<Value>| {
            let observed = observed.clone(); async move {
                assert_eq!(headers["authorization"], "Bearer upstream-secret");
                *observed.lock().unwrap() = Some((uri.path().to_owned(), body));
                ([ ("x-request-id", "upstream-request") ], Json(json!({"id":"response","usage":{"input_tokens":20,"output_tokens":3,"input_tokens_details":{"cached_tokens":8}}})))
            }
        }))).await;
        let (_dir, state) = fixture(&[first, second]);
        let input = json!({"model":"chosen","instructions":"Existing","input":[{"role":"user","content":"hello"}],"tools":[{"type":"function","name":"tool"}],"metadata":{"custom":true},"store":false});
        let response = call(state.clone(), "/v1/responses/compact", input.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-request-id"], "upstream-request");
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["id"],
            "response"
        );
        let captured = received.lock().unwrap().clone().unwrap();
        assert_eq!(captured.0, "/v1/responses/compact");
        assert_eq!(captured.1["instructions"], "Existing");
        for field in ["input", "tools", "metadata", "store"] {
            assert_eq!(captured.1[field], input[field]);
        }
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, 200);
        assert_eq!(logs[0].attempts, 2);
        assert_eq!(logs[0].usage.cached_tokens, 8);
        let runtime = state.runtime.lock().unwrap();
        assert!(runtime["account-0"].cooldown_until > store::now());
        assert_eq!(runtime["account-0"].active, 0);
        assert_eq!(runtime["account-1"].active, 0);
    }
    #[tokio::test]
    async fn client_error_is_forwarded_without_retry_or_cooldown_and_secrets_are_redacted() {
        use axum::routing::post;
        let (endpoint, _server) = serve(axum::Router::new().route(
            "/v1/responses",
            post(|| async {
                (
                    StatusCode::BAD_REQUEST,
                    [("content-type", "application/json"), ("retry-after", "7")],
                    r#"{"error":"upstream-secret invalid input"}"#,
                )
            }),
        ))
        .await;
        let (_dir, state) = fixture(&[endpoint]);
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","input":"hello"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["retry-after"], "7");
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("upstream-secret"));
        assert!(String::from_utf8_lossy(&bytes).contains("[redacted]"));
        let runtime = state.runtime.lock().unwrap();
        assert_eq!(runtime["account-0"].failures, 0);
        assert_eq!(runtime["account-0"].active, 0);
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs[0].status, 400);
        assert_eq!(logs[0].attempts, 1);
    }
    #[tokio::test]
    async fn dropping_unconsumed_stream_releases_admission_and_records_499() {
        use axum::routing::post;
        let (endpoint, _server) = serve(axum::Router::new().route("/v1/responses", post(|| async {
            let output = async_stream::stream! {
                yield Ok::<_, std::io::Error>(Bytes::from_static(b"data: {\"type\":\"response.created\"}\n\n"));
                tokio::time::sleep(Duration::from_secs(300)).await;
            };
            ([("content-type", "text/event-stream")], Body::from_stream(output))
        }))).await;
        let (_dir, state) = fixture(&[endpoint]);
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","input":"hello","stream":true}),
        )
        .await;
        assert_eq!(state.runtime.lock().unwrap()["account-0"].active, 1);
        drop(response);
        assert_eq!(state.runtime.lock().unwrap()["account-0"].active, 0);
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs[0].status, 499);
    }

    #[tokio::test]
    async fn sse_success_bytes_and_usage_are_preserved() {
        use axum::routing::post;
        const SSE: &[u8] = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"cyber\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":40,\"output_tokens\":6,\"input_tokens_details\":{\"cached_tokens\":30}}}}\n\ndata: [DONE]\n\n";
        let (endpoint, _server) = serve(axum::Router::new().route(
            "/v1/responses",
            post(|| async {
                let stream = futures_util::stream::iter(
                    SSE.chunks(11)
                        .map(|chunk| Ok::<_, std::io::Error>(Bytes::copy_from_slice(chunk))),
                );
                (
                    [("content-type", "text/event-stream")],
                    Body::from_stream(stream),
                )
            }),
        ))
        .await;
        let (_dir, state) = fixture(&[endpoint]);
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","input":"hello","stream":true}),
        )
        .await;
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert_eq!(&body[..], SSE);
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs[0].status, 200);
        assert_eq!(logs[0].usage.cached_tokens, 30);
        assert!(logs[0].cyber);
        assert_eq!(state.runtime.lock().unwrap()["account-0"].active, 0);
    }

    #[tokio::test]
    async fn explicit_sse_failure_preserves_events_and_counts_failure() {
        use axum::routing::post;
        const SSE: &str = "event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"usage\":{\"input_tokens\":7}}}\n\n";
        let (endpoint, _server) = serve(axum::Router::new().route(
            "/v1/responses",
            post(|| async { ([("content-type", "text/event-stream")], SSE) }),
        ))
        .await;
        let (_dir, state) = fixture(&[endpoint]);
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","input":"hello","stream":true}),
        )
        .await;
        assert_eq!(response.status(), 200);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert_eq!(&body[..], SSE.as_bytes());
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs[0].status, 502);
        assert_eq!(logs[0].usage.input_tokens, 7);
        let runtime = state.runtime.lock().unwrap();
        assert_eq!(runtime["account-0"].active, 0);
        assert_eq!(runtime["account-0"].failures, 1);
    }

    #[tokio::test]
    async fn truncated_sse_reports_transport_failure_and_releases_slot() {
        use axum::routing::post;
        let (endpoint, _server) = serve(axum::Router::new().route(
            "/v1/responses",
            post(|| async {
                (
                    [("content-type", "text/event-stream")],
                    "data: {\"type\":\"response.created\"}\n\n",
                )
            }),
        ))
        .await;
        let (_dir, state) = fixture(&[endpoint]);
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","input":"hello","stream":true}),
        )
        .await;
        assert!(
            axum::body::to_bytes(response.into_body(), 4096)
                .await
                .is_err()
        );
        assert_eq!(
            state.store.logs(0, store::now() + 10).unwrap()[0].status,
            502
        );
        assert_eq!(state.runtime.lock().unwrap()["account-0"].active, 0);
    }

    #[tokio::test]
    async fn cooled_fallback_retries_unless_concurrency_blocked_and_preserves_upstream_error() {
        use axum::routing::post;
        let (endpoint, _server) = serve(axum::Router::new().route(
            "/v1/responses",
            post(|| async {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [("retry-after", "23")],
                    "original upstream failure",
                )
            }),
        ))
        .await;
        for fallback_busy in [false, true] {
            let (_dir, state) = fixture(&[endpoint.clone(), endpoint.clone()]);
            state
                .store
                .update(|config| {
                    config.accounts[1].concurrency = 1;
                    Ok(())
                })
                .unwrap();
            state.runtime.lock().unwrap().insert(
                "account-1".into(),
                crate::Runtime {
                    active: u32::from(fallback_busy),
                    cooldown_until: store::now() + 100,
                    ..Default::default()
                },
            );
            let response = call(
                state.clone(),
                "/v1/responses",
                json!({"model":"chosen","input":"hello"}),
            )
            .await;
            assert_eq!(response.status(), 503);
            assert_eq!(response.headers()["retry-after"], "23");
            let body = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            assert_eq!(&body[..], b"original upstream failure");
            let logs = state.store.logs(0, store::now() + 10).unwrap();
            assert_eq!(logs[0].status, 503);
            assert_eq!(logs[0].attempts, if fallback_busy { 1 } else { 2 });
            assert_eq!(
                state.runtime.lock().unwrap()["account-1"].active,
                u32::from(fallback_busy)
            );
        }
    }

    #[tokio::test]
    async fn client_disconnect_after_completed_event_keeps_success_status() {
        use axum::routing::post;
        let (endpoint, _server) = serve(axum::Router::new().route("/v1/responses", post(|| async {
            let output = async_stream::stream! {
                yield Ok::<_, std::io::Error>(Bytes::from_static(b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":12,\"output_tokens\":2}}}\n\n"));
                tokio::time::sleep(Duration::from_secs(300)).await;
            };
            ([("content-type", "text/event-stream")], Body::from_stream(output))
        }))).await;
        let (_dir, state) = fixture(&[endpoint]);
        let response = call(
            state.clone(),
            "/v1/responses",
            json!({"model":"chosen","input":"hello","stream":true}),
        )
        .await;
        let mut stream = response.into_body().into_data_stream();
        assert!(stream.next().await.unwrap().is_ok());
        drop(stream);
        let logs = state.store.logs(0, store::now() + 10).unwrap();
        assert_eq!(logs[0].status, 200);
        assert_eq!(logs[0].usage.input_tokens, 12);
        assert_eq!(state.runtime.lock().unwrap()["account-0"].active, 0);
    }
}
