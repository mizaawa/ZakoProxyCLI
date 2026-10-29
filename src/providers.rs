//! Independent provider management. Credentials are encrypted at rest and omitted from public APIs.
use crate::{
    ApiError, ApiResult, Shared,
    api::auth,
    bad,
    store::{id, now},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) const OFFICIAL_PROVIDER_ID: &str = "official-zayuai";

pub(crate) fn is_official(provider_id: &str) -> bool {
    provider_id == OFFICIAL_PROVIDER_ID
}

fn official_provider() -> Provider {
    Provider {
        preflight_key: String::new(),
        id: OFFICIAL_PROVIDER_ID.into(),
        name: "杂鱼AI".into(),
        kind: "sub2api".into(),
        base_url: "https://www.zayuapi.com".into(),
        auth_mode: "password".into(),
        username: String::new(),
        user_id: String::new(),
        password: String::new(),
        access_token: String::new(),
        refresh_token: String::new(),
        token_type: bearer(),
        price_multiplier: one(),
        notes: String::new(),
        metrics: Metrics {
            currency: usd(),
            ..Metrics::default()
        },
        status: "not_authenticated".into(),
        last_error: None,
        last_synced_at: None,
        created_at: 0,
        updated_at: 0,
    }
}

fn require_deletable(provider_id: &str) -> Result<(), ApiError> {
    if is_official(provider_id) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "官方预置服务商不能删除".into(),
        ));
    }
    Ok(())
}

pub(crate) fn require_authenticated(p: &Provider) -> Result<(), ApiError> {
    let missing = if p.auth_mode == "password" {
        p.username.is_empty() || p.password.is_empty()
    } else {
        p.access_token.is_empty() && p.refresh_token.is_empty()
    };
    if missing {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "服务商尚未登录，请先完成站点登录后再读取或创建密钥".into(),
        ));
    }
    Ok(())
}

fn bearer() -> String {
    "Bearer".into()
}
fn one() -> f64 {
    1.0
}
fn usd() -> String {
    "USD".into()
}
fn unverified() -> String {
    "unverified".into()
}
fn input_user_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::String(value) => Ok(value),
        Value::Number(value) if value.is_u64() => Ok(value.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err(serde::de::Error::custom(
            "user_id must be a string or positive integer",
        )),
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub balance: Option<f64>,
    pub today_usage: Option<f64>,
    pub total_usage: Option<f64>,
    pub total_recharged: Option<f64>,
    #[serde(default)]
    pub total_recharged_estimated: bool,
    #[serde(default = "usd")]
    pub currency: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Provider {
    #[serde(skip)]
    preflight_key: String,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub auth_mode: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default = "bearer")]
    pub token_type: String,
    #[serde(default = "one")]
    pub price_multiplier: f64,
    #[serde(default)]
    pub notes: String,
    #[serde(flatten)]
    pub metrics: Metrics,
    #[serde(default = "unverified")]
    pub status: String,
    pub last_error: Option<String>,
    pub last_synced_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Deserialize)]
pub struct Input {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub auth_mode: String,
    #[serde(default)]
    pub username: String,
    #[serde(default, deserialize_with = "input_user_id")]
    pub user_id: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default = "bearer")]
    pub token_type: String,
    #[serde(default = "one")]
    pub price_multiplier: f64,
    #[serde(default)]
    pub notes: String,
    /// One-time proof forwarded only to an upstream Sub2API login request.
    #[serde(default)]
    pub turnstile_token: String,
}

fn public(p: &Provider) -> Value {
    let mut v = json!(p);
    let obj = v.as_object_mut().expect("Provider is an object");
    for field in ["password", "access_token", "refresh_token"] {
        obj.remove(field);
    }
    obj.insert("has_password".into(), json!(!p.password.is_empty()));
    obj.insert("has_access_token".into(), json!(!p.access_token.is_empty()));
    obj.insert(
        "has_refresh_token".into(),
        json!(!p.refresh_token.is_empty()),
    );
    let official = is_official(&p.id);
    for field in ["official", "is_official", "pinned"] {
        obj.insert(field.into(), json!(official));
    }
    obj.insert("readonly".into(), json!(false));
    v
}

pub(crate) fn lookup(s: &Shared, provider_id: &str) -> Result<Provider, ApiError> {
    s.store
        .config()?
        .providers
        .into_iter()
        .find(|p| p.id == provider_id)
        .or_else(|| is_official(provider_id).then(official_provider))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "服务商不存在".into()))
}

