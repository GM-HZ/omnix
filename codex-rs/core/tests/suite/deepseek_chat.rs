//! End-to-end tests for the DeepSeek Chat Completions supported path, driven by
//! the offline mock chat-completions SSE server. These exercise the same
//! wire-agnostic turn loop, compaction, and resume machinery the Responses
//! suite covers, but over `wire_api = "chat_completions"` — the Runtime 0.0
//! release-gated path.

use codex_core::CodexThread;
use codex_model_provider_info::WireApi;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::user_input::UserInput;
use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::mount_chat_completions_once;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;

/// Submit a plain text user turn to a Codex thread.
async fn submit_text(codex: &CodexThread, text: &str) {
    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: Default::default(),
            thread_settings: Default::default(),
        })
        .await
        .expect("submit user input");
}

/// A1 acceptance + B3 text case: a full text turn over the chat_completions
/// mock. Proves the mock server, provider routing, and turn loop all work on
/// the DeepSeek path.
#[tokio::test]
async fn chat_completions_text_turn_completes() {
    let server = start_mock_chat_completions_server().await;
    mount_chat_completions_once(
        &server,
        cc_text_turn(
            "c1",
            "Hello from DeepSeek",
            /*prompt*/ 12,
            /*completion*/ 5,
        ),
    )
    .await;

    let fixture = test_codex()
        .with_config(|config| {
            config.model_provider.wire_api = WireApi::ChatCompletions;
            config.model_provider.supports_websockets = false;
        })
        .build(&server)
        .await
        .expect("build chat_completions test codex");
    let codex = fixture.codex;

    submit_text(&codex, "hi").await;

    // The assistant message lands...
    let message = wait_for_event(&codex, |ev| matches!(ev, EventMsg::AgentMessage(_))).await;
    let EventMsg::AgentMessage(message) = message else {
        unreachable!("matched AgentMessage above");
    };
    assert_eq!(message.message, "Hello from DeepSeek");

    // ...and the turn completes with token usage carried through.
    let token_count = wait_for_event(&codex, |ev| matches!(ev, EventMsg::TokenCount(_))).await;
    let EventMsg::TokenCount(token_count) = token_count else {
        unreachable!("matched TokenCount above");
    };
    let info = token_count.info.expect("token usage info present");
    assert_eq!(info.last_token_usage.input_tokens, 12);
    assert_eq!(info.last_token_usage.output_tokens, 5);

    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
}
