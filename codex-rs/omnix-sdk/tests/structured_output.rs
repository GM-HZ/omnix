//! Structured JSON runs reach DeepSeek Chat Completions through the public SDK.

use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::mount_chat_completions_sequence;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use omnix_sdk::AgentEvent;
use omnix_sdk::Credentials;
use omnix_sdk::ModelConfig;
use omnix_sdk::Omnix;
use omnix_sdk::OmnixErrorKind;
use omnix_sdk::RunConfig;
use omnix_sdk::RunStatus;
use omnix_sdk::RuntimeConfig;
use omnix_sdk::RuntimeScope;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn json_run_reaches_chat_completions_response_format_and_schema_guidance() {
    let server = start_mock_chat_completions_server().await;
    mount_chat_completions_sequence(
        &server,
        vec![cc_text_turn("cc-1", r#"{"summary":"ok"}"#, 10, 4)],
    )
    .await;

    let home = tempfile::tempdir().expect("temp dir");
    let mut config = RuntimeConfig {
        scope: RuntimeScope::Application(home.path().to_path_buf()),
        model: ModelConfig::default(),
        context: Default::default(),
        permissions: Default::default(),
        tools: Default::default(),
    };
    config.model.base_url = server.uri();
    config.model.model = "mock-model".to_string();

    let runtime = Omnix::builder()
        .config(config)
        .credentials(Credentials::from_api_key("test-key"))
        .build()
        .await
        .expect("runtime");
    let mut session = runtime
        .sessions()
        .create(Default::default())
        .await
        .expect("session");
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" }
        },
        "required": ["summary"],
        "additionalProperties": false
    });

    let mut run = session
        .run_with_config("Summarize as JSON.", RunConfig::json(schema.clone()))
        .await
        .expect("structured run");
    let mut message = None;
    let mut completed = false;
    while let Some(event) = run.next().await {
        match event {
            AgentEvent::MessageCompleted { text, .. } => message = Some(text),
            AgentEvent::Completed(result) => {
                assert_eq!(result.status, RunStatus::Completed);
                completed = true;
            }
            AgentEvent::Failed(failure) => panic!("structured run failed: {}", failure.message),
            _ => {}
        }
    }
    assert!(completed, "structured run must emit a terminal completion");
    let message = message.expect("completed JSON message");
    assert_eq!(message, r#"{"summary":"ok"}"#);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&message).expect("valid JSON"),
        serde_json::json!({ "summary": "ok" })
    );

    let requests = server.received_requests().await.expect("recorded requests");
    let request = requests
        .iter()
        .find(|request| request.url.path().ends_with("/chat/completions"))
        .expect("chat completions request");
    let body: serde_json::Value = serde_json::from_slice(&request.body).expect("request JSON");
    assert_eq!(
        body["response_format"],
        serde_json::json!({ "type": "json_object" })
    );
    let schema_guidance = body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter_map(|message| message["content"].as_str())
        .find(|content| content.contains("<json_output_schema>"))
        .expect("schema guidance");
    assert!(schema_guidance.contains(&schema.to_string()));

    runtime.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn invalid_json_run_configs_fail_before_provider_request() {
    let server = start_mock_chat_completions_server().await;
    let home = tempfile::tempdir().expect("temp dir");
    let mut config = RuntimeConfig {
        scope: RuntimeScope::Application(home.path().to_path_buf()),
        model: ModelConfig::default(),
        context: Default::default(),
        permissions: Default::default(),
        tools: Default::default(),
    };
    config.model.base_url = server.uri();
    config.model.model = "mock-model".to_string();

    let runtime = Omnix::builder()
        .config(config)
        .credentials(Credentials::from_api_key("test-key"))
        .build()
        .await
        .expect("runtime");
    let mut session = runtime
        .sessions()
        .create(Default::default())
        .await
        .expect("session");

    let non_object = match session
        .run_with_config("invalid", RunConfig::json(serde_json::json!(true)))
        .await
    {
        Ok(_) => panic!("non-object schema must fail"),
        Err(error) => error,
    };
    assert_eq!(non_object.kind(), OmnixErrorKind::InvalidConfig);

    let oversized = match session
        .run_with_config(
            "invalid",
            RunConfig::json(serde_json::json!({
                "type": "object",
                "description": "token ".repeat(2_000)
            })),
        )
        .await
    {
        Ok(_) => panic!("oversized schema must fail"),
        Err(error) => error,
    };
    assert_eq!(oversized.kind(), OmnixErrorKind::InvalidConfig);

    assert!(
        server
            .received_requests()
            .await
            .expect("recorded requests")
            .is_empty()
    );
    runtime.shutdown().await.expect("shutdown");
}