fn build(s: &Shared, input: Input, existing: Option<&Provider>) -> Result<Provider, ApiError> {
    if input.name.trim().is_empty() || input.name.chars().count() > 100 {
        return Err(bad("名称须为 1 到 100 个字符"));
    }
    if !["sub2api", "newapi"].contains(&input.kind.as_str()) {
        return Err(bad("不支持的服务商类型"));
    }
    if !["password", "token"].contains(&input.auth_mode.as_str()) {
        return Err(bad("不支持的认证方式"));
    }
    if input.base_url.len() > 2048 {
        return Err(bad("站点地址过长"));
    }
    let mut url = reqwest::Url::parse(input.base_url.trim())
        .map_err(|_| bad("请输入有效的 HTTP 或 HTTPS 站点地址"))?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(bad(
            "站点地址仅支持 HTTP/HTTPS，不能包含凭据、查询参数或片段",
        ));
    }
    url.set_path(url.path().trim_end_matches('/').to_owned().as_str());
    let base_url = url.as_str().trim_end_matches('/').to_owned();
    if existing.is_some_and(|p| is_official(&p.id))
        && (input.name.trim() != "杂鱼AI"
            || base_url != "https://www.zayuapi.com"
            || input.kind != "sub2api")
    {
        return Err(bad("官方服务商名称、站点地址和平台类型不可修改"));
    }
    if !input.price_multiplier.is_finite()
        || input.price_multiplier <= 0.0
        || input.price_multiplier > 1_000_000.0
    {
        return Err(bad("充值倍率须为正数且不超过 1000000"));
    }
    if input.notes.chars().count() > 2000 || input.username.len() > 254 || input.user_id.len() > 64
    {
        return Err(bad("备注或账号信息过长"));
    }
    if [
        &input.password,
        &input.access_token,
        &input.refresh_token,
        &input.turnstile_token,
    ]
    .iter()
    .any(|v| v.len() > 16384)
    {
        return Err(bad("认证凭据过长"));
    }
    let username = input.username.trim().to_owned();
    let same_identity = existing.is_some_and(|p| {
        p.kind == input.kind
            && p.base_url == base_url
            && p.auth_mode == input.auth_mode
            && p.username == username
            && (input.auth_mode != "token" || p.user_id == input.user_id.trim())
    });
    let secret = |plain: &str, old: Option<&str>| -> Result<String, ApiError> {
        if !plain.is_empty() {
            Ok(s.store.seal(plain)?)
        } else if same_identity {
            Ok(old.unwrap_or_default().to_owned())
        } else {
            Ok(String::new())
        }
    };
    let mut p = Provider {
        preflight_key: String::new(),
        id: existing.map(|p| p.id.clone()).unwrap_or_else(id),
        name: input.name.trim().into(),
        kind: input.kind,
        base_url,
        auth_mode: input.auth_mode,
        username,
        user_id: input.user_id.trim().into(),
        password: secret(&input.password, existing.map(|p| p.password.as_str()))?,
        access_token: secret(
            input.access_token.trim(),
            existing.map(|p| p.access_token.as_str()),
        )?,
        refresh_token: secret(
            input.refresh_token.trim(),
            existing.map(|p| p.refresh_token.as_str()),
        )?,
        token_type: input.token_type.trim().into(),
        price_multiplier: input.price_multiplier,
        notes: input.notes,
        metrics: Metrics {
            currency: usd(),
            ..Metrics::default()
        },
        status: unverified(),
        last_error: None,
        last_synced_at: None,
        created_at: existing
            .map(|p| p.created_at)
            .filter(|t| *t > 0)
            .unwrap_or_else(now),
        updated_at: now(),
    };
    if p.token_type.is_empty() {
        p.token_type = bearer();
    }
    // An explicit refresh-only input is a new credential pair. Combining it
    // with a previously rotated access token also breaks the preflight key.
    if p.kind == "sub2api"
        && p.auth_mode == "token"
        && !input.refresh_token.trim().is_empty()
        && input.access_token.trim().is_empty()
    {
        p.access_token.clear();
    }
    if p.token_type.len() > 32
        || !p
            .token_type
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(bad("Token Type 格式无效"));
    }
    // A newly entered password must perform a fresh upstream login. A blank
    // password in an unchanged edit keeps the encrypted session established by
    // the previous successful Turnstile preflight.
    if p.auth_mode == "password" && !input.password.is_empty() {
        p.access_token.clear();
        p.refresh_token.clear();
    }
    if p.auth_mode == "password" {
        if p.username.is_empty() || p.password.is_empty() {
            return Err(bad("请输入账号和密码；更换站点或认证方式后须重新输入凭据"));
        }
    } else {
        p.password.clear();
        if p.access_token.is_empty() && !(p.kind == "sub2api" && !p.refresh_token.is_empty()) {
            return Err(bad("请输入站点访问令牌；Sub2API 也可仅使用 Refresh Token"));
        }
        if p.kind == "newapi" {
            let access = decrypt(s, &p.access_token).map_err(|error| bad(&error.message))?;
            if !access.starts_with("nap_")
                && (p.user_id.is_empty()
                    || !p.user_id.bytes().all(|c| c.is_ascii_digit())
                    || p.user_id.parse::<u64>().unwrap_or(0) == 0)
            {
                return Err(bad(
                    "旧版 NewAPI 令牌认证需要有效的用户 ID；新版 nap_ 令牌可留空",
                ));
            }
        }
        if p.kind == "newapi" {
            p.refresh_token.clear();
        }
    }
    if same_identity
        && input.password.is_empty()
        && input.access_token.is_empty()
        && input.refresh_token.is_empty()
        && let Some(old) = existing
    {
        p.metrics = old.metrics.clone();
        p.status = old.status.clone();
        p.last_error = old.last_error.clone();
        p.last_synced_at = old.last_synced_at;
    }
    p.preflight_key = connection_key(s, &p).map_err(|error| bad(&error.message))?;
    if p.kind == "sub2api" {
        let mut cached = s
            .provider_preflights
            .lock()
            .map_err(|_| bad("服务商预检缓存不可用"))?;
        cached.retain(|_, (expires, _)| *expires > now());
        if let Some((_, staged)) = cached.get(&p.preflight_key) {
            p.access_token = staged.access_token.clone();
            p.refresh_token = staged.refresh_token.clone();
            p.token_type = staged.token_type.clone();
        }
    }
    Ok(p)
}

