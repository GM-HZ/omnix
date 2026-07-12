//! End-to-end tests for the DeepSeek Chat Completions supported path, driven by
//! the offline mock chat-completions SSE server. These exercise the same
//! wire-agnostic turn loop, compaction, and resume machinery the Responses
//! suite covers, but over `wire_api = "chat_completions"` — the Runtime 0.0
//! release-gated path.

use std::sync::Arc;

use codex_core::CodexThread;
use codex_model_provider_info::WireApi;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::user_input::UserInput;
use core_test_support::chat_completions::cc_content_delta;
use core_test_support::chat_completions::cc_finish;
use core_test_support::chat_completions::cc_reasoning;
use core_test_support::chat_completions::cc_role_content;
use core_test_support::chat_completions::cc_sse;
use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::cc_tool_call_open;
use core_test_support::chat_completions::cc_usage;
use core_test_support::chat_completions::mount_chat_completions_once;
use core_test_support::chat_completions::mount_chat_completions_sequence;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use serde_json::json;

/// Build a Codex thread wired to the chat_completions mock `server`.
async fn chat_codex(server: &wiremock::MockServer) -> Arc<CodexThread> {
    test_codex()
        .with_config(|config| {
            config.model_provider.wire_api = WireApi::ChatCompletions;
            config.model_provider.supports_websockets = false;
        })
        .build(server)
        .await
        .expect("build chat_completions test codex")
        .codex
}

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

/// Submit a text turn under an explicit sandbox policy so built-in tools may run.
async fn submit_text_with_sandbox(codex: &CodexThread, text: &str, sandbox: SandboxPolicy) {
    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: Default::default(),
            thread_settings: codex_protocol::protocol::ThreadSettingsOverrides {
                approval_policy: Some(AskForApproval::Never),
                sandbox_policy: Some(sandbox),
                ..Default::default()
            },
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

    let codex = chat_codex(&server).await;
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

/// B3: reasoning + text. A DeepSeek thinking-mode turn streams reasoning_content
/// then content; the assistant message must land (reasoning is internal, not
/// duplicated as the final message).
#[tokio::test]
async fn chat_completions_reasoning_then_text_turn() {
    let server = start_mock_chat_completions_server().await;
    let body = cc_sse(vec![
        cc_reasoning("c1", "Let me think"),
        cc_role_content("c1", "Final "),
        cc_content_delta("c1", "answer"),
        cc_finish("c1", "stop"),
        cc_usage(
            "c1", /*prompt*/ 20, /*hit*/ 0, /*miss*/ 20, /*completion*/ 8,
        ),
    ]);
    mount_chat_completions_once(&server, body).await;

    let codex = chat_codex(&server).await;
    submit_text(&codex, "think about it").await;

    let message = wait_for_event(&codex, |ev| matches!(ev, EventMsg::AgentMessage(_))).await;
    let EventMsg::AgentMessage(message) = message else {
        unreachable!("matched AgentMessage above");
    };
    assert_eq!(message.message, "Final answer");
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
}

/// B3: single tool call → tool result → continuation. The first chat response
/// is a tool call; core runs the built-in shell tool; the continuation request
/// (carrying the tool result) produces the final assistant message.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_completions_tool_call_then_continuation() {
    let server = start_mock_chat_completions_server().await;

    let args = json!({"command": "echo hi", "timeout_ms": 5_000}).to_string();
    let tool_turn = cc_sse(vec![
        cc_tool_call_open("c1", 0, "call_echo", "shell_command", &args),
        cc_finish("c1", "tool_calls"),
        cc_usage(
            "c1", /*prompt*/ 30, /*hit*/ 0, /*miss*/ 30, /*completion*/ 4,
        ),
    ]);
    let continuation = cc_text_turn("c2", "done", /*prompt*/ 40, /*completion*/ 2);
    let chat_mock = mount_chat_completions_sequence(&server, vec![tool_turn, continuation]).await;

    let codex = chat_codex(&server).await;
    submit_text_with_sandbox(&codex, "run echo", SandboxPolicy::DangerFullAccess).await;

    // The tool executes...
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::ExecCommandEnd(_))).await;
    // ...and the continuation produces the final message + completes.
    let message = wait_for_event(&codex, |ev| matches!(ev, EventMsg::AgentMessage(_))).await;
    let EventMsg::AgentMessage(message) = message else {
        unreachable!("matched AgentMessage above");
    };
    assert_eq!(message.message, "done");
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // Two chat requests were made; the continuation carried the tool result
    // (a "tool" role message) back to the model.
    assert_eq!(chat_mock.request_count(), 2);
    let continuation_messages = chat_mock
        .requests()
        .last()
        .expect("continuation request captured")
        .messages();
    assert!(
        continuation_messages
            .iter()
            .any(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool")),
        "continuation must carry the tool result, got: {continuation_messages:?}"
    );
}

