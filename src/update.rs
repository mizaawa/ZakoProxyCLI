//! The console delegates updates to an isolated, authenticated Docker controller.
use crate::{ApiError, ApiResult, Shared, api, bad, store::now};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::Mutex,
};

const REPOSITORY: &str = "https://github.com/mizaawa/ZakoProxyCLI.git";
const RELEASE_API: &str = "https://api.github.com/repos/mizaawa/ZakoProxyCLI/releases/latest";
const IMAGE: &str = "ghcr.io/mizaawa/zakoai-proxy-cli:latest";
const MAX_LOGS: usize = 150;
const RELEASE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const RELEASE_RETRY_DELAY: Duration = Duration::from_secs(60);
static RELEASE_CACHE: Mutex<ReleaseCache> = Mutex::const_new(ReleaseCache {
    attempted_at: None,
    result: None,
    latest: None,
});

#[derive(Default)]
struct ReleaseCache {
    attempted_at: Option<Instant>,
    result: Option<Result<Release, String>>,
    latest: Option<Release>,
}

async fn fetch_release(client: &reqwest::Client, url: &str) -> Result<Release, String> {
    let reply = client
        .get(url)
        .header("User-Agent", "ZakoAI-Updater")
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| "GitHub release lookup failed".to_owned())?;
    if !reply.status().is_success() {
        return Err(format!("GitHub release lookup returned {}", reply.status()));
    }
    let bytes = reply
        .bytes()
        .await
        .map_err(|_| "Invalid GitHub response".to_owned())?;
    if bytes.len() > 512 * 1024 {
        return Err("GitHub release response is too large".into());
    }
    parse_release(
        &serde_json::from_slice(&bytes).map_err(|_| "Invalid GitHub response".to_owned())?,
    )
}

async fn cached_release(
    cache: &Mutex<ReleaseCache>,
    client: &reqwest::Client,
    url: &str,
) -> Result<Release, String> {
    // Serialize lookups so multiple authenticated tabs share one GitHub request.
    let mut cache = cache.lock().await;
    if let (Some(attempted), Some(result)) = (cache.attempted_at, &cache.result) {
        let ttl = if result.is_ok() {
            RELEASE_CACHE_TTL
        } else {
            RELEASE_RETRY_DELAY
        };
        if attempted.elapsed() < ttl {
            return result.clone();
        }
    }
    let result = fetch_release(client, url).await;
    cache.attempted_at = Some(Instant::now());
    if let Ok(release) = &result {
        cache.latest = Some(release.clone());
    }
    cache.result = Some(result.clone());
    result
}

fn merge_release(state: &mut AgentStatus, latest: Option<Release>) {
    if let Some(latest) = latest {
        state.latest = Some(latest);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub url: String,
    pub published_at: String,
    pub notes: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Job {
    pub id: String,
    pub status: String,
    pub version: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub message: String,
    pub logs: Vec<String>,
}
impl Job {
    fn active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "fetching" | "building" | "deploying" | "verifying"
        )
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct AgentStatus {
    pub latest: Option<Release>,
    pub job: Option<Job>,
}
#[derive(Serialize)]
pub struct UpdateStatus {
    pub current_version: &'static str,
    pub configured: bool,
    pub latest: Option<Release>,
    pub update_available: bool,
    pub job: Option<Job>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
fn response(state: AgentStatus, configured: bool, error: Option<String>) -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION");
    let available = option_env!("ZAKO_DEPLOYMENT_FLAVOR") != Some("source-maintenance")
        && state.latest.as_ref().is_some_and(|r| r.version != current);
    UpdateStatus {
        current_version: current,
        configured,
        latest: state.latest,
        update_available: available,
        job: state.job,
        error,
    }
}
fn helper_url() -> Result<Option<reqwest::Url>, ApiError> {
    let Ok(raw) = std::env::var("UPDATER_URL") else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let url = reqwest::Url::parse(&raw).map_err(|_| bad("Invalid updater configuration"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(bad("Invalid updater configuration"));
    }
    Ok(Some(url))
}
async fn helper(s: &Shared, path: &str, body: Option<Value>) -> Result<AgentStatus, ApiError> {
    let url = helper_url()?.ok_or_else(|| bad("In-site updates are not configured"))?;
    let token_file =
        std::env::var("UPDATER_TOKEN_FILE").unwrap_or_else(|_| "/control/token".into());
    let token = tokio::fs::read_to_string(token_file)
        .await
        .map_err(|_| bad("Updater token is not available"))?;
    if !valid_token(token.trim()) {
        return Err(bad("Updater token is invalid"));
    }
    let url = url
        .join(path)
        .map_err(|_| bad("Invalid updater configuration"))?;
    let req = if let Some(body) = body {
        s.client.post(url).json(&body)
    } else {
        s.client.get(url)
    };
    let reply = req
        .bearer_auth(token.trim())
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .map_err(|_| ApiError(StatusCode::BAD_GATEWAY, "Updater is unreachable".into()))?;
    if !reply.status().is_success() {
        let status = match reply.status().as_u16() {
            409 => StatusCode::CONFLICT,
            400 => StatusCode::BAD_REQUEST,
            _ => StatusCode::BAD_GATEWAY,
        };
        let message = reply
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| "Updater request failed".into());
        return Err(ApiError(status, message));
    }
    reply.json().await.map_err(|_| {
        ApiError(
            StatusCode::BAD_GATEWAY,
            "Updater returned an invalid response".into(),
        )
    })
}
pub async fn status(State(s): State<Shared>, h: HeaderMap) -> ApiResult<UpdateStatus> {
    api::auth(&s, &h)?;
    let latest = RELEASE_CACHE.lock().await.latest.clone();
    if helper_url()?.is_none() {
        return Ok(Json(response(
            AgentStatus { latest, job: None },
            false,
            None,
        )));
    }
    match helper(&s, "status", None).await {
        Ok(mut value) => {
            merge_release(&mut value, latest);
            Ok(Json(response(value, true, None)))
        }
        Err(e) => Ok(Json(response(
            AgentStatus { latest, job: None },
            true,
            Some(e.1),
        ))),
    }
}
pub async fn latest(State(s): State<Shared>, h: HeaderMap) -> ApiResult<UpdateStatus> {
    api::auth(&s, &h)?;
    let latest = cached_release(&RELEASE_CACHE, &s.client, RELEASE_API)
        .await
        .map_err(|error| ApiError(StatusCode::BAD_GATEWAY, error))?;
    let mut value = if helper_url()?.is_some() {
        match helper(&s, "latest", None).await {
            Ok(value) => response(value, true, None),
            Err(_error) => status(State(s.clone()), h.clone()).await?.0,
        }
    } else {
        status(State(s.clone()), h.clone()).await?.0
    };
    let mut state = AgentStatus {
        latest: value.latest.take(),
        job: value.job.take(),
    };
    merge_release(&mut state, Some(latest));
    Ok(Json(response(state, value.configured, value.error)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartUpdate {
    tag: String,
    current_password: String,
}
pub async fn start(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(input): Json<StartUpdate>,
) -> ApiResult<UpdateStatus> {
    api::auth(&s, &h)?;
    if parse_tag(&input.tag).is_none() {
        return Err(bad("Invalid release tag"));
    }
    if input.current_password.is_empty() {
        return Err(bad("Current password is required"));
    }
    {
        let mut attempts = s.login_attempts.lock().unwrap();
        let entry = attempts.entry("update".into()).or_insert((now(), 0));
        if now() - entry.0 > 60 {
            *entry = (now(), 0)
        }
        if entry.1 >= 5 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many attempts; wait one minute".into(),
            ));
        }
        entry.1 += 1;
    }
    let hash = s.store.config()?.password_hash;
    let valid = tokio::task::spawn_blocking(move || api::verify(&input.current_password, &hash))
        .await
        .unwrap_or(false);
    if !valid {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Current password is incorrect".into(),
        ));
    }
    s.login_attempts.lock().unwrap().remove("update");
    let state = helper(&s, "start", Some(json!({"tag":input.tag}))).await?;
    Ok(Json(response(state, true, None)))
}

