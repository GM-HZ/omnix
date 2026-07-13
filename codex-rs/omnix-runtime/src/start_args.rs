//! Assemble the 19-field [`InProcessClientStartArgs`] for the embedded runtime.
//!
//! Mirrors the construction `codex exec` performs (`exec/src/lib.rs`), but with
//! an in-memory `Config` and no CLI/TOML overrides. The `initialize` +
//! `initialized` handshake is performed inside
//! [`InProcessAppServerClient::start`], so callers never issue it manually.

use std::sync::Arc;

use codex_app_server_client::DEFAULT_IN_PROCESS_CHANNEL_CAPACITY;
use codex_app_server_client::EnvironmentManager;
use codex_app_server_client::ExecServerRuntimePaths;
use codex_app_server_client::InProcessClientStartArgs;
use codex_app_server_client::StateDbHandle;
use codex_config::CloudConfigBundleLoader;
use codex_config::TomlValue;
use codex_core::config::Config;
use codex_core::init_state_db;
use codex_feedback::CodexFeedback;
use codex_protocol::protocol::SessionSource;

use crate::config_translate::isolated_loader_overrides;
use crate::dirs::RuntimePaths;
use crate::error::RuntimeError;
use crate::process::EmbeddedProcess;

/// Client identity reported to the app-server during initialize.
const CLIENT_NAME: &str = "omnix-sdk";
/// Session source tag recorded for threads started by the SDK.
const SESSION_SOURCE_TAG: &str = "omnix-sdk";

/// Build start args for `config`. Initializes the state DB and environment
/// manager against the config's `codex_home` (the `.omnix` directory). The
/// `cli_overrides` (which define the in-memory provider) are threaded through
/// so the app-server's own config reloads see the same provider.
pub async fn build_start_args(
    config: Arc<Config>,
    cli_overrides: Vec<(String, TomlValue)>,
    paths: &RuntimePaths,
    process: Option<&EmbeddedProcess>,
) -> Result<InProcessClientStartArgs, RuntimeError> {
    let state_db: Option<StateDbHandle> = init_state_db(config.as_ref()).await;

    let arg0_paths = process
        .map(EmbeddedProcess::paths)
        .cloned()
        .unwrap_or_default();
    let environment_manager = match arg0_paths.codex_self_exe.clone() {
        Some(codex_self_exe) => {
            let runtime_paths = ExecServerRuntimePaths::from_optional_paths(
                Some(codex_self_exe),
                arg0_paths.codex_linux_sandbox_exe.clone(),
            )
            .map_err(|source| RuntimeError::Environment { source })?;
            EnvironmentManager::from_codex_home(config.codex_home.clone(), Some(runtime_paths))
                .await
                .map_err(|err| RuntimeError::Environment {
                    source: std::io::Error::other(err.to_string()),
                })?
        }
        None => EnvironmentManager::without_environments(),
    };

    Ok(InProcessClientStartArgs {
        arg0_paths,
        config,
        cli_overrides,
        loader_overrides: isolated_loader_overrides(paths),
        strict_config: false,
        cloud_config_bundle: CloudConfigBundleLoader::default(),
        feedback: CodexFeedback::new(),
        log_db: None,
        state_db,
        environment_manager: Arc::new(environment_manager),
        config_warnings: Vec::new(),
        session_source: SessionSource::Custom(SESSION_SOURCE_TAG.to_string()),
        enable_codex_api_key_env: false,
        client_name: CLIENT_NAME.to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
    })
}

#[cfg(test)]
#[path = "start_args_tests.rs"]
mod tests;
