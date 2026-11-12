use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
    routing::post,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;
use zakoai_proxy::{
    Shared, app, new_app, profiles,
    store::{self, Account, ApiKey, Group},
};

const PROXY_KEY: &str = "sk-zako-multimodel-integration";
const UPSTREAM_KEY: &str = "multimodel-upstream-secret";

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    state
        .store
        .update(|config| {
            config.groups.push(Group {
                id: "multi-group".into(),
                name: "All model profiles".into(),
                profile_ids: Vec::new(),
                default_strategy_enabled: false,
                description: String::new(),
            });
            config.keys.push(ApiKey {
                id: "multi-key".into(),
                name: "Integration".into(),
                group_id: "multi-group".into(),
                hash: store::digest(PROXY_KEY),
                enabled: true,
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
    (dir, state)
}

fn add_account(
    state: &Shared,
    endpoint: &str,
    id: &str,
    models: Vec<String>,
    mappings: BTreeMap<String, String>,
    protocols: &[&str],
    priority: i32,
) {
    let secret = state.store.seal(UPSTREAM_KEY).unwrap();
    state
        .store
        .update(|config| {
            config.accounts.push(Account {
                id: id.into(),
                name: id.into(),
                endpoint: endpoint.into(),
                api_key: secret,
                models,
                model_mappings: mappings,
                group_ids: vec!["multi-group".into()],
                protocols: protocols
                    .iter()
                    .map(|protocol| (*protocol).into())
                    .collect(),
                enabled: true,
                priority,
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
}

fn select_profiles(state: &Shared, ids: Vec<String>) {
    state
        .store
        .update(|config| {
            config.keys[0].profile_ids = ids;
            Ok(())
        })
        .unwrap();
}

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn serve(router: Router) -> (String, Server) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (endpoint, Server(task))
}

fn path(protocol: &str) -> &'static str {
    match protocol {
        "responses" => "/v1/responses",
        "compact" => "/v1/responses/compact",
        "chat" => "/v1/chat/completions",
        "anthropic" => "/v1/messages",
        _ => unreachable!(),
    }
}

async fn request(state: &Shared, protocol: &str, body: Value) -> axum::response::Response {
    app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path(protocol))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {PROXY_KEY}"))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn json_response(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn echo(Json(body): Json<Value>) -> Json<Value> {
    Json(body)
}

fn original_body(protocol: &str, model: &str) -> Value {
    match protocol {
        "responses" | "compact" => json!({
            "model":model,
            "instructions":"Client instructions remain intact.",
            "input":[{"role":"user", "content":[
                {"type":"input_text", "text":"Describe this image"},
                {"type":"input_image", "image_url":"https://example.test/photo.png"}
            ]}],
            "tools":[{"type":"function", "name":"lookup", "parameters":{"type":"object"}}],
            "store":false,
            "metadata":{"preserved":true}
        }),
        "chat" => json!({
            "model":model,
            "messages":[
                {"role":"system", "content":"Client system remains intact."},
                {"role":"developer", "content":"Client developer instruction remains intact."},
                {"role":"user", "content":[
                    {"type":"text", "text":"Describe this image"},
                    {"type":"image_url", "image_url":{"url":"https://example.test/photo.png"}}
                ]},
                {"role":"assistant", "content":null, "tool_calls":[{"id":"call-1", "type":"function", "function":{"name":"lookup", "arguments":"{}"}}]},
                {"role":"tool", "tool_call_id":"call-1", "content":"Earlier tool result"}
            ],
            "tools":[{"type":"function", "function":{"name":"lookup", "parameters":{"type":"object"}}}],
            "tool_choice":"auto",
            "stream":false
        }),
        "anthropic" => json!({
            "model":model,
            "max_tokens":512,
            "system":[
                {"type":"text", "text":"Client system remains intact."},
                {"type":"text", "text":"Second system block", "cache_control":{"type":"ephemeral"}}
            ],
            "messages":[{"role":"user", "content":[
                {"type":"text", "text":"Describe this image"},
                {"type":"image", "source":{"type":"url", "url":"https://example.test/photo.png"}}
            ]}],
            "tools":[{"name":"lookup", "input_schema":{"type":"object"}}],
            "metadata":{"preserved":true}
        }),
        _ => unreachable!(),
    }
}

fn expected_body(
    protocol: &str,
    mut body: Value,
    actual_model: &str,
    profile: Option<&profiles::Profile>,
) -> Value {
    body["model"] = json!(actual_model);
    let Some(profile) = profile else {
        return body;
    };
    match protocol {
        "responses" | "compact" => {
            let existing = body["instructions"].as_str().unwrap().to_owned();
            body["instructions"] = json!(format!("{}\n\n{existing}", profile.instruction));
        }
        "chat" => body["messages"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"role":"system", "content":profile.instruction})),
        "anthropic" => body["system"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"type":"text", "text":profile.instruction})),
        _ => unreachable!(),
    }
    body
}