/// B3: cancellation while a tool (and thus the turn) is active. A long-running
/// tool call is interrupted mid-turn and the turn aborts cleanly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_completions_interrupt_aborts_active_turn() {
    let server = start_mock_chat_completions_server().await;

    let args = json!({"command": "sleep 60", "timeout_ms": 60_000}).to_string();
    let tool_turn = cc_sse(vec![
        cc_tool_call_open("c1", 0, "call_sleep", "shell_command", &args),
        cc_finish("c1", "tool_calls"),
        cc_usage(
            "c1", /*prompt*/ 30, /*hit*/ 0, /*miss*/ 30, /*completion*/ 4,
        ),
    ]);
    mount_chat_completions_once(&server, tool_turn).await;

    let codex = chat_codex(&server).await;
    submit_text_with_sandbox(&codex, "sleep please", SandboxPolicy::DangerFullAccess).await;

    // Wait until the tool starts, then interrupt mid-turn.
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::ExecCommandBegin(_))).await;
    codex.submit(Op::Interrupt).await.expect("submit interrupt");

    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TurnAborted(_))).await;
}

/// The compaction summarization prompt, set explicitly so the mock can detect
/// the auto-compaction request among ordinary turns.
const TEST_COMPACT_PROMPT: &str = "Summarize the conversation so far.";

/// Does any message in the request body carry `needle` in its content?
fn request_contains_text(
    request: &core_test_support::chat_completions::ChatCompletionsRequest,
    needle: &str,
) -> bool {
    request.messages().iter().any(|message| {
        message
            .get("content")
            .map(|content| content.to_string().contains(needle))
            .unwrap_or(false)
    })
}

/// B4: automatic compaction on the chat path. The first turn reports usage over
/// a scaled-down `auto_compact_token_limit`; core must run a compaction request
/// (carrying the compact prompt) and then complete the follow-up turn — i.e.
/// compact-then-continue works over `wire_api = "chat_completions"`.
#[tokio::test]
async fn chat_completions_auto_compact_then_continue() {
    let server = start_mock_chat_completions_server().await;

    // Turn 1 crosses the (scaled-down) 200-token compaction threshold.
    let turn1 = cc_text_turn(
        "c1",
        "first reply",
        /*prompt*/ 150,
        /*completion*/ 100,
    );
    // The compaction summary request → a short summary response.
    let summary = cc_text_turn("c2", "summary", /*prompt*/ 50, /*completion*/ 5);
    // The follow-up turn after compaction completes normally.
    let follow_up = cc_text_turn(
        "c3",
        "after compact",
        /*prompt*/ 40,
        /*completion*/ 3,
    );
    let chat_mock = mount_chat_completions_sequence(&server, vec![turn1, summary, follow_up]).await;

    let codex = test_codex()
        .with_config(|config| {
            config.model_provider.wire_api = WireApi::ChatCompletions;
            config.model_provider.supports_websockets = false;
            config.compact_prompt = Some(TEST_COMPACT_PROMPT.to_string());
            config.model_auto_compact_token_limit = Some(200);
        })
        .build(&server)
        .await
        .expect("build chat_completions test codex")
        .codex;

    // Turn 1 — crosses the threshold; auto-compaction runs before the follow-up.
    submit_text(&codex, "first message").await;
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // Follow-up turn — must complete (compact then continue).
    submit_text(&codex, "second message").await;
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // Exactly one request carried the compaction prompt.
    let compaction_requests = chat_mock
        .requests()
        .into_iter()
        .filter(|request| request_contains_text(request, TEST_COMPACT_PROMPT))
        .count();
    assert_eq!(
        compaction_requests, 1,
        "expected exactly one auto-compaction request on the chat path"
    );
}

