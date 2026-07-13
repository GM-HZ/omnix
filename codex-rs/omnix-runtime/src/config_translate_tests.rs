use std::fs;

use tempfile::tempdir;

use super::*;
use crate::dirs::RuntimeScope;
use crate::dirs::prepare_runtime_dirs;
use crate::spec::ContextSpec;
use crate::spec::ModelSpec;
use crate::spec::PermissionSpec;
use crate::spec::ToolSpec;

#[tokio::test]
async fn ignores_ambient_dot_omnix_config() {
    let root = tempdir().expect("tempdir");
    let scope = RuntimeScope::Application {
        root: root.path().to_path_buf(),
    };
    let paths = prepare_runtime_dirs(&scope).expect("prepare runtime dirs");
    fs::write(
        paths.codex_home.join("config.toml"),
        "this is not valid toml = [",
    )
    .expect("write hostile config");

    let spec = RuntimeSpec {
        scope,
        model: ModelSpec {
            model: "deepseek-v4-flash".to_string(),
            base_url: "https://example.invalid".to_string(),
            wire_api: codex_model_provider_info::WireApi::ChatCompletions,
            api_key: "test-key".to_string(),
        },
        context: ContextSpec {
            model_context_tokens: 1_000_000,
            auto_compact_tokens: 850_000,
        },
        permissions: PermissionSpec {
            approval_policy: codex_protocol::protocol::AskForApproval::Never,
            sandbox_mode: codex_protocol::config_types::SandboxMode::ReadOnly,
        },
        tools: ToolSpec::default(),
        base_instructions: None,
        developer_instructions: None,
        tool_invoker: None,
        process: None,
    };

    let translated = build_config(&spec, &paths)
        .await
        .expect("ambient config must be ignored");

    assert_eq!(
        translated.config.model,
        Some("deepseek-v4-flash".to_string())
    );
    assert_eq!(translated.config.model_provider_id, OMNIX_PROVIDER_ID);
    assert_eq!(translated.config.model_context_window, Some(1_000_000));
    assert_eq!(
        translated.config.model_auto_compact_token_limit,
        Some(850_000)
    );
}