pub async fn list(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    let config = s.store.config()?;
    let official = config
        .providers
        .iter()
        .find(|p| is_official(&p.id))
        .cloned()
        .unwrap_or_else(official_provider);
    let mut providers = vec![public(&official)];
    providers.extend(
        config
            .providers
            .iter()
            .filter(|p| !is_official(&p.id))
            .map(public),
    );
    Ok(Json(Value::Array(providers)))
}

/// Read only the upstream's public CAPTCHA configuration. The site key is
/// public configuration; no upstream secret or user credential is returned.
#[derive(Deserialize)]
pub struct AuthRequirementsInput {
    pub kind: String,
    pub base_url: String,
}

fn probe_provider(kind: &str, base_url: &str) -> Result<Provider, ApiError> {
    if !["sub2api", "newapi"].contains(&kind) {
        return Err(bad("不支持的服务商类型"));
    }
    if base_url.trim().len() > 2048 {
        return Err(bad("站点地址过长"));
    }
    let mut url = reqwest::Url::parse(base_url.trim())
        .map_err(|_| bad("请输入有效的 HTTP 或 HTTPS 站点地址"))?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(bad(
            "站点地址仅支持 HTTP/HTTPS，不能包含凭据、查询参数或片段",
        ));
    }
    url.set_path(url.path().trim_end_matches('/').to_owned().as_str());
    let mut provider = official_provider();
    provider.id = "auth-requirements-probe".into();
    provider.kind = kind.into();
    provider.base_url = url.as_str().trim_end_matches('/').to_owned();
    Ok(provider)
}

async fn read_auth_requirements(s: &Shared, provider: &Provider) -> Result<Value, ApiError> {
    if provider.kind != "sub2api" {
        return Ok(json!({
            "turnstile_required": false,
            "turnstile_site_key": ""
        }));
    }
    let response = request(
        s,
        provider,
        "/api/v1/settings/public",
        &HeaderMap::new(),
        None,
    )
    .await
    .map_err(|error| ApiError(StatusCode::BAD_GATEWAY, error.message))?;
    let enabled = response
        .data
        .get("turnstile_enabled")
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.as_str().and_then(|value| value.parse::<bool>().ok()))
        })
        .unwrap_or(false);
    let site_key = response
        .data
        .get("turnstile_site_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    Ok(json!({
        "turnstile_required": enabled,
        "turnstile_site_key": site_key,
    }))
}

