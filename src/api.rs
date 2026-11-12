use crate::{
    ApiError, ApiResult, Shared, bad, profiles,
    store::{
        Account, ApiKey, Config, Group, LOG_PAGE_SIZE, RequestLog, Settings, Usage, digest, id,
        now, token,
    },
    unauthorized,
};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn is_https(headers: &HeaderMap) -> bool {
    // Forwarded HTTPS is accepted only when explicitly enabled behind a trusted proxy.
    std::env::var("TRUST_PROXY").as_deref() == Ok("1")
        && headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            == Some("https")
}
pub(crate) fn cookie(headers: &HeaderMap) -> &str {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .find_map(|part| part.trim().strip_prefix("zako_session="))
        .unwrap_or("")
}

/// Returns whether a browser request carries a syntactically valid authority
/// and, when present, an Origin that belongs to that authority.  The console
/// uses this before redirecting an authenticated request from `/` so a forged
/// Host/Origin pair cannot turn the redirect into an open redirect oracle.
pub(crate) fn valid_access_host(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    if host.is_empty()
        || host.chars().any(|c| {
            c.is_ascii_control() || c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#' | '@')
        })
    {
        return false;
    }
    let Ok(host_url) = reqwest::Url::parse(&format!("http://{host}/")) else {
        return false;
    };
    if host_url.host_str().is_none()
        || !host_url.username().is_empty()
        || host_url.password().is_some()
    {
        return false;
    }
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = reqwest::Url::parse(origin.to_str().unwrap_or("")) else {
        return false;
    };
    if !matches!(origin.scheme(), "http" | "https") {
        return false;
    }
    let authority = origin[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
    authority == host
}
fn entry(state: &Shared, headers: &HeaderMap) -> Result<(), ApiError> {
    let cfg = state.store.config()?;
    let requested = headers
        .get("x-console-path")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("/");
    if requested != format!("/{}", cfg.settings.security_path) {
        return Err(ApiError(StatusCode::NOT_FOUND, "Not found".into()));
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = reqwest::Url::parse(origin.to_str().unwrap_or(""))
            .map_err(|_| bad("Invalid origin"))?;
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let authority = origin[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
        if authority != host {
            return Err(ApiError(StatusCode::FORBIDDEN, "Origin mismatch".into()));
        }
    }
    Ok(())
}
pub fn auth(state: &Shared, headers: &HeaderMap) -> Result<(), ApiError> {
    entry(state, headers)?;
    if !state.store.authenticated(cookie(headers)) {
        return Err(unauthorized());
    }
    Ok(())
}
#[derive(Deserialize)]
pub struct LogsQuery {
    #[serde(default)]
    page: u64,
}
fn log_items(logs: Vec<RequestLog>, config: &Config) -> Vec<Value> {
    logs.into_iter()
        .map(|log| {
            let mut item = serde_json::to_value(&log).expect("RequestLog is serializable");
            item["key_name"] = json!(
                config
                    .keys
                    .iter()
                    .find(|key| key.id == log.key_id)
                    .map(|key| &key.name)
            );
            item["account_name"] = json!(
                config
                    .accounts
                    .iter()
                    .find(|account| account.id == log.account_id)
                    .map(|account| &account.name)
            );
            item
        })
        .collect()
}
pub async fn logs(
    State(s): State<Shared>,
    h: HeaderMap,
    Query(query): Query<LogsQuery>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let page = s.store.logs_page(query.page)?;
    let config = s.store.config()?;
    Ok(Json(json!({
        "items": log_items(page.items, &config),
        "page": page.page,
        "page_size": LOG_PAGE_SIZE,
        "total": page.total,
        "total_pages": page.total_pages,
        "retention_days": page.retention_days,
    })))
}
pub async fn export_logs(State(s): State<Shared>, h: HeaderMap) -> Result<Response, ApiError> {
    auth(&s, &h)?;
    let (retention_days, logs) = s.store.export_logs()?;
    let config = s.store.config()?;
    Ok((
        [
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"request-logs.json\"",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Json(json!({
            "exported_at": now(),
            "retention_days": retention_days,
            "logs": log_items(logs, &config),
        })),
    )
        .into_response())
}
pub async fn delete_logs(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    Ok(Json(json!({"deleted": s.store.delete_logs()?})))
}
fn session_response(
    state: &Shared,
    headers: &HeaderMap,
    username: &str,
    hash: &str,
) -> Result<Response, ApiError> {
    let secret = token("");
    if !state
        .store
        .session_for_credentials(&secret, username, hash)?
    {
        return Err(unauthorized());
    }
    let secure = if is_https(headers) { "; Secure" } else { "" };
    Ok((
        [(
            header::SET_COOKIE,
            format!(
                "zako_session={secret}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400{secure}"
            ),
        )],
        Json(json!({"ok":true})),
    )
        .into_response())
}
fn password_hash(password: &str) -> Result<String, ApiError> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|h| h.to_string())
        .map_err(|_| bad("Password hashing failed"))
}
pub(crate) fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).ok().is_some_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}
fn valid_password(password: &str) -> Result<(), ApiError> {
    if password.len() > 1024 || !(12..=256).contains(&password.chars().count()) {
        return Err(bad("Password must contain 12 to 256 characters"));
    }
    Ok(())
}
fn name(value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() || value.chars().count() > 100 {
        return Err(bad("Name must contain 1 to 100 characters"));
    }
    Ok(())
}

