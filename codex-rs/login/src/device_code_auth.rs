// Stub: device code login and OAuth server removed per slim-agent-loop design.
use std::path::PathBuf;

/// Stub — ServerOptions was previously in server.rs (ChatGPT OAuth, now removed).
#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub codex_home: PathBuf,
    pub client_id: String,
    pub issuer: String,
    pub port: u16,
    pub open_browser: bool,
    pub force_state: Option<String>,
    pub forced_chatgpt_workspace_id: Option<Vec<String>>,
    pub codex_streamlined_login: bool,
    pub cli_auth_credentials_store_mode: codex_config::types::AuthCredentialsStoreMode,
    pub auth_keyring_backend_kind: super::auth::AuthKeyringBackendKind,
    pub auth_route_config: Option<crate::outbound_proxy::AuthRouteConfig>,
}

impl ServerOptions {
    pub fn new(
        codex_home: PathBuf,
        client_id: String,
        forced_chatgpt_workspace_id: Option<Vec<String>>,
        cli_auth_credentials_store_mode: codex_config::types::AuthCredentialsStoreMode,
        auth_keyring_backend_kind: super::auth::AuthKeyringBackendKind,
        auth_route_config: Option<crate::outbound_proxy::AuthRouteConfig>,
    ) -> Self {
        Self {
            codex_home,
            client_id,
            issuer: "https://auth.openai.com".to_string(),
            port: 1455,
            open_browser: false,
            force_state: None,
            forced_chatgpt_workspace_id,
            codex_streamlined_login: false,
            cli_auth_credentials_store_mode,
            auth_keyring_backend_kind,
            auth_route_config,
        }
    }
}

/// Stub struct — device code login is no longer supported.
#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub verification_url: String,
    pub user_code: String,
    device_auth_id: String,
    interval: u64,
}

/// Stub — device code login is no longer supported.
pub async fn request_device_code(
    _opts: &ServerOptions,
) -> std::io::Result<DeviceCode> {
    Err(std::io::Error::other(
        "Device code login is no longer supported",
    ))
}

/// Stub — device code login is no longer supported.
pub async fn complete_device_code_login(
    _opts: ServerOptions,
    _device_code: DeviceCode,
) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "Device code login is no longer supported",
    ))
}

/// Stub — device code login is no longer supported.
pub async fn run_device_code_login(
    _opts: ServerOptions,
) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "Device code login is no longer supported",
    ))
}

/// Stub — OAuth login server removed per slim-agent-loop design.
#[derive(Clone, Debug)]
pub struct ShutdownHandle;

impl ShutdownHandle {
    pub fn shutdown(&self) {}
}

/// Stub — OAuth login server removed per slim-agent-loop design.
pub struct LoginServer {
    pub auth_url: String,
    pub actual_port: u16,
}

impl LoginServer {
    pub async fn block_until_done(self) -> std::io::Result<()> {
        Err(std::io::Error::other("Login server is no longer supported"))
    }
    pub fn cancel(&self) {}
    pub fn cancel_handle(&self) -> ShutdownHandle {
        ShutdownHandle
    }
}

/// Stub — OAuth login server removed per slim-agent-loop design.
pub fn run_login_server(_opts: ServerOptions) -> std::io::Result<LoginServer> {
    Err(std::io::Error::other(
        "Login server is no longer supported",
    ))
}

/// Validates a ChatGPT account ID against an optional workspace restriction.
/// Moved from server.rs which was removed per slim-agent-loop design.
pub(crate) fn ensure_workspace_account_allowed(
    expected: Option<&[String]>,
    actual: &str,
) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if expected.iter().any(|workspace_id| workspace_id == actual) {
        Ok(())
    } else {
        Err(format!(
            "Login is restricted to workspace id(s) {}.",
            expected.join(", ")
        ))
    }
}