pub async fn auth_requirements(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(provider_id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let provider = lookup(&s, &provider_id)?;
    Ok(Json(read_auth_requirements(&s, &provider).await?))
}

pub async fn auth_requirements_input(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<AuthRequirementsInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let provider = probe_provider(&input.kind, &input.base_url)?;
    Ok(Json(read_auth_requirements(&s, &provider).await?))
}

pub async fn create(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<Input>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let p = build(&s, input, None)?;
    s.store.update(|c| {
        c.providers.push(p.clone());
        Ok(())
    })?;
    Ok(Json(public(&p)))
}

pub async fn edit(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(provider_id): Path<String>,
    Json(input): Json<Input>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let old = lookup(&s, &provider_id)?;
    let p = build(&s, input, Some(&old))?;
    let identity_changed = p.base_url != old.base_url
        || p.kind != old.kind
        || p.auth_mode != old.auth_mode
        || p.username != old.username
        || (p.auth_mode == "token" && p.user_id != old.user_id);
    s.store.update(|c| {
        if let Some(stored) = c.providers.iter_mut().find(|p| p.id == provider_id) {
            if !same_connection(stored, &old) {
                return Err("服务商登录信息已更改，请重新打开编辑".into());
            }
            *stored = p.clone();
        } else if is_official(&provider_id) && old.created_at == 0 {
            c.providers.push(p.clone());
        } else {
            return Err("服务商不存在".into());
        }
        if identity_changed {
            for account in &mut c.accounts {
                if let Some(binding) = account
                    .provider_binding
                    .as_mut()
                    .filter(|binding| binding.provider_id == provider_id)
                {
                    binding.status = "provider_changed".into();
                    binding.last_synced_at = now();
                    account.api_key.clear();
                }
            }
        }
        Ok(())
    })?;
    Ok(Json(public(&p)))
}

pub async fn delete(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(provider_id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    require_deletable(&provider_id)?;
    lookup(&s, &provider_id)?;
    s.store.update(|c| {
        c.providers.retain(|p| p.id != provider_id);
        for account in &mut c.accounts {
            if let Some(binding) = account
                .provider_binding
                .as_mut()
                .filter(|b| b.provider_id == provider_id)
            {
                binding.status = "provider_missing".into();
                binding.last_synced_at = now();
                account.api_key.clear();
            }
        }
        Ok(())
    })?;
    Ok(Json(json!({"ok":true})))
}

pub(crate) struct RemoteError {
    pub(crate) message: String,
    pub(crate) unauthorized: bool,
}
impl RemoteError {
    pub(crate) fn invalid(path: &str) -> Self {
        Self {
            message: format!("站点返回的数据无效（{path}）"),
            unauthorized: false,
        }
    }
}
pub(crate) struct RemoteResponse {
    pub(crate) data: Value,
    pub(crate) envelope: Value,
    pub(crate) headers: HeaderMap,
}

pub(crate) async fn request(
    s: &Shared,
    p: &Provider,
    path: &str,
    headers: &HeaderMap,
    body: Option<Value>,
) -> Result<RemoteResponse, RemoteError> {
    let url = format!("{}{path}", p.base_url);
    let builder = if let Some(body) = body {
        s.client.post(&url).json(&body)
    } else {
        s.client.get(&url)
    };
    let mut response = builder
        .headers(headers.clone())
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| RemoteError {
            message: if e.is_timeout() {
                "站点请求超时".into()
            } else if e.is_connect() {
                "无法连接站点，请检查地址和网络".into()
            } else {
                "站点请求失败，请检查网络和 TLS 配置".into()
            },
            unauthorized: false,
        })?;
    let status = response.status();
    if !status.is_success() {
        let pat_scope = if status == StatusCode::FORBIDDEN
            && p.kind == "newapi"
            && p.auth_mode == "token"
            && decrypt(s, &p.access_token).is_ok_and(|token| token.starts_with("nap_"))
        {
            if path.starts_with("/api/token/") {
                Some(if path.ends_with("/key") {
                    "api_key:reveal"
                } else if path.starts_with("/api/token/?") {
                    "api_key:read"
                } else {
                    "api_key:write"
                })
            } else if path == "/api/log/self/stat" {
                Some("usage:read")
            } else if path == "/api/user/self" {
                Some("profile:read")
            } else {
                None
            }
        } else {
            None
        };
        return Err(RemoteError {
            message: if let Some(scope) = pat_scope {
                format!("New API 令牌缺少 {scope} 权限（{path}）")
            } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                "站点认证失败，请检查登录信息或更新令牌".into()
            } else {
                format!("站点返回 HTTP {}（{path}）", status.as_u16())
            },
            unauthorized: status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        });
    }
    let response_headers = response.headers().clone();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| RemoteError::invalid(path))?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err(RemoteError::invalid(path));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| RemoteError::invalid(path))?;
    if value.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(RemoteError {
            message: format!("站点拒绝了请求，请检查账号权限与登录信息（{path}）"),
            unauthorized: true,
        });
    }
    if value
        .get("code")
        .and_then(Value::as_i64)
        .is_some_and(|code| code != 0 && code != 200)
    {
        return Err(RemoteError {
            message: format!("站点返回失败状态（{path}）"),
            unauthorized: value["code"] == 401 || value["code"] == 403,
        });
    }
    let data = value.get("data").cloned().unwrap_or_else(|| value.clone());
    if !data.is_object() && !data.is_array() {
        return Err(RemoteError::invalid(path));
    }
    Ok(RemoteResponse {
        data,
        envelope: value,
        headers: response_headers,
    })
}

