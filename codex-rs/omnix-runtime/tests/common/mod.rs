//! Shared helpers for omnix-runtime integration tests.

use std::path::PathBuf;

use omnix_runtime::AskForApproval;
use omnix_runtime::ContextSpec;
use omnix_runtime::ModelSpec;
use omnix_runtime::PermissionSpec;
use omnix_runtime::RuntimeScope;
use omnix_runtime::RuntimeSpec;
use omnix_runtime::SandboxMode;
use omnix_runtime::ToolSpec;
use omnix_runtime::WireApi;

/// Build a Runtime 0.0-shaped spec pointing at a mock provider `base_url`.
pub fn test_spec(root: PathBuf, base_url: String) -> RuntimeSpec {
    RuntimeSpec {
        scope: RuntimeScope::Application { root },
        model: ModelSpec {
            model: "mock-model".to_string(),
            base_url,
            wire_api: WireApi::ChatCompletions,
            api_key: "test-key".to_string(),
        },
        context: ContextSpec {
            model_context_tokens: 1_000_000,
            auto_compact_tokens: 850_000,
        },
        permissions: PermissionSpec {
            approval_policy: AskForApproval::Never,
            sandbox_mode: SandboxMode::ReadOnly,
        },
        tools: ToolSpec::default(),
        base_instructions: None,
        developer_instructions: None,
        tool_invoker: None,
        process: None,
    }
}
