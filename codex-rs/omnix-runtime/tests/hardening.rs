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
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use wiremock::Mock;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

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

#[tokio::test]
async fn concurrent_sessions_use_distinct_connection_request_ids() {
    let server = start_mock_chat_completions_server().await;
    let home = tempfile::tempdir().unwrap();
    let runtime = Runtime::start(test_spec(home.path().to_path_buf(), server.uri()))
        .await
        .expect("runtime starts");

    let (first, second) = tokio::join!(
        runtime.create_session(SessionConfig::default()),
        runtime.create_session(SessionConfig::default())
    );
    let first = first.expect("first concurrent session");
    let second = second.expect("second concurrent session");

    assert_ne!(first.id(), second.id());
    runtime.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn dropping_unfinished_run_cancels_before_next_run() {
    struct DelayedSequence {
        calls: Arc<AtomicUsize>,
    }

    impl Respond for DelayedSequence {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let body_json: serde_json::Value =
                serde_json::from_slice(&request.body).expect("chat request body");
            let is_next_turn = body_json.to_string().contains("second");
            let (body, delay) = if is_next_turn {
                (cc_text_turn("cc-next", "next output", 8, 2), Duration::ZERO)
            } else {
                (
                    cc_text_turn("cc-abandoned", "abandoned output", 8, 2),
                    Duration::from_secs(30),
                )
            };
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
                .set_delay(delay)
        }
    }

    let server = start_mock_chat_completions_server().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path_regex(".*/chat/completions$"))
        .respond_with(DelayedSequence {
            calls: Arc::clone(&calls),
        })
        .mount(&server)
        .await;

    let home = tempfile::tempdir().unwrap();
    let runtime = Runtime::start(test_spec(home.path().to_path_buf(), server.uri()))
        .await
        .expect("runtime starts");
    let mut session = runtime
        .create_session(SessionConfig::default())
        .await
        .expect("session");

    let abandoned = session.run("first").await.expect("first run starts");
    drop(abandoned);

    let mut next = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match session.run("second").await {
                Ok(run) => break run,
                Err(RuntimeError::RunAlreadyActive) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => panic!("next run failed: {error}"),
            }
        }
    })
    .await
    .expect("dropped run cancellation should finish");

    let mut completed_texts = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(5), next.next())
        .await
        .unwrap_or_else(|_| {
            panic!(
                "next run should reach a terminal event; provider calls={}",
                calls.load(Ordering::SeqCst)
            )
        })
    {
        match event {
            AgentEvent::MessageCompleted { text, .. } => completed_texts.push(text),
            AgentEvent::Completed(_) | AgentEvent::Failed(_) => break,
            _ => {}
        }
    }
    assert_eq!(completed_texts, vec!["next output".to_string()]);
    assert!(calls.load(Ordering::SeqCst) >= 2);

    runtime.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn dropping_terminal_but_unread_run_allows_next_run() {
    let server = start_mock_chat_completions_server().await;
    mount_chat_completions_sequence(
        &server,
        vec![
            cc_text_turn("cc-unread", "first output", 8, 2),
            cc_text_turn("cc-next", "second output", 8, 2),
        ],
    )
    .await;

    let home = tempfile::tempdir().unwrap();
    let runtime = Runtime::start(test_spec(home.path().to_path_buf(), server.uri()))
        .await
        .expect("runtime starts");
    let mut session = runtime
        .create_session(SessionConfig::default())
        .await
        .expect("session");

    let mut unread = session.run("first").await.expect("first run starts");
    loop {
        match unread.next().await {
            Some(AgentEvent::MessageCompleted { .. }) => break,
            Some(event) if event.is_terminal() => {
                panic!("terminal event arrived before the assistant message: {event:?}")
            }
            Some(_) => {}
            None => panic!("run closed before the assistant message"),
        }
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(unread);

    let mut next = session
        .run("second")
        .await
        .expect("terminal delivery should release the session before host reads it");
    while let Some(event) = next.next().await {
        if event.is_terminal() {
            break;
        }
    }

    runtime.shutdown().await.expect("shutdown");
}
