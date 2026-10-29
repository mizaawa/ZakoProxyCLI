use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt;
use zakoai_proxy::{Shared, app, new_app};

const ACCESS: &str = "provider-access-secret-73";
const FULL_KEY: &str = "sk-imported-complete-key-secret-9481";

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone, Debug)]
struct Call {
    method: String,
    path: String,
    query: String,
    headers: HeaderMap,
    body: Value,
}

struct Mock {
    keys: Mutex<Vec<Value>>,
    calls: Mutex<Vec<Call>>,
    unavailable: AtomicBool,
    reject_auth: AtomicBool,
    reject_create_after_commit: AtomicBool,
    multiplier: Mutex<f64>,
    pause_once: Mutex<Option<Arc<Pause>>>,
}

struct Pause {
    path: &'static str,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

fn sub_key(id: u64) -> Value {
    json!({"id":id,"name":format!("Sub key {id}"),"key":FULL_KEY,"group_id":7,
        "group":{"id":7,"name":"vip","rate_multiplier":1.5},"status":"active",
        "expires_at":null,"quota":0,"quota_used":0})
}

fn new_key(id: u64) -> Value {
    json!({"id":id,"name":format!("New key {id}"),"key":"sk-masked-****",
        "group":"vip","status":1,"expired_time":-1,"unlimited_quota":true,"remain_quota":0})
}

async fn upstream(State(mock): State<Arc<Mock>>, req: Request<Body>) -> Response {
    let path = req.uri().path().to_owned();
    let query = req.uri().query().unwrap_or("").to_owned();
    let method = req.method().to_string();
    let headers = req.headers().clone();
    let bytes = to_bytes(req.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    mock.calls.lock().unwrap().push(Call {
        method: method.clone(),
        path: path.clone(),
        query: query.clone(),
        headers: headers.clone(),
        body: body.clone(),
    });
    let pause = {
        let mut slot = mock.pause_once.lock().unwrap();
        if slot.as_ref().is_some_and(|pause| pause.path == path) {
            slot.take()
        } else {
            None
        }
    };
    if let Some(pause) = pause {
        pause.entered.notify_one();
        pause.resume.notified().await;
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    if path == "/v1/models" {
        if header("authorization") != format!("Bearer {FULL_KEY}") {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error":"wrong imported key"})),
            )
                .into_response();
        }
        return Json(json!({"data":[{"id":"mock-provider-model"}]})).into_response();
    }
    if mock.unavailable.load(Ordering::SeqCst) {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"message":"temporary upstream outage"})),
        )
            .into_response();
    }
    if path == "/api/status" {
        return Json(json!({"success":true,"data":{"quota_per_unit":500000}})).into_response();
    }
    if path == "/api/user/login" {
        return if body["username"] == "modern-user" && body["password"] == "modern-password" {
            Json(json!({"success":true,"data":{"access_token":"new-login-access-secret","token_type":"Bearer","user":{"id":42}}})).into_response()
        } else {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"message":"invalid login"})),
            )
                .into_response()
        };
    }
    let pat_auth = header("authorization") == "Bearer nap_system-token-secret"
        && header("new-api-user").is_empty();
    let modern_login_auth = header("authorization") == "Bearer new-login-access-secret"
        && header("new-api-user").is_empty();
    let legacy_auth = header("authorization") == format!("Bearer {ACCESS}")
        && (path.starts_with("/api/v1/") || header("new-api-user") == "42");
    if mock.reject_auth.load(Ordering::SeqCst) || !(pat_auth || modern_login_auth || legacy_auth) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message":"invalid upstream session"})),
        )
            .into_response();
    }
    if path == "/api/v1/keys" || path == "/api/token/" {
        if method == "POST" {
            let id = mock
                .keys
                .lock()
                .unwrap()
                .iter()
                .filter_map(|key| key["id"].as_u64())
                .max()
                .unwrap_or(0)
                + 1;
            let mut key = if path == "/api/v1/keys" {
                sub_key(id)
            } else {
                new_key(id)
            };
            key["name"] = body["name"].clone();
            mock.keys.lock().unwrap().push(key.clone());
            if mock.reject_create_after_commit.load(Ordering::SeqCst) {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"message":"response lost after commit"})),
                )
                    .into_response();
            }
            return if path == "/api/v1/keys" {
                Json(json!({"code":0,"data":key})).into_response()
            } else {
                Json(json!({"success":true,"message":"success"})).into_response()
            };
        }
        let params: std::collections::HashMap<_, _> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        let page = params
            .get(if path == "/api/v1/keys" { "page" } else { "p" })
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1);
        let page_size = params
            .get("page_size")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);
        let keys = mock.keys.lock().unwrap();
        let items = keys
            .iter()
            .skip(page.saturating_sub(1) * page_size)
            .take(page_size)
            .cloned()
            .collect::<Vec<_>>();
        return Json(json!({"success":true,"code":0,"data":{"items":items,"total":keys.len(),"page":page,"page_size":page_size}})).into_response();
    }
    if let Some(id) = path
        .strip_prefix("/api/v1/keys/")
        .and_then(|v| v.parse::<u64>().ok())
    {
        let found = mock
            .keys
            .lock()
            .unwrap()
            .iter()
            .find(|v| v["id"].as_u64() == Some(id))
            .cloned();
        return match found {
            Some(key) => Json(json!({"code":0,"data":key})).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({"message":"key deleted"})),
            )
                .into_response(),
        };
    }
    if let Some(id) = path
        .strip_prefix("/api/token/")
        .and_then(|v| v.strip_suffix("/key"))
        .and_then(|v| v.parse::<u64>().ok())
    {
        if method != "POST" {
            return (
                StatusCode::METHOD_NOT_ALLOWED,
                Json(json!({"message":"POST required"})),
            )
                .into_response();
        }
        if !mock
            .keys
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["id"].as_u64() == Some(id))
        {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"message":"token deleted"})),
            )
                .into_response();
        }
        return Json(json!({"success":true,"data":{"key":FULL_KEY}})).into_response();
    }
    let ratio = *mock.multiplier.lock().unwrap();
    Json(match path.as_str() {
        "/api/v1/auth/me" => json!({"code":0,"data":{"id":7,"balance":12.5}}),
        "/api/v1/usage/dashboard/stats" => json!({"code":0,"data":{"today_actual_cost":1,"total_actual_cost":4}}),
        "/api/v1/groups/available" => json!({"code":0,"data":[{"id":7,"name":"vip","rate_multiplier":1.5}]}),
        "/api/v1/groups/rates" => json!({"code":0,"data":{"7":ratio}}),
        "/api/user/self" => json!({"success":true,"data":{"id":42,"quota":500000,"used_quota":0,"group":"vip"}}),
        "/api/log/self/stat" => json!({"success":true,"data":{"quota":0}}),
        "/api/user/self/groups" | "/api/user/groups" => json!({"success":true,"data":{"vip":{"ratio":1.5},"auto":{}}}),
        "/api/pricing" => json!({"success":true,"data":[],"group_ratio":{"vip":ratio,"pricing-only":0.5},"usable_group":{"vip":"VIP","auto":"Automatic"}}),
        _ => return (StatusCode::NOT_FOUND, Json(json!({"message":"unexpected mock path"}))).into_response(),
    }).into_response()
}

