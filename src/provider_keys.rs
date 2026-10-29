//! Provider-owned API keys are resolved only by the backend; public results contain metadata.
use crate::{
    ApiError, ApiResult, Shared, bad,
    providers::{self, Provider, RemoteError},
    store::{Account, ProviderBinding, now},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Serialize)]
pub struct ProviderKey {
    pub id: String,
    pub name: String,
    pub group_name: String,
    pub group_multiplier: Option<f64>,
    pub status: String,
    pub available: bool,
    #[serde(skip)]
    secret: String,
}

#[derive(Serialize)]
struct ProviderGroup {
    id: String,
    name: String,
    multiplier: Option<f64>,
}

#[derive(Deserialize)]
pub struct CreateKeyInput {
    group_id: String,
}

pub(crate) struct ResolvedKey {
    pub endpoint: String,
    pub secret: String,
    pub binding: ProviderBinding,
    pub snapshot: Provider,
}

/// Missing fields preserve a saved binding. Explicit null/empty pairs detach it.
#[derive(Deserialize, Default)]
pub(crate) struct Selection {
    #[serde(default, deserialize_with = "explicit_field")]
    provider_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "explicit_field")]
    provider_key_id: Option<Option<String>>,
}
fn explicit_field<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}
pub(crate) enum Choice {
    Api { detached: bool },
    Provider { provider_id: String, key_id: String },
}
impl Selection {
    pub(crate) fn choose(&self, account: Option<&Account>) -> Result<Choice, ApiError> {
        match (&self.provider_id, &self.provider_key_id) {
            (None, None) => Ok(match account.and_then(|a| a.provider_binding.as_ref()) {
                Some(b) if b.status == "provider_changed" => {
                    return Err(bad("服务商站点或身份已更改，请重新选择 API 密钥"));
                }
                Some(b) => Choice::Provider {
                    provider_id: b.provider_id.clone(),
                    key_id: b.key_id.clone(),
                },
                None => Choice::Api { detached: false },
            }),
            (Some(p), Some(k)) => {
                let p = p.as_deref().unwrap_or_default().trim();
                let k = k.as_deref().unwrap_or_default().trim();
                if p.is_empty() && k.is_empty() {
                    return Ok(Choice::Api {
                        detached: account.is_some_and(|a| a.provider_binding.is_some()),
                    });
                }
                if p.is_empty() || k.is_empty() || p.len() > 128 || !valid_id(k) {
                    return Err(bad("请选择完整的服务商和 API 密钥"));
                }
                Ok(Choice::Provider {
                    provider_id: p.into(),
                    key_id: k.into(),
                })
            }
            _ => Err(bad("provider_id 和 provider_key_id 必须同时提供")),
        }
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn remote(error: RemoteError) -> ApiError {
    ApiError(StatusCode::BAD_GATEWAY, error.message)
}

async fn renew_sub_session(s: &Shared, p: &mut Provider) -> Result<String, RemoteError> {
    if !p.refresh_token.is_empty() {
        match providers::sub_refresh(s, p).await {
            Ok(access) => return Ok(access),
            Err(error) if error.unauthorized && p.auth_mode == "password" => {}
            Err(error) => return Err(error),
        }
    }
    if p.auth_mode == "password" {
        providers::sub_password_login(s, p, None).await
    } else {
        providers::sub_refresh(s, p).await
    }
}

async fn headers(s: &Shared, p: &mut Provider) -> Result<HeaderMap, RemoteError> {
    let mut h = HeaderMap::new();
    if p.kind == "sub2api" {
        let access = if p.auth_mode == "password" {
            let mut access = providers::decrypt(s, &p.access_token)?;
            if access.is_empty() && !p.refresh_token.is_empty() {
                access = renew_sub_session(s, p).await?;
            }
            if access.is_empty() {
                access = renew_sub_session(s, p).await?;
            }
            access
        } else {
            let access = providers::decrypt(s, &p.access_token)?;
            if access.is_empty() {
                providers::sub_refresh(s, p).await?
            } else {
                access
            }
        };
        providers::insert_header(
            &mut h,
            "authorization",
            &format!("{} {access}", p.token_type),
        )?;
    } else {
        h = providers::new_auth_headers(s, p).await?;
    }
    Ok(h)
}

fn rows(data: &Value) -> Option<&Vec<Value>> {
    data.as_array().or_else(|| {
        ["items", "list", "records", "keys", "groups"]
            .iter()
            .find_map(|k| data.get(*k).and_then(Value::as_array))
    })
}
fn multiplier(data: &Value) -> Option<f64> {
    data.as_f64()
        .or_else(|| data.as_str().and_then(|v| v.parse().ok()))
        .or_else(|| {
            providers::number(
                data,
                &[
                    "rate_multiplier",
                    "rateMultiplier",
                    "ratio",
                    "multiplier",
                    "rate",
                ],
            )
        })
        .filter(|v| v.is_finite() && *v >= 0.0)
}
async fn group_info(
    s: &Shared,
    p: &Provider,
    h: &HeaderMap,
) -> HashMap<String, (String, Option<f64>)> {
    let mut groups = HashMap::new();
    if p.kind == "sub2api" {
        if let Ok(response) = providers::request(s, p, "/api/v1/groups/available", h, None).await
            && let Some(items) = rows(&response.data)
        {
            for item in items {
                if let Some(id) = providers::text(item, &["id", "group_id", "groupId"]) {
                    groups.insert(
                        id,
                        (
                            providers::text(item, &["name"]).unwrap_or_default(),
                            multiplier(item),
                        ),
                    );
                }
            }
        }
        if let Ok(response) = providers::request(s, p, "/api/v1/groups/rates", h, None).await {
            if let Some(items) = rows(&response.data) {
                for item in items {
                    if let (Some(id), Some(rate)) = (
                        providers::text(item, &["group_id", "groupId", "id"]),
                        multiplier(item),
                    ) {
                        groups.entry(id).or_insert_with(|| (String::new(), None)).1 = Some(rate);
                    }
                }
            } else if let Some(items) = response.data.as_object() {
                for (id, value) in items {
                    if let Some(rate) = multiplier(value) {
                        groups
                            .entry(id.clone())
                            .or_insert_with(|| (String::new(), None))
                            .1 = Some(rate);
                    }
                }
            }
        }
    } else {
        if let Ok(response) = providers::request(s, p, "/api/user/self/groups", h, None).await
            && let Some(items) = response.data.as_object()
        {
            for (id, value) in items {
                groups.insert(id.clone(), (id.clone(), multiplier(value)));
            }
        }
        if let Ok(response) = providers::request(s, p, "/api/pricing", h, None).await {
            let ratios = response
                .envelope
                .get("group_ratio")
                .or_else(|| response.data.get("group_ratio"));
            if let Some(items) = ratios.and_then(Value::as_object) {
                for (id, value) in items {
                    if let Some(rate) = multiplier(value) {
                        groups.insert(id.clone(), (id.clone(), Some(rate)));
                    }
                }
            }
        }
    }
    groups
}

// Creation may only use the groups currently available to this user. Pricing
// metadata can enrich those groups but must never introduce extra choices.
async fn available_groups(
    s: &Shared,
    p: &Provider,
    h: &HeaderMap,
) -> Result<Vec<ProviderGroup>, RemoteError> {
    let mut groups = Vec::new();
    if p.kind == "sub2api" {
        let response = providers::request(s, p, "/api/v1/groups/available", h, None).await?;
        let items = rows(&response.data).ok_or_else(|| RemoteError::invalid("可用分组"))?;
        for item in items {
            let id = providers::text(item, &["id", "group_id", "groupId"])
                .filter(|id| id.parse::<u64>().is_ok_and(|id| id > 0))
                .ok_or_else(|| RemoteError::invalid("可用分组 ID"))?;
            let name = providers::text(item, &["name"])
                .ok_or_else(|| RemoteError::invalid("可用分组名称"))?;
            groups.push(ProviderGroup {
                id,
                name,
                multiplier: multiplier(item),
            });
        }
        if let Ok(rates) = providers::request(s, p, "/api/v1/groups/rates", h, None).await {
            for group in &mut groups {
                let rate = if let Some(items) = rows(&rates.data) {
                    items
                        .iter()
                        .find(|item| {
                            providers::text(item, &["group_id", "groupId", "id"]).as_deref()
                                == Some(&group.id)
                        })
                        .and_then(multiplier)
                } else {
                    rates.data.get(&group.id).and_then(multiplier)
                };
                if rate.is_some() {
                    group.multiplier = rate;
                }
            }
        }
    } else {
        let response = match providers::request(s, p, "/api/user/self/groups", h, None).await {
            Ok(response) => response,
            Err(_) => providers::request(s, p, "/api/user/groups", h, None).await?,
        };
        let items = response
            .data
            .as_object()
            .ok_or_else(|| RemoteError::invalid("可用分组"))?;
        for (id, value) in items {
            if id.is_empty() {
                return Err(RemoteError::invalid("可用分组名称"));
            }
            groups.push(ProviderGroup {
                id: id.clone(),
                name: id.clone(),
                multiplier: multiplier(value),
            });
        }
        if let Ok(pricing) = providers::request(s, p, "/api/pricing", h, None).await {
            let ratios = pricing
                .envelope
                .get("group_ratio")
                .or_else(|| pricing.data.get("group_ratio"));
            for group in &mut groups {
                if let Some(rate) = ratios
                    .and_then(|ratios| ratios.get(&group.id))
                    .and_then(multiplier)
                {
                    group.multiplier = Some(rate);
                }
            }
        }
    }
    let mut ids = HashSet::new();
    if groups.iter().any(|group| !ids.insert(group.id.clone())) {
        return Err(RemoteError::invalid("可用分组 ID 重复"));
    }
    groups.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(groups)
}

async fn authenticated_groups(
    s: &Shared,
    p: &mut Provider,
) -> Result<(HeaderMap, Vec<ProviderGroup>), RemoteError> {
    let mut h = headers(s, p).await?;
    let groups = match available_groups(s, p, &h).await {
        Err(error)
            if error.unauthorized
                && p.kind == "sub2api"
                && (!p.refresh_token.is_empty() || p.auth_mode == "password") =>
        {
            let access = renew_sub_session(s, p).await?;
            providers::insert_header(
                &mut h,
                "authorization",
                &format!("{} {access}", p.token_type),
            )?;
            available_groups(s, p, &h).await?
        }
        result => result?,
    };
    Ok((h, groups))
}

fn key_metadata(
    p: &Provider,
    item: &Value,
    groups: &HashMap<String, (String, Option<f64>)>,
) -> Result<ProviderKey, RemoteError> {
    let id = providers::text(item, &["id"])
        .filter(|v| valid_id(v))
        .ok_or_else(|| RemoteError::invalid("API 密钥 ID"))?;
    let raw = providers::text(item, &["status"])
        .unwrap_or_default()
        .to_lowercase();
    let mut status = match raw.as_str() {
        "1" | "enabled" | "active" => "active".into(),
        "2" | "inactive" | "disabled" => "disabled".into(),
        "3" | "expired" => "expired".into(),
        "4" | "quota_exhausted" => "quota_exhausted".into(),
        "" => "unknown".into(),
        _ => raw,
    };
    let expires = if p.kind == "newapi" {
        providers::number(item, &["expired_time"])
            .filter(|v| *v != -1.0)
            .map(|v| v as i64)
    } else {
        item.get("expires_at")
            .and_then(Value::as_str)
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|v| v.timestamp())
    };
    if status == "active" && expires.is_some_and(|v| v <= now()) {
        status = "expired".into();
    }
    let exhausted = if p.kind == "sub2api" {
        providers::number(item, &["quota"])
            .filter(|v| *v > 0.0)
            .zip(providers::number(item, &["quota_used"]))
            .is_some_and(|(quota, used)| used >= quota)
    } else {
        item.get("unlimited_quota").and_then(Value::as_bool) == Some(false)
            && providers::number(item, &["remain_quota"]).is_some_and(|v| v <= 0.0)
    };
    if status == "active" && exhausted {
        status = "quota_exhausted".into();
    }
    let group_id = if p.kind == "sub2api" {
        providers::text(item, &["group_id"]).unwrap_or_default()
    } else {
        providers::text(item, &["group"]).unwrap_or_default()
    };
    let group = groups.get(&group_id);
    let group_name = providers::text(&item["group"], &["name"])
        .or_else(|| group.map(|g| g.0.clone()).filter(|v| !v.is_empty()))
        .unwrap_or(group_id);
    Ok(ProviderKey {
        id,
        name: providers::text(item, &["name"]).unwrap_or_default(),
        group_name,
        group_multiplier: group
            .and_then(|g| g.1)
            .or_else(|| multiplier(&item["group"])),
        available: status == "active",
        status,
        secret: providers::text(item, &["key", "token", "api_key"]).unwrap_or_default(),
    })
}

async fn read_keys(
    s: &Shared,
    p: &Provider,
    h: &HeaderMap,
) -> Result<Vec<ProviderKey>, RemoteError> {
    const PAGE_SIZE: usize = 100;
    let mut items = Vec::new();
    let mut ids = HashSet::new();
    for page in 1..=100 {
        let path = if p.kind == "sub2api" {
            format!(
                "/api/v1/keys?page={page}&page_size={PAGE_SIZE}&sort_by=created_at&sort_order=desc"
            )
        } else {
            format!("/api/token/?p={page}&page_size={PAGE_SIZE}")
        };
        let response = providers::request(s, p, &path, h, None).await?;
        let page_items =
            rows(&response.data).ok_or_else(|| RemoteError::invalid("API 密钥列表"))?;
        let total = providers::number(&response.data, &["total", "count"])
            .or_else(|| providers::number(&response.envelope, &["total", "count"]));
        for item in page_items {
            let id = providers::text(item, &["id"])
                .filter(|v| valid_id(v))
                .ok_or_else(|| RemoteError::invalid("API 密钥 ID"))?;
            if !ids.insert(id) {
                return Err(RemoteError::invalid("API 密钥列表分页重复"));
            }
            items.push(item.clone());
        }
        if page_items.is_empty()
            || total.is_some_and(|t| items.len() as f64 >= t)
            || (total.is_none() && page_items.len() < PAGE_SIZE)
        {
            break;
        }
        if page == 100 {
            return Err(RemoteError::invalid("API 密钥列表超过10000条"));
        }
    }
    let groups = group_info(s, p, h).await;
    items
        .iter()
        .map(|item| key_metadata(p, item, &groups))
        .collect()
}

fn persist_auth(s: &Shared, before: &Provider, after: &Provider) -> Result<(), ApiError> {
    s.store.update(|c| {
        let stored = c
            .providers
            .iter_mut()
            .find(|v| v.id == before.id)
            .ok_or("服务商已删除，请重新选择")?;
        if !providers::same_connection(stored, before) {
            return Err("服务商登录信息已更改，请重新操作".into());
        }
        stored.access_token = after.access_token.clone();
        stored.refresh_token = after.refresh_token.clone();
        stored.token_type = after.token_type.clone();
        stored.user_id = after.user_id.clone();
        Ok(())
    })?;
    Ok(())
}
async fn load(
    s: &Shared,
    provider_id: &str,
    selected: Option<&str>,
) -> Result<(Provider, Vec<ProviderKey>), ApiError> {
    let before = providers::lookup(s, provider_id)?;
    providers::require_authenticated(&before)?;
    let mut p = before.clone();
    let result = async {
        let mut h = headers(s, &mut p).await?;
        let mut keys = match read_keys(s, &p, &h).await {
            Err(e)
                if e.unauthorized
                    && p.kind == "sub2api"
                    && (!p.refresh_token.is_empty() || p.auth_mode == "password") =>
            {
                let access = renew_sub_session(s, &mut p).await?;
                providers::insert_header(
                    &mut h,
                    "authorization",
                    &format!("{} {access}", p.token_type),
                )?;
                read_keys(s, &p, &h).await?
            }
            result => result?,
        };
        if let Some(id) = selected
            && let Some(key) = keys.iter_mut().find(|v| v.id == id && v.available)
        {
            if p.kind == "newapi" {
                let response =
                    providers::request(s, &p, &format!("/api/token/{id}/key"), &h, Some(json!({})))
                        .await?;
                key.secret = providers::text(&response.data, &["key", "token", "api_key"])
                    .ok_or_else(|| RemoteError::invalid("完整 API 密钥"))?;
            }
            if key.secret.is_empty()
                || key.secret.len() > 4096
                || key.secret.contains('*')
                || key.secret.chars().any(char::is_whitespace)
            {
                return Err(RemoteError::invalid("完整 API 密钥"));
            }
        }
        Ok(keys)
    }
    .await;
    // A successful token rotation must survive even if the subsequent key request fails.
    persist_auth(s, &before, &p)?;
    Ok((p, result.map_err(remote)?))
}

pub async fn list(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    crate::api::auth(&s, &h)?;
    let (p, keys) = load(&s, &id, None).await?;
    Ok(Json(
        json!({"provider_id":p.id,"endpoint":p.base_url,"keys":keys,"synced_at":now()}),
    ))
}

pub async fn groups(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    crate::api::auth(&s, &h)?;
    let before = providers::lookup(&s, &id)?;
    providers::require_authenticated(&before)?;
    let mut provider = before.clone();
    let result = authenticated_groups(&s, &mut provider).await;
    persist_auth(&s, &before, &provider)?;
    let (_, groups) = result.map_err(remote)?;
    Ok(Json(json!({"provider_id":id,"groups":groups})))
}

pub async fn create(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<CreateKeyInput>,
) -> ApiResult<Value> {
    crate::api::auth(&s, &h)?;
    let before = providers::lookup(&s, &id)?;
    providers::require_authenticated(&before)?;
    if input.group_id.is_empty() || input.group_id.len() > 256 {
        return Err(bad("请选择有效的上游分组"));
    }
    let mut provider = before.clone();
    let result = authenticated_groups(&s, &mut provider).await;
    // Persist any single-use refresh rotation before the mutation. A later
    // local error must not report a successful key creation as retryable.
    persist_auth(&s, &before, &provider)?;
    let (headers, groups) = result.map_err(remote)?;
    let group = groups
        .iter()
        .find(|group| group.id == input.group_id)
        .ok_or_else(|| bad("该分组已不可用，请刷新后重新选择"))?;
    let name = format!("【ZakoAI】{}", group.name);
    let (path, body) = if provider.kind == "sub2api" {
        (
            "/api/v1/keys",
            json!({"name":name,"group_id":group.id.parse::<u64>().map_err(|_| bad("无效的上游分组 ID"))?}),
        )
    } else {
        (
            "/api/token/",
            json!({"name":name,"group":group.id,"remain_quota":0,"unlimited_quota":true,"expired_time":-1,
            "model_limits_enabled":false,"model_limits":"","allow_ips":"","cross_group_retry":false}),
        )
    };
    // Never retry a creation POST, even after an authentication or transport
    // failure: the upstream may already have committed it.
    providers::request(&s, &provider, path, &headers, Some(body))
        .await
        .map_err(|error| {
            ApiError(
                StatusCode::BAD_GATEWAY,
                format!(
                    "{}；请先刷新密钥列表确认结果，再决定是否重试",
                    error.message
                ),
            )
        })?;
    Ok(Json(
        json!({"provider_id":id,"created":true,"name":name,"group_id":group.id}),
    ))
}
pub(crate) async fn resolve(
    s: &Shared,
    provider_id: &str,
    key_id: &str,
) -> Result<ResolvedKey, ApiError> {
    if !valid_id(key_id) {
        return Err(bad("无效的服务商 API 密钥 ID"));
    }
    let (p, keys) = load(s, provider_id, Some(key_id)).await?;
    let key = keys
        .into_iter()
        .find(|v| v.id == key_id)
        .ok_or_else(|| bad("服务商 API 密钥已删除或不存在，请重新选择"))?;
    if !key.available {
        return Err(bad("服务商 API 密钥已禁用、过期或额度耗尽，请重新选择"));
    }
    Ok(ResolvedKey {
        snapshot: p.clone(),
        endpoint: p.base_url,
        secret: key.secret,
        binding: ProviderBinding {
            provider_id: p.id,
            key_id: key.id,
            key_name: key.name,
            group_name: key.group_name,
            group_multiplier: key.group_multiplier,
            status: key.status,
            last_synced_at: now(),
        },
    })
}

pub(crate) async fn connection(
    s: &Shared,
    selection: &Selection,
    account: Option<&Account>,
    endpoint: &str,
    api_key: &str,
) -> Result<(String, String), ApiError> {
    match selection.choose(account)? {
        Choice::Provider {
            provider_id,
            key_id,
        } => {
            if !api_key.is_empty() {
                return Err(bad("服务商绑定模式不能提交手填 API 密钥"));
            }
            let resolved = resolve(s, &provider_id, &key_id).await?;
            Ok((resolved.endpoint, resolved.secret))
        }
        Choice::Api { detached } => {
            if detached && api_key.is_empty() {
                return Err(bad("解除服务商绑定后必须填写新的 API 密钥"));
            }
            let key = if api_key.is_empty() {
                account
                    .map(|a| s.store.unseal(&a.api_key))
                    .transpose()?
                    .unwrap_or_default()
            } else {
                api_key.into()
            };
            let endpoint = if endpoint.is_empty() {
                account.map(|a| a.endpoint.as_str()).unwrap_or_default()
            } else {
                endpoint
            };
            Ok((endpoint.into(), key))
        }
    }
}

pub async fn refresh_binding(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    crate::api::auth(&s, &h)?;
    let mut account = s
        .store
        .config()?
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Account not found".into()))?;
    let Some(mut binding) = account.provider_binding.clone() else {
        return Ok(Json(
            json!({"account_id":id,"endpoint":account.endpoint,"provider_binding":null,"available":false,"synced_at":now()}),
        ));
    };
    let before = binding.clone();
    let expected_secret = account.api_key.clone();
    let mut provider_snapshot = None;
    let mut available = false;
    match providers::lookup(&s, &binding.provider_id) {
        Err(ApiError(StatusCode::NOT_FOUND, _)) => {
            binding.status = "provider_missing".into();
            account.api_key.clear();
        }
        Err(e) => return Err(e),
        Ok(_) if binding.status == "provider_changed" => {
            // IDs are scoped to an upstream identity. Require an explicit new
            // selection instead of resolving the same ID on a different site.
            return Ok(Json(json!({
                "account_id":id,"endpoint":account.endpoint,"provider_binding":binding,
                "available":false,"synced_at":binding.last_synced_at
            })));
        }
        Ok(_) => {
            let (p, keys) = load(&s, &binding.provider_id, Some(&binding.key_id)).await?;
            provider_snapshot = Some(p.clone());
            account.endpoint = p.base_url;
            if let Some(key) = keys.into_iter().find(|k| k.id == binding.key_id) {
                binding.key_name = key.name;
                binding.group_name = key.group_name;
                binding.group_multiplier = key.group_multiplier;
                binding.status = key.status;
                available = key.available;
                account.api_key = if available {
                    s.store.seal(&key.secret)?
                } else {
                    String::new()
                };
            } else {
                binding.status = "missing".into();
                account.api_key.clear();
            }
        }
    }
    binding.last_synced_at = now();
    account.provider_binding = Some(binding.clone());
    s.store.update(|c| {
        let current_provider = c.providers.iter().find(|p| p.id == before.provider_id);
        match (&provider_snapshot, current_provider) {
            (Some(snapshot), Some(current)) if providers::same_connection(snapshot, current) => {}
            (None, None) => {}
            _ => return Err("服务商已删除或登录信息已更改，请重新打开编辑".into()),
        }
        let stored = c
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        if !stored.provider_binding.as_ref().is_some_and(|b| {
            b.provider_id == before.provider_id
                && b.key_id == before.key_id
                && b.status == before.status
                && b.last_synced_at == before.last_synced_at
        }) || stored.api_key != expected_secret
        {
            return Err("账号绑定已更改，请重新打开编辑".into());
        }
        stored.endpoint = account.endpoint.clone();
        stored.api_key = account.api_key.clone();
        stored.provider_binding = account.provider_binding.clone();
        Ok(())
    })?;
    s.proxy_clients.lock().unwrap().remove(&id);
    Ok(Json(
        json!({"account_id":id,"endpoint":account.endpoint,"provider_binding":binding,"available":available,"synced_at":binding.last_synced_at}),
    ))
}