pub(crate) fn number(data: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        data.get(*key)
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|v| v.parse().ok()))
            })
            .filter(|v: &f64| v.is_finite())
    })
}
pub(crate) fn text(data: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        data.get(*key)
            .and_then(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .filter(|v| !v.is_empty())
    })
}
pub(crate) fn insert_header(
    headers: &mut HeaderMap,
    key: &'static str,
    value: &str,
) -> Result<(), RemoteError> {
    headers.insert(
        key,
        value.parse().map_err(|_| RemoteError {
            message: "认证凭据含无效字符".into(),
            unauthorized: false,
        })?,
    );
    Ok(())
}
pub(crate) fn decrypt(s: &Shared, value: &str) -> Result<String, RemoteError> {
    if value.is_empty() {
        return Ok(String::new());
    }
    s.store.unseal(value).map_err(|_| RemoteError {
        message: "无法读取已保存的凭据，请重新输入".into(),
        unauthorized: false,
    })
}
fn save_token(s: &Shared, value: &str) -> Result<String, RemoteError> {
    s.store.seal(value).map_err(|_| RemoteError {
        message: "无法加密站点凭据".into(),
        unauthorized: false,
    })
}

pub(crate) async fn sub_refresh(s: &Shared, p: &mut Provider) -> Result<String, RemoteError> {
    let refresh_token = decrypt(s, &p.refresh_token)?;
    if refresh_token.is_empty() {
        return Err(RemoteError {
            message: "访问令牌已过期，请重新输入".into(),
            unauthorized: true,
        });
    }
    let response = request(
        s,
        p,
        "/api/v1/auth/refresh",
        &HeaderMap::new(),
        Some(json!({"refresh_token":refresh_token})),
    )
    .await?;
    let access = text(&response.data, &["access_token", "accessToken"])
        .ok_or_else(|| RemoteError::invalid("/api/v1/auth/refresh"))?;
    p.access_token = save_token(s, &access)?;
    if let Some(refresh) = text(&response.data, &["refresh_token", "refreshToken"]) {
        p.refresh_token = save_token(s, &refresh)?;
    }
    if let Some(kind) = text(&response.data, &["token_type", "tokenType"]) {
        p.token_type = kind;
    }
    Ok(access)
}

async fn sub_metrics(
    s: &Shared,
    p: &Provider,
    headers: &HeaderMap,
) -> Result<Metrics, RemoteError> {
    let me = request(s, p, "/api/v1/auth/me", headers, None).await?.data;
    let stats = request(s, p, "/api/v1/usage/dashboard/stats", headers, None)
        .await?
        .data;
    let balance =
        number(&me, &["balance"]).ok_or_else(|| RemoteError::invalid("/api/v1/auth/me"))?;
    let today = number(&stats, &["today_actual_cost"])
        .ok_or_else(|| RemoteError::invalid("/api/v1/usage/dashboard/stats"))?;
    let total = number(&stats, &["total_actual_cost"]);
    let reported_recharged = number(&me, &["total_recharged"]);
    let estimated = reported_recharged.is_none() && total.is_some();
    let recharged = if estimated {
        total.map(|v| v + balance)
    } else {
        reported_recharged
    };
    Ok(Metrics {
        balance: Some(balance),
        today_usage: Some(today),
        total_usage: total,
        total_recharged: recharged,
        total_recharged_estimated: estimated,
        currency: usd(),
    })
}

