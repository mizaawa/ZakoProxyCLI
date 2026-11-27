use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use zakoai_proxy::{
    Shared, app, new_app, profiles,
    store::{Account, Group, ProviderBinding},
};

async fn request(
    state: &Shared,
    method: &str,
    path: &str,
    input: Value,
    cookie: &str,
) -> (StatusCode, Value, String) {
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("x-console-path", "/")
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(if input.is_null() {
                    Body::empty()
                } else {
                    Body::from(input.to_string())
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
        .unwrap()
        .to_owned();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let body =
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)));
    (status, body, cookie)
}

async fn ok(state: &Shared, method: &str, path: &str, input: Value, cookie: &str) -> Value {
    let (status, body, _) = request(state, method, path, input, cookie).await;
    assert_eq!(status, StatusCode::OK, "{method} {path}: {body}");
    body
}

async fn fixture() -> (tempfile::TempDir, Shared, String) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    let (status, _, cookie) = request(
        &state,
        "POST",
        "/api/setup",
        json!({"username":"admin","password":"x"}),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    state
        .store
        .update(|config| {
            config.groups = ["main", "other"]
                .map(|id| Group {
                    id: id.into(),
                    name: id.into(),
                    ..Default::default()
                })
                .into();
            let account = Account {
                id: "enabled".into(),
                name: "enabled".into(),
                enabled: true,
                group_ids: vec!["main".into()],
                models: [
                    "gpt-6.2-custom",
                    "claude-custom",
                    "glm-5.3",
                    "deepseek-4.1-flash",
                    "deepseek-v4.1-flash",
                    "private-model",
                    "unknown",
                ]
                .map(str::to_owned)
                .into(),
                model_mappings: [("public-alias".into(), "gpt-6.2-custom".into())].into(),
                ..Default::default()
            };
            config.accounts = vec![
                account.clone(),
                Account {
                    id: "duplicate".into(),
                    ..account.clone()
                },
                Account {
                    id: "disabled".into(),
                    enabled: false,
                    models: vec!["gpt-disabled".into()],
                    ..account.clone()
                },
                Account {
                    id: "other".into(),
                    group_ids: vec!["other".into()],
                    models: vec!["grok-other".into()],
                    ..account.clone()
                },
                Account {
                    id: "unavailable".into(),
                    models: vec!["gpt-unavailable".into()],
                    provider_binding: Some(ProviderBinding {
                        status: "deleted".into(),
                        ..Default::default()
                    }),
                    ..account
                },
            ];
            Ok(())
        })
        .unwrap();
    (dir, state, cookie)
}