pub async fn bootstrap(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    entry(&s, &h)?;
    let c = s.store.config()?;
    let authenticated = s.store.authenticated(cookie(&h));
    Ok(Json(
        json!({"initialized":!c.password_hash.is_empty(),"authenticated":authenticated,"username":if authenticated {c.username} else {String::new()},"turnstile_site_key": if s.turnstile_secret_key().is_empty() { String::new() } else { s.turnstile_site_key() }}),
    ))
}
#[derive(Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
    #[serde(default)]
    turnstile_token: String,
}
pub async fn setup(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Response, ApiError> {
    entry(&s, &h)?;
    name(&input.username)?;
    valid_password(&input.password)?;
    if !s.store.config()?.password_hash.is_empty() {
        return Err(bad("Administrator is already initialized"));
    }
    let hash = tokio::task::spawn_blocking(move || password_hash(&input.password))
        .await
        .map_err(|_| bad("Password hashing failed"))??;
    let username = input.username.trim().to_owned();
    s.store.update(|c| {
        if !c.password_hash.is_empty() {
            return Err("Administrator is already initialized".into());
        }
        c.username = username.clone();
        c.password_hash = hash.clone();
        Ok(())
    })?;
    session_response(&s, &h, &username, &hash)
}
pub async fn login(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Response, ApiError> {
    entry(&s, &h)?;
    // A global budget also prevents distributed username/IP rotation on single-admin installs.
    {
        let mut attempts = s.login_attempts.lock().unwrap();
        let a = attempts.entry("admin".into()).or_insert((now(), 0));
        if now() - a.0 > 60 {
            *a = (now(), 0);
        }
        if a.1 >= 10 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many attempts; wait one minute".into(),
            ));
        }
        a.1 += 1;
    }
    let c = s.store.config()?;
    let password = input.password;
    let hash = c.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify(&password, &hash))
        .await
        .unwrap_or(false);
    if input.username != c.username || !valid {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid credentials".into(),
        ));
    }
    let turnstile_secret = s.turnstile_secret_key();
    if !turnstile_secret.is_empty() {
        verify_turnstile(&s, &h, &turnstile_secret, &input.turnstile_token).await?;
    }
    s.login_attempts.lock().unwrap().remove("admin");
    session_response(&s, &h, &c.username, &c.password_hash)
}

#[derive(Deserialize)]
struct TurnstileResponse {
    success: bool,
    #[serde(default)]
    hostname: String,
    #[serde(default)]
    #[serde(rename = "error-codes")]
    error_codes: Vec<String>,
}

fn turnstile_hostname_matches(headers: &HeaderMap, challenge_hostname: &str) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if host.is_empty()
        || host.chars().any(|c| {
            c.is_ascii_control() || c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#' | '@')
        })
    {
        return false;
    }
    let Ok(host_url) = reqwest::Url::parse(&format!("http://{host}/")) else {
        return false;
    };
    if !host_url.username().is_empty() || host_url.password().is_some() {
        return false;
    }
    let Some(request_hostname) = host_url.host_str() else {
        return false;
    };
    request_hostname
        .trim_end_matches('.')
        .eq_ignore_ascii_case(challenge_hostname.trim().trim_end_matches('.'))
}

async fn verify_turnstile(
    state: &Shared,
    headers: &HeaderMap,
    secret: &str,
    token: &str,
) -> Result<(), ApiError> {
    if token.trim().is_empty() {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "请完成 Cloudflare Turnstile 安全校验".into(),
        ));
    }
    let remoteip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut form = vec![("secret", secret), ("response", token)];
    if let Some(ip) = remoteip {
        form.push(("remoteip", ip));
    }
    let verify_url = std::env::var("TURNSTILE_VERIFY_URL")
        .unwrap_or_else(|_| "https://challenges.cloudflare.com/turnstile/v0/siteverify".into());
    let response = state
        .client
        .post(verify_url)
        .timeout(std::time::Duration::from_secs(10))
        .form(&form)
        .send()
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Cloudflare Turnstile 校验服务不可用".into(),
            )
        })?;
    let result = response.json::<TurnstileResponse>().await.map_err(|_| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "Cloudflare Turnstile 校验服务不可用".into(),
        )
    })?;
    if !result.success || !turnstile_hostname_matches(headers, &result.hostname) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Cloudflare Turnstile 安全校验失败".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct TurnstileTestInput {
    #[serde(default)]
    pub turnstile_site_key: Option<String>,
    #[serde(default)]
    pub turnstile_secret_key: Option<String>,
}