pub(crate) async fn sub_password_login(
    s: &Shared,
    p: &mut Provider,
    turnstile_token: Option<&str>,
) -> Result<String, RemoteError> {
    if turnstile_token.is_none_or(|token| token.trim().is_empty()) {
        let requirements = read_auth_requirements(s, p)
            .await
            .map_err(|error| RemoteError {
                message: error.1,
                unauthorized: false,
            })?;
        if requirements["turnstile_required"] == true {
            return Err(RemoteError {
                message: "请在服务商管理中完成 Cloudflare Turnstile 验证后重新测试登录".into(),
                unauthorized: true,
            });
        }
    }
    let password = decrypt(s, &p.password)?;
    let mut body = json!({"email":p.username,"password":password});
    if let Some(token) = turnstile_token.filter(|token| !token.trim().is_empty()) {
        body["turnstile_token"] = json!(token);
    }
    let login = request(s, p, "/api/v1/auth/login", &HeaderMap::new(), Some(body)).await?;
    let access = text(&login.data, &["access_token", "accessToken"])
        .ok_or_else(|| RemoteError::invalid("/api/v1/auth/login"))?;
    p.access_token = save_token(s, &access)?;
    if let Some(refresh) = text(&login.data, &["refresh_token", "refreshToken"]) {
        p.refresh_token = save_token(s, &refresh)?;
    } else {
        p.refresh_token.clear();
    }
    if let Some(kind) = text(&login.data, &["token_type", "tokenType"]) {
        p.token_type = kind;
    }
    Ok(access)
}

async fn fetch_sub(
    s: &Shared,
    p: &mut Provider,
    turnstile_token: Option<&str>,
) -> Result<Metrics, RemoteError> {
    let mut access;
    let mut refreshed = false;
    let mut logged_in = false;
    if p.auth_mode == "password" {
        access = decrypt(s, &p.access_token)?;
        if access.is_empty() && !p.refresh_token.is_empty() {
            match sub_refresh(s, p).await {
                Ok(token) => {
                    access = token;
                    refreshed = true;
                }
                Err(error) if error.unauthorized => {
                    access = sub_password_login(s, p, turnstile_token).await?;
                    logged_in = true;
                }
                Err(error) => return Err(error),
            }
        }
        if access.is_empty() {
            access = sub_password_login(s, p, turnstile_token).await?;
            logged_in = true;
        }
    } else {
        access = decrypt(s, &p.access_token)?;
    }
    if access.is_empty() {
        access = sub_refresh(s, p).await?;
        refreshed = true;
    }
    let mut headers = HeaderMap::new();
    insert_header(
        &mut headers,
        "authorization",
        &format!("{} {access}", p.token_type),
    )?;
    match sub_metrics(s, p, &headers).await {
        Err(error)
            if error.unauthorized && !p.refresh_token.is_empty() && !refreshed && !logged_in =>
        {
            access = match sub_refresh(s, p).await {
                Ok(token) => token,
                Err(refresh_error) if refresh_error.unauthorized && p.auth_mode == "password" => {
                    sub_password_login(s, p, turnstile_token).await?
                }
                Err(refresh_error) => return Err(refresh_error),
            };
            insert_header(
                &mut headers,
                "authorization",
                &format!("{} {access}", p.token_type),
            )?;
            sub_metrics(s, p, &headers).await
        }
        Err(error) if error.unauthorized && p.auth_mode == "password" && !logged_in => {
            access = sub_password_login(s, p, turnstile_token).await?;
            insert_header(
                &mut headers,
                "authorization",
                &format!("{} {access}", p.token_type),
            )?;
            sub_metrics(s, p, &headers).await
        }
        result => result,
    }
}