#[tokio::test]
async fn model_options_are_authenticated_distinct_and_scoped_to_enabled_actual_models() {
    let (_dir, state, cookie) = fixture().await;
    let path = "/api/strategy-models?group_id=main";
    assert_eq!(
        request(&state, "GET", path, Value::Null, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    let rows = ok(&state, "GET", path, Value::Null, &cookie).await;
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 7);
    assert!(rows.iter().all(|row| row["selected"] == false));
    for model in [
        "public-alias",
        "gpt-disabled",
        "gpt-unavailable",
        "grok-other",
    ] {
        assert!(!rows.iter().any(|row| row["model"] == model));
    }
    assert!(rows.iter().find(|r| r["model"] == "unknown").unwrap()["profile_id"].is_null());
    for (model, source) in [
        ("gpt-6.2-custom", "冷咖啡"),
        ("glm-5.3", "冷咖啡"),
        ("deepseek-4.1-flash", "寒霜 / Hanshuang"),
    ] {
        assert_eq!(
            rows.iter().find(|r| r["model"] == model).unwrap()["profile_source"],
            source
        );
    }
    assert!(
        rows.iter()
            .find(|r| r["model"] == "gpt-6.2-custom")
            .unwrap()["profile_id"]
            .as_str()
            .unwrap()
            .starts_with("coldcoffee@codex@")
    );
    for path in [
        "/api/strategy-models?group_id=missing",
        "/api/strategy-models?group_id=main&key_id=missing",
    ] {
        assert_eq!(
            request(&state, "GET", path, Value::Null, &cookie).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn model_selection_round_trips_and_deselection_is_per_key() {
    let (dir, state, cookie) = fixture().await;
    let first = ok(&state, "POST", "/api/keys", json!({"name":"first","group_id":"main", "strategy_models":["gpt-6.2-custom", "claude-custom", "gpt-6.2-custom"]}), &cookie).await;
    let second = ok(
        &state,
        "POST",
        "/api/keys",
        json!({"name":"second","group_id":"main"}),
        &cookie,
    )
    .await;
    let first_id = first["id"].as_str().unwrap();
    let second_id = second["id"].as_str().unwrap();
    let snapshot = state.store.config().unwrap();
    assert_eq!(snapshot.keys[0].profile_ids.len(), 2);
    assert!(snapshot.keys[1].profile_ids.is_empty());
    assert!(!snapshot.keys[1].default_strategy_enabled);
    let rows = ok(
        &state,
        "GET",
        &format!("/api/strategy-models?group_id=main&key_id={first_id}"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .filter(|row| row["selected"] == true)
            .count(),
        2
    );
    ok(&state, "PUT", &format!("/api/keys/{first_id}"), json!({"name":"first","group_id":"main","strategy_models":[],"default_strategy_enabled":true}), &cookie).await;
    let snapshot = state.store.config().unwrap();
    assert!(snapshot.keys[0].profile_ids.is_empty());
    assert!(snapshot.keys[0].default_strategy_enabled);
    assert!(
        snapshot
            .keys
            .iter()
            .find(|key| key.id == second_id)
            .unwrap()
            .profile_ids
            .is_empty()
    );
    drop(state);
    let reopened = new_app(dir.path(), dir.path().join("web")).unwrap();
    let persisted = reopened.store.config().unwrap();
    assert!(persisted.keys[0].profile_ids.is_empty());
    assert!(persisted.keys[0].default_strategy_enabled);
}

#[tokio::test]
async fn saving_existing_models_preserves_sources_but_splits_aliases() {
    let (_dir, state, cookie) = fixture().await;
    let custom = format!("coldcoffee@workbuddy@{}", hex::encode("private-model"));
    let created = ok(&state, "POST", "/api/keys", json!({"name":"legacy","group_id":"main", "profile_ids":["aijailbreak-glm-5.3-v1", "coldcoffee-deepseek-v4.1-flash-v1", custom]}), &cookie).await;
    let id = created["id"].as_str().unwrap();
    let path = format!("/api/keys/{id}");
    let options = ok(
        &state,
        "GET",
        &format!("/api/strategy-models?group_id=main&key_id={id}"),
        Value::Null,
        &cookie,
    )
    .await;
    assert_eq!(
        options
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["selected"] == true)
            .count(),
        4
    );
    let input = json!({"name":"legacy","group_id":"main", "strategy_models":["glm-5.3", "deepseek-4.1-flash", "private-model"]});
    for (model, source) in [("glm-5.3", "AI-Jailbreak"), ("private-model", "冷咖啡")] {
        assert_eq!(
            options
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["model"] == model)
                .unwrap()["profile_source"],
            source
        );
    }
    ok(&state, "PUT", &path, input.clone(), &cookie).await;
    let selected = state.store.config().unwrap().keys[0].profile_ids.clone();
    assert!(selected.contains(&"aijailbreak-glm-5.3-v1".into()));
    assert!(selected.contains(&custom));
    let group = Group {
        profile_ids: selected.clone(),
        ..Default::default()
    };
    assert!(
        profiles::instruction(&group, "deepseek-4.1-flash")
            .unwrap()
            .contains("Hanshuang")
    );
    assert!(profiles::instruction(&group, "deepseek-v4.1-flash").is_none());
    ok(&state, "PUT", &path, input, &cookie).await;
    assert_eq!(state.store.config().unwrap().keys[0].profile_ids, selected);
    ok(
        &state,
        "PUT",
        &path,
        json!({"name":"moved","group_id":"other","strategy_models":["grok-other"]}),
        &cookie,
    )
    .await;
    let group = Group {
        profile_ids: state.store.config().unwrap().keys[0].profile_ids.clone(),
        ..Default::default()
    };
    assert!(profiles::instruction(&group, "grok-other").is_some());
    assert!(profiles::instruction(&group, "glm-5.3").is_none());
}

#[tokio::test]
async fn invalid_selections_fail_without_mutation_or_implicit_fallback() {
    let (_dir, state, cookie) = fixture().await;
    let created = ok(
        &state,
        "POST",
        "/api/keys",
        json!({"name":"test","group_id":"main", "strategy_models":["claude-custom"]}),
        &cookie,
    )
    .await;
    let before = state.store.config().unwrap().keys[0].profile_ids.clone();
    let path = format!("/api/keys/{}", created["id"].as_str().unwrap());
    for model in [
        "unknown",
        "public-alias",
        "gpt-disabled",
        "grok-other",
        "gpt-unavailable",
    ] {
        let (status, _, _) = request(
            &state,
            "PUT",
            &path,
            json!({"name":"test","group_id":"main", "strategy_models":[model]}),
            &cookie,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{model}");
        assert_eq!(state.store.config().unwrap().keys[0].profile_ids, before);
    }
    for input in [
        json!({"name":"test","group_id":"main","strategy_models":[],"profile_ids":[]}),
        json!({"name":"test","group_id":"missing","strategy_models":[]}),
    ] {
        assert_eq!(
            request(&state, "PUT", &path, input, &cookie).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(
            &state,
            "PUT",
            &path,
            json!({"name":"test","group_id":"main","strategy_models":null}),
            &cookie
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    ok(
        &state,
        "PUT",
        &path,
        json!({"name":"renamed","group_id":"main"}),
        &cookie,
    )
    .await;
    assert_eq!(state.store.config().unwrap().keys[0].profile_ids, before);
}