fn logs(state: &Shared) -> Vec<store::RequestLog> {
    state.store.logs(0, i64::MAX).unwrap()
}

#[tokio::test]
async fn all_catalog_models_inject_native_instructions_without_mutating_client_context() {
    let (_dir, state) = fixture();
    let catalog = profiles::all();
    assert_eq!(
        catalog.len(),
        42,
        "expected three GPT, 27 Coldcoffee, two Hanshuang, four Kimi, and six AI-Jailbreak profiles"
    );
    assert_eq!(
        catalog
            .iter()
            .map(|profile| profile.models.len())
            .sum::<usize>(),
        43
    );
    assert_eq!(
        catalog
            .iter()
            .flat_map(|profile| &profile.models)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        41
    );

    let captured = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let mut router = Router::new();
    for protocol in ["responses", "compact", "chat", "anthropic"] {
        let records = captured.clone();
        let route_protocol = protocol.to_owned();
        router = router.route(
            path(protocol),
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let records = records.clone();
                let route_protocol = route_protocol.clone();
                async move {
                    if route_protocol == "anthropic" {
                        assert_eq!(headers["x-api-key"], UPSTREAM_KEY);
                        assert_eq!(headers["anthropic-version"], "2023-06-01");
                    } else {
                        assert_eq!(headers["authorization"], format!("Bearer {UPSTREAM_KEY}"));
                    }
                    records.lock().unwrap().push((route_protocol, body.clone()));
                    Json(body)
                }
            }),
        );
    }
    let (endpoint, _server) = serve(router).await;
    let model_ids: Vec<_> = catalog
        .iter()
        .flat_map(|profile| profile.models.iter().cloned())
        .collect();
    add_account(
        &state,
        &endpoint,
        "all-models",
        model_ids.clone(),
        BTreeMap::new(),
        &["responses", "compact", "chat", "anthropic"],
        0,
    );

    for profile in catalog {
        select_profiles(&state, vec![profile.id.clone()]);
        for actual_model in &profile.models {
            for protocol in ["responses", "compact", "chat", "anthropic"] {
                let body = original_body(protocol, actual_model);
                let (status, output) =
                    json_response(request(&state, protocol, body.clone()).await).await;
                assert_eq!(
                    status,
                    StatusCode::OK,
                    "{protocol} {actual_model}: {output}"
                );
                assert_eq!(
                    output,
                    expected_body(protocol, body, actual_model, Some(profile)),
                    "{protocol} {actual_model}"
                );
            }
        }
    }
    assert_eq!(captured.lock().unwrap().len(), model_ids.len() * 4);
    assert_eq!(logs(&state).len(), model_ids.len() * 4);
}

