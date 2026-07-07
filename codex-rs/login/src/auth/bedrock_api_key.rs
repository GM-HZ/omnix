// Stub: Amazon Bedrock API key support removed per slim-agent-loop design.
use serde::Deserialize;
use serde::Serialize;

/// Managed Amazon Bedrock API key (stub — no longer supported).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BedrockApiKeyAuth {
    pub api_key: String,
    pub region: String,
}

/// Stub — Bedrock API key login is no longer supported.
pub fn login_with_bedrock_api_key(
    _auth_file: &std::path::Path,
    _api_key: String,
    _region: Option<String>,
    _store_mode: codex_config::types::AuthCredentialsStoreMode,
    _keyring_backend: super::storage::AuthKeyringBackendKind,
) -> std::io::Result<BedrockApiKeyAuth> {
    Err(std::io::Error::other(
        "Amazon Bedrock API key authentication is no longer supported",
    ))
}
