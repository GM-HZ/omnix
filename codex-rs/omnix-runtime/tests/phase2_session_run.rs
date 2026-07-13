//! Phase 2 exit test: a full session/run lifecycle over a mock Chat Completions
//! provider — create a session, run a text turn, assert ordered `AgentEvent`s,
//! then resume the session and run again.

mod common;

use common::test_spec;
use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::mount_chat_completions_sequence;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use omnix_runtime::AgentEvent;
use omnix_runtime::Runtime;
use omnix_runtime::SessionConfig;

/// Drain a run, returning (all events, the concatenated assistant text).
async fn drain_run(run: &mut omnix_runtime::Run) -> (Vec<AgentEvent>, String) {
    let mut events = Vec::new();
    let mut text = String::new();
    while let Some(event) = run.next().await {
        if let AgentEvent::MessageCompleted { text: t, .. } = &event {
            text = t.clone();
        }
        events.push(event);
    }
    (events, text)
}

#[tokio::test]
async fn session_run_streams_ordered_events_then_resumes() {
    let server = start_mock_chat_completions_server().await;
    // Two turns: the first run, and a second run after resume.
    mount_chat_completions_sequence(
        &server,
        vec![
            cc_text_turn("cc-1", "Hello from run one", 12, 5),
            cc_text_turn("cc-2", "Hello from run two", 14, 6),
        ],
    )
    .await;

    let home = tempfile::tempdir().expect("temp dir");
    let root = home.path().to_path_buf();

    let runtime = Runtime::start(test_spec(root.clone(), server.uri()))
        .await
        .expect("runtime starts");

    // Create a session and run a turn.
    let mut session = runtime
        .create_session(SessionConfig::default())
        .await
        .expect("session created");
    let session_id = session.id().to_string();
    assert!(!session_id.is_empty());

    let mut run = session.run("ping").await.expect("run starts");
    let (events, text) = drain_run(&mut run).await;

    assert_eq!(text, "Hello from run one");
    // The stream must start with Started and end with a terminal Completed.
    assert!(
        matches!(events.first(), Some(AgentEvent::Started { .. })),
        "first event should be Started, got {:?}",
        events.first()
    );
    assert!(
        matches!(events.last(), Some(AgentEvent::Completed(_))),
        "last event should be Completed, got {:?}",
        events.last()
    );
    // A completed assistant message must appear before the terminal event.
    let msg_idx = events
        .iter()
        .position(|e| matches!(e, AgentEvent::MessageCompleted { .. }))
        .expect("a MessageCompleted event");
    let done_idx = events.len() - 1;
    assert!(msg_idx < done_idx, "message must precede completion");

    // Shut the runtime down and start a fresh one against the same .omnix home,
    // then resume the session and run a second turn.
    runtime.shutdown().await.expect("clean shutdown");

    let runtime2 = Runtime::start(test_spec(root.clone(), server.uri()))
        .await
        .expect("runtime restarts");
    let mut resumed = runtime2
        .resume_session(session_id.clone())
        .await
        .expect("session resumes");
    assert_eq!(resumed.id(), session_id);

    let mut run2 = resumed.run("again").await.expect("second run starts");
    let (events2, text2) = drain_run(&mut run2).await;
    assert_eq!(text2, "Hello from run two");
    assert!(matches!(events2.last(), Some(AgentEvent::Completed(_))));

    runtime2.shutdown().await.expect("clean shutdown 2");
}
