use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use zakoai_proxy::{Shared, app, new_app};

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    (dir, state)
}

async fn request(
    state: &Shared,
    method: &str,
    path: &str,
    body: Option<Value>,
    cookie: &str,
    console_path: &str,
) -> (StatusCode, Vec<u8>, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("x-console-path", console_path);
    if !cookie.is_empty() {
        builder = builder.header("cookie", cookie);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let body = body.map_or_else(Body::empty, |value| Body::from(value.to_string()));
    let response = app(state.clone())
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .to_owned();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, body, cookie)
}

async fn setup(state: &Shared, password: &str) -> String {
    let (status, _, cookie) = request(
        state,
        "POST",
        "/api/setup",
        Some(json!({"username":"admin","password":password})),
        "",
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!cookie.is_empty());
    cookie
}

fn settings(security_path: &str) -> Value {
    json!({
        "security_path":security_path,
        "max_retries":2,
        "failure_threshold":2,
        "cooldown_seconds":60,
        "request_timeout_seconds":120,
        "retention_days":90
    })
}

#[tokio::test]
async fn administrator_passwords_have_no_length_limit_but_cannot_be_empty() {
    let (_dir, state) = fixture();
    let initial_password = "x";
    let cookie = setup(&state, initial_password).await;
    let long_password = "密".repeat(513);
    let (status, _, _) = request(
        &state,
        "POST",
        "/api/password",
        Some(json!({
            "current_password":initial_password,
            "new_password":long_password,
            "confirm_password":long_password
        })),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, login_cookie) = request(
        &state,
        "POST",
        "/api/login",
        Some(json!({"username":"admin","password":long_password})),
        "",
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body, _) = request(
        &state,
        "POST",
        "/api/password",
        Some(json!({"current_password":long_password,"new_password":"","confirm_password":""})),
        &login_cookie,
        "/",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "{}",
        String::from_utf8_lossy(&body)
    );
}

#[tokio::test]
async fn security_entry_can_change_disable_and_accept_trailing_slashes() {
    let (dir, state) = fixture();
    let web = dir.path().join("web");
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), "console-entry").unwrap();
    let cookie = setup(&state, "x").await;

    let (status, _, _) = request(
        &state,
        "PUT",
        "/api/settings",
        Some(settings("hidden-entry")),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = request(&state, "GET", "/", None, "", "/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body, _) =
        request(&state, "GET", "/hidden-entry/", None, "", "/hidden-entry/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"console-entry");
    let (status, body, _) =
        request(&state, "GET", "/api/bootstrap", None, "", "/hidden-entry/").await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["initialized"],
        true
    );

    let (status, _, _) = request(
        &state,
        "PUT",
        "/api/settings",
        Some(settings("renamed-entry")),
        &cookie,
        "/hidden-entry/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = request(&state, "GET", "/hidden-entry/", None, "", "/hidden-entry/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = request(
        &state,
        "GET",
        "/renamed-entry/",
        None,
        "",
        "/renamed-entry/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = request(
        &state,
        "PUT",
        "/api/settings",
        Some(settings("")),
        &cookie,
        "/renamed-entry/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = request(&state, "GET", "/", None, "", "/").await;
    assert_eq!(status, StatusCode::OK);
}
