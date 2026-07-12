//! App-server lifecycle smoke over a `wire_api = "chat_completions"` provider —
//! the Runtime 0.0 release-gated DeepSeek path. Prior app-server tests only
//! exercised the Responses wire API; this confirms the initialize handshake,
//! thread start, and a full turn (real user input → streamed assistant message
//! → completion) all work when the configured provider speaks Chat Completions.

use anyhow::Result;
use app_test_support::TestAppServer;
use app_test_support::create_mock_chat_completions_server_sequence;
use app_test_support::to_response;
use app_test_support::write_mock_chat_completions_config_toml;
use codex_app_server_protocol::ItemCompletedNotification;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadSource;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput as V2UserInput;
use core_test_support::chat_completions::cc_text_turn;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// B6: initialize → thread start → turn start with real user input → streamed
/// assistant message → completion, all over a Chat Completions (DeepSeek)
/// provider. Asserts the application-facing data path end to end: the user
/// input reaches the outgoing Chat Completions request, the assistant message
/// notification is emitted with the expected text, and the turn completes.
#[tokio::test]
async fn chat_completions_app_server_turn_lifecycle() -> Result<()> {
    let assistant_text = "Hello from DeepSeek";
    let server = create_mock_chat_completions_server_sequence(vec![cc_text_turn(
        "cc-1",
        assistant_text,
        /*prompt*/ 12,
        /*completion*/ 5,
    )])
    .await;

    let codex_home = TempDir::new()?;
    write_mock_chat_completions_config_toml(codex_home.path(), &server.uri())?;

    // Initialize / initialized handshake. The mock does not validate auth, but
    // the provider's `env_key` must be present in the child process, so set a
    // dummy value.
    let mut mcp = TestAppServer::new_with_env(
        codex_home.path(),
        &[("DEEPSEEK_API_KEY", Some("test-key"))],
    )
    .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    // Thread start.
    let thread_req = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            thread_source: Some(ThreadSource::User),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(thread_resp)?;

    // Turn start with a real user message.
    let user_text = "ping-from-app-server";
    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            client_user_message_id: None,
            input: vec![V2UserInput::Text {
                text: user_text.to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let TurnStartResponse { turn } = to_response::<TurnStartResponse>(turn_resp)?;
    assert!(!turn.id.is_empty());

    // The streamed assistant message notification carries the expected text.
    let assistant_item = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let notification = mcp
                .read_stream_until_notification_message("item/completed")
                .await?;
            let completed: ItemCompletedNotification =
                serde_json::from_value(notification.params.expect("item/completed params"))?;
            if let ThreadItem::AgentMessage { text, .. } = &completed.item {
                return Ok::<String, anyhow::Error>(text.clone());
            }
        }
    })
    .await??;
    assert_eq!(assistant_item, assistant_text);

    // turn/completed with success status.
    let completed_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    let completed: TurnCompletedNotification =
        serde_json::from_value(completed_notif.params.expect("params present"))?;
    assert_eq!(completed.thread_id, thread.id);
    assert_eq!(completed.turn.id, turn.id);
    assert_eq!(completed.turn.status, TurnStatus::Completed);

    // The user input reached the outgoing Chat Completions request body.
    let requests = server
        .received_requests()
        .await
        .expect("mock server recorded requests");
    let chat_request = requests
        .iter()
        .find(|request| request.url.path().ends_with("/chat/completions"))
        .expect("a chat/completions request was made");
    let body = String::from_utf8_lossy(&chat_request.body);
    assert!(
        body.contains(user_text),
        "user input must reach the Chat Completions request; body: {body}"
    );

    Ok(())
}