pub(crate) async fn new_auth_headers(
    s: &Shared,
    p: &mut Provider,
) -> Result<HeaderMap, RemoteError> {
    let mut headers = HeaderMap::new();
    if p.auth_mode == "password" {
        let password = decrypt(s, &p.password)?;
        let login = request(
            s,
            p,
            "/api/user/login",
            &HeaderMap::new(),
            Some(json!({"username":p.username,"password":password})),
        )
        .await?;
        if let Some(access) = text(&login.data, &["access_token", "accessToken"]) {
            p.user_id = text(&login.data["user"], &["id", "user_id", "userId"])
                .or_else(|| text(&login.data, &["id", "user_id", "userId"]))
                .ok_or_else(|| RemoteError::invalid("/api/user/login"))?;
            insert_header(&mut headers, "authorization", &format!("Bearer {access}"))?;
        } else {
            // Older NewAPI installations return a user ID and session cookies.
            p.user_id = text(&login.data, &["id", "user_id", "userId"])
                .ok_or_else(|| RemoteError::invalid("/api/user/login"))?;
            let cookies = login
                .headers
                .get_all("set-cookie")
                .iter()
                .filter_map(|v| v.to_str().ok())
                .filter_map(|v| v.split(';').next())
                .filter(|v| v.contains('='))
                .collect::<Vec<_>>()
                .join("; ");
            if cookies.is_empty() {
                return Err(RemoteError {
                    message: "站点登录未返回会话 Cookie，请检查账号或站点的二次验证要求".into(),
                    unauthorized: true,
                });
            }
            insert_header(&mut headers, "cookie", &cookies)?;
            insert_header(&mut headers, "new-api-user", &p.user_id)?;
        }
    } else {
        let access = decrypt(s, &p.access_token)?;
        insert_header(&mut headers, "authorization", &format!("Bearer {access}"))?;
        if !access.starts_with("nap_") {
            insert_header(&mut headers, "new-api-user", &p.user_id)?;
        }
    }
    Ok(headers)
}

async fn fetch_new(s: &Shared, p: &mut Provider) -> Result<Metrics, RemoteError> {
    let headers = new_auth_headers(s, p).await?;
    let unit = match request(s, p, "/api/status", &HeaderMap::new(), None).await {
        Ok(response) => number(&response.data, &["quota_per_unit"])
            .filter(|v| *v > 0.0)
            .unwrap_or(500_000.0),
        Err(_) => 500_000.0,
    };
    let me = request(s, p, "/api/user/self", &headers, None).await?.data;
    if let Some(user_id) = text(&me, &["id", "user_id", "userId"]) {
        p.user_id = user_id;
    }
    let local = chrono::Local::now();
    let midnight = local
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("Midnight is valid");
    use chrono::TimeZone;
    let start = chrono::Local
        .from_local_datetime(&midnight)
        .earliest()
        .ok_or_else(|| RemoteError {
            message: "无法确定当前日期范围".into(),
            unauthorized: false,
        })?
        .timestamp();
    let end = local.timestamp();
    let stat_path =
        format!("/api/log/self/stat?type=2&start_timestamp={start}&end_timestamp={end}");
    let stat = request(s, p, &stat_path, &headers, None).await?.data;
    let quota =
        number(&me, &["quota", "balance"]).ok_or_else(|| RemoteError::invalid("/api/user/self"))?;
    let used = number(&me, &["used_quota", "usedQuota", "used"]);
    let today = number(&stat, &["quota", "used_quota", "usedQuota"])
        .ok_or_else(|| RemoteError::invalid("/api/log/self/stat"))?;
    Ok(Metrics {
        balance: Some(quota / unit),
        today_usage: Some(today / unit),
        total_usage: used.map(|v| v / unit),
        total_recharged: used.map(|v| (quota + v) / unit),
        total_recharged_estimated: used.is_some(),
        currency: usd(),
    })
}

async fn fetch(
    s: &Shared,
    p: &mut Provider,
    turnstile_token: Option<&str>,
) -> Result<Metrics, RemoteError> {
    if p.kind == "sub2api" {
        fetch_sub(s, p, turnstile_token).await
    } else {
        fetch_new(s, p).await
    }
}

pub(crate) fn same_connection(a: &Provider, b: &Provider) -> bool {
    a.base_url == b.base_url
        && a.kind == b.kind
        && a.auth_mode == b.auth_mode
        && a.username == b.username
        && a.user_id == b.user_id
        && a.password == b.password
        && a.access_token == b.access_token
        && a.refresh_token == b.refresh_token
        && a.token_type == b.token_type
}

