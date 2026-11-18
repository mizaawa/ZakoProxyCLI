use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use tower::ServiceExt;
use zakoai_proxy::{
    Shared, app, new_app,
    store::{Account, ApiKey, RequestLog, Settings, Store, now, usage},
};

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    state.store.session("logs-session").unwrap();
    state
        .store
        .update(|config| {
            config.settings.security_path = "console".into();
            config.keys.push(ApiKey {
                id: "key".into(),
                name: "Development".into(),
                ..Default::default()
            });
            config.accounts.push(Account {
                id: "account".into(),
                name: "Upstream".into(),
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
    (dir, state)
}

fn seed(state: &Shared, count: u32) {
    let timestamp = now();
    for index in 0..count {
        state
            .store
            .log(
                &RequestLog {
                    id: format!("request-{index:03}"),
                    timestamp,
                    key_id: "key".into(),
                    account_id: "account".into(),
                    model: "test-model".into(),
                    protocol: "responses".into(),
                    request_path: "/v1/responses".into(),
                    status: if index == count - 1 { 502 } else { 200 },
                    cyber: index == count - 1,
                    error: (index == count - 1).then(|| "upstream quota exceeded".into()),
                    first_token_ms: Some(18),
                    ..Default::default()
                },
                7,
            )
            .unwrap();
    }
}

async fn request(state: &Shared, method: &str, path: &str) -> Response {
    app(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("cookie", "zako_session=logs-session")
                .header("x-console-path", "/console")
                .header("host", "localhost:1990")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn json_body(response: Response) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn insert_raw(dir: &std::path::Path, entry: &Value) {
    Connection::open(dir.join("zako.db"))
        .unwrap()
        .execute(
            "INSERT INTO requests(id,timestamp,value) VALUES(?1,?2,?3)",
            params![
                entry["id"].as_str().unwrap(),
                entry["timestamp"].as_i64().unwrap(),
                entry.to_string()
            ],
        )
        .unwrap();
}

fn legacy_entry(id: &str, timestamp: i64) -> Value {
    json!({
        "id":id,"timestamp":timestamp,"key_id":"deleted-key","account_id":"deleted-account",
        "model":"legacy","protocol":"responses","status":200,"latency_ms":12,
        "usage":{"input_tokens":2,"output_tokens":1,"cached_tokens":0},"cyber":false,"attempts":1,
    })
}

#[tokio::test]
async fn logs_paginate_twenty_at_a_time_with_stable_ties_and_clamp_page_numbers() {
    let (_dir, state) = fixture();
    seed(&state, 45);
    let first = json_body(request(&state, "GET", "/api/logs").await).await;
    assert_eq!(first["page"], 1);
    assert_eq!(first["page_size"], 20);
    assert_eq!(first["total"], 45);
    assert_eq!(first["total_pages"], 3);
    assert_eq!(first["retention_days"], 7);
    assert_eq!(first["items"].as_array().unwrap().len(), 20);
    assert_eq!(first["items"][0]["id"], "request-044");
    assert_eq!(first["items"][19]["id"], "request-025");
    assert_eq!(first["items"][0]["key_name"], "Development");
    assert_eq!(first["items"][0]["account_name"], "Upstream");
    assert_eq!(first["items"][0]["status"], 502);
    assert_eq!(first["items"][0]["first_token_ms"], 18);
    assert_eq!(first["items"][0]["request_path"], "/v1/responses");
    assert_eq!(first["items"][0]["cyber"], true);
    assert_eq!(first["items"][0]["error"], "upstream quota exceeded");
    let second = json_body(request(&state, "GET", "/api/logs?page=2").await).await;
    assert_eq!(second["items"].as_array().unwrap().len(), 20);
    assert_eq!(second["items"][0]["id"], "request-024");
    assert_eq!(second["items"][19]["id"], "request-005");
    let last = json_body(request(&state, "GET", "/api/logs?page=18446744073709551615").await).await;
    assert_eq!(last["page"], 3);
    assert_eq!(last["items"].as_array().unwrap().len(), 5);
    assert_eq!(last["items"][0]["id"], "request-004");
    assert_eq!(last["items"][4]["id"], "request-000");
    let zero = json_body(request(&state, "GET", "/api/logs?page=0").await).await;
    assert_eq!(zero["page"], 1);
}

#[tokio::test]
async fn export_includes_all_retained_logs_and_delete_clears_every_page() {
    let (_dir, state) = fixture();
    seed(&state, 45);
    let response = request(&state, "GET", "/api/logs/export").await;
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.headers()["content-disposition"],
        "attachment; filename=\"request-logs.json\""
    );
    let exported = json_body(response).await;
    assert_eq!(exported["retention_days"], 7);
    assert!(exported["exported_at"].as_i64().unwrap() > 0);
    assert_eq!(exported["logs"].as_array().unwrap().len(), 45);
    assert_eq!(exported["logs"][0]["id"], "request-044");
    assert_eq!(exported["logs"][44]["id"], "request-000");
    assert_eq!(
        json_body(request(&state, "DELETE", "/api/logs").await).await["deleted"],
        45
    );
    let empty = json_body(request(&state, "GET", "/api/logs?page=9").await).await;
    assert_eq!(empty["page"], 1);
    assert_eq!(empty["total_pages"], 1);
    assert_eq!(empty["total"], 0);
    assert!(empty["items"].as_array().unwrap().is_empty());
    assert_eq!(
        json_body(request(&state, "DELETE", "/api/logs").await).await["deleted"],
        0
    );
}

#[tokio::test]
async fn log_routes_require_session_console_entry_and_same_origin() {
    let (_dir, state) = fixture();
    seed(&state, 1);
    for (method, path) in [
        ("GET", "/api/logs"),
        ("GET", "/api/logs/export"),
        ("DELETE", "/api/logs"),
    ] {
        for (cookie, console, origin, expected) in [
            (
                "",
                "/console",
                "http://localhost:1990",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "zako_session=logs-session",
                "/wrong",
                "http://localhost:1990",
                StatusCode::NOT_FOUND,
            ),
            (
                "zako_session=logs-session",
                "/console",
                "https://attacker.invalid",
                StatusCode::FORBIDDEN,
            ),
        ] {
            let response = app(state.clone())
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("cookie", cookie)
                        .header("x-console-path", console)
                        .header("host", "localhost:1990")
                        .header("origin", origin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{method} {path}");
        }
    }
    assert_eq!(state.store.logs_page(1).unwrap().total, 1);
}

#[tokio::test]
async fn default_retention_prunes_on_idle_read_and_legacy_fields_default() {
    let (dir, state) = fixture();
    assert_eq!(Settings::default().retention_days, 7);
    insert_raw(dir.path(), &legacy_entry("expired", now() - 8 * 86400));
    insert_raw(dir.path(), &legacy_entry("retained", now() - 6 * 86400));
    let logs = json_body(request(&state, "GET", "/api/logs").await).await;
    assert_eq!(logs["total"], 1);
    assert_eq!(logs["items"][0]["id"], "retained");
    assert_eq!(logs["items"][0]["request_path"], "");
    assert!(logs["items"][0]["first_token_ms"].is_null());
    assert_eq!(logs["items"][0]["usage"]["cache_creation_input_tokens"], 0);
    assert_eq!(logs["items"][0]["usage"]["cache_read_input_tokens"], 0);
    assert!(logs["items"][0]["error"].is_null());
    assert!(logs["items"][0]["key_name"].is_null());
    assert!(logs["items"][0]["account_name"].is_null());
    let count: u64 = Connection::open(dir.path().join("zako.db"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    insert_raw(
        dir.path(),
        &legacy_entry("expired-export", now() - 8 * 86400),
    );
    let exported = json_body(request(&state, "GET", "/api/logs/export").await).await;
    assert_eq!(exported["logs"].as_array().unwrap().len(), 1);
    insert_raw(
        dir.path(),
        &legacy_entry("expired-stats", now() - 8 * 86400),
    );
    assert_eq!(state.store.logs(0, now()).unwrap().len(), 1);
}

#[test]
fn retention_prunes_at_startup_and_write_and_respects_saved_setting() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .update(|config| {
            config.settings.retention_days = 90;
            Ok(())
        })
        .unwrap();
    let old = RequestLog {
        id: "old".into(),
        timestamp: now() - 30 * 86400,
        ..Default::default()
    };
    store.log(&old, 90).unwrap();
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.config().unwrap().settings.retention_days, 90);
    assert_eq!(store.logs_page(1).unwrap().total, 1);
    store
        .update(|config| {
            config.settings.retention_days = 7;
            Ok(())
        })
        .unwrap();
    assert_eq!(store.logs_page(1).unwrap().total, 0);
    store.log(&old, 7).unwrap();
    assert_eq!(store.logs_page(1).unwrap().total, 0);
    drop(store);
    insert_raw(
        dir.path(),
        &legacy_entry("expired-startup", now() - 8 * 86400),
    );
    let store = Store::open(dir.path()).unwrap();
    let count: u64 = Connection::open(dir.path().join("zako.db"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(store.logs_page(1).unwrap().total, 0);
}

#[test]
fn cache_usage_keeps_read_creation_and_input_totals_distinct() {
    let anthropic = usage(&json!({"usage":{
        "input_tokens":20,"output_tokens":7,
        "cache_creation_input_tokens":30,"cache_read_input_tokens":50
    }}));
    assert_eq!(anthropic.input_tokens, 100);
    assert_eq!(anthropic.output_tokens, 7);
    assert_eq!(anthropic.cache_creation_input_tokens, 30);
    assert_eq!(anthropic.cache_read_input_tokens, 50);
    assert_eq!(anthropic.cached_tokens, 50);
    let responses = usage(&json!({"response":{"usage":{
        "input_tokens":100,"output_tokens":7,
        "input_tokens_details":{"cached_tokens":50,"cache_write_tokens":30}
    }}}));
    assert_eq!(responses.input_tokens, 100);
    assert_eq!(responses.cache_creation_input_tokens, 30);
    assert_eq!(responses.cache_read_input_tokens, 50);
    assert_eq!(responses.cached_tokens, 50);
    let chat = usage(&json!({"usage":{
        "prompt_tokens":100,"completion_tokens":7,
        "prompt_tokens_details":{"cached_tokens":50}
    }}));
    assert_eq!(chat.input_tokens, 100);
    assert_eq!(chat.cache_creation_input_tokens, 0);
    assert_eq!(chat.cache_read_input_tokens, 50);
    let legacy = usage(&json!({"input_tokens":100,"cached_tokens":25}));
    assert_eq!(legacy.cache_read_input_tokens, 25);
    assert_eq!(legacy.cached_tokens, 25);
    assert_eq!(legacy.cache_creation_input_tokens, 0);
    let explicit_zero = usage(&json!({"input_tokens":100,"cache_read_input_tokens":0,
        "input_tokens_details":{"cached_tokens":50}}));
    assert_eq!(explicit_zero.cache_read_input_tokens, 0);
}

#[tokio::test]
async fn cache_metrics_aggregate_without_counting_creation_as_cache_hits() {
    let (_dir, state) = fixture();
    let timestamp = now();
    for (id, usage_value) in [
        (
            "anthropic",
            json!({"input_tokens":20,"output_tokens":7,
            "cache_creation_input_tokens":30,"cache_read_input_tokens":50}),
        ),
        (
            "responses",
            json!({"input_tokens":200,"output_tokens":3,
            "input_tokens_details":{"cached_tokens":150,"cache_creation_tokens":10}}),
        ),
    ] {
        state
            .store
            .log(
                &RequestLog {
                    id: id.into(),
                    timestamp,
                    key_id: "key".into(),
                    status: 200,
                    first_token_ms: Some(17),
                    usage: usage(&usage_value),
                    ..Default::default()
                },
                7,
            )
            .unwrap();
    }
    let keys = json_body(request(&state, "GET", "/api/keys").await).await;
    let light = json_body(request(&state, "GET", "/api/keys?usage=false").await).await;
    let mut expected = keys.clone();
    expected[0].as_object_mut().unwrap().remove("usage");
    assert_eq!(light, expected);
    assert_eq!(keys[0]["usage"]["input_tokens"], 300);
    assert_eq!(keys[0]["usage"]["output_tokens"], 10);
    assert_eq!(keys[0]["usage"]["cached_tokens"], 200);
    assert_eq!(keys[0]["usage"]["cache_creation_input_tokens"], 40);
    assert_eq!(keys[0]["usage"]["cache_read_input_tokens"], 200);
    let stats = json_body(request(&state, "GET", "/api/stats").await).await;
    let overview = json_body(request(&state, "GET", "/api/stats?recent=false").await).await;
    assert_eq!(
        overview["summary"]["total_tokens"],
        stats["summary"]["total_tokens"]
    );
    assert_eq!(overview["daily"], stats["daily"]);
    assert!(overview["recent"].as_array().unwrap().is_empty());
    assert!(!stats["recent"].as_array().unwrap().is_empty());
    assert_eq!(stats["summary"]["input_tokens"], 300);
    assert_eq!(stats["summary"]["total_tokens"], 310);
    assert_eq!(stats["summary"]["uncached_tokens"], 100);
    assert_eq!(stats["summary"]["cached_tokens"], 200);
    assert_eq!(stats["summary"]["cache_creation_input_tokens"], 40);
    assert_eq!(stats["summary"]["cache_read_input_tokens"], 200);
    assert!((stats["summary"]["cache_hit_rate"].as_f64().unwrap() - 200.0 / 3.0).abs() < 0.0001);
    assert_eq!(stats["daily"][0]["input_tokens"], 300);
    assert_eq!(stats["daily"][0]["cache_creation_input_tokens"], 40);
    assert_eq!(stats["daily"][0]["cache_read_input_tokens"], 200);
    let logs = json_body(request(&state, "GET", "/api/logs").await).await;
    assert_eq!(logs["items"][0]["first_token_ms"], 17);
}

#[tokio::test]
async fn historical_cached_counts_populate_new_read_metrics_and_keep_explicit_zero() {
    let (dir, state) = fixture();
    let mut historical = legacy_entry("historical-cache", now());
    historical["key_id"] = json!("key");
    historical["usage"] = json!({"input_tokens":100,"output_tokens":5,"cached_tokens":60});
    insert_raw(dir.path(), &historical);
    let logs = json_body(request(&state, "GET", "/api/logs").await).await;
    assert_eq!(logs["items"][0]["usage"]["cached_tokens"], 60);
    assert_eq!(logs["items"][0]["usage"]["cache_read_input_tokens"], 60);
    assert_eq!(logs["items"][0]["usage"]["cache_creation_input_tokens"], 0);
    let keys = json_body(request(&state, "GET", "/api/keys").await).await;
    assert_eq!(keys[0]["usage"]["cache_read_input_tokens"], 60);
    let stats = json_body(request(&state, "GET", "/api/stats").await).await;
    assert_eq!(stats["summary"]["cache_read_input_tokens"], 60);
    assert_eq!(stats["summary"]["cache_hit_rate"], 60.0);
    let explicit: RequestLog = serde_json::from_value({
        historical["usage"]["cache_read_input_tokens"] = json!(0);
        historical
    })
    .unwrap();
    assert_eq!(explicit.usage.cache_read_input_tokens, 0);
}

#[tokio::test]
async fn stats_count_cyber_events_beyond_one_thousand() {
    let (dir, state) = fixture();
    let timestamp = now();
    let db = Connection::open(dir.path().join("zako.db")).unwrap();
    let tx = db.unchecked_transaction().unwrap();
    {
        let mut insert = tx
            .prepare("INSERT INTO requests(id,timestamp,value) VALUES(?1,?2,?3)")
            .unwrap();
        for index in 0..1_105 {
            let entry = RequestLog {
                id: format!("cyber-{index}"),
                timestamp,
                status: 200,
                cyber: true,
                ..Default::default()
            };
            let id = entry.id.clone();
            let value = serde_json::to_string(&entry).unwrap();
            insert.execute(params![id, timestamp, value]).unwrap();
        }
    }
    tx.commit().unwrap();

    let stats = json_body(request(&state, "GET", "/api/stats").await).await;
    assert_eq!(stats["summary"]["cyber_count"], 1_105);
    assert_eq!(stats["summary"]["total_requests"], 1_105);
    assert_eq!(stats["daily"][0]["cyber_count"], 1_105);

    state
        .store
        .log(
            &RequestLog {
                id: "next-cyber".into(),
                timestamp,
                status: 200,
                cyber: true,
                ..Default::default()
            },
            7,
        )
        .unwrap();
    let next = json_body(request(&state, "GET", "/api/stats").await).await;
    assert_eq!(next["summary"]["cyber_count"], 1_106);
    assert_eq!(next["daily"][0]["cyber_count"], 1_106);
}
