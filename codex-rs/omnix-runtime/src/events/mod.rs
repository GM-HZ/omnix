//! Stable, plain-typed agent events produced by the runtime.
//!
//! These types deliberately use only `String`/`i64`/`serde_json::Value` so the
//! public `omnix-sdk` surface can re-export them directly — no app-server or
//! `codex-*` protocol type leaks into the SDK's stable API.

pub(crate) mod consumer;
pub(crate) mod mapping;
pub(crate) mod tool_dispatch;

/// Streamed lifecycle events for a single run (one turn).
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// The turn has started; carries the server-assigned turn id.
    Started { turn_id: String },
    /// Incremental assistant message text.
    MessageDelta { item_id: String, delta: String },
    /// A completed assistant message.
    MessageCompleted { item_id: String, text: String },
    /// Incremental reasoning text (summary or raw content stream).
    ReasoningDelta { item_id: String, delta: String },
    /// A completed reasoning item.
    ReasoningCompleted {
        item_id: String,
        summary: Vec<String>,
        content: Vec<String>,
    },
    /// A tool/command call was requested by the model.
    ToolCallRequested {
        call_id: String,
        tool: String,
        arguments: serde_json::Value,
    },
    /// A tool/command call finished.
    ToolCallCompleted {
        call_id: String,
        tool: String,
        success: bool,
        output: Option<String>,
    },
    /// Updated token/cache accounting.
    Usage(Usage),
    /// Automatic (or requested) context compaction completed.
    CompactCompleted,
    /// The runtime observed and auto-decided a privileged-action approval.
    /// Runtime 0.0 is non-interactive, so this is an audit event rather than a
    /// request that the host can answer.
    ApprovalDecided(ApprovalRequest),
    /// The turn finished (successfully or interrupted).
    Completed(RunResult),
    /// The turn failed.
    Failed(AgentFailure),
}

impl AgentEvent {
    /// Whether this event terminates the run's event stream.
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentEvent::Completed(_) | AgentEvent::Failed(_))
    }
}

/// Token accounting for a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

/// How a run finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    Interrupted,
}

/// The terminal result of a successful (or interrupted) run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub turn_id: String,
    pub status: RunStatus,
}

/// A run failure surfaced to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFailure {
    pub message: String,
    pub turn_id: Option<String>,
}

/// What kind of privileged action was auto-decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalKind {
    /// A shell/command execution.
    CommandExecution,
    /// A file change / patch application.
    FileChange,
    /// A permission-profile grant.
    Permissions,
}

/// Details of an approval request and the decision already applied by Omnix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub kind: ApprovalKind,
    /// The item id the approval is for.
    pub item_id: String,
    /// Optional explanatory reason from the agent.
    pub reason: Option<String>,
    /// The command, when `kind` is `CommandExecution`.
    pub command: Option<String>,
    /// The decision the runtime applied (SDK 0.0 auto-decides).
    pub decision: ApprovalDecision,
}

/// The decision the runtime applied to an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Accept,
    Decline,
}