/// Only canonical stable tags are accepted, never arbitrary refs or command options.
pub fn parse_tag(tag: &str) -> Option<(u32, u32, u32)> {
    if tag.len() > 35 {
        return None;
    }
    let mut parts = tag.strip_prefix('v')?.split('.');
    let mut component = || {
        let value = parts.next()?;
        if value.is_empty()
            || !value.bytes().all(|b| b.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return None;
        }
        value.parse::<u32>().ok()
    };
    let version = (component()?, component()?, component()?);
    if parts.next().is_some() {
        return None;
    }
    Some(version)
}
fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|v| v.is_ascii_hexdigit())
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub struct Agent {
    token: String,
    state_dir: PathBuf,
    data_dir: PathBuf,
    project: String,
    service: String,
    client: reqwest::Client,
    docker: reqwest::Client,
    docker_base: String,
    data_volume: String,
    state: Mutex<AgentStatus>,
    // Retained for the process lifetime so only one controller can own this volume.
    _lock: std::fs::File,
    #[cfg(test)]
    command_hook: Option<CommandHook>,
}
#[cfg(test)]
type CommandHook = Arc<dyn Fn(&str, &[&str], Option<&Path>) -> Result<(), String> + Send + Sync>;
impl Agent {
    pub async fn from_env() -> Result<Arc<Self>, String> {
        #[cfg(not(unix))]
        return Err("The isolated updater requires Linux and a Unix Docker socket".into());
        #[cfg(unix)]
        {
            let state_dir = PathBuf::from(
                std::env::var("UPDATER_STATE_DIR").unwrap_or_else(|_| "/updates".into()),
            );
            let data_dir = PathBuf::from(
                std::env::var("UPDATER_DATA_DIR").unwrap_or_else(|_| "/app-data".into()),
            );
            let token_file = PathBuf::from(
                std::env::var("UPDATER_TOKEN_FILE").unwrap_or_else(|_| "/control/token".into()),
            );
            if state_dir != Path::new("/updates")
                || data_dir != Path::new("/app-data")
                || token_file != Path::new("/control/token")
            {
                return Err("Updater must use its fixed /updates, /app-data and /control/token volume paths".into());
            }
            for path in [&state_dir, &data_dir] {
                if !path.is_absolute() {
                    return Err("Updater paths must be absolute".into());
                }
            }
            private_dir(&state_dir).await?;
            tokio::fs::create_dir_all(&data_dir)
                .await
                .map_err(|_| "Cannot create application data directory".to_owned())?;
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(state_dir.join("controller.lock"))
                .map_err(|e| e.to_string())?;
            lock.try_lock()
                .map_err(|_| "Another updater owns this state volume".to_owned())?;
            let token = read_or_create_token(&token_file).await?;
            let state: AgentStatus = match tokio::fs::read(state_dir.join("status.json")).await {
                Ok(bytes) => serde_json::from_slice(&bytes)
                    .map_err(|_| "Updater state is corrupt; refusing to overwrite it".to_owned())?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => AgentStatus::default(),
                Err(e) => return Err(e.to_string()),
            };
            let interrupted = state.job.as_ref().filter(|j| j.active()).cloned();
            let project =
                std::env::var("UPDATER_PROJECT_NAME").unwrap_or_else(|_| "zakoai-proxy".into());
            let service = std::env::var("UPDATER_SERVICE").unwrap_or_else(|_| "proxy".into());
            if !valid_label(&project) || !valid_label(&service) || service != "proxy" {
                return Err("Invalid fixed compose scope".into());
            }
            if std::env::var("DOCKER_HOST").is_ok_and(|h| h != "unix:///var/run/docker.sock") {
                return Err("Only the local Docker Unix socket is supported".into());
            }
            let docker = reqwest::Client::builder()
                .unix_socket("/var/run/docker.sock")
                .timeout(Duration::from_secs(45))
                .build()
                .map_err(|e| e.to_string())?;
            let hostname = std::env::var("HOSTNAME")
                .map_err(|_| "Updater container identity is required".to_owned())?;
            if !(12..=64).contains(&hostname.len())
                || !hostname.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Updater must run in its isolated Docker container".into());
            }
            let own: Value = docker
                .get(format!("http://localhost/containers/{hostname}/json"))
                .send()
                .await
                .map_err(|_| "Cannot inspect updater container".to_owned())?
                .error_for_status()
                .map_err(|_| "Cannot inspect updater container".to_owned())?
                .json()
                .await
                .map_err(|_| "Cannot parse updater container".to_owned())?;
            let data_volume = own["Mounts"]
                .as_array()
                .and_then(|m| {
                    m.iter().find(|m| {
                        m["Destination"].as_str() == data_dir.to_str() && m["Type"] == "volume"
                    })
                })
                .and_then(|m| m["Name"].as_str())
                .ok_or("Updater requires the application named data volume")?
                .to_owned();
            let agent = Arc::new(Self {
                token,
                state_dir,
                data_dir,
                project,
                service,
                client: reqwest::Client::builder()
                    .user_agent("ZakoAI-Updater")
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(20))
                    .build()
                    .map_err(|e| e.to_string())?,
                docker,
                docker_base: "http://localhost".into(),
                data_volume,
                state: Mutex::new(state),
                _lock: lock,
                #[cfg(test)]
                command_hook: None,
            });
            if let Some(job) = interrupted {
                agent.recover_interrupted(&job).await?;
            }
            agent.persist().await?;
            Ok(agent)
        }
    }
    async fn persist_locked(&self, state: &AgentStatus) -> Result<(), String> {
        let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
        let temp = self.state_dir.join("status.json.tmp");
        tokio::fs::write(&temp, bytes)
            .await
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::rename(temp, self.state_dir.join("status.json"))
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(unix)]
    async fn persist(&self) -> Result<(), String> {
        self.persist_locked(&*self.state.lock().await).await
    }
    async fn progress(&self, status: &str, message: &str) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if let Some(job) = &mut state.job {
            job.status = status.into();
            job.message = message.into();
            job.logs.push(format!("{} {message}", now()));
            if job.logs.len() > MAX_LOGS {
                job.logs.drain(..job.logs.len() - MAX_LOGS);
            }
            if !job.active() {
                job.finished_at = Some(now())
            }
        }
        self.persist_locked(&state).await
    }
    async fn log(&self, line: &str) {
        let mut state = self.state.lock().await;
        if let Some(job) = &mut state.job {
            // Build output is bounded; container config, credentials and runtime logs are never emitted.
            let line: String = line
                .chars()
                .filter(|c| !c.is_control() || *c == '\t')
                .take(500)
                .collect();
            job.logs.push(line);
            if job.logs.len() > MAX_LOGS {
                job.logs.drain(..job.logs.len() - MAX_LOGS);
            }
            let _ = self.persist_locked(&state).await;
        }
    }
    async fn fetch_release(&self) -> Result<Release, String> {
        fetch_release(&self.client, RELEASE_API).await
    }
    async fn command(
        &self,
        program: &str,
        args: &[&str],
        directory: Option<&Path>,
        timeout: Duration,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(hook) = &self.command_hook {
            return hook(program, args, directory);
        }
        let mut command = Command::new(program);
        command
            .args(args)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Some(directory) = directory {
            command.current_dir(directory);
        }
        let mut child = command
            .spawn()
            .map_err(|_| format!("Cannot start {program}"))?;
        let out = child.stdout.take().ok_or("Missing command stdout")?;
        let err = child.stderr.take().ok_or("Missing command stderr")?;
        let capture = async {
            let mut out = BufReader::new(out).lines();
            let mut err = BufReader::new(err).lines();
            let (mut out_done, mut err_done) = (false, false);
            while !out_done || !err_done {
                tokio::select! {
                    line = out.next_line(), if !out_done => match line { Ok(Some(line)) => self.log(&line).await, _ => out_done = true },
                    line = err.next_line(), if !err_done => match line { Ok(Some(line)) => self.log(&line).await, _ => err_done = true },
                }
            }
            child
                .wait()
                .await
                .map_err(|_| format!("Cannot wait for {program}"))
        };
        match tokio::time::timeout(timeout, capture).await {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(status)) => Err(format!("{program} failed with exit status {status}")),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(format!("{program} timed out")),
        }
    }
    async fn docker(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut request = self
            .docker
            .request(method, format!("{}{path}", self.docker_base));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let reply = request
            .send()
            .await
            .map_err(|_| "Docker daemon request failed".to_owned())?;
        let status = reply.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Value::Null);
        }
        if !status.is_success() {
            return Err(format!("Docker operation returned {status}"));
        }
        let bytes = reply
            .bytes()
            .await
            .map_err(|_| "Docker response failed".to_owned())?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| "Invalid Docker response".into())
    }
    async fn target(&self) -> Result<Value, String> {
        let filters = serde_json::to_string(&json!({"label":[format!("com.docker.compose.project={}",self.project), format!("com.docker.compose.service={}",self.service)]})).unwrap();
        let query: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("all", "0")
            .append_pair("filters", &filters)
            .finish();
        let containers = self
            .docker(
                reqwest::Method::GET,
                &format!("/containers/json?{query}"),
                None,
            )
            .await?;
        let containers = containers
            .as_array()
            .ok_or("Invalid Docker container list")?;
        if containers.len() != 1 {
            return Err("Exactly one running compose proxy is required".into());
        }
        let id = containers[0]["Id"]
            .as_str()
            .filter(|v| valid_id(v))
            .ok_or("Invalid container identity")?;
        let target = self
            .docker(
                reqwest::Method::GET,
                &format!("/containers/{id}/json"),
                None,
            )
            .await?;
        validate_target(&target, &self.project, &self.service)?;
        if !target["Mounts"].as_array().is_some_and(|mounts| {
            mounts
                .iter()
                .any(|m| m["Destination"] == "/data" && m["Name"] == self.data_volume)
        }) {
            return Err("Updater and proxy data volumes do not match".into());
        }
        Ok(target)
    }
    async fn wait_healthy(&self, id: &str) -> Result<(), String> {
        for _ in 0..60 {
            let state = self
                .docker(
                    reqwest::Method::GET,
                    &format!("/containers/{id}/json"),
                    None,
                )
                .await?;
            let running = state["State"]["Running"].as_bool() == Some(true);
            let health = state["State"]["Health"]["Status"].as_str();
            if running && health == Some("healthy") {
                return Ok(());
            }
            if !running || health == Some("unhealthy") {
                return Err("Proxy failed its Docker health check".into());
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err("Proxy health check timed out".into())
    }
    async fn run(self: Arc<Self>, tag: String, id: String) {
        let outcome = self.build_and_deploy(&tag, &id).await;
        if let Err(error) = outcome {
            let state = self.state.lock().await;
            let terminal = state.job.as_ref().is_some_and(|j| !j.active());
            drop(state);
            if !terminal {
                let _ = self.progress("failed", &error).await;
            }
        }
    }
    #[cfg(any(unix, test))]
    async fn recover_interrupted(&self, job: &Job) -> Result<(), String> {
        if !matches!(job.status.as_str(), "deploying" | "verifying") {
            return self
                .progress(
                    "failed",
                    "The updater was interrupted before deployment; no replacement was attempted",
                )
                .await;
        }
        if job.id.len() != 32 || !job.id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return self.progress("failed", "Interrupted update has an invalid transaction identity; manual inspection is required").await;
        }
        let transaction = self.state_dir.join("transactions").join(&job.id);
        let recovery = async {
            let bytes = tokio::fs::read(transaction.join("container.json"))
                .await
                .map_err(|_| "Interrupted container snapshot is unavailable".to_owned())?;
            let snapshot: Value = serde_json::from_slice(&bytes)
                .map_err(|_| "Interrupted container snapshot is invalid".to_owned())?;
            validate_target(&snapshot, &self.project, &self.service)?;
            let id = snapshot["Id"]
                .as_str()
                .filter(|v| valid_id(v))
                .ok_or("Invalid interrupted container identity")?;
            let actual = self
                .docker(
                    reqwest::Method::GET,
                    &format!("/containers/{id}/json"),
                    None,
                )
                .await?;
            if actual["Config"]["Labels"]["com.docker.compose.project"] != self.project
                || actual["Config"]["Labels"]["com.docker.compose.service"] != self.service
            {
                return Err(
                    "Interrupted original no longer matches the fixed compose scope".into(),
                );
            }
            let backup = transaction.join("data.tar.gz");
            if tokio::fs::try_exists(&backup)
                .await
                .map_err(|_| "Cannot inspect interrupted backup".to_owned())?
            {
                let name = snapshot["Name"]
                    .as_str()
                    .ok_or("Missing interrupted container name")?
                    .trim_start_matches('/');
                self.rollback(&snapshot, name, &backup).await
            } else {
                // The backup is atomically finalized before rename/create, so no data migration could have started.
                self.docker(
                    reqwest::Method::POST,
                    &format!("/containers/{id}/start"),
                    None,
                )
                .await?;
                self.wait_healthy(id).await
            }
        }
        .await;
        match recovery {
            Ok(()) => self.progress("rolled_back", "Recovered an interrupted deployment and restored the original healthy proxy").await,
            Err(error) => self.progress("failed", &format!("Interrupted update recovery failed: {error}. Retained backup requires administrator inspection.")).await,
        }
    }
    async fn build_and_deploy(&self, tag: &str, id: &str) -> Result<(), String> {
        let target = self.target().await?;
        let original_id = target["Id"].as_str().ok_or("Missing proxy identity")?;
        let original_image = target["Image"].as_str().ok_or("Missing proxy image")?;
        let original_name = target["Name"]
            .as_str()
            .ok_or("Missing proxy name")?
            .trim_start_matches('/');
        let directory = self.state_dir.join("sources").join(id);
        private_dir(&directory).await?;
        self.command(
            "git",
            &["init", "--quiet"],
            Some(&directory),
            Duration::from_secs(30),
        )
        .await?;
        self.command(
            "git",
            &[
                "-c",
                "credential.helper=",
                "fetch",
                "--depth=1",
                "--no-tags",
                REPOSITORY,
                &format!("refs/tags/{tag}:refs/tags/{tag}"),
            ],
            Some(&directory),
            Duration::from_secs(180),
        )
        .await?;
        self.command(
            "git",
            &["checkout", "--detach", tag, "--"],
            Some(&directory),
            Duration::from_secs(30),
        )
        .await?;
        let version = tokio::fs::read_to_string(directory.join("Cargo.toml"))
            .await
            .map_err(|_| "Release Cargo.toml is missing".to_owned())?;
        let manifest: toml::Value =
            toml::from_str(&version).map_err(|_| "Invalid release Cargo.toml".to_owned())?;
        if manifest
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(toml::Value::as_str)
            != Some(&tag[1..])
        {
            return Err("Release source version does not match its tag".into());
        }
        let built_image = format!("zakoai-proxy:source-{id}");
        self.progress(
            "building",
            "Building the official release source; the current site remains online",
        )
        .await?;
        self.command(
            "docker",
            &[
                "build",
                "--pull",
                "--target",
                "runtime",
                "--tag",
                &built_image,
                ".",
            ],
            Some(&directory),
            Duration::from_secs(3600),
        )
        .await?;
        let built = self
            .docker(
                reqwest::Method::GET,
                &format!("/images/{built_image}/json"),
                None,
            )
            .await?;
        if built["Config"]["Healthcheck"]["Test"].as_array().is_none() {
            return Err("Source-built image has no Docker health check".into());
        }
        if built["Config"]["Labels"]["org.opencontainers.image.version"].as_str() != Some(&tag[1..])
        {
            return Err("Source-built image version does not match the selected release".into());
        }
        // Reinspect immediately before stopping: a concurrent external deployment invalidates this transaction.
        if self.target().await?["Id"] != target["Id"] {
            return Err(
                "Proxy changed while the build was running; no deployment performed".into(),
            );
        }
        let transaction = self.state_dir.join("transactions").join(id);
        private_dir(&transaction).await?;
        write_private(
            &transaction.join("container.json"),
            &serde_json::to_vec(&target).map_err(|e| e.to_string())?,
        )
        .await?;
        let master = tokio::fs::read(self.data_dir.join("master.key"))
            .await
            .map_err(|_| "Cannot read application encryption key before update".to_owned())?;
        write_private(
            &transaction.join("master-key.sha256"),
            hex::encode(Sha256::digest(master)).as_bytes(),
        )
        .await?;
        let old_tag = format!("zakoai-proxy:rollback-{id}");
        self.command(
            "docker",
            &["image", "tag", original_image, &old_tag],
            None,
            Duration::from_secs(30),
        )
        .await?;
        let backup = transaction.join("data.tar.gz");
        let backup_temp = transaction.join("data.tar.gz.partial");
        write_private(&backup_temp, &[]).await?;
        self.progress(
            "deploying",
            "Source build passed; stopping the proxy for a consistent data backup",
        )
        .await?;
        self.docker(
            reqwest::Method::POST,
            &format!("/containers/{original_id}/stop?t=30"),
            None,
        )
        .await?;
        let backup_string = backup_temp.to_str().ok_or("Invalid backup path")?;
        if let Err(error) = self
            .command(
                "tar",
                &[
                    "-czf",
                    backup_string,
                    "-C",
                    self.data_dir.to_str().ok_or("Invalid data path")?,
                    ".",
                ],
                None,
                Duration::from_secs(300),
            )
            .await
        {
            self.docker(
                reqwest::Method::POST,
                &format!("/containers/{original_id}/start"),
                None,
            )
            .await?;
            return Err(format!("Backup failed; original proxy restarted: {error}"));
        }
        if tokio::fs::rename(&backup_temp, &backup).await.is_err() {
            self.docker(
                reqwest::Method::POST,
                &format!("/containers/{original_id}/start"),
                None,
            )
            .await?;
            return Err("Cannot finalize consistent backup; original proxy restarted".into());
        }
        let rollback_name = format!("{original_name}-rollback-{id}");
        let deploy = self
            .swap(
                &target,
                &built,
                &built_image,
                original_name,
                &rollback_name,
                &transaction,
            )
            .await;
        if let Err(error) = deploy {
            self.log("Deployment failed; restoring the original container and data backup")
                .await;
            match self.rollback(&target, original_name, &backup).await {
                Ok(()) => {
                    self.progress(
                        "rolled_back",
                        &format!("Update failed and the original site was restored: {error}"),
                    )
                    .await?;
                    return Ok(());
                }
                Err(rollback_error) => {
                    self.progress("failed", &format!("Update failed: {error}. Automatic rollback failed: {rollback_error}. Backup is retained; administrator intervention required.")).await?;
                    return Ok(());
                }
            }
        }
        if self
            .command(
                "docker",
                &["image", "tag", &built_image, IMAGE],
                None,
                Duration::from_secs(30),
            )
            .await
            .is_err()
        {
            self.log("Release is healthy; updating the local latest image alias failed")
                .await;
        }
        self.progress(
            "succeeded",
            "The source-built release is healthy; previous image and data backup were retained",
        )
        .await?;
        if self
            .docker(
                reqwest::Method::DELETE,
                &format!("/containers/{original_id}"),
                None,
            )
            .await
            .is_err()
        {
            self.log("Release is healthy; the stopped rollback container was retained")
                .await;
        }
        Ok(())
    }
    async fn swap(
        &self,
        target: &Value,
        built: &Value,
        image: &str,
        name: &str,
        rollback_name: &str,
        transaction: &Path,
    ) -> Result<(), String> {
        let id = target["Id"].as_str().ok_or("Missing proxy identity")?;
        let query: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("name", rollback_name)
            .finish();
        self.docker(
            reqwest::Method::POST,
            &format!("/containers/{id}/rename?{query}"),
            None,
        )
        .await?;
        let mut body = replacement_config(target, image)?;
        for key in [
            "Entrypoint",
            "Cmd",
            "WorkingDir",
            "User",
            "Healthcheck",
            "ExposedPorts",
            "StopSignal",
        ] {
            if let Some(value) = built["Config"].get(key) {
                body[key] = value.clone();
            }
        }
        if let Some(version) =
            built["Config"]["Labels"]["org.opencontainers.image.version"].as_str()
        {
            body["Labels"]["org.opencontainers.image.version"] = json!(version);
        }
        let query: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("name", name)
            .finish();
        let created = self
            .docker(
                reqwest::Method::POST,
                &format!("/containers/create?{query}"),
                Some(body),
            )
            .await?;
        let id = created["Id"]
            .as_str()
            .filter(|v| valid_id(v))
            .ok_or("Invalid replacement container identity")?;
        self.docker(
            reqwest::Method::POST,
            &format!("/containers/{id}/start"),
            None,
        )
        .await?;
        self.progress(
            "verifying",
            "Waiting for the replacement proxy Docker health check",
        )
        .await?;
        self.wait_healthy(id).await?;
        self.verify_master_key(transaction).await
    }
    async fn rollback(&self, target: &Value, name: &str, backup: &Path) -> Result<(), String> {
        let old_id = target["Id"].as_str().ok_or("Missing original identity")?;
        self.docker(
            reqwest::Method::POST,
            &format!("/containers/{old_id}/stop?t=30"),
            None,
        )
        .await?;
        let reply = self
            .docker
            .get(format!("{}/containers/{name}/json", self.docker_base))
            .send()
            .await
            .map_err(|_| "Cannot inspect replacement during rollback".to_owned())?;
        let current = match reply.status() {
            reqwest::StatusCode::NOT_FOUND => None,
            status if status.is_success() => Some(
                reply
                    .json::<Value>()
                    .await
                    .map_err(|_| "Invalid rollback inspection".to_owned())?,
            ),
            _ => return Err("Cannot safely inspect replacement during rollback".into()),
        };
        let original_has_name = current
            .as_ref()
            .is_some_and(|c| c["Id"].as_str() == Some(old_id));
        if let Some(current) = current {
            let id = current["Id"]
                .as_str()
                .filter(|v| valid_id(v))
                .ok_or("Invalid replacement identity")?;
            if id != old_id {
                if current["Config"]["Labels"]["com.docker.compose.project"].as_str()
                    != Some(&self.project)
                    || current["Config"]["Labels"]["com.docker.compose.service"].as_str()
                        != Some(&self.service)
                {
                    return Err(
                        "Refusing to remove a container outside the fixed compose scope".into(),
                    );
                }
                self.docker(
                    reqwest::Method::DELETE,
                    &format!("/containers/{id}?force=true"),
                    None,
                )
                .await?;
            }
        }
        let mut entries = tokio::fs::read_dir(&self.data_dir)
            .await
            .map_err(|_| "Cannot open data volume for restore".to_owned())?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|_| "Cannot list data volume".to_owned())?
        {
            let kind = entry
                .file_type()
                .await
                .map_err(|_| "Cannot inspect data volume entry".to_owned())?;
            if kind.is_dir() {
                tokio::fs::remove_dir_all(entry.path()).await
            } else {
                tokio::fs::remove_file(entry.path()).await
            }
            .map_err(|_| "Cannot clear data volume for restore".to_owned())?;
        }
        self.command(
            "tar",
            &[
                "-xzf",
                backup.to_str().ok_or("Invalid backup path")?,
                "-C",
                self.data_dir.to_str().ok_or("Invalid data path")?,
            ],
            None,
            Duration::from_secs(300),
        )
        .await?;
        let query: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("name", name)
            .finish();
        if !original_has_name {
            self.docker(
                reqwest::Method::POST,
                &format!("/containers/{old_id}/rename?{query}"),
                None,
            )
            .await?;
        }
        self.docker(
            reqwest::Method::POST,
            &format!("/containers/{old_id}/start"),
            None,
        )
        .await?;
        self.wait_healthy(old_id).await?;
        self.verify_master_key(backup.parent().ok_or("Missing rollback transaction path")?)
            .await
    }
    async fn verify_master_key(&self, transaction: &Path) -> Result<(), String> {
        let expected = tokio::fs::read_to_string(transaction.join("master-key.sha256"))
            .await
            .map_err(|_| "Encryption-key verification record is unavailable".to_owned())?;
        let master = tokio::fs::read(self.data_dir.join("master.key"))
            .await
            .map_err(|_| "Application encryption key is missing after update".to_owned())?;
        if hex::encode(Sha256::digest(master)) != expected {
            return Err("Application encryption key changed; refusing to accept the update".into());
        }
        Ok(())
    }
}
#[cfg(any(unix, test))]
fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}
fn valid_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|v| v.is_ascii_hexdigit())
}
fn parse_release(value: &Value) -> Result<Release, String> {
    let tag = value["tag_name"].as_str().ok_or("Release tag is missing")?;
    if parse_tag(tag).is_none()
        || value["draft"].as_bool() != Some(false)
        || value["prerelease"].as_bool() != Some(false)
    {
        return Err("Only canonical stable official releases are supported".into());
    }
    let canonical_url = format!("https://github.com/mizaawa/ZakoProxyCLI/releases/tag/{tag}");
    let legacy_url = format!("https://github.com/mizaawa/zakoai-proxy-cli/releases/tag/{tag}");
    let url = value["html_url"].as_str().unwrap_or_default();
    if url != canonical_url && url != legacy_url {
        return Err("Release URL does not match the official repository".into());
    }
    Ok(Release {
        tag: tag.into(),
        version: tag[1..].into(),
        url: url.into(),
        published_at: value["published_at"].as_str().unwrap_or("").into(),
        notes: value["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(16000)
            .collect(),
    })
}
fn validate_target(target: &Value, project: &str, service: &str) -> Result<(), String> {
    if target["Config"]["Labels"]["com.docker.compose.project"].as_str() != Some(project)
        || target["Config"]["Labels"]["com.docker.compose.service"].as_str() != Some(service)
        || target["State"]["Running"].as_bool() != Some(true)
    {
        return Err("Target is not the running scoped compose proxy".into());
    }
    if target["Config"]["Healthcheck"]["Test"].as_array().is_none() {
        return Err("A Docker health check is required for safe updates".into());
    }
    if !target["Mounts"].as_array().is_some_and(|mounts| {
        mounts
            .iter()
            .any(|m| m["Destination"] == "/data" && m["Type"] == "volume")
    }) {
        return Err("Proxy must use the shared named data volume".into());
    }
    Ok(())
}
fn replacement_config(target: &Value, image: &str) -> Result<Value, String> {
    let mut config = target["Config"]
        .as_object()
        .cloned()
        .ok_or("Proxy config is missing")?;
    config.insert("Image".into(), json!(image));
    // Hostname identifies a container instance and must not be copied to its replacement.
    config.remove("Hostname");
    config.insert("HostConfig".into(), target["HostConfig"].clone());
    let mut endpoints = serde_json::Map::new();
    if let Some(networks) = target["NetworkSettings"]["Networks"].as_object() {
        for (name, network) in networks {
            endpoints.insert(name.clone(), json!({"Aliases":network["Aliases"], "DriverOpts":network["DriverOpts"], "IPAMConfig":network["IPAMConfig"]}));
        }
    }
    config.insert(
        "NetworkingConfig".into(),
        json!({"EndpointsConfig":endpoints}),
    );
    Ok(Value::Object(config))
}
async fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    use tokio::io::AsyncWriteExt;
    let mut file = options
        .open(path)
        .await
        .map_err(|_| "Cannot create private transaction record".to_owned())?;
    file.write_all(bytes)
        .await
        .map_err(|_| "Cannot write transaction record".to_owned())?;
    file.sync_all()
        .await
        .map_err(|_| "Cannot sync transaction record".to_owned())
}
async fn private_dir(path: &Path) -> Result<(), String> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|_| "Cannot create private updater directory".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|_| "Cannot secure updater directory".to_owned())?;
    }
    Ok(())
}
#[cfg(unix)]
async fn read_or_create_token(path: &Path) -> Result<String, String> {
    match tokio::fs::read_to_string(path).await {
        Ok(token) if valid_token(token.trim()) => Ok(token.trim().into()),
        Ok(_) => Err("Existing updater token is invalid".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or("Invalid token path")?;
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| "Cannot create control directory".to_owned())?;
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            let token = hex::encode(bytes);
            write_private(path, token.as_bytes()).await?;
            // The main non-root app only has this volume mounted read-only.
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
                .await
                .map_err(|_| "Cannot set control token permissions".to_owned())?;
            Ok(token)
        }
        Err(_) => Err("Cannot read updater token".into()),
    }
}
fn agent_auth(agent: &Agent, headers: &HeaderMap) -> Result<(), ApiError> {
    let supplied = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    if !constant_eq(supplied.as_bytes(), agent.token.as_bytes()) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid controller authentication".into(),
        ));
    }
    Ok(())
}
pub fn agent_app(agent: Arc<Agent>) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"status":"ok", "version":env!("CARGO_PKG_VERSION")})) }),
        )
        .route("/status", get(agent_status))
        .route("/latest", get(agent_latest))
        .route("/start", axum::routing::post(agent_start))
        .layer(axum::extract::DefaultBodyLimit::max(2048))
        .with_state(agent)
}
async fn agent_status(
    State(agent): State<Arc<Agent>>,
    headers: HeaderMap,
) -> ApiResult<AgentStatus> {
    agent_auth(&agent, &headers)?;
    Ok(Json(agent.state.lock().await.clone()))
}
async fn agent_latest(
    State(agent): State<Arc<Agent>>,
    headers: HeaderMap,
) -> ApiResult<AgentStatus> {
    agent_auth(&agent, &headers)?;
    let release = agent
        .fetch_release()
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    let mut state = agent.state.lock().await;
    state.latest = Some(release);
    agent.persist_locked(&state).await?;
    Ok(Json(state.clone()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentStart {
    tag: String,
}
async fn agent_start(
    State(agent): State<Arc<Agent>>,
    headers: HeaderMap,
    Json(input): Json<AgentStart>,
) -> ApiResult<AgentStatus> {
    agent_auth(&agent, &headers)?;
    parse_tag(&input.tag).ok_or_else(|| bad("Invalid release tag"))?;
    if option_env!("ZAKO_DEPLOYMENT_FLAVOR") == Some("source-maintenance") {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "此镜像从项目源码部署，请在原项目目录运行部署脚本更新两个服务".into(),
        ));
    }
    {
        let state = agent.state.lock().await;
        if state.job.as_ref().is_some_and(Job::active) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "An update is already running".into(),
            ));
        }
    }
    let release = agent
        .fetch_release()
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    if release.tag != input.tag {
        return Err(bad(
            "The selected release is no longer GitHub's latest stable version; check again",
        ));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let mut state = agent.state.lock().await;
    if state.job.as_ref().is_some_and(Job::active) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "An update is already running".into(),
        ));
    }
    state.latest = Some(release);
    state.job = Some(Job {
        id: id.clone(),
        status: "fetching".into(),
        version: input.tag[1..].into(),
        started_at: now(),
        finished_at: None,
        message: "Fetching the latest stable official release source".into(),
        logs: Vec::new(),
    });
    agent.persist_locked(&state).await?;
    let reply = state.clone();
    drop(state);
    tokio::spawn(agent.run(input.tag, id));
    Ok(Json(reply))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, response::IntoResponse};
    use tower::ServiceExt;

    #[tokio::test]
    async fn release_checks_share_cache_and_preserve_last_success_on_failure() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let requests = Arc::new(AtomicUsize::new(0));
        let calls = requests.clone();
        let router = Router::new().route(
            "/latest",
            get(move || {
                let calls = calls.clone();
                async move {
                    let count = calls.fetch_add(1, Ordering::Relaxed);
                    if count > 0 {
                        return StatusCode::SERVICE_UNAVAILABLE.into_response();
                    }
                    Json(json!({
                        "tag_name": "v999.0.0", "draft": false, "prerelease": false,
                        "html_url": "https://github.com/mizaawa/zakoai-proxy-cli/releases/tag/v999.0.0"
                    })).into_response()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/latest", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let cache = Mutex::new(ReleaseCache::default());
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let (first, second) = tokio::join!(
            cached_release(&cache, &client, &url),
            cached_release(&cache, &client, &url)
        );
        assert_eq!(first.unwrap().tag, "v999.0.0");
        assert_eq!(second.unwrap().tag, "v999.0.0");
        assert_eq!(requests.load(Ordering::Relaxed), 1);
        cache.lock().await.attempted_at = Some(Instant::now() - RELEASE_CACHE_TTL);
        assert!(cached_release(&cache, &client, &url).await.is_err());
        assert!(cached_release(&cache, &client, &url).await.is_err());
        assert_eq!(requests.load(Ordering::Relaxed), 2);
        let status = response(
            AgentStatus {
                latest: cache.lock().await.latest.clone(),
                job: None,
            },
            false,
            None,
        );
        assert!(!status.configured);
        assert_eq!(
            status.update_available,
            option_env!("ZAKO_DEPLOYMENT_FLAVOR") != Some("source-maintenance")
        );
        assert_eq!(status.latest.unwrap().tag, "v999.0.0");
        task.abort();
    }

    const OLD_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const NEW_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    struct DockerFixture {
        target: Value,
        stopped: bool,
        renamed: bool,
        new_exists: bool,
        fail_health: bool,
        calls: Vec<String>,
        data_dir: PathBuf,
        create_body: Option<Value>,
    }
    async fn docker_fixture(
        State(state): State<Arc<Mutex<DockerFixture>>>,
        request: Request<Body>,
    ) -> axum::response::Response {
        let method = request.method().to_string();
        let path = request.uri().path().to_owned();
        let body = axum::body::to_bytes(request.into_body(), 65536)
            .await
            .unwrap();
        let mut s = state.lock().await;
        s.calls.push(format!("{method} {path}"));
        let value = if path == "/containers/json" {
            json!([{"Id":OLD_ID}])
        } else if path.starts_with("/images/") {
            json!({"Config":{"Labels":{"org.opencontainers.image.version":"0.3.0"},"Entrypoint":["/app/new-proxy"],"Healthcheck":{"Test":["CMD","curl"]}}})
        } else if path == format!("/containers/{OLD_ID}/json") {
            let mut target = s.target.clone();
            target["State"]["Running"] = json!(!s.stopped);
            target
        } else if path == "/containers/proxy/json" {
            if s.new_exists {
                json!({"Id":NEW_ID,"Config":{"Labels":{"com.docker.compose.project":"zakoai-proxy","com.docker.compose.service":"proxy"}}})
            } else if !s.renamed {
                json!({"Id":OLD_ID})
            } else {
                return StatusCode::NOT_FOUND.into_response();
            }
        } else if path == format!("/containers/{NEW_ID}/json") {
            json!({"State":{"Running":true,"Health":{"Status":if s.fail_health {"unhealthy"} else {"healthy"}}}})
        } else if path == format!("/containers/{OLD_ID}/stop") {
            s.stopped = true;
            Value::Null
        } else if path == format!("/containers/{OLD_ID}/start") {
            s.stopped = false;
            Value::Null
        } else if path == format!("/containers/{OLD_ID}/rename") {
            s.renamed = !s.renamed;
            Value::Null
        } else if path == "/containers/create" {
            s.create_body = Some(serde_json::from_slice(&body).unwrap());
            s.new_exists = true;
            json!({"Id":NEW_ID})
        } else if path == format!("/containers/{NEW_ID}/start") {
            if s.fail_health {
                std::fs::write(s.data_dir.join("database"), "migration-mutated-data").unwrap();
            }
            Value::Null
        } else if path == format!("/containers/{NEW_ID}") && method == "DELETE" {
            s.new_exists = false;
            Value::Null
        } else if path == format!("/containers/{OLD_ID}") && method == "DELETE" {
            Value::Null
        } else {
            return (StatusCode::NOT_FOUND, format!("unexpected {method} {path}")).into_response();
        };
        Json(value).into_response()
    }
    async fn fixture(
        fail_build: bool,
        fail_health: bool,
    ) -> (
        tempfile::TempDir,
        Arc<Agent>,
        Arc<Mutex<DockerFixture>>,
        tokio::task::JoinHandle<()>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let data_dir = directory.path().join("data");
        std::fs::create_dir(&data_dir).unwrap();
        std::fs::write(data_dir.join("database"), "pristine-data").unwrap();
        std::fs::write(data_dir.join("master.key"), "persistent-key").unwrap();
        let state_dir = directory.path().join("state");
        std::fs::create_dir(&state_dir).unwrap();
        let target = json!({"Id":OLD_ID,"Name":"/proxy","Image":"sha256:old-image","Config":{"Image":"old","Env":["DATA_DIR=/data","PRIVATE=must-not-be-logged"],"Labels":{"com.docker.compose.project":"zakoai-proxy","com.docker.compose.service":"proxy"},"Healthcheck":{"Test":["CMD","curl"]}},"HostConfig":{"Binds":["app-data:/data:rw"],"PortBindings":{"1990/tcp":[{"HostPort":"1990"}]},"ReadonlyRootfs":true},"Mounts":[{"Type":"volume","Destination":"/data","Name":"app-data"}],"NetworkSettings":{"Networks":{"default":{"Aliases":["proxy"]}}},"State":{"Running":true,"Health":{"Status":"healthy"}}});
        let docker = Arc::new(Mutex::new(DockerFixture {
            target,
            stopped: false,
            renamed: false,
            new_exists: false,
            fail_health,
            calls: Vec::new(),
            data_dir: data_dir.clone(),
            create_body: None,
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let docker_base = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new()
            .fallback(docker_fixture)
            .with_state(docker.clone());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let data_copy = data_dir.clone();
        let hook: CommandHook = Arc::new(move |program, args, directory| {
            if program == "git" && args.first() == Some(&"checkout") {
                std::fs::write(
                    directory.unwrap().join("Cargo.toml"),
                    "[package]\nname=\"zakoai-proxy\"\nversion = \"0.3.0\"\n",
                )
                .unwrap();
            }
            if program == "docker" && args.first() == Some(&"build") && fail_build {
                return Err("fixture build failure".into());
            }
            if program == "tar" && args.first() == Some(&"-czf") {
                std::fs::copy(data_copy.join("database"), args[1]).unwrap();
            }
            if program == "tar" && args.first() == Some(&"-xzf") {
                std::fs::copy(args[1], data_copy.join("database")).unwrap();
                std::fs::write(data_copy.join("master.key"), "persistent-key").unwrap();
            }
            Ok(())
        });
        let agent = Arc::new(Agent {
            token: "a".repeat(64),
            state_dir: state_dir.clone(),
            data_dir,
            project: "zakoai-proxy".into(),
            service: "proxy".into(),
            client: reqwest::Client::new(),
            docker: reqwest::Client::new(),
            docker_base,
            data_volume: "app-data".into(),
            state: Mutex::new(AgentStatus {
                latest: None,
                job: Some(Job {
                    id: "testjob".into(),
                    status: "fetching".into(),
                    version: "0.3.0".into(),
                    started_at: now(),
                    finished_at: None,
                    message: String::new(),
                    logs: Vec::new(),
                }),
            }),
            _lock: std::fs::File::create(state_dir.join("lock")).unwrap(),
            command_hook: Some(hook),
        });
        (directory, agent, docker, task)
    }
    #[tokio::test]
    async fn failed_build_never_stops_original_proxy() {
        let (_dir, agent, docker, task) = fixture(true, false).await;
        agent.clone().run("v0.3.0".into(), "testjob".into()).await;
        assert_eq!(
            agent.state.lock().await.job.as_ref().unwrap().status,
            "failed"
        );
        let docker = docker.lock().await;
        assert!(!docker.stopped);
        assert!(!docker.calls.iter().any(|v| v.ends_with("/stop")));
        assert_eq!(
            std::fs::read_to_string(agent.data_dir.join("database")).unwrap(),
            "pristine-data"
        );
        task.abort();
    }
    #[tokio::test]
    async fn successful_swap_preserves_mounts_ports_and_backup() {
        let (dir, agent, docker, task) = fixture(false, false).await;
        agent.clone().run("v0.3.0".into(), "testjob".into()).await;
        let state = agent.state.lock().await;
        assert_eq!(state.job.as_ref().unwrap().status, "succeeded");
        assert!(state.job.as_ref().unwrap().finished_at.is_some());
        assert!(
            !serde_json::to_string(&*state)
                .unwrap()
                .contains("must-not-be-logged")
        );
        let docker = docker.lock().await;
        assert_eq!(
            docker.create_body.as_ref().unwrap()["HostConfig"],
            docker.target["HostConfig"]
        );
        assert_eq!(
            docker.create_body.as_ref().unwrap()["Labels"]["org.opencontainers.image.version"],
            "0.3.0"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("state/transactions/testjob/data.tar.gz"))
                .unwrap(),
            "pristine-data"
        );
        assert!(
            dir.path()
                .join("state/transactions/testjob/container.json")
                .exists()
        );
        task.abort();
    }
    #[tokio::test]
    async fn failed_replacement_restores_original_data_and_container() {
        let (_dir, agent, docker, task) = fixture(false, true).await;
        agent.clone().run("v0.3.0".into(), "testjob".into()).await;
        assert_eq!(
            agent.state.lock().await.job.as_ref().unwrap().status,
            "rolled_back"
        );
        let docker = docker.lock().await;
        assert!(!docker.stopped);
        assert!(!docker.renamed);
        assert!(!docker.new_exists);
        assert_eq!(
            std::fs::read_to_string(agent.data_dir.join("database")).unwrap(),
            "pristine-data"
        );
        task.abort();
    }
    #[tokio::test]
    async fn interrupted_deployment_is_recovered_once_from_retained_snapshot() {
        let (_dir, agent, docker, task) = fixture(false, false).await;
        let id = "c".repeat(32);
        let transaction = agent.state_dir.join("transactions").join(&id);
        std::fs::create_dir_all(&transaction).unwrap();
        {
            let mut docker = docker.lock().await;
            std::fs::write(
                transaction.join("container.json"),
                serde_json::to_vec(&docker.target).unwrap(),
            )
            .unwrap();
            std::fs::write(transaction.join("data.tar.gz"), "pristine-data").unwrap();
            std::fs::write(
                transaction.join("master-key.sha256"),
                hex::encode(Sha256::digest(b"persistent-key")),
            )
            .unwrap();
            std::fs::write(agent.data_dir.join("database"), "interrupted-migration").unwrap();
            docker.stopped = true;
            docker.renamed = true;
            docker.new_exists = true;
        }
        let mut job = agent.state.lock().await.job.as_ref().unwrap().clone();
        job.id = id;
        job.status = "verifying".into();
        agent.state.lock().await.job = Some(job.clone());
        agent.recover_interrupted(&job).await.unwrap();
        assert_eq!(
            agent.state.lock().await.job.as_ref().unwrap().status,
            "rolled_back"
        );
        let docker = docker.lock().await;
        assert!(!docker.stopped);
        assert!(!docker.renamed);
        assert!(!docker.new_exists);
        assert_eq!(
            std::fs::read_to_string(agent.data_dir.join("database")).unwrap(),
            "pristine-data"
        );
        task.abort();
    }
    #[tokio::test]
    async fn interrupted_source_build_is_marked_failed_without_touching_docker() {
        let (_dir, agent, docker, task) = fixture(false, false).await;
        let job = agent.state.lock().await.job.as_ref().unwrap().clone();
        agent.recover_interrupted(&job).await.unwrap();
        assert_eq!(
            agent.state.lock().await.job.as_ref().unwrap().status,
            "failed"
        );
        assert!(docker.lock().await.calls.is_empty());
        task.abort();
    }
    #[tokio::test]
    async fn controller_auth_tag_and_single_flight_are_enforced() {
        let (_dir, agent, _docker, task) = fixture(false, false).await;
        for path in ["/status", "/latest"] {
            let response = agent_app(agent.clone())
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        for (body, expected) in [
            (json!({"tag":"v0.3.0;sh"}), StatusCode::BAD_REQUEST),
            (
                json!({"tag":"v0.3.0","url":"https://evil.invalid"}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({"tag":format!("v{}", env!("CARGO_PKG_VERSION"))}),
                StatusCode::CONFLICT,
            ),
        ] {
            let response = agent_app(agent.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/start")
                        .header("authorization", format!("Bearer {}", "a".repeat(64)))
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        task.abort();
    }
    #[tokio::test]
    async fn console_update_endpoints_require_session_and_password() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::new_app(dir.path(), dir.path().join("web")).unwrap();
        let app = crate::app(state);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/updates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/updates/latest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/setup")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"username":"admin","password":"sufficient-password-123"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/updates")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/updates")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"tag":"v0.3.0","current_password":"incorrect"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let long_password = "x".repeat(2048);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/updates")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"tag":"v0.3.0","current_password":long_password}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/updates")
                    .header("cookie", &cookie)
                    .header("host", "127.0.0.1")
                    .header("origin", "https://evil.invalid")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"tag":"v0.3.0","current_password":"sufficient-password-123"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    #[test]
    fn accepts_only_stable_canonical_tags() {
        assert_eq!(parse_tag("v1.20.300"), Some((1, 20, 300)));
        for invalid in [
            "1.2.3",
            "v01.2.3",
            "v1.2",
            "v1.2.3-beta",
            "v1.2.3;sh",
            "--help",
            "v1.2.3/../../",
            "v1.2.3\n",
            "v1.2.4294967296",
            "v１.2.3",
        ] {
            assert_eq!(parse_tag(invalid), None, "{invalid}");
        }
    }
    #[test]
    fn rejects_nonofficial_and_prerelease_metadata() {
        let mut release = json!({"tag_name":"v1.2.3","draft":false,"prerelease":false,"html_url":"https://github.com/mizaawa/zakoai-proxy-cli/releases/tag/v1.2.3","body":"notes"});
        for url in [
            "https://github.com/mizaawa/zakoai-proxy-cli/releases/tag/v1.2.3",
            "https://github.com/mizaawa/ZakoProxyCLI/releases/tag/v1.2.3",
        ] {
            release["html_url"] = json!(url);
            let parsed = parse_release(&release).unwrap();
            assert_eq!(parsed.version, "1.2.3");
            assert_eq!(parsed.url, url);
        }
        release["draft"] = json!(true);
        assert!(parse_release(&release).is_err());
        release["draft"] = json!(false);
        release["prerelease"] = json!(true);
        assert!(parse_release(&release).is_err());
        release["prerelease"] = json!(false);
        for url in [
            "https://evil.invalid/",
            "https://github.com/other/ZakoProxyCLI/releases/tag/v1.2.3",
            "https://github.com/mizaawa/ZakoProxyCLI/releases/tag/v1.2.4",
            "https://github.com/mizaawa/ZakoProxyCLI/releases/tag/v1.2.3?download=1",
            "https://github.com/mizaawa/ZakoProxyCLI/releases/tag/v1.2.3#fragment",
            "https://github.com.evil.invalid/mizaawa/ZakoProxyCLI/releases/tag/v1.2.3",
        ] {
            release["html_url"] = json!(url);
            assert!(parse_release(&release).is_err(), "{url}");
        }
    }
    #[test]
    fn replacement_preserves_runtime_contract() {
        let target = json!({"Config":{"Image":"old","Hostname":"old-id","Env":["DATA_DIR=/data"],"Entrypoint":["/app/zakoai-proxy"],"Labels":{"scope":"proxy"},"Healthcheck":{"Test":["CMD","curl"]}},"HostConfig":{"PortBindings":{"1990/tcp":[{"HostPort":"1990"}]},"Binds":["data:/data:rw","control:/control:ro"],"ReadonlyRootfs":true,"CapDrop":["ALL"]},"NetworkSettings":{"Networks":{"default":{"Aliases":["proxy"],"IPAddress":"172.18.0.2"}}}});
        let config = replacement_config(&target, "new").unwrap();
        assert_eq!(config["Image"], "new");
        assert_eq!(config["Env"], target["Config"]["Env"]);
        assert_eq!(config["HostConfig"], target["HostConfig"]);
        assert!(config.get("Hostname").is_none());
        assert!(
            config["NetworkingConfig"]["EndpointsConfig"]["default"]
                .get("IPAddress")
                .is_none()
        );
    }
    #[test]
    fn token_comparison_and_active_states() {
        assert!(constant_eq(b"abc", b"abc"));
        assert!(!constant_eq(b"abc", b"abx"));
        assert!(!constant_eq(b"abc", b"ab"));
        assert!(valid_token(&"a".repeat(64)));
        assert!(!valid_token(&"g".repeat(64)));
        assert!(!valid_label("proxy;sh"));
        assert!(!valid_label("../proxy"));
    }

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "requires root, a local Docker daemon and UPDATER_TEST_IMAGE; uses an isolated project and random loopback port"]
    async fn real_docker_replacement_and_rollback_preserve_login_and_data() {
        let image = std::env::var("UPDATER_TEST_IMAGE")
            .expect("Set UPDATER_TEST_IMAGE to the locally built runtime image");
        let directory = tempfile::tempdir().unwrap();
        let state_dir = directory.path().join("updates");
        private_dir(&state_dir).await.unwrap();
        let unique = uuid::Uuid::new_v4().simple().to_string();
        let project = format!("zako-update-test-{unique}");
        let name = format!("{project}-proxy");
        let volume = format!("{project}-data");
        let network = format!("{project}-network");
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let host_port = reservation.local_addr().unwrap().port().to_string();
        drop(reservation);
        let docker = reqwest::Client::builder()
            .unix_socket("/var/run/docker.sock")
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap();
        let agent = Arc::new(Agent {
            token: "a".repeat(64),
            state_dir: state_dir.clone(),
            data_dir: PathBuf::new(),
            project: project.clone(),
            service: "proxy".into(),
            client: reqwest::Client::new(),
            docker,
            docker_base: "http://localhost".into(),
            data_volume: volume.clone(),
            state: Mutex::new(AgentStatus {
                latest: None,
                job: Some(Job {
                    id: unique.clone(),
                    status: "deploying".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    started_at: now(),
                    finished_at: None,
                    message: String::new(),
                    logs: Vec::new(),
                }),
            }),
            _lock: std::fs::File::create(state_dir.join("lock")).unwrap(),
            command_hook: None,
        });
        let image_config = agent
            .docker(reqwest::Method::GET, &format!("/images/{image}/json"), None)
            .await
            .expect("Runtime image must be built before running this test");
        let created_volume = agent
            .docker(
                reqwest::Method::POST,
                "/volumes/create",
                Some(json!({"Name":volume})),
            )
            .await
            .unwrap();
        agent
            .docker(
                reqwest::Method::POST,
                "/networks/create",
                Some(json!({"Name":network})),
            )
            .await
            .unwrap();
        let data_dir = PathBuf::from(created_volume["Mountpoint"].as_str().unwrap());
        let mut agent = Arc::try_unwrap(agent).ok().unwrap();
        agent.data_dir = data_dir.clone();
        let agent = Arc::new(agent);
        let outcome: Result<(),String> = async {
            let mut config = image_config["Config"].clone();
            config["Image"] = json!(image);
            config["Labels"]["com.docker.compose.project"] = json!(project);
            config["Labels"]["com.docker.compose.service"] = json!("proxy");
            config["HostConfig"] = json!({"Binds":[format!("{volume}:/data")],"PortBindings":{"1990/tcp":[{"HostIp":"127.0.0.1","HostPort":host_port}]},"ReadonlyRootfs":true,"Tmpfs":{"/tmp":"size=16m,mode=1777"},"CapDrop":["ALL"],"SecurityOpt":["no-new-privileges:true"]});
            config["NetworkingConfig"] = json!({"EndpointsConfig":{(network.clone()):{"Aliases":["proxy"]}}});
            let created = agent.docker(reqwest::Method::POST,&format!("/containers/create?name={name}"),Some(config)).await?;
            let old_id = created["Id"].as_str().ok_or("Missing fixture container id")?;
            agent.docker(reqwest::Method::POST,&format!("/containers/{old_id}/start"),None).await?;
            agent.wait_healthy(old_id).await?;
            let old = agent.target().await?;
            let port = old["NetworkSettings"]["Ports"]["1990/tcp"][0]["HostPort"].as_str().ok_or("Missing fixture published port")?;
            let base = format!("http://127.0.0.1:{port}");
            let password = format!("fixture-{unique}");
            let setup = agent.client.post(format!("{base}/api/setup")).json(&json!({"username":"update-fixture","password":password})).send().await.map_err(|e|e.to_string())?;
            if !setup.status().is_success() { return Err("Fixture administrator setup failed".into()) }
            println!("BASELINE: isolated proxy healthy; administrator initialized");
            tokio::fs::write(data_dir.join("update-sentinel"),"pristine-data").await.map_err(|e|format!("This test must run as root to inspect its private data volume: {e}"))?;
            let transaction = state_dir.join("transactions").join(&unique); private_dir(&transaction).await?;
            write_private(&transaction.join("container.json"),&serde_json::to_vec(&old).unwrap()).await?;
            let key = tokio::fs::read(data_dir.join("master.key")).await.map_err(|e|e.to_string())?;
            write_private(&transaction.join("master-key.sha256"),hex::encode(Sha256::digest(&key)).as_bytes()).await?;
            let backup = transaction.join("data.tar.gz"); write_private(&backup,&[]).await?;
            agent.docker(reqwest::Method::POST,&format!("/containers/{old_id}/stop?t=30"),None).await?;
            agent.command("tar",&["-czf",backup.to_str().unwrap(),"-C",data_dir.to_str().unwrap(),"."],None,Duration::from_secs(30)).await?;
            agent.swap(&old,&image_config,&image,&name,&format!("{name}-rollback"),&transaction).await?;
            let new = agent.target().await?;
            if new["Id"] == old["Id"] { return Err("Replacement did not create a distinct container".into()) }
            if new["HostConfig"]["PortBindings"] != old["HostConfig"]["PortBindings"] || new["HostConfig"]["Binds"] != old["HostConfig"]["Binds"] { return Err("Replacement changed ports or mounts".into()) }
            let login = agent.client.post(format!("{base}/api/login")).json(&json!({"username":"update-fixture","password":password})).send().await.map_err(|e|e.to_string())?;
            if !login.status().is_success() { return Err("Administrator login failed after real Docker replacement".into()) }
            println!("MODIFIED: new container healthy; ports, mounts, encryption key and administrator login preserved");
            tokio::fs::write(data_dir.join("update-sentinel"),"modified-data").await.map_err(|e|e.to_string())?;
            agent.rollback(&old,&name,&backup).await?;
            let restored = agent.target().await?;
            if restored["Id"] != old["Id"] { return Err("Rollback did not restore the original container".into()) }
            if tokio::fs::read_to_string(data_dir.join("update-sentinel")).await.map_err(|e|e.to_string())? != "pristine-data" { return Err("Rollback did not restore the stopped-volume backup".into()) }
            let login = agent.client.post(format!("{base}/api/login")).json(&json!({"username":"update-fixture","password":password})).send().await.map_err(|e|e.to_string())?;
            if !login.status().is_success() { return Err("Administrator login failed after real Docker rollback".into()) }
            println!("ROLLBACK: original container healthy; pristine volume, encryption key and administrator login restored");
            Ok(())
        }.await;
        let filters = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("all", "1")
            .append_pair(
                "filters",
                &json!({"label":[format!("com.docker.compose.project={project}")]}).to_string(),
            )
            .finish();
        if let Ok(containers) = agent
            .docker(
                reqwest::Method::GET,
                &format!("/containers/json?{filters}"),
                None,
            )
            .await
        {
            for container in containers.as_array().unwrap() {
                let _ = agent
                    .docker(
                        reqwest::Method::DELETE,
                        &format!(
                            "/containers/{}?force=true",
                            container["Id"].as_str().unwrap()
                        ),
                        None,
                    )
                    .await;
            }
        }
        let _ = agent
            .docker(
                reqwest::Method::DELETE,
                &format!("/networks/{network}"),
                None,
            )
            .await;
        let _ = agent
            .docker(reqwest::Method::DELETE, &format!("/volumes/{volume}"), None)
            .await;
        outcome.unwrap();
    }
}
