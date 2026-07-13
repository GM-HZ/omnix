//! Model + credential configuration.

use serde::Deserialize;
use serde::Serialize;

/// Default release-gated model for Runtime 0.0.
pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";
/// Default DeepSeek Chat Completions endpoint.
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com/v1";

/// Which OpenAI-compatible wire protocol the provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WireApi {
    /// DeepSeek / Qwen style `POST /chat/completions`.
    #[default]
    ChatCompletions,
    /// OpenAI Responses API. Not a supported Runtime 0.0 target.
    Responses,
}

/// Model and provider connection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model slug (defaults to [`DEFAULT_MODEL`]).
    pub model: String,
    /// Provider base URL (defaults to [`DEFAULT_BASE_URL`]).
    pub base_url: String,
    /// Wire protocol (defaults to Chat Completions).
    pub wire_api: WireApi,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
            wire_api: WireApi::default(),
        }
    }
}

/// Provider credentials, held in memory only.
///
/// The API key is never serialized (so it cannot leak into `runtime.json`,
/// logs, or debug output) and is injected as the provider's bearer token at
/// runtime. The host is expected to read it from a system credential store and
/// pass it here as an in-memory value.
#[derive(Clone)]
pub struct Credentials {
    api_key: String,
}

impl Credentials {
    /// Build credentials from a raw API key.
    pub fn from_api_key(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }

    /// Borrow the raw key. Internal to the SDK; not part of the stable surface.
    pub(crate) fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Whether a non-empty key is present.
    pub(crate) fn is_present(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("api_key", &"<redacted>")
            .finish()
    }
}