fn connection_key(s: &Shared, p: &Provider) -> Result<String, RemoteError> {
    // Only this digest is used as the lookup key; cached credentials remain encrypted.
    let identity = json!({"kind":p.kind,"base_url":p.base_url,"auth_mode":p.auth_mode,
        "username":p.username,"user_id":if p.auth_mode == "token" {p.user_id.as_str()} else {""},
        "password":decrypt(s, &p.password)?,"access_token":decrypt(s, &p.access_token)?,
        "refresh_token":decrypt(s, &p.refresh_token)?,"token_type":p.token_type});
    Ok(crate::store::digest(&identity.to_string()))
}

fn cache_rotation(s: &Shared, before: &Provider, after: &Provider) -> Result<(), ApiError> {
    if before.access_token == after.access_token && before.refresh_token == after.refresh_token {
        return Ok(());
    }
    let mut cache = s
        .provider_preflights
        .lock()
        .map_err(|_| bad("服务商预检缓存不可用"))?;
    cache.retain(|_, (expires, _)| *expires > now());
    if cache.len() >= 256
        && !cache.contains_key(&before.preflight_key)
        && let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, (expires, _))| *expires)
            .map(|(key, _)| key.clone())
    {
        cache.remove(&oldest);
    }
    cache.insert(before.preflight_key.clone(), (now() + 600, after.clone()));
    Ok(())
}

fn persist_result(
    s: &Shared,
    before: &Provider,
    after: &Provider,
    metrics: Option<Metrics>,
    error: Option<String>,
) -> Result<Provider, ApiError> {
    s.store
        .update(|c| {
            let current = c
                .providers
                .iter_mut()
                .find(|p| p.id == before.id)
                .ok_or("服务商已被删除")?;
            if !same_connection(current, before) {
                return Err("服务商认证配置已更改，请重新刷新".into());
            }
            current.access_token = after.access_token.clone();
            current.refresh_token = after.refresh_token.clone();
            current.token_type = after.token_type.clone();
            current.user_id = after.user_id.clone();
            if let Some(metrics) = metrics {
                current.metrics = metrics;
                current.last_synced_at = Some(now());
            }
            current.status = if error.is_some() {
                "error"
            } else {
                "connected"
            }
            .into();
            current.last_error = error;
            current.updated_at = now();
            Ok(current.clone())
        })
        .map_err(|error| ApiError(StatusCode::CONFLICT, error))
}

pub async fn refresh(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(provider_id): Path<String>,
) -> Result<Response, ApiError> {
    auth(&s, &h)?;
    let before = lookup(&s, &provider_id)?;
    require_authenticated(&before)?;
    let mut after = before.clone();
    match fetch(&s, &mut after, None).await {
        Ok(metrics) => {
            let stored = persist_result(&s, &before, &after, Some(metrics), None)?;
            Ok(Json(public(&stored)).into_response())
        }
        Err(error) => {
            let stored = persist_result(&s, &before, &after, None, Some(error.message.clone()))?;
            Ok((
                StatusCode::BAD_GATEWAY,
                Json(json!({"error":error.message,"provider":public(&stored)})),
            )
                .into_response())
        }
    }
}

pub async fn test(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<Input>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    let turnstile_token =
        (!input.turnstile_token.trim().is_empty()).then(|| input.turnstile_token.clone());
    let existing = input
        .id
        .as_deref()
        .map(|provider_id| lookup(&s, provider_id))
        .transpose()?;
    let mut candidate = build(&s, input, existing.as_ref())?;
    let before = candidate.clone();
    let result = fetch(&s, &mut candidate, turnstile_token.as_deref()).await;
    cache_rotation(&s, &before, &candidate)?;
    // A test of saved credentials may rotate a single-use refresh token.
    // Keep the rotation so the next refresh does not reuse an invalid token.
    if let Some(existing) = &existing {
        let same_credentials = connection_key(&s, existing).ok().is_some_and(|key| {
            key == before.preflight_key || connection_key(&s, &before).ok().as_ref() == Some(&key)
        });
        if same_credentials
            && (candidate.access_token != existing.access_token
                || candidate.refresh_token != existing.refresh_token)
        {
            match &result {
                Ok(metrics) => {
                    persist_result(&s, existing, &candidate, Some(metrics.clone()), None)?;
                }
                Err(error) => {
                    persist_result(&s, existing, &candidate, None, Some(error.message.clone()))?;
                }
            }
        }
    }
    let metrics = result.map_err(|error| ApiError(StatusCode::BAD_GATEWAY, error.message))?;
    let mut out = json!(metrics);
    out["ok"] = json!(true);
    out["user_id"] = json!(candidate.user_id);
    Ok(Json(out))
}
