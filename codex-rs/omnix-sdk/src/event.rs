//! Public agent-event types.
//!
//! These re-export the runtime's plain-typed events under stable SDK names so
//! host applications never see app-server protocol types.
//!
//! ## Divergence from the design sketch (§8.4)
//!
//! The design doc sketches a coarser set (`ReasoningSummary(String)`,
//! `ToolCallRequested(ToolCall)`, `ToolCallCompleted(ToolResult)`). This
//! implementation deliberately uses finer, self-describing struct variants:
//! reasoning is split into streaming [`AgentEvent::ReasoningDelta`] and terminal
//! [`AgentEvent::ReasoningCompleted`] (mirroring the message delta/completed
//! split), and tool events carry `call_id`/`tool`/`arguments` inline rather than
//! an opaque payload struct. The event set is otherwise a superset of the sketch
//! and adds [`AgentEvent::WaitingForApproval`]. This is an intentional
//! refinement, not a gap.

pub use omnix_runtime::AgentFailure;
pub use omnix_runtime::ApprovalDecision;
pub use omnix_runtime::ApprovalKind;
pub use omnix_runtime::ApprovalRequest;
pub use omnix_runtime::RunResult;
pub use omnix_runtime::RunStatus;
pub use omnix_runtime::Usage;

use omnix_runtime::AgentEvent as RuntimeEvent;

/// A streamed event within a single [`crate::AgentRun`].
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// The turn started.
    Started { turn_id: String },
    /// Incremental assistant message text.
    MessageDelta { item_id: String, delta: String },
    /// A completed assistant message.
    MessageCompleted { item_id: String, text: String },
    /// Incremental reasoning text.
    ReasoningDelta { item_id: String, delta: String },
    /// A completed reasoning item (summary + raw content).
    ReasoningCompleted {
        item_id: String,
        summary: Vec<String>,
        content: Vec<String>,
    },
    /// The model requested a tool/command call.
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
    /// The agent is waiting for approval of a privileged action. In SDK 0.0 the
    /// runtime auto-decides (non-interactive); this is emitted for observability.
    WaitingForApproval(ApprovalRequest),
    /// The run finished (completed or interrupted).
    Completed(RunResult),
    /// The run failed.
    Failed(AgentFailure),
}

impl AgentEvent {
    /// Whether this event terminates the run's stream.
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentEvent::Completed(_) | AgentEvent::Failed(_))
    }
}

impl From<RuntimeEvent> for AgentEvent {
    fn from(event: RuntimeEvent) -> Self {
        match event {
            RuntimeEvent::Started { turn_id } => AgentEvent::Started { turn_id },
            RuntimeEvent::MessageDelta { item_id, delta } => {
                AgentEvent::MessageDelta { item_id, delta }
            }
            RuntimeEvent::MessageCompleted { item_id, text } => {
                AgentEvent::MessageCompleted { item_id, text }
            }
            RuntimeEvent::ReasoningDelta { item_id, delta } => {
                AgentEvent::ReasoningDelta { item_id, delta }
            }
            RuntimeEvent::ReasoningCompleted {
                item_id,
                summary,
                content,
            } => AgentEvent::ReasoningCompleted {
                item_id,
                summary,
                content,
            },
            RuntimeEvent::ToolCallRequested {
                call_id,
                tool,
                arguments,
            } => AgentEvent::ToolCallRequested {
                call_id,
                tool,
                arguments,
            },
            RuntimeEvent::ToolCallCompleted {
                call_id,
                tool,
                success,
                output,
            } => AgentEvent::ToolCallCompleted {
                call_id,
                tool,
                success,
                output,
            },
            RuntimeEvent::Usage(usage) => AgentEvent::Usage(usage),
            RuntimeEvent::CompactCompleted => AgentEvent::CompactCompleted,
            RuntimeEvent::WaitingForApproval(request) => AgentEvent::WaitingForApproval(request),
            RuntimeEvent::Completed(result) => AgentEvent::Completed(result),
            RuntimeEvent::Failed(failure) => AgentEvent::Failed(failure),
        }
    }
}