#[tokio::test]
async fn every_coldcoffee_template_binds_exact_actual_models_on_all_protocols() {
    let (_dir, state) = fixture();
    let mut router = Router::new();
    for protocol in ["responses", "compact", "chat", "anthropic"] {
        router = router.route(path(protocol), post(echo));
    }
    let (endpoint, _server) = serve(router).await;
    let model = "provider/实际模型@2026";
    add_account(
        &state,
        &endpoint,
        "template-account",
        vec![model.into()],
        [("client-alias".into(), model.into())].into(),
        &["responses", "compact", "chat", "anthropic"],
        0,
    );
    assert_eq!(profiles::templates().len(), 15);
    for template in profiles::templates() {
        let id = format!("coldcoffee@{}@{}", template.id, hex::encode(model));
        let profile = profiles::find(&id).unwrap();
        select_profiles(&state, vec![id]);
        for protocol in ["responses", "compact", "chat", "anthropic"] {
            let body = original_body(protocol, "client-alias");
            let (status, output) =
                json_response(request(&state, protocol, body.clone()).await).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "{} {protocol}: {output}",
                template.id
            );
            assert_eq!(output, expected_body(protocol, body, model, Some(&profile)));
        }
    }
}

#[tokio::test]
async fn new_glm_and_grok_aliases_select_actual_model_profiles_on_all_protocols() {
    let (_dir, state) = fixture();
    let models = ["glm-5.3-flash", "glm-5.3-air", "grok-4.6", "grok-build"];
    select_profiles(
        &state,
        models
            .iter()
            .map(|model| format!("aijailbreak-{model}-v1"))
            .collect(),
    );
    let router = Router::new()
        .route("/v1/responses", post(echo))
        .route("/v1/responses/compact", post(echo))
        .route("/v1/chat/completions", post(echo))
        .route("/v1/messages", post(echo));
    let (endpoint, _server) = serve(router).await;
    let mappings = models
        .iter()
        .enumerate()
        .map(|(index, model)| (format!("public-{index}"), (*model).into()))
        .collect();
    add_account(
        &state,
        &endpoint,
        "new-models",
        models.iter().map(|model| (*model).into()).collect(),
        mappings,
        &["responses", "compact", "chat", "anthropic"],
        0,
    );
    for (index, model) in models.iter().enumerate() {
        let profile = profiles::find(&format!("aijailbreak-{model}-v1")).unwrap();
        for protocol in ["responses", "compact", "chat", "anthropic"] {
            let original = original_body(protocol, &format!("public-{index}"));
            let (status, output) =
                json_response(request(&state, protocol, original.clone()).await).await;
            assert_eq!(status, StatusCode::OK, "{protocol} {model}: {output}");
            assert_eq!(
                output,
                expected_body(protocol, original, model, Some(&profile))
            );
        }
    }
}