/// B5: persist → stop → resume → continue on the chat path. Turn 1 runs a tool
/// call (so reasoning-free tool call + result are persisted); the worker is
/// dropped; a fresh worker resumes the same thread from its rollout; turn 2
/// completes. Asserts the resumed outgoing history carries the prior tool
/// result (call continuity survives) and the tool is NOT re-executed on resume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_completions_persist_resume_continue() {
    // --- First worker: run a tool-call turn, then persist. ---------------
    let server = start_mock_chat_completions_server().await;
    let args = json!({"command": "echo persisted", "timeout_ms": 5_000}).to_string();
    let tool_turn = cc_sse(vec![
        cc_tool_call_open("c1", 0, "call_persist", "shell_command", &args),
        cc_finish("c1", "tool_calls"),
        cc_usage(
            "c1", /*prompt*/ 30, /*hit*/ 0, /*miss*/ 30, /*completion*/ 4,
        ),
    ]);
    let after_tool = cc_text_turn(
        "c2",
        "first done",
        /*prompt*/ 40,
        /*completion*/ 2,
    );
    mount_chat_completions_sequence(&server, vec![tool_turn, after_tool]).await;

    let mut builder = test_codex().with_config(|config| {
        config.model_provider.wire_api = WireApi::ChatCompletions;
        config.model_provider.supports_websockets = false;
    });
    let initial = builder
        .build(&server)
        .await
        .expect("build initial chat_completions test codex");
    let home = initial.home.clone();
    let rollout_path = initial
        .session_configured
        .rollout_path
        .clone()
        .expect("rollout path present");

    submit_text_with_sandbox(&initial.codex, "run echo", SandboxPolicy::DangerFullAccess).await;
    wait_for_event(&initial.codex, |ev| {
        matches!(ev, EventMsg::ExecCommandEnd(_))
    })
    .await;
    wait_for_event(&initial.codex, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // Drop the first worker to simulate stop.
    drop(initial);

    // --- Second worker: resume the same thread and continue. -------------
    let resume_server = start_mock_chat_completions_server().await;
    let continuation = cc_text_turn(
        "c3",
        "after resume",
        /*prompt*/ 60,
        /*completion*/ 3,
    );
    let resume_mock = mount_chat_completions_once(&resume_server, continuation).await;

    let mut resume_builder = test_codex().with_config(|config| {
        config.model_provider.wire_api = WireApi::ChatCompletions;
        config.model_provider.supports_websockets = false;
    });
    let resumed = resume_builder
        .resume(&resume_server, home, rollout_path)
        .await
        .expect("resume chat_completions thread");

    submit_text(&resumed.codex, "continue please").await;
    let message =
        wait_for_event(&resumed.codex, |ev| matches!(ev, EventMsg::AgentMessage(_))).await;
    let EventMsg::AgentMessage(message) = message else {
        unreachable!("matched AgentMessage above");
    };
    assert_eq!(message.message, "after resume");
    wait_for_event(&resumed.codex, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // The resumed continuation request must carry the persisted tool result
    // (a "tool" role message) so DeepSeek call continuity survives resume...
    let resumed_request = resume_mock
        .last_request()
        .expect("resumed continuation request captured");
    let messages = resumed_request.messages();
    assert!(
        messages
            .iter()
            .any(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool")),
        "resumed history must carry the prior tool result, got: {messages:?}"
    );

    // ...and the tool was NOT re-executed on resume: the resume server saw
    // exactly one request (the continuation), no tool-call round-trip.
    assert_eq!(
        resume_mock.request_count(),
        1,
        "resume must not replay the tool side effect"
    );
}
