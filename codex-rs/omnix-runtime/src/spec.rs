//! Resolved, validated runtime input consumed by [`crate::runtime::Runtime`].
//!
//! `omnix-sdk` owns the ergonomic public `RuntimeConfig` and its builder; it
//! lowers that into a [`RuntimeSpec`] here. Keeping this type plain (no
//! builders, no defaulting magic) means the runtime crate never needs to depend
//! back on `omnix-sdk`, so the public API can evolve without touching the
//! adapter.

use std::sync::Arc;
use std::time::Duration;

use codex_model_provider_info::WireApi;
use codex_protocol::config_types::SandboxMode;
use codex_protocol::protocol::AskForApproval;

use crate::dirs::RuntimeScope;
use crate::tools::ToolInvoker;

/// The provider identifier the runtime registers for the in-memory DeepSeek
/// (or other Chat Completions) provider.
pub const OMNIX_PROVIDER_ID: &str = "omnix-deepseek";

/// Everything the runtime needs to start an in-process app-server, already
/// validated by the SDK layer.
#[derive(Clone)]
pub struct RuntimeSpec {
    pub scope: RuntimeScope,
    pub model: ModelSpec,
    pub context: ContextSpec,
    pub permissions: PermissionSpec,
    pub tools: ToolSpec,
    /// System/methodology prompt (from a Business Pack). Maps to
    /// `Config.base_instructions`.
    pub base_instructions: Option<String>,
    /// Per-runtime developer instructions. Maps to
    /// `ConfigOverrides.developer_instructions`.
    pub developer_instructions: Option<String>,
    /// Optional tool invoker. When present, its descriptors are advertised on
    /// every thread and its `invoke` handles dynamic tool calls.
    pub tool_invoker: Option<Arc<dyn ToolInvoker>>,
}

/// Model + provider connection, including the in-memory bearer token.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub model: String,
    pub base_url: String,
    pub wire_api: WireApi,
    /// Raw API key, held in memory only. Injected as the provider's
    /// `experimental_bearer_token`; never written to disk or an env var.
    pub api_key: String,
}

/// Context-window policy (raw / effective guardrail / auto-compact threshold).
#[derive(Debug, Clone)]
pub struct ContextSpec {
    pub model_context_tokens: u64,
    pub effective_guardrail_tokens: u64,
    pub auto_compact_tokens: u64,
}

/// Runtime-level permission ceiling applied to every thread.
#[derive(Debug, Clone)]
pub struct PermissionSpec {
    pub approval_policy: AskForApproval,
    pub sandbox_mode: SandboxMode,
}

/// Tool execution limits shared across sessions.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub call_timeout: Duration,
    pub max_concurrency: usize,
}

impl Default for ToolSpec {
    fn default() -> Self {
        Self {
            call_timeout: Duration::from_secs(60),
            max_concurrency: 4,
        }
    }
}