pub async fn test_turnstile(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<TurnstileTestInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let site_key = input
        .turnstile_site_key
        .unwrap_or_else(|| s.turnstile_site_key());
    let secret = input
        .turnstile_secret_key
        .unwrap_or_else(|| s.turnstile_secret_key());
    if site_key.trim().is_empty() || secret.trim().is_empty() {
        return Err(bad("请先填写 Cloudflare Turnstile 站点密钥和密钥"));
    }
    if site_key
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
        || secret.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(bad("Cloudflare Turnstile 密钥格式无效"));
    }
    let verify_url = std::env::var("TURNSTILE_VERIFY_URL")
        .unwrap_or_else(|_| "https://challenges.cloudflare.com/turnstile/v0/siteverify".into());
    let response = s
        .client
        .post(verify_url)
        .timeout(std::time::Duration::from_secs(10))
        .form(&[
            ("secret", secret.as_str()),
            ("response", "zako-connectivity-test"),
        ])
        .send()
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::BAD_GATEWAY,
                "Cloudflare Turnstile 连接失败".into(),
            )
        })?;
    let status = response.status();
    let result = response.json::<TurnstileResponse>().await.map_err(|_| {
        ApiError(
            StatusCode::BAD_GATEWAY,
            "Cloudflare Turnstile 返回无效响应".into(),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError(
            StatusCode::BAD_GATEWAY,
            "Cloudflare Turnstile 连接失败".into(),
        ));
    }
    if result
        .error_codes
        .iter()
        .any(|code| code == "invalid-input-secret")
    {
        return Err(bad("Cloudflare Turnstile 密钥无效"));
    }
    Ok(Json(json!({
        "ok": true,
        "connected": true,
        "message": "Cloudflare Turnstile 连接正常，密钥可用",
    })))
}
pub async fn logout(State(s): State<Shared>, h: HeaderMap) -> Result<Response, ApiError> {
    auth(&s, &h)?;
    s.store.logout(cookie(&h));
    Ok((
        [(
            header::SET_COOKIE,
            "zako_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        )],
        Json(json!({"ok":true})),
    )
        .into_response())
}
pub async fn groups(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    Ok(Json(json!(s.store.config()?.groups)))
}
#[derive(Deserialize)]
pub struct GroupSaveInput {
    name: String,
    #[serde(default, deserialize_with = "explicit_description")]
    description: Option<String>,
}
fn explicit_description<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer).map(Some)
}
fn explicit_default_strategy<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    bool::deserialize(deserializer).map(Some)
}
pub async fn save_group(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(g): Json<GroupSaveInput>,
) -> ApiResult<Value> {
    save_group_impl(&s, &h, None, g)
}
pub async fn save_group_id(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(g): Json<GroupSaveInput>,
) -> ApiResult<Value> {
    save_group_impl(&s, &h, Some(id), g)
}
fn save_group_impl(
    s: &Shared,
    h: &HeaderMap,
    path: Option<String>,
    input: GroupSaveInput,
) -> ApiResult<Value> {
    auth(s, h)?;
    name(&input.name)?;
    let description = input.description.as_deref().unwrap_or_default().trim();
    if description.chars().count() > 2000 {
        return Err(bad("Group description must not exceed 2000 characters"));
    }
    let mut g = Group {
        id: path.clone().unwrap_or_else(id),
        name: input.name.trim().into(),
        description: description.into(),
        ..Default::default()
    };
    s.store.update(|c| {
        if let Some(p) = path {
            let existing = c
                .groups
                .iter_mut()
                .find(|x| x.id == p)
                .ok_or("Group not found")?;
            if input.description.is_none() {
                g.description = existing.description.clone();
            }
            *existing = g.clone();
        } else {
            c.groups.push(g.clone());
        }
        Ok(())
    })?;
    Ok(Json(json!(g)))
}
pub async fn delete_group(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    s.store.update(|c| {
        if c.accounts.iter().any(|a| a.group_ids.contains(&id))
            || c.keys.iter().any(|k| k.group_id == id)
        {
            return Err("Group is in use by accounts or API keys".into());
        }
        c.groups.retain(|g| g.id != id);
        Ok(())
    })?;
    Ok(Json(json!({"ok":true})))
}
pub async fn accounts(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    let c = s.store.config()?;
    let runtime = s.runtime.lock().unwrap();
    Ok(Json(Value::Array(
        c.accounts
            .iter()
            .map(|a| {
                let mut v = public_account(&s, a);
                v["runtime"] = json!(runtime.get(&a.id).cloned().unwrap_or_default());
                v
            })
            .collect(),
    )))
}
fn public_account(state: &Shared, account: &Account) -> Value {
    let mut out = json!(account);
    out.as_object_mut().unwrap().remove("api_key");
    out.as_object_mut().unwrap().remove("proxy_url");
    out["has_api_key"] = json!(!account.api_key.is_empty());
    out["has_proxy"] = json!(!account.proxy_url.is_empty());
    out["proxy_display"] = json!(
        state
            .store
            .unseal(&account.proxy_url)
            .and_then(|url| crate::network::proxy_display(&url))
            .unwrap_or_default()
    );
    out
}
#[derive(Deserialize)]
pub struct AccountInput {
    #[serde(flatten)]
    account: Account,
    #[serde(default)]
    proxy_url: Option<String>,
    #[serde(flatten)]
    provider_selection: crate::provider_keys::Selection,
}
pub async fn save_account(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(a): Json<AccountInput>,
) -> ApiResult<Value> {
    save_account_impl(&s, &h, None, a).await
}
pub async fn save_account_id(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(a): Json<AccountInput>,
) -> ApiResult<Value> {
    save_account_impl(&s, &h, Some(id), a).await
}
async fn save_account_impl(
    s: &Shared,
    h: &HeaderMap,
    path: Option<String>,
    input: AccountInput,
) -> ApiResult<Value> {
    auth(s, h)?;
    let existing = path
        .as_ref()
        .map(|id| {
            s.store
                .config()?
                .accounts
                .into_iter()
                .find(|a| &a.id == id)
                .ok_or_else(|| bad("Account not found"))
        })
        .transpose()?;
    let mut a = input.account;
    let input_has_api_key = !a.api_key.is_empty();
    let mut provider_snapshot = None;
    // Binding metadata is backend-owned and may not be forged through Account's flattened input.
    a.provider_binding = None;
    match input.provider_selection.choose(existing.as_ref())? {
        crate::provider_keys::Choice::Provider {
            provider_id,
            key_id,
        } => {
            if !a.api_key.is_empty() {
                return Err(bad("服务商绑定模式不能提交手填 API 密钥"));
            }
            let resolved = crate::provider_keys::resolve(s, &provider_id, &key_id).await?;
            provider_snapshot = Some(resolved.snapshot);
            a.endpoint = resolved.endpoint;
            a.api_key = resolved.secret;
            a.provider_binding = Some(resolved.binding);
        }
        crate::provider_keys::Choice::Api { detached } => {
            if detached && a.api_key.is_empty() {
                return Err(bad("解除服务商绑定后必须填写新的 API 密钥"));
            }
        }
    }
    name(&a.name)?;
    crate::proxy::upstream_url(&a.endpoint, "models")?;
    if a.protocols.is_empty()
        || a.protocols
            .iter()
            .any(|p| !matches!(p.as_str(), "responses" | "compact" | "chat" | "anthropic"))
    {
        return Err(bad(
            "Select responses, compact, chat, or anthropic protocol",
        ));
    }
    if a.api_key.len() > 4096 || a.models.len() > 500 || a.model_mappings.len() > 500 {
        return Err(bad("Account configuration is too large"));
    }
    let valid_model = |model: &str| {
        !model.is_empty()
            && model.len() <= 256
            && !model
                .chars()
                .any(|ch| ch.is_whitespace() || ch.is_control())
    };
    if a.models.iter().any(|model| !valid_model(model))
        || a.model_mappings.iter().any(|(requested, actual)| {
            !valid_model(requested) || !valid_model(actual) || !a.models.contains(actual)
        })
    {
        return Err(bad(
            "Model IDs must contain 1 to 256 bytes without whitespace; mapping targets must be selected available models",
        ));
    }
    let mut selected_models = std::collections::HashSet::new();
    a.models
        .retain(|model| selected_models.insert(model.clone()));
    a.id = path.clone().unwrap_or_else(id);
    if !a.api_key.is_empty() {
        a.api_key = s.store.seal(&a.api_key)?;
    }
    let preserve_proxy = input.proxy_url.is_none();
    if let Some(value) = input.proxy_url.filter(|value| !value.is_empty()) {
        let url = crate::network::validate_proxy(&value)?;
        a.proxy_url = s.store.seal(url.as_str())?;
    }
    s.store.update(|c| {
        if let Some(snapshot) = provider_snapshot.as_ref()
            && !c
                .providers
                .iter()
                .any(|p| p.id == snapshot.id && crate::providers::same_connection(p, snapshot))
        {
            return Err("服务商已删除或站点已更改，请重新选择".into());
        }
        if a.group_ids
            .iter()
            .any(|g| !c.groups.iter().any(|v| &v.id == g))
        {
            return Err("Unknown group".into());
        }
        if let Some(p) = path {
            let stored = c
                .accounts
                .iter_mut()
                .find(|x| x.id == p)
                .ok_or("Account not found")?;
            // Resolving a provider key awaits remote requests. Do not let that
            // older edit revive or replace a binding changed while it waited.
            if existing.as_ref().is_some_and(|before| before != stored) {
                return Err("账号已更改，请重新打开编辑后保存".into());
            }
            if stored.provider_binding.is_some()
                && a.provider_binding.is_none()
                && !input_has_api_key
            {
                return Err("解除服务商绑定后必须填写新的 API 密钥".into());
            }
            if a.api_key.is_empty() {
                a.api_key = stored.api_key.clone();
            }
            if preserve_proxy {
                a.proxy_url = stored.proxy_url.clone();
            }
            *stored = a.clone();
        } else {
            if a.api_key.is_empty() {
                return Err("Upstream API key is required".into());
            }
            c.accounts.push(a.clone());
        }
        Ok(())
    })?;
    s.proxy_clients.lock().unwrap().remove(&a.id);
    Ok(Json(public_account(s, &a)))
}
pub async fn delete_account(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    s.store.update(|c| {
        c.accounts.retain(|a| a.id != id);
        Ok(())
    })?;
    s.runtime.lock().unwrap().remove(&id);
    s.proxy_clients.lock().unwrap().remove(&id);
    Ok(Json(json!({"ok":true})))
}
pub async fn keys(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    let c = s.store.config()?;
    let logs = s.store.logs(0, now())?;
    Ok(Json(Value::Array(c.keys.iter().map(|k|{
        let mut u=Usage::default();for l in logs.iter().filter(|l|l.key_id==k.id){u.input_tokens+=l.usage.input_tokens;u.output_tokens+=l.usage.output_tokens;u.cached_tokens+=l.usage.cached_tokens;u.cache_creation_input_tokens+=l.usage.cache_creation_input_tokens;u.cache_read_input_tokens+=l.usage.cache_read_input_tokens;}
        json!({"id":k.id,"name":k.name,"group_id":k.group_id,"profile_ids":k.profile_ids,"default_strategy_enabled":k.default_strategy_enabled,"prefix":k.prefix,"enabled":k.enabled,"created_at":k.created_at,"usage":u})
    }).collect())))
}
#[derive(Deserialize)]
pub struct KeyInput {
    name: String,
    group_id: String,
    #[serde(default, deserialize_with = "explicit_key_profiles")]
    profile_ids: Option<Vec<String>>,
    #[serde(default, deserialize_with = "explicit_default_strategy")]
    default_strategy_enabled: Option<bool>,
    #[serde(default = "key_enabled")]
    enabled: bool,
}
fn key_enabled() -> bool {
    true
}
fn explicit_key_profiles<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<String>::deserialize(deserializer).map(Some)
}
fn key_profiles(
    config: &Config,
    group_id: &str,
    mut selected: Vec<String>,
    prune_unsupported: bool,
) -> crate::store::Result<Vec<String>> {
    if !config.groups.iter().any(|group| group.id == group_id) {
        return Err("Unknown group".into());
    }
    let supported: std::collections::HashSet<_> = config
        .accounts
        .iter()
        .filter(|account| {
            account.enabled
                && account.provider_available()
                && account.group_ids.iter().any(|id| id == group_id)
        })
        .flat_map(|account| account.models.iter().map(String::as_str))
        .collect();
    let available = |id: &str| {
        profiles::find(id).is_some_and(|profile| {
            profile
                .models
                .iter()
                .any(|model| supported.contains(model.as_str()))
        })
    };
    if prune_unsupported {
        selected.retain(|id| available(id));
    }
    let mut ids = std::collections::HashSet::new();
    selected.retain(|id| ids.insert(id.clone()));
    let mut models = std::collections::HashSet::new();
    for id in &selected {
        let profile = profiles::find(id).ok_or("Unknown instruction profile")?;
        if !available(id) {
            return Err(
                "Instruction profile is not supported by this group's enabled accounts".into(),
            );
        }
        for model in &profile.models {
            if !models.insert(model.clone()) {
                return Err(format!(
                    "Only one instruction profile may target model {model}"
                ));
            }
        }
    }
    Ok(selected)
}
pub async fn create_key(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<KeyInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    name(&input.name)?;
    let key = token("sk-zako-");
    let mut k = ApiKey {
        id: id(),
        name: input.name,
        group_id: input.group_id,
        profile_ids: input.profile_ids.unwrap_or_default(),
        default_strategy_enabled: input.default_strategy_enabled.unwrap_or(false),
        hash: digest(&key),
        secret: s.store.seal(&key)?,
        prefix: format!("{}...{}", &key[..15], &key[key.len() - 4..]),
        enabled: input.enabled,
        created_at: now(),
    };
    s.store.update(|c| {
        k.profile_ids = key_profiles(c, &k.group_id, k.profile_ids.clone(), false)?;
        c.keys.push(k.clone());
        Ok(())
    })?;
    Ok(Json(json!({"id":k.id,"key":key})))
}
pub async fn update_key(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<KeyInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    name(&input.name)?;
    s.store.update(|c| {
        let existing = c
            .keys
            .iter()
            .find(|key| key.id == id)
            .ok_or("Key not found")?;
        let selected = key_profiles(
            c,
            &input.group_id,
            input
                .profile_ids
                .clone()
                .unwrap_or_else(|| existing.profile_ids.clone()),
            input.profile_ids.is_none(),
        )?;
        let k = c
            .keys
            .iter_mut()
            .find(|k| k.id == id)
            .ok_or("Key not found")?;
        k.name = input.name;
        k.group_id = input.group_id;
        k.profile_ids = selected;
        if let Some(enabled) = input.default_strategy_enabled {
            k.default_strategy_enabled = enabled;
        }
        k.enabled = input.enabled;
        Ok(())
    })?;
    Ok(Json(json!({"ok":true})))
}
pub async fn delete_key(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    s.store.update(|c| {
        c.keys.retain(|k| k.id != id);
        Ok(())
    })?;
    Ok(Json(json!({"ok":true})))
}
pub async fn reveal_key(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let c = s.store.config()?;
    let k = c
        .keys
        .iter()
        .find(|k| k.id == id)
        .ok_or_else(|| bad("Key not found"))?;
    Ok(Json(json!({"key":s.store.unseal(&k.secret)?})))
}
pub async fn settings(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    let settings = s.store.config()?.settings;
    Ok(Json(settings_public(
        &settings,
        &s.turnstile_site_key(),
        !s.turnstile_secret_key().is_empty(),
    )))
}

