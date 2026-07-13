//! Lower a validated [`RuntimeSpec`] into a fully-formed in-memory [`Config`]
//! plus the CLI-override layer that defines the DeepSeek/Chat Completions
//! provider.
//!
//! No `config.toml` is required at startup. The provider (including the API key
//! as an `experimental_bearer_token`) is expressed as in-memory dotted-path CLI
//! overrides. These flow into BOTH the initial `Config` build here AND the
//! app-server's own `ConfigManager` reload (which re-resolves config on
//! `thread/start`), so the provider is visible everywhere. The key is held in
//! memory only — never written to disk, never read from a process env var.

use std::sync::Arc;

use codex_config::TomlValue;
use codex_core::config::Config;
use codex_core::config::ConfigBuilder;
use codex_core::config::ConfigOverrides;

use crate::dirs::RuntimePaths;
use crate::error::RuntimeError;
use crate::spec::OMNIX_PROVIDER_ID;
use crate::spec::RuntimeSpec;

/// The built config plus the CLI-override layer that must be threaded into the
/// in-process app-server start args.
pub struct TranslatedConfig {
    pub config: Arc<Config>,
    pub cli_overrides: Vec<(String, TomlValue)>,
}

/// Build the in-memory [`Config`] and provider-defining CLI overrides for
/// `spec`, using `paths.codex_home` as the app-server home and
/// `paths.workspace` as the agent cwd.
pub async fn build_config(
    spec: &RuntimeSpec,
    paths: &RuntimePaths,
) -> Result<TranslatedConfig, RuntimeError> {
    let cli_overrides = provider_overrides(spec);

    let overrides = ConfigOverrides {
        model: Some(spec.model.model.clone()),
        model_provider: Some(OMNIX_PROVIDER_ID.to_string()),
        cwd: Some(paths.workspace.clone()),
        approval_policy: Some(spec.permissions.approval_policy),
        sandbox_mode: Some(spec.permissions.sandbox_mode),
        base_instructions: spec.base_instructions.clone(),
        developer_instructions: spec.developer_instructions.clone(),
        ..ConfigOverrides::default()
    };

    let mut config = ConfigBuilder::default()
        .codex_home(paths.codex_home.clone())
        .cli_overrides(cli_overrides.clone())
        .harness_overrides(overrides)
        .build()
        .await
        .map_err(|source| RuntimeError::ConfigBuild { source })?;

    // Apply the context-window policy so accounting and auto-compaction match
    // the runtime contract regardless of any bundled model metadata.
    config.model_context_window = Some(saturating_i64(spec.context.model_context_tokens));
    config.model_auto_compact_token_limit = Some(saturating_i64(spec.context.auto_compact_tokens));

    Ok(TranslatedConfig {
        config: Arc::new(config),
        cli_overrides,
    })
}

/// Build the dotted-path CLI overrides that register the Omnix provider.
///
/// Equivalent to a `[model_providers.omnix-deepseek]` config.toml block, but
/// entirely in memory.
fn provider_overrides(spec: &RuntimeSpec) -> Vec<(String, TomlValue)> {
    let prefix = format!("model_providers.{OMNIX_PROVIDER_ID}");
    let wire_api = match spec.model.wire_api {
        codex_model_provider_info::WireApi::ChatCompletions => "chat_completions",
        codex_model_provider_info::WireApi::Responses => "responses",
    };
    vec![
        (
            format!("{prefix}.name"),
            TomlValue::String("Omnix DeepSeek".to_string()),
        ),
        (
            format!("{prefix}.base_url"),
            TomlValue::String(spec.model.base_url.clone()),
        ),
        (
            format!("{prefix}.wire_api"),
            TomlValue::String(wire_api.to_string()),
        ),
        (
            format!("{prefix}.experimental_bearer_token"),
            TomlValue::String(spec.model.api_key.clone()),
        ),
        (
            format!("{prefix}.requires_openai_auth"),
            TomlValue::Boolean(false),
        ),
    ]
}

/// Clamp a token count into the `i64` domain the Config fields use.
fn saturating_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