#[tokio::test]
async fn unmatched_models_and_deselected_profiles_leave_all_four_protocols_untouched() {
    let (_dir, state) = fixture();
    let catalog = profiles::all();
    select_profiles(
        &state,
        catalog.iter().map(|profile| profile.id.clone()).collect(),
    );
    let router = Router::new()
        .route("/v1/responses", post(echo))
        .route("/v1/responses/compact", post(echo))
        .route("/v1/chat/completions", post(echo))
        .route("/v1/messages", post(echo));
    let (endpoint, _server) = serve(router).await;
    let mut model_ids: Vec<_> = catalog
        .iter()
        .flat_map(|profile| profile.models.iter().cloned())
        .collect();
    model_ids.push("unmatched-local-model".into());
    add_account(
        &state,
        &endpoint,
        "unmatched",
        model_ids,
        BTreeMap::new(),
        &["responses", "compact", "chat", "anthropic"],
        0,
    );

    for protocol in ["responses", "compact", "chat", "anthropic"] {
        let body = original_body(protocol, "unmatched-local-model");
        let (status, output) = json_response(request(&state, protocol, body.clone()).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            output,
            expected_body(protocol, body, "unmatched-local-model", None)
        );
    }

    select_profiles(&state, Vec::new());
    let selected_model = &catalog[0].models[0];
    for protocol in ["responses", "compact", "chat", "anthropic"] {
        let body = original_body(protocol, selected_model);
        let (status, output) = json_response(request(&state, protocol, body.clone()).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(output, expected_body(protocol, body, selected_model, None));
    }
}

#[tokio::test]
async fn mapped_cross_family_failover_rebuilds_only_the_second_model_instruction() {
    let (_dir, state) = fixture();
    let catalog = profiles::all();
    let first = &catalog[0];
    let second = catalog
        .iter()
        .find(|profile| profile.model_family != first.model_family)
        .expect("catalog must contain models from more than one family");
    select_profiles(&state, vec![first.id.clone(), second.id.clone()]);
    let failing = Router::new().route(
        "/v1/chat/completions",
        post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "retry") }),
    );
    let (first_endpoint, _first_server) = serve(failing).await;
    let captured = Arc::new(Mutex::new(None));
    let records = captured.clone();
    let succeeding = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let records = records.clone();
            async move {
                *records.lock().unwrap() = Some(body.clone());
                Json(body)
            }
        }),
    );
    let (second_endpoint, _second_server) = serve(succeeding).await;
    add_account(
        &state,
        &first_endpoint,
        "first-family",
        vec![first.models[0].clone()],
        BTreeMap::from([("public-model".into(), first.models[0].clone())]),
        &["chat"],
        0,
    );
    add_account(
        &state,
        &second_endpoint,
        "second-family",
        vec![second.models[0].clone()],
        BTreeMap::from([("public-model".into(), second.models[0].clone())]),
        &["chat"],
        1,
    );
    let original = json!({"model":"public-model", "messages":[{"role":"user", "content":"Keep this history"}], "tools":[{"type":"function"}]});
    let (status, output) = json_response(request(&state, "chat", original.clone()).await).await;
    assert_eq!(status, StatusCode::OK);
    let sent = captured.lock().unwrap().clone().unwrap();
    assert_eq!(output, sent);
    assert_eq!(sent["model"], second.models[0]);
    assert_eq!(sent["messages"][0]["content"], second.instruction);
    assert_eq!(
        &sent["messages"].as_array().unwrap()[1..],
        original["messages"].as_array().unwrap()
    );
    let request_log = &logs(&state)[0];
    assert_eq!(request_log.attempts, 2);
    assert_eq!(request_log.account_id, "second-family");
}

#[tokio::test]
async fn all_protocol_streams_forward_sse_bytes_without_rewriting() {
    let (_dir, state) = fixture();
    let profile = &profiles::all()[0];
    select_profiles(&state, vec![profile.id.clone()]);
    let streams = [
        (
            "responses",
            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}}\n\n".as_slice(),
        ),
        (
            "compact",
            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}}\n\n".as_slice(),
        ),
        (
            "chat",
            b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n".as_slice(),
        ),
        (
            "anthropic",
            b"event: message_stop\ndata: {\"type\":\"message_stop\",\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}\n\n".as_slice(),
        ),
    ];
    let mut router = Router::new();
    for (protocol, bytes) in streams {
        let bytes = bytes.to_vec();
        router = router.route(
            path(protocol),
            post(move || {
                let bytes = bytes.clone();
                async move { ([("content-type", "text/event-stream")], Body::from(bytes)) }
            }),
        );
    }
    let (endpoint, _server) = serve(router).await;
    add_account(
        &state,
        &endpoint,
        "streaming",
        profile.models.clone(),
        BTreeMap::new(),
        &["responses", "compact", "chat", "anthropic"],
        0,
    );

    for (protocol, expected) in streams {
        let mut body = original_body(protocol, &profile.models[0]);
        body["stream"] = json!(true);
        let response = request(&state, protocol, body).await;
        assert_eq!(response.status(), StatusCode::OK, "{protocol}");
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(bytes.as_ref(), expected, "{protocol}");
    }
    assert_eq!(logs(&state).len(), 4);
    assert!(logs(&state).iter().all(|entry| entry.status == 200));
}
