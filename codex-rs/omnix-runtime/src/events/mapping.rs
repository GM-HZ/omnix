//! Translate app-server `ServerNotification`s into stable [`AgentEvent`]s.
//!
//! Routing is scoped by `thread_id`, which is known at session-create time —
//! before any turn starts — so a run can register its event sink before issuing
//! `turn/start` and never miss the `TurnStarted` or early delta notifications.
//! Phase 2 permits one active run per session (per thread), so a thread-scoped
//! match is unambiguous. The design doc's `AgentEvent` list is honored, with one
//! correction: there is no dedicated "turn failed" notification, so failure is
//! derived from `Error` and from `TurnCompleted` with a non-completed status
//! (see [`map_turn_completed`]).

use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::TokenUsageBreakdown;
use codex_app_server_protocol::TurnStatus;

use crate::events::AgentEvent;
use crate::events::AgentFailure;
use crate::events::RunResult;
use crate::events::RunStatus;
use crate::events::Usage;

/// Map a notification to an event for the run on `thread_id`.
///
/// Returns `None` for notifications that belong to another thread or that the
/// stable API does not surface.
pub fn map_notification(notification: &ServerNotification, thread_id: &str) -> Option<AgentEvent> {
    match notification {
        ServerNotification::TurnStarted(n) if n.thread_id == thread_id => {
            Some(AgentEvent::Started {
                turn_id: n.turn.id.clone(),
            })
        }
        ServerNotification::AgentMessageDelta(n) if n.thread_id == thread_id => {
            Some(AgentEvent::MessageDelta {
                item_id: n.item_id.clone(),
                delta: n.delta.clone(),
            })
        }
        ServerNotification::ReasoningTextDelta(n) if n.thread_id == thread_id => {
            Some(AgentEvent::ReasoningDelta {
                item_id: n.item_id.clone(),
                delta: n.delta.clone(),
            })
        }
        ServerNotification::ReasoningSummaryTextDelta(n) if n.thread_id == thread_id => {
            Some(AgentEvent::ReasoningDelta {
                item_id: n.item_id.clone(),
                delta: n.delta.clone(),
            })
        }
        ServerNotification::ItemCompleted(n) if n.thread_id == thread_id => {
            map_completed_item(&n.item)
        }
        ServerNotification::ItemStarted(n) if n.thread_id == thread_id => map_started_item(&n.item),
        ServerNotification::ThreadTokenUsageUpdated(n) if n.thread_id == thread_id => {
            Some(AgentEvent::Usage(map_usage(&n.token_usage.last)))
        }
        ServerNotification::TurnCompleted(n) if n.thread_id == thread_id => Some(
            map_turn_completed(&n.turn.status, &n.turn.id, n.turn.error.as_ref()),
        ),
        ServerNotification::Error(n) if n.thread_id == thread_id && !n.will_retry => {
            Some(AgentEvent::Failed(AgentFailure {
                message: n.error.message.clone(),
                turn_id: Some(n.turn_id.clone()),
            }))
        }
        _ => None,
    }
}

/// Map an item-started notification (tool calls surface here first).
fn map_started_item(item: &ThreadItem) -> Option<AgentEvent> {
    match item {
        ThreadItem::CommandExecution { id, command, .. } => Some(AgentEvent::ToolCallRequested {
            call_id: id.clone(),
            tool: "command_execution".to_string(),
            arguments: serde_json::json!({ "command": command }),
        }),
        ThreadItem::DynamicToolCall {
            id,
            tool,
            arguments,
            ..
        } => Some(AgentEvent::ToolCallRequested {
            call_id: id.clone(),
            tool: tool.clone(),
            arguments: arguments.clone(),
        }),
        ThreadItem::McpToolCall {
            id,
            tool,
            arguments,
            ..
        } => Some(AgentEvent::ToolCallRequested {
            call_id: id.clone(),
            tool: tool.clone(),
            arguments: arguments.clone(),
        }),
        _ => None,
    }
}

/// Map an item-completed notification to the corresponding terminal item event.
fn map_completed_item(item: &ThreadItem) -> Option<AgentEvent> {
    match item {
        ThreadItem::AgentMessage { id, text, .. } => Some(AgentEvent::MessageCompleted {
            item_id: id.clone(),
            text: text.clone(),
        }),
        ThreadItem::Reasoning {
            id,
            summary,
            content,
        } => Some(AgentEvent::ReasoningCompleted {
            item_id: id.clone(),
            summary: summary.clone(),
            content: content.clone(),
        }),
        ThreadItem::CommandExecution {
            id,
            aggregated_output,
            exit_code,
            ..
        } => Some(AgentEvent::ToolCallCompleted {
            call_id: id.clone(),
            tool: "command_execution".to_string(),
            success: exit_code.map(|code| code == 0).unwrap_or(false),
            output: aggregated_output.clone(),
        }),
        ThreadItem::DynamicToolCall {
            id, tool, success, ..
        } => Some(AgentEvent::ToolCallCompleted {
            call_id: id.clone(),
            tool: tool.clone(),
            success: success.unwrap_or(false),
            output: None,
        }),
        ThreadItem::McpToolCall {
            id, tool, status, ..
        } => Some(AgentEvent::ToolCallCompleted {
            call_id: id.clone(),
            tool: tool.clone(),
            success: matches!(
                status,
                codex_app_server_protocol::McpToolCallStatus::Completed
            ),
            output: None,
        }),
        ThreadItem::ContextCompaction { .. } => Some(AgentEvent::CompactCompleted),
        _ => None,
    }
}

