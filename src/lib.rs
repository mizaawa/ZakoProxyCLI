pub mod api;
pub mod network;
pub mod playground;
pub mod profiles;
pub mod provider_keys;
pub mod providers;
pub mod proxy;
pub mod store;
pub mod update;

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use store::Store;

#[derive(Default, Clone, serde::Serialize)]
pub struct Runtime {
    pub active: u32,
    pub failures: u32,
    pub cooldown_until: i64,
    #[serde(skip)]
    pub window: i64,
    #[serde(skip)]
    pub requests: u32,
}
pub struct App {
    pub store: Store,
    pub turnstile_site_key: std::sync::RwLock<String>,
    pub turnstile_secret_key: std::sync::RwLock<String>,
    pub turnstile_env_file: std::path::PathBuf,
    pub client: reqwest::Client,
    pub proxy_clients: Mutex<HashMap<String, (String, reqwest::Client)>>,
    pub runtime: Mutex<HashMap<String, Runtime>>,
    pub login_attempts: Mutex<HashMap<String, (i64, u32)>>,
    pub(crate) provider_preflights: Mutex<HashMap<String, (i64, providers::Provider)>>,
    pub web_dir: std::path::PathBuf,
}
pub type Shared = Arc<App>;
pub type ApiResult<T> = Result<axum::Json<T>, ApiError>;
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, axum::Json(json!({"error":self.1}))).into_response()
    }
}
impl From<String> for ApiError {
    fn from(value: String) -> Self {
        tracing::error!(error=%value, "Storage or validation error");
        Self(StatusCode::BAD_REQUEST, value)
    }
}
pub fn bad(message: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}
impl App {
    pub fn turnstile_site_key(&self) -> String {
        self.turnstile_site_key.read().unwrap().clone()
    }

    pub fn turnstile_secret_key(&self) -> String {
        self.turnstile_secret_key.read().unwrap().clone()
    }