fn settings_public(settings: &Settings, site_key: &str, turnstile_enabled: bool) -> Value {
    json!({
        "security_path": settings.security_path,
        "turnstile_site_key": site_key,
        "turnstile_enabled": turnstile_enabled,
        "max_retries": settings.max_retries,
        "failure_threshold": settings.failure_threshold,
        "cooldown_seconds": settings.cooldown_seconds,
        "request_timeout_seconds": settings.request_timeout_seconds,
        "retention_days": settings.retention_days,
    })
}

#[derive(Deserialize)]
pub struct SettingsInput {
    pub security_path: String,
    #[serde(default)]
    pub turnstile_site_key: Option<String>,
    #[serde(default)]
    pub turnstile_secret_key: Option<String>,
    pub max_retries: u32,
    pub failure_threshold: u32,
    pub cooldown_seconds: u32,
    pub request_timeout_seconds: u32,
    pub retention_days: u32,
}
pub async fn save_settings(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<SettingsInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let current_site = s.turnstile_site_key();
    let current_secret = s.turnstile_secret_key();
    let update_site = input.turnstile_site_key.is_some();
    let submitted_secret = input.turnstile_secret_key;
    let mut site_key = input.turnstile_site_key.unwrap_or(current_site);
    let update_secret = submitted_secret.is_some();
    let mut secret_key = current_secret;
    let mut settings = Settings {
        security_path: input.security_path,
        max_retries: input.max_retries,
        failure_threshold: input.failure_threshold,
        cooldown_seconds: input.cooldown_seconds,
        request_timeout_seconds: input.request_timeout_seconds,
        retention_days: input.retention_days,
    };
    site_key = site_key.trim().to_owned();
    if let Some(secret) = submitted_secret {
        let secret = secret.trim();
        if secret.len() > 512 || secret.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(bad("Cloudflare Turnstile 密钥格式无效"));
        }
        secret_key = secret.to_owned();
        if secret_key.is_empty() {
            site_key.clear();
        }
    }
    if site_key.len() > 256
        || site_key
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(bad("Cloudflare Turnstile 站点密钥格式无效"));
    }
    if site_key.is_empty() && !secret_key.is_empty() {
        return Err(bad("请输入 Cloudflare Turnstile 站点密钥"));
    }
    if !site_key.is_empty() && secret_key.is_empty() {
        return Err(bad("请输入 Cloudflare Turnstile 密钥"));
    }
    settings.security_path = settings.security_path.trim_matches('/').into();
    if !settings
        .security_path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || settings.security_path.len() > 100
    {
        return Err(bad(
            "Security entry accepts letters, numbers, hyphens and underscores",
        ));
    }
    if ["api", "v1", "health", "assets", "favicon.svg"].contains(&settings.security_path.as_str()) {
        return Err(bad("Security entry is reserved"));
    }
    if settings.max_retries > 10
        || settings.failure_threshold == 0
        || settings.failure_threshold > 100
        || settings.cooldown_seconds == 0
        || settings.cooldown_seconds > 86400
        || settings.request_timeout_seconds < 5
        || settings.request_timeout_seconds > 3600
        || settings.retention_days == 0
        || settings.retention_days > 3650
    {
        return Err(bad("Scheduler values are out of range"));
    }
    if update_site || update_secret {
        s.save_turnstile_keys(&site_key, &secret_key)
            .map_err(|error| ApiError(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    }
    s.store.update(|c| {
        c.settings = settings.clone();
        Ok(())
    })?;
    Ok(Json(settings_public(
        &settings,
        &site_key,
        !secret_key.is_empty(),
    )))
}
#[derive(Deserialize)]
pub struct PasswordInput {
    current_password: String,
    new_password: String,
    confirm_password: String,
}
pub async fn password(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<PasswordInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    valid_password(&input.new_password)?;
    if input.new_password != input.confirm_password {
        return Err(bad("New password confirmation does not match"));
    }
    let old_hash = s.store.config()?.password_hash;
    let expected_hash = old_hash.clone();
    let old = input.current_password;
    let hash = tokio::task::spawn_blocking(move || {
        if !verify(&old, &old_hash) {
            return Err(bad("Current password is incorrect"));
        }
        password_hash(&input.new_password)
    })
    .await
    .map_err(|_| bad("Password hashing failed"))??;
    if !s.store.change_password(cookie(&h), &expected_hash, hash)? {
        return Err(unauthorized());
    }
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
pub struct DiscoverInput {
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    protocols: Option<Vec<String>>,
    #[serde(default)]
    proxy_url: Option<String>,
    #[serde(flatten)]
    provider_selection: crate::provider_keys::Selection,
}
pub async fn discover(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<DiscoverInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let account = if input.account_id.is_some() {
        let c = s.store.config()?;
        c.accounts
            .into_iter()
            .find(|a| Some(&a.id) == input.account_id.as_ref())
            .ok_or_else(|| bad("Account not found"))?
            .into()
    } else {
        None
    };
    let (endpoint, key) = crate::provider_keys::connection(
        &s,
        &input.provider_selection,
        account.as_ref(),
        &input.endpoint,
        &input.api_key,
    )
    .await?;
    if key.is_empty() {
        return Err(bad("API key is required"));
    }
    let url = crate::proxy::upstream_url(&endpoint, "models")?;
    let protocols = input
        .protocols
        .as_ref()
        .or_else(|| account.as_ref().map(|a| &a.protocols));
    let client = crate::network::selected_client(&s, account.as_ref(), input.proxy_url.as_deref())?;
    let request = client.get(url).timeout(std::time::Duration::from_secs(20));
    let request = if protocols.is_some_and(|items| items.iter().any(|p| p == "anthropic")) {
        let request = request
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01");
        if protocols.is_some_and(|items| {
            items
                .iter()
                .any(|p| p == "responses" || p == "compact" || p == "chat")
        }) {
            request.bearer_auth(key)
        } else {
            request
        }
    } else {
        request.bearer_auth(key)
    };
    let mut r = request
        .send()
        .await
        .map_err(|_| bad("Upstream connection failed"))?;
    if !r.status().is_success() {
        return Err(bad(&format!(
            "Upstream returned HTTP {}",
            r.status().as_u16()
        )));
    }
    const MAX_MODEL_RESPONSE_BYTES: usize = 1024 * 1024;
    if r.content_length()
        .is_some_and(|size| size > MAX_MODEL_RESPONSE_BYTES as u64)
    {
        return Err(bad("Upstream model response is too large"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|_| bad("Upstream connection failed"))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_MODEL_RESPONSE_BYTES {
            return Err(bad("Upstream model response is too large"));
        }
        body.extend_from_slice(&chunk);
    }
    let v: Value =
        serde_json::from_slice(&body).map_err(|_| bad("Upstream returned invalid JSON"))?;
    let mut models: Vec<String> = v["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    models.dedup();
    Ok(Json(json!({"models":models})))
}
#[derive(Deserialize)]
pub struct ProxyTestInput {
    #[serde(default)]
    proxy_url: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    protocols: Option<Vec<String>>,
    #[serde(flatten)]
    provider_selection: crate::provider_keys::Selection,
}
pub async fn test_proxy(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<ProxyTestInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let config = s.store.config()?;
    let account = input
        .account_id
        .as_ref()
        .map(|id| {
            config
                .accounts
                .iter()
                .find(|account| &account.id == id)
                .ok_or_else(|| bad("Account not found"))
        })
        .transpose()?;
    let proxy_url = match input.proxy_url {
        Some(value) if !value.is_empty() => value,
        None if account.is_some_and(|account| !account.proxy_url.is_empty()) => {
            s.store.unseal(&account.unwrap().proxy_url)?
        }
        _ => return Err(bad("A SOCKS proxy URL is required for testing")),
    };
    let (endpoint, key) = crate::provider_keys::connection(
        &s,
        &input.provider_selection,
        account,
        input.endpoint.as_deref().unwrap_or_default(),
        &input.api_key,
    )
    .await?;
    let url = crate::proxy::upstream_url(&endpoint, "models")?;
    if key.len() > 4096 {
        return Err(bad("API key is too large"));
    }
    let protocols = input
        .protocols
        .as_ref()
        .or_else(|| account.map(|account| &account.protocols));
    let started = std::time::Instant::now();
    let client = crate::network::proxy_client(&proxy_url)?;
    let mut request = client.get(url).timeout(std::time::Duration::from_secs(12));
    if !key.is_empty() {
        if protocols.is_some_and(|items| items.iter().any(|protocol| protocol == "anthropic")) {
            request = request
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01");
            if protocols.is_some_and(|items| {
                items
                    .iter()
                    .any(|protocol| matches!(protocol.as_str(), "responses" | "compact" | "chat"))
            }) {
                request = request.bearer_auth(&key);
            }
        } else {
            request = request.bearer_auth(&key);
        }
    }
    let result = request.send().await;
    let latency_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok(Json(match result {
        Ok(response) => {
            let status = response.status();
            json!({"connected":true, "ok":status.is_success(), "latency_ms":latency_ms,
                "upstream_status":status.as_u16(), "error":if status.is_success() {None} else {Some(format!("Proxy connected; upstream returned HTTP {}", status.as_u16()))}})
        }
        Err(error) => json!({"connected":false, "ok":false, "latency_ms":latency_ms,
            "upstream_status":null, "error":crate::network::connection_error(&error)}),
    }))
}
pub async fn profiles(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    let config = s.store.config()?;
    let mut catalog: Vec<_> = profiles::all()
        .iter()
        .map(std::borrow::Cow::Borrowed)
        .collect();
    let mut seen: std::collections::HashSet<_> =
        catalog.iter().map(|profile| profile.id.clone()).collect();
    for id in config.keys.iter().flat_map(|key| &key.profile_ids) {
        if seen.insert(id.clone())
            && let Some(profile) = profiles::find(id)
        {
            catalog.push(profile);
        }
    }
    Ok(Json(json!(catalog)))
}
pub async fn profile_templates(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    Ok(Json(json!(profiles::templates())))
}
#[derive(Deserialize)]
pub struct Range {
    from: Option<i64>,
    to: Option<i64>,
}
pub async fn stats(
    State(s): State<Shared>,
    h: HeaderMap,
    Query(range): Query<Range>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let end = range.to.unwrap_or_else(now);
    let start = range.from.unwrap_or(end - 7 * 86400);
    if start > end {
        return Err(bad("Invalid date range"));
    }
    let logs = s.store.logs(start, end)?;
    let live = s.store.logs(now() - 60, now())?;
    let c = s.store.config()?;
    let mut input = 0u64;
    let mut output = 0u64;
    let mut cached = 0u64;
    let mut cache_creation = 0u64;
    let mut cache_read = 0u64;
    let mut latency = 0u64;
    let mut successes = 0u64;
    let mut cyber = 0u64;
    let mut daily: BTreeMap<String, Value> = BTreeMap::new();
    for l in &logs {
        input += l.usage.input_tokens;
        output += l.usage.output_tokens;
        cached += l.usage.cached_tokens;
        cache_creation += l.usage.cache_creation_input_tokens;
        cache_read += l.usage.cache_read_input_tokens;
        latency += l.latency_ms;
        let ok = (200..300).contains(&l.status);
        successes += u64::from(ok);
        cyber += u64::from(l.cyber);
        let date = chrono::DateTime::from_timestamp(l.timestamp, 0)
            .unwrap_or_default()
            .format("%Y-%m-%d")
            .to_string();
        let d=daily.entry(date.clone()).or_insert(json!({"date":date,"input_tokens":0,"output_tokens":0,"cached_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"total_requests":0,"success_count":0,"cyber_count":0}));
        for (key, n) in [
            ("input_tokens", l.usage.input_tokens),
            ("output_tokens", l.usage.output_tokens),
            ("cached_tokens", l.usage.cached_tokens),
            (
                "cache_creation_input_tokens",
                l.usage.cache_creation_input_tokens,
            ),
            ("cache_read_input_tokens", l.usage.cache_read_input_tokens),
            ("total_requests", 1),
            ("success_count", u64::from(ok)),
            ("cyber_count", u64::from(l.cyber)),
        ] {
            d[key] = json!(d[key].as_u64().unwrap_or(0) + n);
        }
    }
    let runtime = s.runtime.lock().unwrap();
    let cooling = c
        .accounts
        .iter()
        .filter(|a| runtime.get(&a.id).is_some_and(|r| r.cooldown_until > now()))
        .count();
    let n = logs.len() as f64;
    let healthy = c
        .accounts
        .iter()
        .filter(|a| a.enabled && !runtime.get(&a.id).is_some_and(|r| r.cooldown_until > now()))
        .count();
    let recent:Vec<Value>=logs.iter().take(100).map(|l|json!({"id":l.id,"created_at":l.timestamp,"model":l.model,"account_name":c.accounts.iter().find(|a|a.id==l.account_id).map(|a|&a.name),"key_name":c.keys.iter().find(|k|k.id==l.key_id).map(|k|&k.name),"status":l.status,"latency_ms":l.latency_ms,"first_token_ms":l.first_token_ms,"input_tokens":l.usage.input_tokens,"output_tokens":l.usage.output_tokens,"cached_tokens":l.usage.cached_tokens,"cache_creation_input_tokens":l.usage.cache_creation_input_tokens,"cache_read_input_tokens":l.usage.cache_read_input_tokens,"cyber_count":u8::from(l.cyber),"attempts":l.attempts,"protocol":l.protocol})).collect();
    Ok(Json(
        json!({"summary":{"input_tokens":input,"output_tokens":output,"cached_tokens":cached,"cache_creation_input_tokens":cache_creation,"cache_read_input_tokens":cache_read,"uncached_tokens":input.saturating_sub(cached),"total_tokens":input+output,"cache_hit_rate":if input>0{cached as f64/input as f64*100.0}else{0.0},"rpm":live.len(),"tpm":live.iter().map(|l|l.usage.input_tokens+l.usage.output_tokens).sum::<u64>(),"total_requests":logs.len(),"success_rate":if n>0.0{successes as f64/n*100.0}else{0.0},"avg_latency_ms":if n>0.0{latency as f64/n}else{0.0},"cyber_count":cyber},"daily":daily.into_values().collect::<Vec<_>>(),"recent":recent,"accounts":{"total":c.accounts.len(),"healthy":healthy,"cooling":cooling},"keys":c.keys.len()}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turnstile_challenge_must_match_request_hostname() {
        let headers =
            HeaderMap::from_iter([(header::HOST, "console.example.test:1990".parse().unwrap())]);
        assert!(turnstile_hostname_matches(&headers, "console.example.test"));
        assert!(turnstile_hostname_matches(
            &headers,
            "CONSOLE.EXAMPLE.TEST."
        ));
        assert!(!turnstile_hostname_matches(&headers, "other.example.test"));
        assert!(!turnstile_hostname_matches(
            &HeaderMap::new(),
            "console.example.test"
        ));
    }

    #[test]
    fn password_verification() {
        let hash = password_hash("a-long-secret-password").unwrap();
        assert!(verify("a-long-secret-password", &hash));
        assert!(!verify("wrong", &hash));
        assert!(valid_password("short").is_err());
        assert!(valid_password(&"\u{4e00}".repeat(11)).is_err());
        assert!(valid_password(&"\u{4e00}".repeat(256)).is_ok());
        assert!(valid_password(&"a".repeat(257)).is_err());
    }
}