/// Map a `TurnCompleted` status into a terminal event. A `Completed` status is
/// success; `Interrupted` is a (clean) terminal completion; `Failed` and
/// `InProgress` (which should not appear on a completion) map to a failure.
fn map_turn_completed(
    status: &TurnStatus,
    turn_id: &str,
    error: Option<&codex_app_server_protocol::TurnError>,
) -> AgentEvent {
    match status {
        TurnStatus::Completed => AgentEvent::Completed(RunResult {
            turn_id: turn_id.to_string(),
            status: RunStatus::Completed,
        }),
        TurnStatus::Interrupted => AgentEvent::Completed(RunResult {
            turn_id: turn_id.to_string(),
            status: RunStatus::Interrupted,
        }),
        TurnStatus::Failed | TurnStatus::InProgress => AgentEvent::Failed(AgentFailure {
            message: error
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "turn failed".to_string()),
            turn_id: Some(turn_id.to_string()),
        }),
    }
}

fn map_usage(breakdown: &TokenUsageBreakdown) -> Usage {
    Usage {
        input_tokens: breakdown.input_tokens,
        cached_input_tokens: breakdown.cached_input_tokens,
        output_tokens: breakdown.output_tokens,
        reasoning_output_tokens: breakdown.reasoning_output_tokens,
        total_tokens: breakdown.total_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const THREAD: &str = "thread-1";
    const TURN: &str = "turn-1";

    /// Build a `ServerNotification` from its wire method + params, matching the
    /// exact JSON the app-server emits.
    fn notification(method: &str, params: serde_json::Value) -> ServerNotification {
        serde_json::from_value(json!({ "method": method, "params": params }))
            .unwrap_or_else(|e| panic!("failed to build {method}: {e}"))
    }

    #[test]
    fn turn_started_maps_to_started() {
        let n = notification(
            "turn/started",
            json!({
                "threadId": THREAD,
                "turn": { "id": TURN, "items": [], "status": "inProgress", "error": null },
            }),
        );
        assert_eq!(
            map_notification(&n, THREAD),
            Some(AgentEvent::Started {
                turn_id: TURN.to_string()
            })
        );
        // A different thread must not match.
        assert_eq!(map_notification(&n, "other"), None);
    }

    #[test]
    fn agent_message_delta_maps_to_message_delta() {
        let n = notification(
            "item/agentMessage/delta",
            json!({"threadId": THREAD, "turnId": TURN, "itemId": "i1", "delta": "hi"}),
        );
        assert_eq!(
            map_notification(&n, THREAD),
            Some(AgentEvent::MessageDelta {
                item_id: "i1".to_string(),
                delta: "hi".to_string()
            })
        );
    }

    #[test]
    fn item_completed_agent_message_maps_to_message_completed() {
        let n = notification(
            "item/completed",
            json!({
                "threadId": THREAD, "turnId": TURN, "completedAtMs": 0,
                "item": { "type": "agentMessage", "id": "i2", "text": "done" },
            }),
        );
        assert_eq!(
            map_notification(&n, THREAD),
            Some(AgentEvent::MessageCompleted {
                item_id: "i2".to_string(),
                text: "done".to_string()
            })
        );
    }

    #[test]
    fn turn_completed_success_maps_to_completed() {
        let n = notification(
            "turn/completed",
            json!({
                "threadId": THREAD,
                "turn": { "id": TURN, "items": [], "status": "completed", "error": null },
            }),
        );
        assert_eq!(
            map_notification(&n, THREAD),
            Some(AgentEvent::Completed(RunResult {
                turn_id: TURN.to_string(),
                status: RunStatus::Completed
            }))
        );
    }

    #[test]
    fn turn_completed_failed_maps_to_failed() {
        // Design correction: failure is derived from a non-completed status on
        // turn/completed, not a dedicated notification.
        let n = notification(
            "turn/completed",
            json!({
                "threadId": THREAD,
                "turn": {
                    "id": TURN, "items": [], "status": "failed",
                    "error": { "message": "boom", "codexErrorInfo": null }
                },
            }),
        );
        assert_eq!(
            map_notification(&n, THREAD),
            Some(AgentEvent::Failed(AgentFailure {
                message: "boom".to_string(),
                turn_id: Some(TURN.to_string())
            }))
        );
    }

    #[test]
    fn error_notification_maps_to_failed_when_not_retrying() {
        let n = notification(
            "error",
            json!({
                "threadId": THREAD, "turnId": TURN, "willRetry": false,
                "error": { "message": "network down", "codexErrorInfo": null }
            }),
        );
        assert_eq!(
            map_notification(&n, THREAD),
            Some(AgentEvent::Failed(AgentFailure {
                message: "network down".to_string(),
                turn_id: Some(TURN.to_string())
            }))
        );
    }

    #[test]
    fn error_notification_is_ignored_when_retrying() {
        let n = notification(
            "error",
            json!({
                "threadId": THREAD, "turnId": TURN, "willRetry": true,
                "error": { "message": "transient", "codexErrorInfo": null }
            }),
        );
        assert_eq!(map_notification(&n, THREAD), None);
    }

    #[test]
    fn context_compaction_item_maps_to_compact_completed() {
        let n = notification(
            "item/completed",
            json!({
                "threadId": THREAD, "turnId": TURN, "completedAtMs": 0,
                "item": { "type": "contextCompaction", "id": "c1" },
            }),
        );
        assert_eq!(
            map_notification(&n, THREAD),
            Some(AgentEvent::CompactCompleted)
        );
    }
}
