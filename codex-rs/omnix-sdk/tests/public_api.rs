//! End-to-end smoke over the PUBLIC `omnix-sdk` API: build a runtime via
//! `Omnix::builder()` (application root + in-memory credentials, no config.toml),
//! create a session, run a text turn against a mock Chat Completions provider,
//! and assert the streamed public `AgentEvent`s.

use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::mount_chat_completions_sequence;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use omnix_sdk::AgentEvent;
use omnix_sdk::Credentials;
use omnix_sdk::ModelConfig;
use omnix_sdk::Omnix;
use omnix_sdk::OmnixErrorKind;
use omnix_sdk::RuntimeConfig;
use omnix_sdk::RuntimeScope;
use omnix_sdk::SessionConfig;

#[tokio::test]
async fn public_api_runs_a_text_turn() {
    let server = start_mock_chat_completions_server().await;
    mount_chat_completions_sequence(&server, vec![cc_text_turn("cc-1", "hi from sdk", 10, 4)])
        .await;

    let home = tempfile::tempdir().expect("temp dir");

    // A full config so we can point the provider base_url at the mock server.
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
        .expect("runtime builds through the public API");

    let mut session = runtime
        .sessions()
        .create(Default::default())
        .await
        .expect("session");

    let mut run = session.run("ping").await.expect("run");

    let mut saw_started = false;
    let mut message = String::new();
    let mut completed = false;
    while let Some(event) = run.next().await {
        match event {
            AgentEvent::Started { .. } => saw_started = true,
            AgentEvent::MessageCompleted { text, .. } => message = text,
            AgentEvent::Completed(_) => completed = true,
            _ => {}
        }
    }

    assert!(saw_started, "should observe a Started event");
    assert_eq!(message, "hi from sdk");
    assert!(completed, "run should complete");

    runtime.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn explicit_developer_instructions_have_a_hard_size_limit() {
    let home = tempfile::tempdir().expect("temp dir");
    let result = Omnix::builder()
        .application_root(home.path())
        .credentials(Credentials::from_api_key("test-key"))
        .developer_instructions("x".repeat(8 * 1024 + 1))
        .build()
        .await;
    let error = match result {
        Ok(_) => panic!("oversized model-visible instructions must be rejected"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), OmnixErrorKind::InvalidConfig);
}

#[tokio::test]
async fn per_session_instructions_have_the_same_hard_size_limit() {
    let home = tempfile::tempdir().expect("temp dir");
    let runtime = Omnix::builder()
        .application_root(home.path())
        .credentials(Credentials::from_api_key("test-key"))
        .build()
        .await
        .expect("runtime starts");
    let result = runtime
        .sessions()
        .create(SessionConfig {
            instructions: Some("x".repeat(8 * 1024 + 1)),
        })
        .await;
    let error = match result {
        Ok(_) => panic!("oversized session instructions must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), OmnixErrorKind::InvalidConfig);

    runtime.shutdown().await.expect("shutdown");
}
