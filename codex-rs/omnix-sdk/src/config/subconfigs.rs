//! Secondary configuration structs with Runtime 0.0 defaults.

use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

/// Context-window policy (design §5.1 / §7.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Raw provider context window.
    pub model_context_tokens: u64,
    /// Automatic-compaction threshold.
    pub auto_compact_tokens: u64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            model_context_tokens: 1_000_000,
            auto_compact_tokens: 850_000,
        }
    }
}

/// How agent-initiated privileged actions are approved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Never prompt; decline anything requiring approval. Default for embedded,
    /// non-interactive hosts.
    #[default]
    Never,
    /// The model decides when to request approval.
    OnRequest,
    /// Only auto-approve known-safe read-only commands.
    UnlessTrusted,
}

/// Sandbox posture for tool/command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPolicy {
    /// Read-only filesystem access. Conservative default.
    #[default]
    ReadOnly,
    /// Writes permitted within the workspace.
    WorkspaceWrite,
    /// No sandbox restrictions.
    DangerFullAccess,
}

/// Runtime-level permission ceiling.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub approval_policy: ApprovalPolicy,
    pub sandbox_policy: SandboxPolicy,
}

/// Tool execution limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// Per-call timeout in milliseconds.
    pub call_timeout_ms: u64,
    /// Maximum concurrent tool calls across the runtime.
    pub max_concurrency: usize,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            call_timeout_ms: 60_000,
            max_concurrency: 4,
        }
    }
}

impl ToolConfig {
    pub(crate) fn call_timeout(&self) -> Duration {
        Duration::from_millis(self.call_timeout_ms)
    }
}
