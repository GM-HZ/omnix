//! App-server lifecycle smoke over a `wire_api = "chat_completions"` provider —
//! the Runtime 0.0 release-gated DeepSeek path. Prior app-server tests only
//! exercised the Responses wire API; this confirms the initialize handshake,
//! thread start, and a turn (start → streamed events → completion) all work
//! when the configured provider speaks Chat Completions.

use anyhow::Result;
use app_test_support::TestAppServer;
use app_test_support::create_mock_chat_completions_server_repeating_text;
use app_test_support::to_response;
use app_test_support::write_mock_chat_completions_config_toml;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadSource;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStartedNotification;
use codex_app_server_protocol::TurnStatus;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// B6: initialize → thread start → turn start → streamed completion, all over a
/// Chat Completions (DeepSeek) provider.
#[tokio::test]
async fn chat_completions_app_server_turn_lifecycle() -> Result<()> {
    let server = create_mock_chat_completions_server_repeating_text("Hello from DeepSeek").await;

    let codex_home = TempDir::new()?;
    write_mock_chat_completions_config_toml(codex_home.path(), &server.uri())?;

    // Initialize / initialized handshake. The mock does not validate auth, but
    // the provider's `env_key` must be present in the child process, so set a
    // dummy value.
    let mut mcp =
        TestAppServer::new_with_env(codex_home.path(), &[("DEEPSEEK_API_KEY", Some("test-key"))])
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

    // Turn start.
    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            client_user_message_id: None,
            input: Vec::new(),
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

    // turn/started notification.
    let started_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/started"),
    )
    .await??;
    let started: TurnStartedNotification =
        serde_json::from_value(started_notif.params.expect("params present"))?;
    assert_eq!(started.thread_id, thread.id);
    assert_eq!(started.turn.id, turn.id);

    // turn/completed notification — the streamed Chat Completions turn finished.
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

    Ok(())
}