    pub fn save_turnstile_keys(&self, site_key: &str, secret_key: &str) -> Result<(), String> {
        let existing = match std::fs::read_to_string(&self.turnstile_env_file) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.to_string()),
        };
        fn quoted(value: &str) -> String {
            format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
        }
        let replacements = [
            ("TURNSTILE_SITE_KEY", quoted(site_key)),
            ("TURNSTILE_SECRET_KEY", quoted(secret_key)),
        ];
        let mut lines = Vec::new();
        let mut replaced = [false, false];
        for line in existing.lines() {
            let candidate = line
                .trim_start()
                .strip_prefix("export ")
                .unwrap_or(line.trim_start());
            let key = candidate.split_once('=').map(|(key, _)| key.trim());
            if let Some(index) =
                key.and_then(|key| replacements.iter().position(|(name, _)| *name == key))
            {
                if !replaced[index] {
                    lines.push(format!(
                        "{}={}",
                        replacements[index].0, replacements[index].1
                    ));
                    replaced[index] = true;
                }
            } else {
                lines.push(line.to_owned());
            }
        }
        for (index, (name, value)) in replacements.iter().enumerate() {
            if !replaced[index] {
                lines.push(format!("{name}={value}"));
            }
        }
        let mut contents = lines.join("\n");
        contents.push('\n');
        use std::io::{Seek, SeekFrom, Write};
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&self.turnstile_env_file)
            .map_err(|error| format!("Could not update project .env: {error}"))?;
        let write_result = file
            .set_len(0)
            .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
            .and_then(|()| file.write_all(contents.as_bytes()))
            .and_then(|()| file.sync_all());
        if let Err(error) = write_result {
            let restore_result = file
                .set_len(0)
                .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
                .and_then(|()| file.write_all(existing.as_bytes()))
                .and_then(|()| file.sync_all());
            if let Err(restore_error) = restore_result {
                return Err(format!(
                    "Could not update project .env: {error}; restoring the previous file also failed: {restore_error}"
                ));
            }
            return Err(format!("Could not update project .env: {error}"));
        }
        *self.turnstile_site_key.write().unwrap() = site_key.to_owned();
        *self.turnstile_secret_key.write().unwrap() = secret_key.to_owned();
        Ok(())
    }
}
pub fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "Please sign in".into())
}
pub fn app(state: Shared) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { axum::Json(json!({"status":"ok"})) }),
        )
        .route("/api/bootstrap", get(api::bootstrap))
        .route("/api/setup", post(api::setup))
        .route("/api/login", post(api::login))
        .route("/api/logout", post(api::logout))
        .route("/api/groups", get(api::groups).post(api::save_group))
        .route(
            "/api/groups/{id}",
            put(api::save_group_id).delete(api::delete_group),
        )
        .route("/api/accounts", get(api::accounts).post(api::save_account))
        .route("/api/accounts/proxy-test", post(api::test_proxy))
        .route(
            "/api/accounts/{id}/provider-binding",
            get(provider_keys::refresh_binding),
        )
        .route(
            "/api/accounts/{id}",
            put(api::save_account_id).delete(api::delete_account),
        )
        .route("/api/keys", get(api::keys).post(api::create_key))
        .route(
            "/api/keys/{id}",
            put(api::update_key).delete(api::delete_key),
        )
        .route("/api/keys/{id}/reveal", post(api::reveal_key))
        .route(
            "/api/providers",
            get(providers::list).post(providers::create),
        )
        .route("/api/providers/test", post(providers::test))
        .route(
            "/api/providers/auth-requirements",
            post(providers::auth_requirements_input),
        )
        .route(
            "/api/providers/{id}/auth-requirements",
            get(providers::auth_requirements),
        )
        .route(
            "/api/providers/{id}",
            put(providers::edit).delete(providers::delete),
        )
        .route("/api/providers/{id}/refresh", post(providers::refresh))
        .route(
            "/api/providers/{id}/keys",
            get(provider_keys::list).post(provider_keys::create),
        )
        .route("/api/providers/{id}/groups", get(provider_keys::groups))
        .route("/api/settings", get(api::settings).put(api::save_settings))
        .route("/api/settings/turnstile-test", post(api::test_turnstile))
        .route("/api/password", post(api::password))
        .route("/api/updates", get(update::status).post(update::start))
        .route("/api/updates/latest", get(update::latest))
        .route("/api/stats", get(api::stats))
        .route("/api/logs", get(api::logs).delete(api::delete_logs))
        .route("/api/logs/export", get(api::export_logs))
        .route("/api/profiles", get(api::profiles))
        .route("/api/profile-templates", get(api::profile_templates))
        .route("/api/models/discover", post(api::discover))
        .route("/api/playground/models", get(playground::models))
        .route("/api/playground/attachment", post(playground::attachment))
        .route("/api/playground/search", post(playground::search))
        .route("/api/playground", post(playground::chat))
        .route("/api/playground/chat", post(playground::chat))
        .route("/v1/models", get(proxy::models))
        .route("/v1/responses", post(proxy::responses))
        .route("/v1/responses/compact", post(proxy::compact))
        .route("/v1/chat/completions", post(proxy::chat_completions))
        .route("/v1/messages", post(proxy::messages))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .fallback(console)
        .with_state(state)
}
async fn console(State(state): State<Shared>, headers: HeaderMap, uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") || path.starts_with("/v1/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(cfg) = state.store.config() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let entry = format!("/{}", cfg.settings.security_path);
    let asset = path.starts_with("/assets/") || path == "/favicon.svg";
    if path == "/"
        && path != entry
        && api::valid_access_host(&headers)
        && state.store.authenticated(api::cookie(&headers))
    {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
        let Ok(location) = entry.parse() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        response
            .headers_mut()
            .insert(axum::http::header::LOCATION, location);
        return response;
    }
    if path != entry && !asset {
        return StatusCode::NOT_FOUND.into_response();
    }
    if asset && (path.contains("..") || path.contains('\\')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let file = state
        .web_dir
        .join(if asset { &path[1..] } else { "index.html" });
    match tokio::fs::read(file).await {
        Ok(bytes) => {
            let content = if path.ends_with(".js") {
                "application/javascript"
            } else if path.ends_with(".css") {
                "text/css"
            } else if path.ends_with(".svg") {
                "image/svg+xml"
            } else if path.ends_with(".woff2") {
                "font/woff2"
            } else if path.ends_with(".woff") {
                "font/woff"
            } else if path.ends_with(".txt") {
                "text/plain; charset=utf-8"
            } else {
                "text/html; charset=utf-8"
            };
            let secure = api::is_https(&headers);
            let mut response = Response::new(Body::from(bytes));
            let h = response.headers_mut();
            h.insert("content-type", content.parse().unwrap());
            h.insert("x-content-type-options", "nosniff".parse().unwrap());
            h.insert("x-frame-options", "DENY".parse().unwrap());
            h.insert("referrer-policy", "same-origin".parse().unwrap());
            h.insert(
                "cache-control",
                if asset {
                    "public, max-age=86400"
                } else {
                    "no-store"
                }
                .parse()
                .unwrap(),
            );
            h.insert("content-security-policy","default-src 'self'; script-src 'self' https://challenges.cloudflare.com; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' https://challenges.cloudflare.com; frame-src https://challenges.cloudflare.com; frame-ancestors 'none'; base-uri 'none'".parse().unwrap());
            if secure {
                h.insert(
                    "strict-transport-security",
                    "max-age=31536000".parse().unwrap(),
                );
            }
            response
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Frontend is not built. Run npm ci && npm run build in web/.",
        )
            .into_response(),
    }
}
pub fn new_app(data_dir: &std::path::Path, web_dir: std::path::PathBuf) -> Result<Shared, String> {
    let turnstile_env_file = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(".env");
    let env_values = std::fs::read_to_string(&turnstile_env_file)
        .ok()
        .and_then(|contents| {
            dotenvy::from_read_iter(contents.as_bytes())
                .collect::<std::result::Result<std::collections::HashMap<_, _>, _>>()
                .ok()
        })
        .unwrap_or_default();
    Ok(Arc::new(App {
        store: Store::open(data_dir)?,
        turnstile_site_key: std::sync::RwLock::new(
            env_values
                .get("TURNSTILE_SITE_KEY")
                .cloned()
                .or_else(|| std::env::var("TURNSTILE_SITE_KEY").ok())
                .unwrap_or_default(),
        ),
        turnstile_secret_key: std::sync::RwLock::new(
            env_values
                .get("TURNSTILE_SECRET_KEY")
                .cloned()
                .or_else(|| std::env::var("TURNSTILE_SECRET_KEY").ok())
                .unwrap_or_default(),
        ),
        turnstile_env_file,
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?,
        runtime: Mutex::new(HashMap::new()),
        proxy_clients: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(HashMap::new()),
        provider_preflights: Mutex::new(HashMap::new()),
        web_dir,
    }))
}
