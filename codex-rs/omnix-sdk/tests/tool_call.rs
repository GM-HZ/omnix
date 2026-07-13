//! Phase 3 exit test: a registered tool is advertised, the model calls it, the
//! SDK dispatches it, feeds the result back, and the continuation turn produces
//! a final message.
//!
//! Uses the PUBLIC `omnix-sdk` tool API (`AgentTool` + `ToolRegistry`).

use core_test_support::chat_completions::cc_finish;
use core_test_support::chat_completions::cc_sse;
use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::cc_tool_call_open;
use core_test_support::chat_completions::cc_usage;
use core_test_support::chat_completions::mount_chat_completions_sequence;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use omnix_sdk::AgentEvent;
use omnix_sdk::AgentTool;
use omnix_sdk::Credentials;
use omnix_sdk::ModelConfig;
use omnix_sdk::Omnix;
use omnix_sdk::RuntimeConfig;
use omnix_sdk::RuntimeScope;
use omnix_sdk::ToolCallContext;
use omnix_sdk::ToolError;
use omnix_sdk::ToolOutput;
use omnix_sdk::ToolRegistry;
use omnix_sdk::ToolSpecification;

/// A trivial tool that echoes its `value` argument back.
struct EchoTool;

impl AgentTool for EchoTool {
    fn specification(&self) -> ToolSpecification {
        ToolSpecification {
            name: "echo_tool".to_string(),
            description: "Echoes the provided value.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "value": { "type": "string" } },
                "required": ["value"]
            }),
        }
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: ToolCallContext,
    ) -> Result<ToolOutput, ToolError> {
        let value = input
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("missing `value`"))?;
        Ok(ToolOutput::text(format!("echo: {value}")))
    }
}

#[tokio::test]
async fn tool_call_then_continuation() {
    let server = start_mock_chat_completions_server().await;

    // Turn 1: the model calls `echo_tool`. Turn 2 (after the tool result is fed
    // back): a final assistant message.
    let tool_turn = cc_sse(vec![
        cc_tool_call_open("cc-1", 0, "call-1", "echo_tool", "{\"value\":\"hi\"}"),
        cc_finish("cc-1", "tool_calls"),
        cc_usage("cc-1", 20, 0, 20, 3),
    ]);
    let final_turn = cc_text_turn("cc-2", "Tool said echo: hi", 30, 5);
    mount_chat_completions_sequence(&server, vec![tool_turn, final_turn]).await;

    let home = tempfile::tempdir().expect("temp dir");
    let mut config = RuntimeConfig {
        scope: RuntimeScope::Application(home.path().to_path_buf()),
        model: ModelConfig::default(),
        context: Default::default(),
        permissions: Default::default(),
        tools: Default::default(),
        skills: Default::default(),
        plugins: Default::default(),
        persistence: Default::default(),
        observability: Default::default(),
    };
    config.model.base_url = server.uri();
    config.model.model = "mock-model".to_string();

    let mut registry = ToolRegistry::new();
    registry.register(EchoTool).expect("register echo tool");

    let runtime = Omnix::builder()
        .config(config)
        .credentials(Credentials::from_api_key("test-key"))
        .tools(registry)
        .build()
        .await
        .expect("runtime builds");

    let mut session = runtime
        .sessions()
        .create(Default::default())
        .await
        .expect("session");
    let mut run = session.run("please echo hi").await.expect("run");

    let mut tool_requested = false;
    let mut tool_completed = false;
    let mut final_message = String::new();
    let mut completed = false;
    while let Some(event) = run.next().await {
        match event {
            AgentEvent::ToolCallRequested { tool, .. } if tool == "echo_tool" => {
                tool_requested = true;
            }
            AgentEvent::ToolCallCompleted { tool, success, .. } if tool == "echo_tool" => {
                tool_completed = true;
                assert!(success, "echo tool should succeed");
            }
            AgentEvent::MessageCompleted { text, .. } => final_message = text,
            AgentEvent::Completed(_) => completed = true,
            AgentEvent::Failed(f) => panic!("run failed: {}", f.message),
            _ => {}
        }
    }

    assert!(tool_requested, "the tool call should be surfaced");
    assert!(tool_completed, "the tool completion should be surfaced");
    assert_eq!(final_message, "Tool said echo: hi");
    assert!(completed, "run should complete");

    // The tool's echoed output must have reached the second Chat Completions
    // request body.
    let requests = server.received_requests().await.expect("recorded requests");
    let second = requests
        .iter()
        .filter(|r| r.url.path().ends_with("/chat/completions"))
        .nth(1)
        .expect("a second chat/completions request");
    let body = String::from_utf8_lossy(&second.body);
    assert!(
        body.contains("echo: hi"),
        "tool result must be fed back to the model; body: {body}"
    );

    runtime.shutdown().await.expect("shutdown");
}