async fn mock_server(keys: Vec<Value>) -> (String, Arc<Mock>, Server) {
    let mock = Arc::new(Mock {
        keys: Mutex::new(keys),
        calls: Mutex::new(Vec::new()),
        unavailable: AtomicBool::new(false),
        reject_auth: AtomicBool::new(false),
        reject_create_after_commit: AtomicBool::new(false),
        multiplier: Mutex::new(0.65),
        pause_once: Mutex::new(None),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(upstream).with_state(mock.clone());
    let server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    (url, mock, server)
}

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    (dir, state)
}

async fn request(
    s: &Shared,
    method: &str,
    path: &str,
    body: Value,
    cookie: &str,
) -> (StatusCode, Value, String) {
    let response = app(s.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .header("x-console-path", "/")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .to_owned();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cookie,
    )
}

async fn setup(s: &Shared) -> String {
    let (status, body, cookie) = request(
        s,
        "POST",
        "/api/setup",
        json!({"username":"admin","password":"integration-password-123"}),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!cookie.is_empty());
    cookie
}

async fn provider(s: &Shared, cookie: &str, url: &str, kind: &str) -> String {
    let (status, body, _) = request(s, "POST", "/api/providers", json!({
        "name":"Imported provider","kind":kind,"base_url":url,"auth_mode":"token",
        "user_id":if kind == "newapi" {"42"} else {""},"access_token":ACCESS,"token_type":"Bearer"
    }), cookie).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["id"].as_str().unwrap().to_owned()
}

fn account(provider_id: &str, key_id: &str) -> Value {
    json!({"name":"Bound provider account","endpoint":"https://spoofed.invalid/v1","api_key":"",
        "provider_id":provider_id,"provider_key_id":key_id,"models":["mock-provider-model"],
        "group_ids":[],"protocols":["responses"],"enabled":true})
}

fn assert_private(value: &Value) {
    let text = value.to_string();
    assert!(!text.contains(ACCESS), "provider session leaked: {value}");
    assert!(!text.contains(FULL_KEY), "full API key leaked: {value}");
}

#[tokio::test]
async fn newapi_pat_imports_keys_without_legacy_user_header() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![new_key(1)]).await;
    let (status, provider, _) = request(
        &state,
        "POST",
        "/api/providers",
        json!({
            "name":"Modern NewAPI","kind":"newapi","base_url":url,"auth_mode":"token",
            "user_id":"","access_token":"nap_system-token-secret","token_type":"Bearer"
        }),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{provider}");
    let id = provider["id"].as_str().unwrap();
    let (status, keys, _) = request(
        &state,
        "GET",
        &format!("/api/providers/{id}/keys"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{keys}");
    assert_eq!(keys["keys"][0]["id"], "1");
    let (status, bound, _) =
        request(&state, "POST", "/api/accounts", account(id, "1"), &cookie).await;
    assert_eq!(status, StatusCode::OK, "{bound}");
    let calls = mock.calls.lock().unwrap();
    assert!(calls.iter().any(|call| call.path == "/api/token/"
        && call.headers.get("authorization").unwrap() == "Bearer nap_system-token-secret"
        && !call.headers.contains_key("new-api-user")));
    assert!(
        calls
            .iter()
            .any(|call| call.path == "/api/token/1/key"
                && !call.headers.contains_key("new-api-user"))
    );
}

#[tokio::test]
async fn newapi_modern_password_login_imports_keys_with_bearer_session() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![new_key(1)]).await;
    let (status, provider, _) = request(
        &state,
        "POST",
        "/api/providers",
        json!({
            "name":"Modern NewAPI","kind":"newapi","base_url":url,"auth_mode":"password",
            "username":"modern-user","password":"modern-password"
        }),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{provider}");
    let id = provider["id"].as_str().unwrap();
    let (status, keys, _) = request(
        &state,
        "GET",
        &format!("/api/providers/{id}/keys"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{keys}");
    assert_eq!(keys["keys"][0]["id"], "1");
    let calls = mock.calls.lock().unwrap();
    assert!(calls.iter().any(|call| call.path == "/api/user/login"));
    assert!(calls.iter().any(|call| call.path == "/api/token/"
        && call.headers.get("authorization").unwrap() == "Bearer new-login-access-secret"
        && !call.headers.contains_key("new-api-user")));
}

async fn create_account(s: &Shared, cookie: &str, pid: &str, kid: &str) -> Value {
    let (status, body, _) = request(s, "POST", "/api/accounts", account(pid, kid), cookie).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_private(&body);
    body
}

#[tokio::test]
async fn provider_groups_and_create_key_use_current_upstream_group_name() {
    for kind in ["sub2api", "newapi"] {
        let (_dir, state) = fixture();
        let cookie = setup(&state).await;
        let (url, mock, _server) = mock_server(vec![]).await;
        let pid = provider(&state, &cookie, &url, kind).await;
        let group_id = if kind == "sub2api" { "7" } else { "vip" };
        let path = format!("/api/providers/{pid}/groups");
        let (status, _, _) = request(&state, "GET", &path, Value::Null, "").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, groups, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
        assert_eq!(status, StatusCode::OK, "{groups}");
        let group = groups["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["id"] == group_id)
            .unwrap();
        assert_eq!(group["name"], "vip");
        assert_eq!(group["multiplier"], 0.65);
        assert!(
            mock.keys.lock().unwrap().is_empty(),
            "opening/cancelling must not create a key"
        );
        let path = format!("/api/providers/{pid}/keys");
        let (status, _, _) = request(&state, "POST", &path, json!({"group_id":group_id}), "").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        for invalid in ["missing", "pricing-only", "../vip", ""] {
            let (status, _, _) = request(
                &state,
                "POST",
                &path,
                json!({"group_id":invalid,"name":"spoofed"}),
                &cookie,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        assert!(mock.keys.lock().unwrap().is_empty());
        let (status, created, _) = request(
            &state,
            "POST",
            &path,
            json!({"group_id":group_id,"name":"spoofed"}),
            &cookie,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        assert_eq!(created["created"], true);
        assert_eq!(created["name"], "【ZakoAI】vip");
        assert_private(&created);
        let calls = mock.calls.lock().unwrap().clone();
        let writes: Vec<_> = calls.iter().filter(|call| call.method == "POST").collect();
        assert_eq!(writes.len(), 1, "exactly one upstream mutation");
        let write = writes[0];
        assert_eq!(write.body["name"], "【ZakoAI】vip");
        if kind == "sub2api" {
            assert_eq!(write.path, "/api/v1/keys");
            assert_eq!(write.body["group_id"], 7);
        } else {
            assert_eq!(write.path, "/api/token/");
            assert_eq!(write.body["group"], "vip");
            assert_eq!(write.body["unlimited_quota"], true);
            assert_eq!(write.body["expired_time"], -1);
            assert_eq!(write.body["remain_quota"], 0);
            assert_eq!(write.body["cross_group_retry"], false);
        }
        let (status, list, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        assert_eq!(list["keys"][0]["name"], "【ZakoAI】vip");
        assert_private(&list);
    }
}

#[tokio::test]
async fn provider_key_creation_does_not_retry_after_upstream_commits() {
    for kind in ["sub2api", "newapi"] {
        let (_dir, state) = fixture();
        let cookie = setup(&state).await;
        let (url, mock, _server) = mock_server(vec![]).await;
        let pid = provider(&state, &cookie, &url, kind).await;
        mock.reject_create_after_commit
            .store(true, Ordering::SeqCst);
        let group_id = if kind == "sub2api" { "7" } else { "vip" };
        let (status, body, _) = request(
            &state,
            "POST",
            &format!("/api/providers/{pid}/keys"),
            json!({"group_id":group_id}),
            &cookie,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
        assert!(body["error"].as_str().unwrap().contains("先刷新"));
        assert_eq!(mock.keys.lock().unwrap().len(), 1);
        assert_eq!(
            mock.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call.method == "POST")
                .count(),
            1
        );
        assert_private(&body);
    }
}

#[tokio::test]
async fn deleting_provider_during_group_lookup_prevents_key_creation() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![]).await;
    let pid = provider(&state, &cookie, &url, "sub2api").await;
    let pause = Arc::new(Pause {
        path: "/api/v1/groups/available",
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    });
    *mock.pause_once.lock().unwrap() = Some(pause.clone());
    let request_state = state.clone();
    let request_cookie = cookie.clone();
    let request_pid = pid.clone();
    let task = tokio::spawn(async move {
        request(
            &request_state,
            "POST",
            &format!("/api/providers/{request_pid}/keys"),
            json!({"group_id":"7"}),
            &request_cookie,
        )
        .await
    });
    pause.entered.notified().await;
    let (status, _, _) = request(
        &state,
        "DELETE",
        &format!("/api/providers/{pid}"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    pause.resume.notify_one();
    let (status, body, _) = task.await.unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(mock.keys.lock().unwrap().is_empty());
    assert!(
        !mock
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.method == "POST")
    );
}

#[tokio::test]
async fn provider_keys_require_console_auth_and_upstream_auth() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![sub_key(1)]).await;
    let pid = provider(&state, &cookie, &url, "sub2api").await;
    for path in [
        format!("/api/providers/{pid}/keys"),
        "/api/accounts/missing/provider-binding".to_owned(),
    ] {
        assert_eq!(
            request(&state, "GET", &path, Value::Null, "").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert!(
        mock.calls.lock().unwrap().is_empty(),
        "unauthenticated console requests contacted upstream"
    );
    mock.reject_auth.store(true, Ordering::SeqCst);
    let (status, body, _) = request(
        &state,
        "GET",
        &format!("/api/providers/{pid}/keys"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_private(&body);
    assert_eq!(
        request(
            &state,
            "GET",
            "/api/providers/missing/keys",
            Value::Null,
            &cookie
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn sub2api_keys_page_all_results_and_use_dedicated_group_rate() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let mut keys = (1..=101).map(sub_key).collect::<Vec<_>>();
    keys[1]["status"] = json!("inactive");
    keys[2]["expires_at"] = json!("2000-01-01T00:00:00Z");
    keys[3]["quota"] = json!(10);
    keys[3]["quota_used"] = json!(10);
    let (url, mock, _server) = mock_server(keys).await;
    let pid = provider(&state, &cookie, &url, "sub2api").await;
    let (status, body, _) = request(
        &state,
        "GET",
        &format!("/api/providers/{pid}/keys"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_private(&body);
    let keys = body["keys"].as_array().expect("keys array");
    assert_eq!(keys.len(), 101);
    assert!(keys.iter().any(|key| key["id"] == "101"));
    let first = keys.iter().find(|key| key["id"] == "1").unwrap();
    assert_eq!(first["name"], "Sub key 1");
    assert_eq!(first["group_name"], "vip");
    assert_eq!(first["group_multiplier"], 0.65);
    assert_eq!(first["available"], true);
    assert!(
        keys.iter()
            .all(|key| key.get("key").is_none() && key.get("api_key").is_none())
    );
    for id in ["2", "3", "4"] {
        assert_eq!(
            keys.iter().find(|key| key["id"] == id).unwrap()["available"],
            false
        );
        let (status, body, _) =
            request(&state, "POST", "/api/accounts", account(&pid, id), &cookie).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let calls = mock.calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|call| call.path == "/api/v1/keys" && call.query.contains("page=2"))
    );
    assert!(calls.iter().all(|call| {
        call.headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {ACCESS}"))
    }));
}

#[tokio::test]
async fn newapi_uses_post_full_key_and_ignores_client_binding_metadata() {
    let (dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![new_key(21)]).await;
    let pid = provider(&state, &cookie, &url, "newapi").await;
    let mut input = account(&pid, "21");
    input["provider_binding"] = json!({"provider_id":"spoofed","key_id":"evil","key_name":"false metadata","group_name":"evil","group_multiplier":999,"status":"active","last_synced_at":0});
    let (status, created, _) = request(&state, "POST", "/api/accounts", input, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_private(&created);
    assert!(created["endpoint"].as_str().unwrap().starts_with(&url));
    assert!(!created["endpoint"].as_str().unwrap().contains("spoofed"));
    assert_eq!(created["provider_binding"]["provider_id"], pid);
    assert_eq!(created["provider_binding"]["key_id"], "21");
    assert_eq!(created["provider_binding"]["key_name"], "New key 21");
    assert_eq!(created["provider_binding"]["group_name"], "vip");
    assert_eq!(created["provider_binding"]["group_multiplier"], 0.65);
    assert_eq!(created["has_api_key"], true);
    let config = state.store.config().unwrap();
    assert_eq!(
        state.store.unseal(&config.accounts[0].api_key).unwrap(),
        FULL_KEY
    );
    assert_private(&serde_json::to_value(&config).unwrap());
    {
        let calls = mock.calls.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|call| call.path == "/api/token/21/key" && call.method == "POST")
        );
        assert!(
            calls
                .iter()
                .filter(|call| call.path.starts_with("/api/token/"))
                .all(|call| call
                    .headers
                    .get("new-api-user")
                    .and_then(|v| v.to_str().ok())
                    == Some("42"))
        );
    }
    drop(config);
    drop(state);
    let reopened = new_app(dir.path(), dir.path().join("web")).unwrap();
    let config = reopened.store.config().unwrap();
    assert_eq!(config.accounts.len(), 1);
    assert_eq!(
        reopened.store.unseal(&config.accounts[0].api_key).unwrap(),
        FULL_KEY
    );
    assert_eq!(
        config.accounts[0]
            .provider_binding
            .as_ref()
            .unwrap()
            .provider_id,
        pid
    );
    let (_, _, cookie) = request(
        &reopened,
        "POST",
        "/api/login",
        json!({"username":"admin","password":"integration-password-123"}),
        "",
    )
    .await;
    let (status, public, _) =
        request(&reopened, "GET", "/api/accounts", Value::Null, &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_private(&public);
}

#[tokio::test]
async fn newapi_pagination_and_auto_group_do_not_invent_a_multiplier() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let mut keys = (1..=101).map(new_key).collect::<Vec<_>>();
    keys[0]["group"] = json!("auto");
    keys[1]["status"] = json!(2);
    keys[2]["status"] = json!(3);
    keys[3]["status"] = json!(4);
    keys[4]["expired_time"] = json!(1);
    let (url, mock, _server) = mock_server(keys).await;
    let pid = provider(&state, &cookie, &url, "newapi").await;
    let (status, body, _) = request(
        &state,
        "GET",
        &format!("/api/providers/{pid}/keys"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_private(&body);
    let keys = body["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 101);
    let automatic = keys.iter().find(|key| key["id"] == "1").unwrap();
    assert_eq!(automatic["group_name"], "auto");
    assert!(
        automatic["group_multiplier"].is_null(),
        "automatic group must not show a made-up fixed rate"
    );
    for id in ["2", "3", "4", "5"] {
        assert_eq!(
            keys.iter().find(|key| key["id"] == id).unwrap()["available"],
            false
        );
    }
    assert!(
        mock.calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.path == "/api/token/" && call.query.contains("p=2"))
    );
}

#[tokio::test]
async fn refresh_detects_rename_rate_change_deleted_key_and_deleted_provider() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![sub_key(1)]).await;
    let pid = provider(&state, &cookie, &url, "sub2api").await;
    let created = create_account(&state, &cookie, &pid, "1").await;
    let aid = created["id"].as_str().unwrap();
    let path = format!("/api/accounts/{aid}/provider-binding");
    mock.keys.lock().unwrap()[0]["name"] = json!("Renamed upstream key");
    *mock.multiplier.lock().unwrap() = 0.8;
    let (status, updated, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["available"], true);
    assert_eq!(
        updated["provider_binding"]["key_name"],
        "Renamed upstream key"
    );
    assert_eq!(updated["provider_binding"]["group_multiplier"], 0.8);
    assert_private(&updated);

    mock.keys.lock().unwrap().clear();
    let (status, missing, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{missing}");
    assert_eq!(missing["available"], false);
    assert_eq!(missing["provider_binding"]["status"], "missing");
    assert!(state.store.config().unwrap().accounts[0].api_key.is_empty());
    assert_eq!(
        request(&state, "POST", "/api/accounts", account(&pid, "1"), &cookie)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    mock.keys.lock().unwrap().push(sub_key(1));
    let (status, restored, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["available"], true);
    assert_eq!(
        state
            .store
            .unseal(&state.store.config().unwrap().accounts[0].api_key)
            .unwrap(),
        FULL_KEY
    );

    assert_eq!(
        request(
            &state,
            "DELETE",
            &format!("/api/providers/{pid}"),
            Value::Null,
            &cookie
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, missing, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{missing}");
    assert_eq!(missing["available"], false);
    assert_eq!(missing["provider_binding"]["status"], "provider_missing");
    assert!(state.store.config().unwrap().accounts[0].api_key.is_empty());
}

#[tokio::test]
async fn transient_upstream_failure_preserves_binding_but_disabled_key_clears_secret() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![sub_key(1)]).await;
    let pid = provider(&state, &cookie, &url, "sub2api").await;
    let created = create_account(&state, &cookie, &pid, "1").await;
    let path = format!(
        "/api/accounts/{}/provider-binding",
        created["id"].as_str().unwrap()
    );
    let before = serde_json::to_value(&state.store.config().unwrap().accounts[0]).unwrap();
    mock.unavailable.store(true, Ordering::SeqCst);
    let (status, failed, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{failed}");
    assert_private(&failed);
    assert_eq!(
        serde_json::to_value(&state.store.config().unwrap().accounts[0]).unwrap(),
        before
    );
    mock.unavailable.store(false, Ordering::SeqCst);
    mock.keys.lock().unwrap()[0]["status"] = json!("inactive");
    let (status, disabled, _) = request(&state, "GET", &path, Value::Null, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{disabled}");
    assert_eq!(disabled["available"], false);
    assert!(state.store.config().unwrap().accounts[0].api_key.is_empty());
}

#[tokio::test]
async fn binding_validation_discovery_and_explicit_detach() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, _mock, _server) = mock_server(vec![new_key(21)]).await;
    let pid = provider(&state, &cookie, &url, "newapi").await;
    for input in [
        {
            let mut v = account(&pid, "21");
            v["api_key"] = json!("client-secret");
            v
        },
        {
            let mut v = account(&pid, "21");
            v.as_object_mut().unwrap().remove("provider_key_id");
            v
        },
        {
            let mut v = account(&pid, "21");
            v["provider_id"] = Value::Null;
            v
        },
    ] {
        let (status, body, _) = request(&state, "POST", "/api/accounts", input, &cookie).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let (status, models, _) = request(&state, "POST", "/api/models/discover", json!({
        "endpoint":"https://spoofed.invalid/v1","provider_id":pid,"provider_key_id":"21","protocols":["responses"]
    }), &cookie).await;
    assert_eq!(status, StatusCode::OK, "{models}");
    assert_eq!(models["models"], json!(["mock-provider-model"]));
    assert_private(&models);

    let created = create_account(&state, &cookie, &pid, "21").await;
    let path = format!("/api/accounts/{}", created["id"].as_str().unwrap());
    let mut edit = account(&pid, "21");
    edit.as_object_mut().unwrap().remove("provider_id");
    edit.as_object_mut().unwrap().remove("provider_key_id");
    edit["name"] = json!("Rename account without changing binding");
    let (status, edited, _) = request(&state, "PUT", &path, edit.clone(), &cookie).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["provider_binding"]["provider_id"], pid);
    assert!(edited["endpoint"].as_str().unwrap().starts_with(&url));
    edit["provider_id"] = Value::Null;
    edit["provider_key_id"] = Value::Null;
    assert_eq!(
        request(&state, "PUT", &path, edit.clone(), &cookie).await.0,
        StatusCode::BAD_REQUEST
    );
    edit["api_key"] = json!("detached-manual-key");
    edit["endpoint"] = json!(&url);
    let (status, detached, _) = request(&state, "PUT", &path, edit, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{detached}");
    assert!(detached["provider_binding"].is_null());
    assert_eq!(
        state
            .store
            .unseal(&state.store.config().unwrap().accounts[0].api_key)
            .unwrap(),
        "detached-manual-key"
    );
}

async fn check_stale_provider_save(change: &str) {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, mock, _server) = mock_server(vec![sub_key(1), sub_key(2)]).await;
    let pid = provider(&state, &cookie, &url, "sub2api").await;
    let created = create_account(&state, &cookie, &pid, "1").await;
    let account_path = format!("/api/accounts/{}", created["id"].as_str().unwrap());
    // Stop after the first request has read the active key list. A later update
    // must survive even when the older remote lookup completes last.
    let pause = Arc::new(Pause {
        path: "/api/v1/groups/rates",
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    });
    *mock.pause_once.lock().unwrap() = Some(pause.clone());
    let stale = {
        let state = state.clone();
        let cookie = cookie.clone();
        let path = account_path.clone();
        let mut input = account(&pid, "1");
        input["name"] = json!("Older in-flight edit");
        input.as_object_mut().unwrap().remove("provider_id");
        input.as_object_mut().unwrap().remove("provider_key_id");
        tokio::spawn(async move { request(&state, "PUT", &path, input, &cookie).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), pause.entered.notified())
        .await
        .expect("in-flight save must reach the controlled checkpoint");
    let (status, body, _) = match change {
        "detach" => {
            let mut input = account(&pid, "1");
            input["provider_id"] = Value::Null;
            input["provider_key_id"] = Value::Null;
            input["endpoint"] = json!(&url);
            input["api_key"] = json!("new-manual-credential");
            request(&state, "PUT", &account_path, input, &cookie).await
        }
        "switch" => request(&state, "PUT", &account_path, account(&pid, "2"), &cookie).await,
        "disable" => {
            mock.keys.lock().unwrap()[0]["status"] = json!("inactive");
            request(
                &state,
                "GET",
                &format!("{account_path}/provider-binding"),
                Value::Null,
                &cookie,
            )
            .await
        }
        _ => unreachable!(),
    };
    assert_eq!(status, StatusCode::OK, "{body}");
    let expected = serde_json::to_value(&state.store.config().unwrap().accounts[0]).unwrap();
    pause.resume.notify_one();
    let (status, body, _) = stale.await.unwrap();
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "stale {change} save must not overwrite the newer state: {body}"
    );
    assert_eq!(
        serde_json::to_value(&state.store.config().unwrap().accounts[0]).unwrap(),
        expected
    );
}

#[tokio::test]
async fn inflight_provider_save_cannot_undo_manual_detach() {
    check_stale_provider_save("detach").await;
}

#[tokio::test]
async fn inflight_provider_save_cannot_undo_key_switch() {
    check_stale_provider_save("switch").await;
}

#[tokio::test]
async fn inflight_provider_save_cannot_revive_disabled_key() {
    check_stale_provider_save("disable").await;
}

#[tokio::test]
async fn changing_provider_identity_invalidates_bound_accounts_until_reselected() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let (url, _mock, _server) = mock_server(vec![sub_key(1)]).await;
    let (other_url, _other_mock, _other_server) = mock_server(vec![sub_key(1)]).await;
    let pid = provider(&state, &cookie, &url, "sub2api").await;
    let created = create_account(&state, &cookie, &pid, "1").await;
    let account_path = format!("/api/accounts/{}", created["id"].as_str().unwrap());
    let update = |site: &str, name: &str, token: &str| {
        json!({
            "name":name,"kind":"sub2api","base_url":site,"auth_mode":"token",
            "access_token":token,"token_type":"Bearer"
        })
    };
    // A display-only edit keeps the already verified binding usable.
    let (status, body, _) = request(
        &state,
        "PUT",
        &format!("/api/providers/{pid}"),
        update(&url, "Renamed provider", ""),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(state.store.config().unwrap().accounts[0].provider_available());
    // Key IDs are scoped to the upstream identity. Reusing "1" on another site
    // must neither retain the former secret nor silently select the new key.
    let (status, body, _) = request(
        &state,
        "PUT",
        &format!("/api/providers/{pid}"),
        update(&other_url, "Different provider", ACCESS),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invalidated = state.store.config().unwrap().accounts.remove(0);
    assert!(
        invalidated.api_key.is_empty(),
        "changing the provider identity retained the old bound credential"
    );
    assert!(!invalidated.provider_available());
    assert_eq!(
        invalidated.provider_binding.as_ref().unwrap().status,
        "provider_changed"
    );
    let (status, body, _) = request(
        &state,
        "GET",
        &format!("{account_path}/provider-binding"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["available"], false);
    assert_eq!(body["provider_binding"]["status"], "provider_changed");
    assert!(state.store.config().unwrap().accounts[0].api_key.is_empty());
    let mut implicit = account(&pid, "1");
    implicit.as_object_mut().unwrap().remove("provider_id");
    implicit.as_object_mut().unwrap().remove("provider_key_id");
    assert_eq!(
        request(&state, "PUT", &account_path, implicit, &cookie)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &state,
            "POST",
            "/api/models/discover",
            json!({"account_id":created["id"]}),
            &cookie
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, rebound, _) =
        request(&state, "PUT", &account_path, account(&pid, "1"), &cookie).await;
    assert_eq!(status, StatusCode::OK, "{rebound}");
    assert_eq!(rebound["endpoint"], other_url);
    assert_eq!(rebound["provider_binding"]["status"], "active");
}

#[tokio::test]
async fn newapi_zero_expiry_is_expired_and_only_minus_one_is_unlimited() {
    let (_dir, state) = fixture();
    let cookie = setup(&state).await;
    let now = zakoai_proxy::store::now();
    let mut keys = vec![new_key(1), new_key(2), new_key(3), new_key(4)];
    for (key, expiry) in keys.iter_mut().zip([-1, 0, now - 1, now + 3600]) {
        key["expired_time"] = json!(expiry);
    }
    let (url, _mock, _server) = mock_server(keys).await;
    let pid = provider(&state, &cookie, &url, "newapi").await;
    let (status, body, _) = request(
        &state,
        "GET",
        &format!("/api/providers/{pid}/keys"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for (index, available) in [true, false, false, true].iter().enumerate() {
        assert_eq!(
            body["keys"][index]["available"], *available,
            "key expiry at index {index}: {body}"
        );
    }
    for id in ["2", "3"] {
        assert_eq!(
            request(&state, "POST", "/api/accounts", account(&pid, id), &cookie)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    create_account(&state, &cookie, &pid, "1").await;
    create_account(&state, &cookie, &pid, "4").await;
}
