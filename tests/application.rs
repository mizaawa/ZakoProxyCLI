use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use zakoai_proxy::{Shared, app, new_app, profiles};

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    (dir, state)
}

fn nonoverlapping_profile_ids() -> Vec<String> {
    let mut models = std::collections::HashSet::new();
    profiles::all()
        .iter()
        .filter(|profile| {
            if profile.models.iter().any(|model| models.contains(model)) {
                return false;
            }
            models.extend(profile.models.iter().cloned());
            true
        })
        .map(|profile| profile.id.clone())
        .collect()
}

#[tokio::test]
async fn local_fonts_have_font_mime_type() {
    let (dir, s) = fixture();
    let assets = dir.path().join("web/assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(assets.join("inter.woff2"), b"font-test").unwrap();
    let response = app(s)
        .oneshot(
            Request::builder()
                .uri("/assets/inter.woff2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "font/woff2");
}

#[tokio::test]
async fn license_notices_are_served_as_plain_text() {
    let (dir, s) = fixture();
    let assets = dir.path().join("web/assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(assets.join("third-party-licenses.txt"), "MIT license").unwrap();
    let response = app(s)
        .oneshot(
            Request::builder()
                .uri("/assets/third-party-licenses.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
}
async fn request(
    s: &Shared,
    method: &str,
    path: &str,
    body: Value,
    cookie: &str,
    entry: &str,
) -> (StatusCode, Value, String) {
    let response = app(s.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .header("x-console-path", entry)
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
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .to_owned();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(json!(null)),
        cookie,
    )
}
async fn setup(s: &Shared) -> String {
    let (status, _, cookie) = request(
        s,
        "POST",
        "/api/setup",
        json!({"username":"admin","password":"integration-password-123"}),
        "",
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!cookie.is_empty());
    cookie
}

#[tokio::test]
async fn first_run_crud_and_authentication() {
    let (_dir, s) = fixture();
    assert_eq!(
        request(&s, "GET", "/api/groups", Value::Null, "", "/")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let c = setup(&s).await;
    assert_eq!(
        request(
            &s,
            "POST",
            "/api/setup",
            json!({"username":"takeover","password":"takeover-password"}),
            "",
            "/"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, g, _) = request(
        &s,
        "POST",
        "/api/groups",
        json!({"name":"Production","models":["mock-model"],"instruction":"Be concise."}),
        &c,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let gid = g["id"].as_str().unwrap();
    let (status, key, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({"name":"Development","group_id":gid}),
        &c,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let kid = key["id"].as_str().unwrap();
    let raw = key["key"].as_str().unwrap();
    assert!(raw.starts_with("sk-zako-"));
    let (_, revealed, _) = request(
        &s,
        "POST",
        &format!("/api/keys/{kid}/reveal"),
        json!({}),
        &c,
        "/",
    )
    .await;
    assert_eq!(revealed["key"], raw);
    let (_, keys, _) = request(&s, "GET", "/api/keys", Value::Null, &c, "/").await;
    assert!(keys[0].get("secret").is_none());
    assert!(keys[0].get("hash").is_none());
    assert_eq!(
        request(
            &s,
            "DELETE",
            &format!("/api/groups/{gid}"),
            Value::Null,
            &c,
            "/"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let response = app(s.clone())
        .oneshot(
            Request::builder()
                .uri("/api/groups")
                .header("cookie", &c)
                .header("host", "localhost:1990")
                .header("origin", "https://attacker.invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(request(&s,"POST","/api/password",json!({"current_password":"wrong","new_password":"new-password-123","confirm_password":"new-password-123"}),&c,"/").await.0,StatusCode::BAD_REQUEST);
    assert_eq!(request(&s,"POST","/api/password",json!({"current_password":"integration-password-123","new_password":"new-password-123","confirm_password":"new-password-123"}),&c,"/").await.0,StatusCode::OK);
    assert_eq!(
        request(&s, "GET", "/api/keys", Value::Null, &c, "/")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &s,
            "POST",
            "/api/login",
            json!({"username":"admin","password":"new-password-123"}),
            "",
            "/"
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn security_entry_hides_console_and_administration() {
    let (_dir, s) = fixture();
    let c = setup(&s).await;
    let settings = json!({"security_path":"secret-entry","max_retries":2,"failure_threshold":2,"cooldown_seconds":60,"request_timeout_seconds":120,"retention_days":90});
    assert_eq!(
        request(&s, "PUT", "/api/settings", settings, &c, "/")
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&s, "GET", "/api/bootstrap", Value::Null, "", "/")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &s,
            "GET",
            "/api/bootstrap",
            Value::Null,
            "",
            "/secret-entry"
        )
        .await
        .0,
        StatusCode::OK
    );
    let r = app(s)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn authenticated_root_redirects_to_security_entry_for_valid_host() {
    let (dir, s) = fixture();
    let cookie = setup(&s).await;
    let settings = json!({"security_path":"secret-entry","max_retries":2,"failure_threshold":2,"cooldown_seconds":60,"request_timeout_seconds":120,"retention_days":90});
    assert_eq!(
        request(&s, "PUT", "/api/settings", settings, &cookie, "/")
            .await
            .0,
        StatusCode::OK
    );

    std::fs::create_dir_all(dir.path().join("web")).unwrap();
    let response = app(s.clone())
        .oneshot(
            Request::builder()
                .uri("/")
                .header("host", "console.example.test")
                .header("origin", "https://console.example.test")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers()["location"], "/secret-entry");

    let response = app(s.clone())
        .oneshot(
            Request::builder()
                .uri("/")
                .header("host", "console.example.test")
                .header("origin", "https://attacker.invalid")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app(s)
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn upstream(failing: bool) -> (String, tokio::task::JoinHandle<()>) {
    let router=Router::new().route("/v1/models",get(||async{axum::Json(json!({"data":[{"id":"mock-model"}]}))}))
        .route("/v1/responses",post(move|headers:axum::http::HeaderMap,axum::Json(body):axum::Json<Value>|async move{
            assert_eq!(headers["authorization"],"Bearer upstream-secret");
            if failing{return (StatusCode::SERVICE_UNAVAILABLE,axum::Json(json!({"error":"temporary"}))).into_response();}
            assert_eq!(body["model"],"mock-model");
            if body["stream"]==true {
                let stream=futures_util::stream::iter(vec![Ok::<_,std::io::Error>(bytes::Bytes::from_static(b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_mock\",\"usage\":{\"input_tokens\":100,\"output_tokens\":20,\"input_tokens_details\":{\"cached_tokens\":80}}}}\n\n"))]);
                return ([("content-type","text/event-stream")],Body::from_stream(stream)).into_response();
            }
            (StatusCode::OK,axum::Json(json!({"id":"resp_mock","object":"response","instructions":body["instructions"],"usage":{"input_tokens":100,"output_tokens":20,"input_tokens_details":{"cached_tokens":80}}}))).into_response()
        }))
        .route("/v1/responses/compact",post(||async{axum::Json(json!({"object":"response.compaction","output":[],"usage":{"input_tokens":12,"output_tokens":3}}))}));
    use axum::response::IntoResponse;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (format!("http://{addr}"), handle)
}
async fn proxy_request(s: &Shared, path: &str, key: &str, body: Value) -> (StatusCode, Vec<u8>) {
    let response = app(s.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}

#[tokio::test]
async fn routing_failover_compact_streaming_and_statistics() {
    let (_dir, s) = fixture();
    let c = setup(&s).await;
    let (bad_url, bad_task) = upstream(true).await;
    let (good_url, good_task) = upstream(false).await;
    let (_, g, _) = request(
        &s,
        "POST",
        "/api/groups",
        json!({"name":"main","models":["mock-model"],"instruction":"Be concise."}),
        &c,
        "/",
    )
    .await;
    let gid = g["id"].as_str().unwrap();
    for (endpoint, priority) in [(bad_url, 0), (good_url, 1)] {
        let (status,account,_)=request(&s,"POST","/api/accounts",json!({"name":"upstream","endpoint":endpoint,"api_key":"upstream-secret","models":["mock-model"],"group_ids":[gid],"priority":priority,"rpm":0,"concurrency":0,"enabled":true,"protocols":["responses","compact"]}),&c,"/").await;
        assert_eq!(status, StatusCode::OK, "{account}");
        assert!(account.get("api_key").is_none());
    }
    let (_, key, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({"name":"local","group_id":gid}),
        &c,
        "/",
    )
    .await;
    let key = key["key"].as_str().unwrap();
    let (status, body) = proxy_request(
        &s,
        "/v1/responses",
        key,
        json!({"model":"mock-model","input":"hello","instructions":"Original."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let output: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        output["instructions"]
            .as_str()
            .unwrap()
            .contains("Original.")
    );
    assert_eq!(output["instructions"], "Original.");
    assert_eq!(
        proxy_request(
            &s,
            "/v1/responses/compact",
            key,
            json!({"model":"mock-model","input":[]})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, stream) = proxy_request(
        &s,
        "/v1/responses",
        key,
        json!({"model":"mock-model","input":"hello","stream":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&stream).contains("response.completed"));
    assert_eq!(
        proxy_request(
            &s,
            "/v1/responses",
            "invalid",
            json!({"model":"mock-model","input":"hello"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, stats, _) = request(&s, "GET", "/api/stats", Value::Null, &c, "/").await;
    assert_eq!(stats["summary"]["total_requests"], 3);
    assert_eq!(stats["summary"]["input_tokens"], 212);
    assert_eq!(stats["summary"]["cached_tokens"], 160);
    assert!(
        stats["recent"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["attempts"].as_u64().unwrap_or(0) > 1)
    );
    bad_task.abort();
    good_task.abort();
}

#[tokio::test]
async fn concurrent_first_run_has_single_owner() {
    let (_dir, s) = fixture();
    let a = Arc::clone(&s);
    let b = Arc::clone(&s);
    let (one, two) = tokio::join!(
        request(
            &a,
            "POST",
            "/api/setup",
            json!({"username":"one","password":"password-owner-one"}),
            "",
            "/"
        ),
        request(
            &b,
            "POST",
            "/api/setup",
            json!({"username":"two","password":"password-owner-two"}),
            "",
            "/"
        )
    );
    assert_eq!(
        [one.0, two.0]
            .iter()
            .filter(|v| **v == StatusCode::OK)
            .count(),
        1
    );
}

#[tokio::test]
async fn simultaneous_password_changes_only_accept_one_current_session() {
    let (_dir, s) = fixture();
    let cookie = setup(&s).await;
    let first = json!({"current_password":"integration-password-123","new_password":"first-new-password","confirm_password":"first-new-password"});
    let second = json!({"current_password":"integration-password-123","new_password":"second-new-password","confirm_password":"second-new-password"});
    let (one, two) = tokio::join!(
        request(&s, "POST", "/api/password", first, &cookie, "/"),
        request(&s, "POST", "/api/password", second, &cookie, "/")
    );
    assert_eq!(
        [one.0, two.0]
            .iter()
            .filter(|s| **s == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        request(&s, "GET", "/api/keys", Value::Null, &cookie, "/")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn discovery_handles_models_and_rejects_oversized_bodies() {
    let (_dir, s) = fixture();
    let cookie = setup(&s).await;
    let router = Router::new()
        .route(
            "/normal/v1/models",
            get(|| async { axum::Json(json!({"data":[{"id":"b"},{"id":"a"},{"id":"a"}]})) }),
        )
        .route(
            "/fixed/v1/models",
            get(|| async { "x".repeat(1024 * 1024 + 1) }),
        )
        .route(
            "/chunked/v1/models",
            get(|| async {
                Body::from_stream(futures_util::stream::iter((0..17).map(|_| {
                    Ok::<_, std::io::Error>(bytes::Bytes::from(vec![b'x'; 64 * 1024]))
                })))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let (status, models, _) = request(
        &s,
        "POST",
        "/api/models/discover",
        json!({"endpoint":format!("{endpoint}/normal"),"api_key":"test-key"}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(models["models"], json!(["a", "b"]));
    for suffix in ["fixed", "chunked"] {
        let (status, body, _) = request(
            &s,
            "POST",
            "/api/models/discover",
            json!({"endpoint":format!("{endpoint}/{suffix}"),"api_key":"test-key"}),
            &cookie,
            "/",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "Upstream model response is too large");
    }
    task.abort();
}

async fn strategy_group(s: &Shared, cookie: &str, name: &str, models: Vec<String>) -> String {
    let (status, group, _) =
        request(s, "POST", "/api/groups", json!({"name":name}), cookie, "/").await;
    assert_eq!(status, StatusCode::OK, "{group}");
    let group_id = group["id"].as_str().unwrap().to_owned();
    let (status, account, _) = request(s, "POST", "/api/accounts", json!({
        "name":format!("{name} upstream"), "endpoint":"https://example.test", "api_key":"secret",
        "models":models, "group_ids":[group_id]
    }), cookie, "/").await;
    assert_eq!(status, StatusCode::OK, "{account}");
    group_id
}

#[tokio::test]
async fn key_profile_selections_validate_edit_and_persist_across_restart() {
    let (dir, s) = fixture();
    let cookie = setup(&s).await;
    let (status, catalog, _) = request(&s, "GET", "/api/profiles", Value::Null, &cookie, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalog.as_array().unwrap().len(), profiles::all().len());
    for profile in profiles::all() {
        let entry = catalog
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == profile.id)
            .unwrap();
        assert_eq!(entry["models"], json!(profile.models));
        assert_eq!(entry["instruction"], profile.instruction);
        assert_eq!(entry["model_family"], profile.model_family);
        if let Some(metadata) = &profile.metadata {
            assert_eq!(entry["metadata"], json!(metadata));
        } else {
            assert!(entry.get("metadata").is_none());
        }
    }
    let ids = nonoverlapping_profile_ids();
    let models = profiles::all()
        .iter()
        .flat_map(|profile| profile.models.clone())
        .collect();
    let group_id = strategy_group(&s, &cookie, "All supported models", models).await;
    for invalid in [
        json!(["unknown-profile"]),
        Value::Null,
        json!([null]),
        json!("not-an-array"),
    ] {
        let (status, _, _) = request(
            &s,
            "POST",
            "/api/keys",
            json!({"name":"Invalid", "group_id":group_id, "profile_ids":invalid}),
            &cookie,
            "/",
        )
        .await;
        assert!(status.is_client_error(), "{status}");
    }
    let duplicate_ids: Vec<_> = ids.iter().chain(&ids).cloned().collect();
    let (status, created, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({"name":"All models", "group_id":group_id, "profile_ids":duplicate_ids}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let key_id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(s.store.config().unwrap().keys[0].profile_ids, ids);
    let key_path = format!("/api/keys/{key_id}");
    let before = serde_json::to_value(&s.store.config().unwrap().keys[0]).unwrap();
    for invalid in [
        json!(["unknown-profile"]),
        Value::Null,
        json!([null]),
        json!("not-an-array"),
    ] {
        let (status, rejected, _) = request(
            &s,
            "PUT",
            &key_path,
            json!({"name":"Bad edit", "group_id":group_id, "profile_ids":invalid}),
            &cookie,
            "/",
        )
        .await;
        assert!(status.is_client_error(), "{status} {rejected}");
        assert_eq!(
            serde_json::to_value(&s.store.config().unwrap().keys[0]).unwrap(),
            before
        );
    }
    let (status, updated, _) = request(
        &s,
        "PUT",
        &key_path,
        json!({"name":"Edited models", "group_id":group_id, "profile_ids":[ids[1],ids[2]]}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    drop(s);
    let s = new_app(dir.path(), dir.path().join("web")).unwrap();
    let (status, keys, _) = request(&s, "GET", "/api/keys", Value::Null, &cookie, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys.as_array().unwrap().len(), 1);
    assert_eq!(keys[0]["id"], key_id);
    assert_eq!(keys[0]["profile_ids"], json!([ids[1], ids[2]]));
    assert!(keys[0].get("secret").is_none());
    assert!(keys[0].get("hash").is_none());
    let (status, cleared, _) = request(
        &s,
        "PUT",
        &key_path,
        json!({"name":"Disabled profiles", "group_id":group_id, "profile_ids":[]}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert!(s.store.config().unwrap().keys[0].profile_ids.is_empty());
}

#[tokio::test]
async fn coldcoffee_template_catalog_and_explicit_bindings_persist_and_validate() {
    let (dir, s) = fixture();
    let (_, response, _) = request(&s, "GET", "/api/profile-templates", Value::Null, "", "/").await;
    assert!(response.get("error").is_some());
    let cookie = setup(&s).await;
    let (status, templates, _) = request(
        &s,
        "GET",
        "/api/profile-templates",
        Value::Null,
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(templates.as_array().unwrap().len(), 15);
    assert!(
        templates
            .as_array()
            .unwrap()
            .iter()
            .all(|template| template.get("instruction").is_none())
    );
    let actual = "provider/actual@model";
    let group_id = strategy_group(&s, &cookie, "Templates", vec![actual.into()]).await;
    let binding = format!("coldcoffee@meta-muse@{}", hex::encode(actual));
    let other = format!("coldcoffee@cursor@{}", hex::encode(actual));
    let (status, created, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({
            "name":"Template key", "group_id":group_id, "profile_ids":[binding]
        }),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let key_id = created["id"].as_str().unwrap();
    for invalid in [
        vec![binding.clone(), other],
        vec!["coldcoffee@unknown@61".into()],
        vec![format!(
            "coldcoffee@kimi@{}",
            hex::encode("unconfigured-model")
        )],
    ] {
        let (status, _, _) = request(
            &s,
            "PUT",
            &format!("/api/keys/{key_id}"),
            json!({
                "name":"Rejected", "group_id":group_id, "profile_ids":invalid
            }),
            &cookie,
            "/",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    drop(s);
    let s = new_app(dir.path(), dir.path().join("web")).unwrap();
    let (_, keys, _) = request(&s, "GET", "/api/keys", Value::Null, &cookie, "/").await;
    assert_eq!(keys[0]["profile_ids"], json!([binding]));
    let (status, catalog, _) = request(&s, "GET", "/api/profiles", Value::Null, &cookie, "/").await;
    assert_eq!(status, StatusCode::OK);
    let saved = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["id"] == binding)
        .unwrap();
    assert_eq!(saved["models"], json!([actual]));
    assert_eq!(saved["metadata"]["variant"], "meta-muse");
    assert!(saved["instruction"].as_str().unwrap().contains("Meta Muse"));
    assert_eq!(catalog.as_array().unwrap().len(), profiles::all().len() + 1);
}

#[tokio::test]
async fn overlapping_strategy_variants_are_rejected_without_mutating_keys() {
    let (_dir, s) = fixture();
    let cookie = setup(&s).await;
    let group_id = strategy_group(
        &s,
        &cookie,
        "Variant models",
        vec!["glm-5.3".into(), "grok-4.7".into()],
    )
    .await;
    for model in ["glm-5.3", "grok-4.7"] {
        let legacy = format!("coldcoffee-{model}-v1");
        let lightweight = format!("aijailbreak-{model}-v1");
        let (status, created, _) = request(&s, "POST", "/api/keys", json!({
            "name":"Original", "group_id":group_id, "profile_ids":[legacy,legacy], "default_strategy_enabled":true
        }), &cookie, "/").await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let key_id = created["id"].as_str().unwrap();
        let path = format!("/api/keys/{key_id}");
        let (_, before, _) = request(&s, "GET", "/api/keys", Value::Null, &cookie, "/").await;
        let key = before
            .as_array()
            .unwrap()
            .iter()
            .find(|key| key["id"] == key_id)
            .unwrap();
        assert_eq!(key["profile_ids"], json!([legacy]));
        for (method, path) in [("POST", "/api/keys"), ("PUT", path.as_str())] {
            let (status, rejected, _) = request(&s, method, path, json!({
                "name":"Rejected edit", "group_id":group_id, "profile_ids":[legacy,lightweight], "default_strategy_enabled":false
            }), &cookie, "/").await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
            assert_eq!(
                rejected["error"],
                format!("Only one instruction profile may target model {model}")
            );
            let (_, after, _) = request(&s, "GET", "/api/keys", Value::Null, &cookie, "/").await;
            assert_eq!(after, before);
        }
        let (status, switched, _) = request(
            &s,
            "PUT",
            &path,
            json!({
                "name":"Lightweight", "group_id":group_id, "profile_ids":[lightweight,lightweight]
            }),
            &cookie,
            "/",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{switched}");
        let config = s.store.config().unwrap();
        let key = config.keys.iter().find(|key| key.id == key_id).unwrap();
        assert_eq!(key.profile_ids, vec![lightweight]);
        assert!(key.default_strategy_enabled);
    }
}

#[tokio::test]
async fn old_groups_migrate_exact_presets_to_existing_keys_once_and_discard_custom_fields() {
    let (dir, s) = fixture();
    let cookie = setup(&s).await;
    let profile = &profiles::all()[1];
    let mut config = serde_json::to_value(s.store.config().unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .remove("key_strategy_schema");
    config["groups"] = json!([
        {"id":"legacy-preset","name":"Old preset","models":profile.models,"instruction":profile.instruction,"default_strategy_enabled":true},
        {"id":"legacy-custom","name":"Custom","models":["custom-model"],"instruction":"Custom instruction"},
        {"id":"explicit-empty","name":"Already deselected","profile_ids":[],"models":profile.models,"instruction":profile.instruction}
    ]);
    config["keys"] = json!([
        {"id":"preset-key","name":"Old key","group_id":"legacy-preset"},
        {"id":"custom-key","name":"Custom key","group_id":"legacy-custom"},
        {"id":"empty-key","name":"Deselected key","group_id":"explicit-empty"},
        {"id":"explicit-key","name":"Explicitly off","group_id":"legacy-preset","profile_ids":[],"default_strategy_enabled":false}
    ]);
    let db = rusqlite::Connection::open(dir.path().join("zako.db")).unwrap();
    db.execute(
        "UPDATE config SET value=?1 WHERE id=1",
        [config.to_string()],
    )
    .unwrap();
    drop(db);
    drop(s);
    let s = new_app(dir.path(), dir.path().join("web")).unwrap();
    let (_, keys, _) = request(&s, "GET", "/api/keys", Value::Null, &cookie, "/").await;
    assert_eq!(keys[0]["profile_ids"], json!([profile.id]));
    assert_eq!(keys[0]["default_strategy_enabled"], true);
    assert_eq!(keys[1]["profile_ids"], json!([]));
    assert_eq!(keys[2]["profile_ids"], json!([]));
    assert_eq!(keys[3]["profile_ids"], json!([]));
    assert_eq!(keys[3]["default_strategy_enabled"], false);
    let (_, groups, _) = request(&s, "GET", "/api/groups", Value::Null, &cookie, "/").await;
    for group in groups.as_array().unwrap() {
        for obsolete in [
            "models",
            "instruction",
            "profile_ids",
            "default_strategy_enabled",
        ] {
            assert!(group.get(obsolete).is_none(), "{group}");
        }
        assert_eq!(group["description"], "");
    }
    let (status, fresh, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({"name":"Fresh key", "group_id":"legacy-preset"}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fresh}");
    let config = s.store.config().unwrap();
    let fresh = config
        .keys
        .iter()
        .find(|key| key.id == fresh["id"])
        .unwrap();
    assert!(fresh.profile_ids.is_empty());
    assert!(!fresh.default_strategy_enabled);
    let (status, cleared, _) = request(&s, "PUT", "/api/keys/preset-key",
        json!({"name":"Deselected", "group_id":"legacy-preset", "profile_ids":[], "default_strategy_enabled":false}), &cookie, "/").await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    drop(s);
    let s = new_app(dir.path(), dir.path().join("web")).unwrap();
    assert!(s.store.config().unwrap().keys[0].profile_ids.is_empty());
    assert!(!s.store.config().unwrap().keys[0].default_strategy_enabled);
    assert!(s.store.config().unwrap().keys[3].profile_ids.is_empty());
    assert!(!s.store.config().unwrap().keys[3].default_strategy_enabled);
    let db = rusqlite::Connection::open(dir.path().join("zako.db")).unwrap();
    let saved: String = db
        .query_row("SELECT value FROM config WHERE id=1", [], |row| row.get(0))
        .unwrap();
    let saved: Value = serde_json::from_str(&saved).unwrap();
    for group in saved["groups"].as_array().unwrap() {
        for obsolete in [
            "models",
            "instruction",
            "profile_ids",
            "default_strategy_enabled",
        ] {
            assert!(group.get(obsolete).is_none(), "{group}");
        }
    }
}

#[tokio::test]
async fn group_api_legacy_fields_never_activate_new_key_policies() {
    let (_dir, s) = fixture();
    let cookie = setup(&s).await;
    let profile = &profiles::all()[0];
    let (status, group, _) = request(
        &s,
        "POST",
        "/api/groups",
        json!({
            "name":"Legacy client", "models":profile.models, "instruction":profile.instruction,
            "profile_ids":[profile.id], "default_strategy_enabled":true
        }),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{group}");
    assert!(group.get("profile_ids").is_none());
    assert!(group.get("default_strategy_enabled").is_none());
    let (status, key, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({"name":"New key", "group_id":group["id"]}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{key}");
    let config = s.store.config().unwrap();
    assert!(config.keys[0].profile_ids.is_empty());
    assert!(!config.keys[0].default_strategy_enabled);
    assert!(config.groups[0].profile_ids.is_empty());
    assert!(!config.groups[0].default_strategy_enabled);
}

#[tokio::test]
async fn selected_profiles_route_both_protocols_without_cross_model_injection() {
    let (_dir, s) = fixture();
    let cookie = setup(&s).await;
    let echo = || async { axum::Json(json!({"unused":true})) };
    let router = Router::new()
        .route(
            "/v1/responses",
            post(|axum::Json(body): axum::Json<Value>| async move { axum::Json(body) }),
        )
        .route(
            "/v1/responses/compact",
            post(|axum::Json(body): axum::Json<Value>| async move { axum::Json(body) }),
        )
        .route("/v1/models", get(echo));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let (status, group, _) = request(
        &s,
        "POST",
        "/api/groups",
        json!({"name":"Multi models"}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{group}");
    let gid = group["id"].as_str().unwrap();
    let models = [
        "gpt-5.6",
        "gpt-5.6-sol",
        "gpt-6",
        "gpt-6-astra",
        "gpt-6-sol",
        "gpt-6.1",
        "gpt-6.1-sol",
        "gpt-6.10-sol",
        "gpt-60",
        "custom-model",
    ];
    let (status, account, _) = request(&s, "POST", "/api/accounts", json!({"name":"Echo","endpoint":endpoint,"api_key":"upstream-secret","models":models,"group_ids":[gid]}), &cookie, "/").await;
    assert_eq!(status, StatusCode::OK, "{account}");
    let profile_ids: Vec<_> = nonoverlapping_profile_ids()
        .into_iter()
        .filter(|id| {
            profiles::find(id)
                .unwrap()
                .models
                .iter()
                .any(|model| models.contains(&model.as_str()))
        })
        .collect();
    let (status, key, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({"name":"Integration","group_id":gid,"profile_ids":profile_ids}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{key}");
    let key_id = key["id"].as_str().unwrap();
    let key = key["key"].as_str().unwrap();
    for path in ["/v1/responses", "/v1/responses/compact"] {
        for model in models {
            let original = json!({"model":model,"instructions":"Original instructions","input":[{"role":"user","content":"hello"}],"tools":[{"type":"function","name":"tool"}],"metadata":{"check":true},"store":false});
            let (status, bytes) = proxy_request(&s, path, key, original.clone()).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "{path} {model}: {}",
                String::from_utf8_lossy(&bytes)
            );
            let mut output: Value = serde_json::from_slice(&bytes).unwrap();
            let expected = if let Some(profile) = profiles::all()
                .iter()
                .find(|profile| profile.models.iter().any(|selected| selected == model))
            {
                format!("{}\n\nOriginal instructions", profile.instruction)
            } else {
                "Original instructions".into()
            };
            assert_eq!(output["instructions"], expected, "{path} {model}");
            output["instructions"] = original["instructions"].clone();
            assert_eq!(output, original, "{path} {model}");
        }
    }
    let six = profiles::all()
        .iter()
        .find(|profile| profile.model_family == "gpt-6")
        .unwrap();
    let (status, updated, _) = request(
        &s,
        "PUT",
        &format!("/api/keys/{key_id}"),
        json!({"name":"Only 6","group_id":gid,"profile_ids":[six.id]}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    for path in ["/v1/responses", "/v1/responses/compact"] {
        for model in ["gpt-5.6-sol", "gpt-6.1-sol"] {
            let original = json!({"model":model,"input":"hello","instructions":"Untouched"});
            let (status, bytes) = proxy_request(&s, path, key, original.clone()).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), original);
        }
    }
    task.abort();
}

#[tokio::test]
async fn account_model_mappings_validate_and_persist_with_selected_targets() {
    let (dir, s) = fixture();
    let cookie = setup(&s).await;
    let invalid = [
        json!({"public":"unselected"}),
        json!({"":"actual"}),
        json!({" public":"actual"}),
        json!({"public ":"actual"}),
        json!({"public\nname":"actual"}),
        json!({"public":"actual "}),
        json!({"x".repeat(257):"actual"}),
        Value::Null,
        json!([]),
        json!({"public":null}),
        json!(
            (0..501)
                .map(|n| (format!("alias-{n}"), "actual"))
                .collect::<std::collections::BTreeMap<_, _>>()
        ),
    ];
    for mappings in invalid {
        let (status, body, _) = request(
            &s,
            "POST",
            "/api/accounts",
            json!({
                "name":"Invalid", "endpoint":"https://example.com", "api_key":"secret",
                "models":["actual"], "model_mappings":mappings
            }),
            &cookie,
            "/",
        )
        .await;
        assert!(status.is_client_error(), "{mappings}: {status} {body}");
    }
    for models in [json!([""]), json!([" actual"]), json!(["actual\n"])] {
        assert_eq!(request(&s, "POST", "/api/accounts", json!({
            "name":"Invalid", "endpoint":"https://example.com", "api_key":"secret", "models":models
        }), &cookie, "/").await.0, StatusCode::BAD_REQUEST);
    }
    let mappings = json!({"public":"actual", "second-alias":"actual"});
    let (status, created, _) = request(
        &s,
        "POST",
        "/api/accounts",
        json!({
            "name":"Mapped", "endpoint":"https://example.com", "api_key":"secret",
            "models":["actual"], "model_mappings":mappings
        }),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["model_mappings"], mappings);
    let id = created["id"].as_str().unwrap();
    assert_eq!(request(&s, "PUT", &format!("/api/accounts/{id}"), json!({
        "name":"Bad target edit", "endpoint":"https://example.com", "models":[], "model_mappings":mappings
    }), &cookie, "/").await.0, StatusCode::BAD_REQUEST);
    let replacement = json!({"replacement":"different"});
    let (status, updated, _) = request(
        &s,
        "PUT",
        &format!("/api/accounts/{id}"),
        json!({
            "name":"Updated mapping", "endpoint":"https://example.com",
            "models":["actual", "different"], "model_mappings":replacement
        }),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["model_mappings"], replacement);
    assert!(updated.get("api_key").is_none());
    let account = &s.store.config().unwrap().accounts[0];
    assert_eq!(s.store.unseal(&account.api_key).unwrap(), "secret");
    assert_eq!(account.actual_model("public"), None);
    assert_eq!(account.actual_model("replacement"), Some("different"));
    drop(s);
    let s = new_app(dir.path(), dir.path().join("web")).unwrap();
    let (status, accounts, _) =
        request(&s, "GET", "/api/accounts", Value::Null, &cookie, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accounts[0]["model_mappings"], replacement);
    assert_eq!(accounts[0]["models"], json!(["actual", "different"]));
    assert!(accounts[0].get("api_key").is_none());
    assert_eq!(
        s.store
            .unseal(&s.store.config().unwrap().accounts[0].api_key)
            .unwrap(),
        "secret"
    );
    let (status, cleared, _) = request(
        &s,
        "PUT",
        &format!("/api/accounts/{id}"),
        json!({
            "name":"Cleared mappings", "endpoint":"https://example.com", "api_key":"",
            "models":["actual", "different"], "model_mappings":{}
        }),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert_eq!(cleared["model_mappings"], json!({}));
    assert!(cleared.get("api_key").is_none());
    let account = &s.store.config().unwrap().accounts[0];
    assert!(account.model_mappings.is_empty());
    assert_eq!(account.actual_model("replacement"), None);
    assert_eq!(account.actual_model("different"), Some("different"));
    assert_eq!(s.store.unseal(&account.api_key).unwrap(), "secret");
    let mut config = serde_json::to_value(s.store.config().unwrap()).unwrap();
    config["accounts"][0]
        .as_object_mut()
        .unwrap()
        .remove("model_mappings");
    let db = rusqlite::Connection::open(dir.path().join("zako.db")).unwrap();
    db.execute(
        "UPDATE config SET value=?1 WHERE id=1",
        [config.to_string()],
    )
    .unwrap();
    assert!(
        s.store.config().unwrap().accounts[0]
            .model_mappings
            .is_empty()
    );
}

#[tokio::test]
async fn mapped_models_apply_actual_strategy_and_advertise_valid_aliases() {
    let (_dir, s) = fixture();
    let cookie = setup(&s).await;
    let router = Router::new()
        .route(
            "/v1/responses",
            post(|axum::Json(body): axum::Json<Value>| async move { axum::Json(body) }),
        )
        .route(
            "/v1/responses/compact",
            post(|axum::Json(body): axum::Json<Value>| async move { axum::Json(body) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let supported = ["gpt-5.6-sol", "gpt-6-astra", "gpt-6.1-sol", "plain"];
    let ids: Vec<_> = nonoverlapping_profile_ids()
        .into_iter()
        .filter(|id| {
            profiles::find(id)
                .unwrap()
                .models
                .iter()
                .any(|model| supported.contains(&model.as_str()))
        })
        .collect();
    let (status, group, _) = request(
        &s,
        "POST",
        "/api/groups",
        json!({"name":"Mapped strategies"}),
        &cookie,
        "/",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let gid = group["id"].as_str().unwrap();
    let (status, account, _) = request(&s, "POST", "/api/accounts", json!({
        "name":"Mapped upstream", "endpoint":endpoint, "api_key":"upstream-secret", "group_ids":[gid],
        "models":["gpt-5.6-sol", "gpt-6-astra", "gpt-6.1-sol", "plain"],
        "model_mappings":{"public":"gpt-5.6-sol", "gpt-6-astra":"gpt-6.1-sol", "chain-start":"gpt-6-astra", "plain-alias":"plain"}
    }), &cookie, "/").await;
    assert_eq!(status, StatusCode::OK, "{account}");
    let (_, key, _) = request(
        &s,
        "POST",
        "/api/keys",
        json!({"name":"Mapped key", "group_id":gid, "profile_ids":ids}),
        &cookie,
        "/",
    )
    .await;
    let key_id = key["id"].as_str().unwrap();
    let key = key["key"].as_str().unwrap();
    for path in ["/v1/responses", "/v1/responses/compact"] {
        for (requested, actual) in [
            ("public", "gpt-5.6-sol"),
            ("gpt-6-astra", "gpt-6.1-sol"),
            ("chain-start", "gpt-6-astra"),
            ("plain-alias", "plain"),
            ("gpt-5.6-sol", "gpt-5.6-sol"),
        ] {
            let original = json!({"model":requested, "instructions":"Original", "input":"hello", "metadata":{"preserved":true}, "tools":[], "store":false});
            let (status, bytes) = proxy_request(&s, path, key, original.clone()).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "{path} {requested}: {}",
                String::from_utf8_lossy(&bytes)
            );
            let output: Value = serde_json::from_slice(&bytes).unwrap();
            let mut expected = original;
            expected["model"] = json!(actual);
            if let Some(profile) = profiles::all()
                .iter()
                .find(|profile| profile.models.iter().any(|model| model == actual))
            {
                expected["instructions"] = json!(format!("{}\n\nOriginal", profile.instruction));
            }
            assert_eq!(output, expected, "{path} {requested} -> {actual}");
        }
    }
    let six = profiles::all()
        .iter()
        .find(|profile| profile.models.iter().any(|model| model == "gpt-6-astra"))
        .unwrap();
    assert_eq!(
        request(
            &s,
            "PUT",
            &format!("/api/keys/{key_id}"),
            json!({"name":"Only six", "group_id":gid, "profile_ids":[six.id]}),
            &cookie,
            "/"
        )
        .await
        .0,
        StatusCode::OK
    );
    for path in ["/v1/responses", "/v1/responses/compact"] {
        let (status, bytes) = proxy_request(
            &s,
            path,
            key,
            json!({"model":"gpt-6-astra", "instructions":"Keep me", "input":"hello"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"model":"gpt-6.1-sol", "instructions":"Keep me", "input":"hello"})
        );
    }
    s.store
        .update(|config| {
            let source = config.accounts[0].clone();
            let mut disabled = source.clone();
            disabled.id = "disabled".into();
            disabled.enabled = false;
            disabled.models = vec!["disabled-actual".into()];
            disabled.model_mappings = std::collections::BTreeMap::from([(
                "disabled-alias".into(),
                "disabled-actual".into(),
            )]);
            config.accounts.push(disabled);
            let mut outside = source;
            outside.id = "outside".into();
            outside.group_ids.clear();
            outside.models = vec!["outside-actual".into()];
            outside.model_mappings = std::collections::BTreeMap::from([(
                "outside-alias".into(),
                "outside-actual".into(),
            )]);
            config.accounts.push(outside);
            config.accounts[0]
                .model_mappings
                .insert("invalid-alias".into(), "unselected".into());
            config.accounts[0]
                .model_mappings
                .insert("plain".into(), "unselected".into());
            Ok(())
        })
        .unwrap();
    let response = app(s.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("authorization", format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let models: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let names: std::collections::BTreeSet<_> = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from([
            "gpt-5.6-sol",
            "gpt-6-astra",
            "gpt-6.1-sol",
            "public",
            "chain-start",
            "plain-alias"
        ])
    );
    let (status, _) = proxy_request(
        &s,
        "/v1/responses",
        key,
        json!({"model":"invalid-alias", "input":"hello"}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _) = proxy_request(
        &s,
        "/v1/responses",
        key,
        json!({"model":"plain", "input":"hello"}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    task.abort();
}
