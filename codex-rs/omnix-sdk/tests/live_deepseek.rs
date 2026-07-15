//! Paid Runtime 0.0 acceptance against the real DeepSeek endpoint.

use omnix_sdk::AgentEvent;
use omnix_sdk::AgentTool;
use omnix_sdk::Credentials;
use omnix_sdk::Omnix;
use omnix_sdk::RunConfig;
use omnix_sdk::ToolCallContext;
use omnix_sdk::ToolError;
use omnix_sdk::ToolOutput;
use omnix_sdk::ToolRegistry;
use omnix_sdk::ToolSpecification;

struct AcceptanceProbe;

impl AgentTool for AcceptanceProbe {
    fn specification(&self) -> ToolSpecification {
        ToolSpecification {
            name: "omnix_acceptance_probe".to_string(),
            description: "Returns the fixed Runtime 0.0 acceptance marker.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        }
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: ToolCallContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text("OMNIX_ACCEPTANCE_OK"))
    }
}

#[derive(Default)]
struct ObservedRun {
    text: String,
    reasoning: bool,
    tool_requested: bool,
    tool_completed: bool,
    cached_input_tokens: i64,
    completed: bool,
}

async fn observe(mut run: omnix_sdk::AgentRun) -> ObservedRun {
    let mut observed = ObservedRun::default();
    while let Some(event) = run.next().await {
        match event {
            AgentEvent::MessageCompleted { text, .. } => observed.text.push_str(&text),
            AgentEvent::ReasoningDelta { .. } | AgentEvent::ReasoningCompleted { .. } => {
                observed.reasoning = true;
            }
            AgentEvent::ToolCallRequested { tool, .. } if tool == "omnix_acceptance_probe" => {
                observed.tool_requested = true;
            }
            AgentEvent::ToolCallCompleted { tool, success, .. }
                if tool == "omnix_acceptance_probe" =>
            {
                observed.tool_completed = success;
            }
            AgentEvent::Usage(usage) => {
                observed.cached_input_tokens =
                    observed.cached_input_tokens.max(usage.cached_input_tokens);
            }
            AgentEvent::Completed(_) => observed.completed = true,
            AgentEvent::Failed(failure) => panic!("live run failed: {}", failure.message),
            _ => {}
        }
    }
    observed
}

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and incurs API cost"]
async fn deepseek_runtime_0_0_acceptance() {
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .expect("DEEPSEEK_API_KEY must be set for the paid SDK acceptance test");
    let root = tempfile::tempdir().expect("temporary application root");
    let mut tools = ToolRegistry::new();
    tools
        .register(AcceptanceProbe)
        .expect("register acceptance tool");

    let runtime = Omnix::builder()
        .application_root(root.path())
        .credentials(Credentials::from_api_key(api_key))
        .developer_instructions(
            "For acceptance prompts, follow the requested tool call exactly and answer concisely.",
        )
        .tools(tools)
        .build()
        .await
        .expect("start live runtime");

    let mut session = runtime
        .sessions()
        .create(Default::default())
        .await
        .expect("create live session");
    let session_id = session.id().to_string();
    let first = observe(
        session
            .run(
                "Call omnix_acceptance_probe exactly once, then report its returned marker in one sentence.",
            )
            .await
            .expect("start tool run"),
    )
    .await;
    assert!(first.completed);
    assert!(first.reasoning, "DeepSeek reasoning events must be visible");
    assert!(first.tool_requested && first.tool_completed);
    assert!(first.text.contains("OMNIX_ACCEPTANCE_OK"));

    drop(session);
    let mut resumed = runtime
        .sessions()
        .resume(session_id)
        .await
        .expect("resume live session");
    let second = observe(
        resumed
            .run("Reply with exactly RESUME_OK.")
            .await
            .expect("start resumed run"),
    )
    .await;
    assert!(second.completed);
    assert!(second.text.contains("RESUME_OK"));

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string" },
            "count": { "type": "integer" }
        },
        "required": ["status", "count"],
        "additionalProperties": false
    });
    let structured = observe(
        resumed
            .run_with_config(
                "Return JSON with status set to STRUCTURED_OK and count set to 1.",
                RunConfig::json(schema),
            )
            .await
            .expect("start structured run"),
    )
    .await;
    assert!(structured.completed);
    let structured_json: serde_json::Value =
        serde_json::from_str(&structured.text).expect("DeepSeek JSON Output returns valid JSON");
    assert_eq!(
        structured_json,
        serde_json::json!({ "status": "STRUCTURED_OK", "count": 1 })
    );
    assert!(
        structured.cached_input_tokens > 0,
        "the structured transition must retain a cached conversation prefix"
    );

    runtime.shutdown().await.expect("shutdown live runtime");
}
