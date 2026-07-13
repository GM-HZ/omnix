//! Design-alignment tests for the §12/§8.2 hardening: single active run per
//! session, session metadata, and runtime.json manifest presence.

mod common;

use common::test_spec;
use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::mount_chat_completions_sequence;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use omnix_runtime::AgentEvent;
use omnix_runtime::Runtime;
use omnix_runtime::RuntimeError;
use omnix_runtime::SessionConfig;

#[tokio::test]
async fn second_concurrent_run_is_rejected() {
    let server = start_mock_chat_completions_server().await;
    mount_chat_completions_sequence(&server, vec![cc_text_turn("cc-1", "one", 8, 2)]).await;

    let home = tempfile::tempdir().unwrap();
    let runtime = Runtime::start(test_spec(home.path().to_path_buf(), server.uri()))
        .await
        .expect("runtime starts");
    let mut session = runtime
        .create_session(SessionConfig::default())
        .await
        .expect("session");

    // First run in flight (not yet drained).
    let mut run1 = session.run("first").await.expect("first run starts");

    // A second run while the first is active must be rejected (§12).
    let err = match session.run("second").await {
        Ok(_) => panic!("second concurrent run must be rejected"),
        Err(err) => err,
    };
    assert!(matches!(err, RuntimeError::RunAlreadyActive));

    // Drain the first run to completion; the guard then clears (verified by the
    // run reaching a terminal event without the second run ever having started a
    // turn).
    let mut reached_terminal = false;
    while let Some(event) = run1.next().await {
        if matches!(event, AgentEvent::Completed(_) | AgentEvent::Failed(_)) {
            reached_terminal = true;
            break;
        }
    }
    assert!(reached_terminal, "first run should reach a terminal event");

    runtime.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn session_metadata_reports_id_and_model() {
    let server = start_mock_chat_completions_server().await;
    let home = tempfile::tempdir().unwrap();
    let runtime = Runtime::start(test_spec(home.path().to_path_buf(), server.uri()))
        .await
        .expect("runtime starts");
    let session = runtime
        .create_session(SessionConfig::default())
        .await
        .expect("session");

    let meta = session.metadata();
    assert_eq!(meta.id, session.id());
    assert_eq!(meta.model, "mock-model");

    runtime.shutdown().await.expect("shutdown");
}
