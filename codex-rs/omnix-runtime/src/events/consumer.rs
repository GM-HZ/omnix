//! Background event-consumer task.
//!
//! `next_event()` requires `&mut` on the client, so a single task owns the
//! client and pumps its event stream. Runs register an event sink keyed by
//! `thread_id`; the task maps each notification (via [`super::mapping`]) and
//! forwards it to the matching sink. Requests (thread/turn start, interrupt)
//! are issued out-of-band through the cloneable, `Send`
//! `InProcessAppServerRequestHandle`, so they never contend with `next_event()`.
//!
//! Tool calls arrive as `ServerRequest::DynamicToolCall`. The task spawns each
//! invocation on its own task (so tool work never blocks event consumption) and
//! receives the result back on an internal channel; only the (fast) client
//! `resolve_server_request` call runs on the consumer, and it owns the client.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessServerEvent;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use super::tool_dispatch::ToolDispatch;
use super::tool_dispatch::dispatch_tool_call;
use crate::events::AgentEvent;
use crate::events::AgentFailure;
use crate::events::ApprovalDecision;
use crate::events::ApprovalKind;
use crate::events::ApprovalRequest;
use crate::events::mapping::map_notification;
use crate::events::mapping::notification_scope;

/// A sink for one run's events, plus the thread id it is scoped to.
pub(crate) struct RunSink {
    pub thread_id: String,
    pub sender: mpsc::Sender<AgentEvent>,
    pub active: Arc<AtomicBool>,
    pub active_turn_id: Arc<Mutex<Option<String>>>,
}

/// Commands the consumer task accepts from runtime handles.
pub(crate) enum ConsumerCommand {
    /// Register a run's event sink. The consumer routes events whose `thread_id`
    /// matches until a terminal event is delivered.
    RegisterRun {
        sink: RunSink,
        registered: oneshot::Sender<()>,
    },
    /// Bind a pending thread sink to the server-assigned turn id.
    BindRun {
        thread_id: String,
        turn_id: String,
        bound: oneshot::Sender<()>,
    },
    /// Remove a run's event sink. Used when a `Run` is dropped without reaching a
    /// terminal event, so its stale sink doesn't poison the next run on the same
    /// thread (review finding #4).
    UnregisterRun {
        thread_id: String,
        unregistered: Option<oneshot::Sender<()>>,
    },
    /// Stop routing an abandoned run but retain its active guard until the
    /// server emits the old turn's real terminal notification.
    AbandonRun {
        thread_id: String,
        turn_id: String,
        active: Arc<AtomicBool>,
        active_turn_id: Arc<Mutex<Option<String>>>,
    },
    /// Stop the consumer loop (runtime shutdown).
    Stop,
}

struct ActiveSink {
    turn_id: Option<String>,
    sender: mpsc::Sender<AgentEvent>,
    active: Arc<AtomicBool>,
    active_turn_id: Arc<Mutex<Option<String>>>,
}

struct AbandonedRun {
    active: Arc<AtomicBool>,
    active_turn_id: Arc<Mutex<Option<String>>>,
}

/// A completed server-request resolution ready to send back to the app-server.
pub(super) enum ResolveRequest {
    Resolve {
        request_id: RequestId,
        result: serde_json::Value,
        thread_id: Option<String>,
    },
    Reject {
        request_id: RequestId,
        error: JSONRPCErrorError,
        thread_id: Option<String>,
    },
}

/// Spawn the consumer task. Returns the command sender.
pub(crate) fn spawn_consumer(
    mut client: InProcessAppServerClient,
    tools: Option<ToolDispatch>,
) -> (
    mpsc::Sender<ConsumerCommand>,
    tokio::task::JoinHandle<InProcessAppServerClient>,
) {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ConsumerCommand>(32);
    // Async resolutions (tool results, approval decisions) report here; the
    // consumer drains this and resolves the server request against the client it
    // owns.
    let (resolve_tx, mut resolve_rx) = mpsc::channel::<ResolveRequest>(64);

    let handle = tokio::spawn(async move {
        let mut sinks: HashMap<String, ActiveSink> = HashMap::new();
        let mut pending_notifications: HashMap<(String, String), Vec<ServerNotification>> =
            HashMap::new();
        let mut abandoned_runs: HashMap<(String, String), AbandonedRun> = HashMap::new();

        loop {
            tokio::select! {
                // Prefer draining commands so a run is registered before its
                // events arrive.
                biased;

                cmd = cmd_rx.recv() => match cmd {
                    Some(ConsumerCommand::RegisterRun { sink, registered }) => {
                        sinks.insert(sink.thread_id, ActiveSink {
                            turn_id: None,
                            sender: sink.sender,
                            active: sink.active,
                            active_turn_id: sink.active_turn_id,
                        });
                        let _ = registered.send(());
                    }
                    Some(ConsumerCommand::BindRun { thread_id, turn_id, bound }) => {
                        if let Some(sink) = sinks.get_mut(&thread_id) {
                            sink.turn_id = Some(turn_id.clone());
                        }
                        let buffered = pending_notifications
                            .remove(&(thread_id.clone(), turn_id.clone()))
                            .unwrap_or_default();
                        pending_notifications.retain(|(pending_thread, _), _| {
                            pending_thread != &thread_id
                        });
                        for notification in buffered {
                            deliver_bound_notification(&mut sinks, notification).await;
                        }
                        let _ = bound.send(());
                    }
                    Some(ConsumerCommand::UnregisterRun { thread_id, unregistered }) => {
                        sinks.remove(&thread_id);
                        pending_notifications
                            .retain(|(pending_thread, _), _| pending_thread != &thread_id);
                        if let Some(unregistered) = unregistered {
                            let _ = unregistered.send(());
                        }
                    }
                    Some(ConsumerCommand::AbandonRun {
                        thread_id,
                        turn_id,
                        active,
                        active_turn_id,
                    }) => {
                        sinks.remove(&thread_id);
                        pending_notifications
                            .retain(|(pending_thread, _), _| pending_thread != &thread_id);
                        if active.load(Ordering::Acquire) {
                            abandoned_runs.insert(
                                (thread_id, turn_id),
                                AbandonedRun {
                                    active,
                                    active_turn_id,
                                },
                            );
                        }
                    }
                    Some(ConsumerCommand::Stop) | None => break,
                },

                // A tool/approval finished: resolve its server request.
                Some(resolve) = resolve_rx.recv() => {
                    match resolve {
                        ResolveRequest::Resolve { request_id, result, thread_id } => {
                            if let Err(error) = client.resolve_server_request(request_id, result).await {
                                fail_resolution(&mut sinks, thread_id.as_deref(), &error).await;
                            }
                        }
                        ResolveRequest::Reject { request_id, error, thread_id } => {
                            if let Err(error) = client.reject_server_request(request_id, error).await {
                                fail_resolution(&mut sinks, thread_id.as_deref(), &error).await;
                            }
                        }
                    }
                }

                event = client.next_event() => match event {
                    Some(event) => {
                        route_event(
                            &mut sinks,
                            &mut pending_notifications,
                            &mut abandoned_runs,
                            event,
                            tools.as_ref(),
                            &resolve_tx,
                        )
                        .await;
                    }
                    None => {
                        fail_all(&mut sinks, "runtime event stream closed").await;
                        break;
                    }
                },
            }
        }

        client
    });

    (cmd_tx, handle)
}

/// Route a single server event to the matching run sink or tool dispatcher.
async fn route_event(
    sinks: &mut HashMap<String, ActiveSink>,
    pending_notifications: &mut HashMap<(String, String), Vec<ServerNotification>>,
    abandoned_runs: &mut HashMap<(String, String), AbandonedRun>,
    event: InProcessServerEvent,
    tools: Option<&ToolDispatch>,
    resolve_tx: &mpsc::Sender<ResolveRequest>,
) {
    match event {
        InProcessServerEvent::ServerNotification(notification) => {
            complete_abandoned_run(abandoned_runs, &notification);
            if let Some((event_thread_id, event_turn_id)) = notification_scope(&notification)
                && let Some(sink) = sinks.get(event_thread_id)
                && sink.turn_id.is_none()
            {
                let buffered = pending_notifications
                    .entry((event_thread_id.to_string(), event_turn_id.to_string()))
                    .or_default();
                if buffered.len() < 256 {
                    buffered.push(notification);
                } else {
                    let thread_id = event_thread_id.to_string();
                    let sender = sink.sender.clone();
                    let _ = sender
                        .send(AgentEvent::Failed(AgentFailure {
                            message: "too many events arrived before turn binding".to_string(),
                            turn_id: Some(event_turn_id.to_string()),
                        }))
                        .await;
                    if let Some(sink) = sinks.remove(&thread_id) {
                        clear_active_guard(&sink);
                    }
                }
                return;
            }
            deliver_bound_notification(sinks, notification).await;
        }
        InProcessServerEvent::ServerRequest(request) => {
            dispatch_server_request(request, tools, sinks, resolve_tx).await;
        }
        InProcessServerEvent::Lagged { skipped } => {
            // Backpressure: the transport dropped events. We must not silently
            // lose a completion, so surface the gap as a failure to every active
            // run (design §12).
            tracing::warn!(
                target: "omnix::backpressure",
                skipped,
                "runtime event stream lagged; failing active runs"
            );
            fail_all(
                sinks,
                &format!("runtime event stream lagged; {skipped} events dropped"),
            )
            .await;
        }
    }
}

fn complete_abandoned_run(
    abandoned_runs: &mut HashMap<(String, String), AbandonedRun>,
    notification: &ServerNotification,
) {
    let is_terminal = match notification {
        ServerNotification::TurnCompleted(_) => true,
        ServerNotification::Error(error) => !error.will_retry,
        _ => false,
    };
    if !is_terminal {
        return;
    }
    let Some((thread_id, turn_id)) = notification_scope(notification) else {
        return;
    };
    let Some(abandoned) = abandoned_runs.remove(&(thread_id.to_string(), turn_id.to_string()))
    else {
        return;
    };
    *abandoned
        .active_turn_id
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    abandoned.active.store(false, Ordering::Release);
}

async fn deliver_bound_notification(
    sinks: &mut HashMap<String, ActiveSink>,
    notification: ServerNotification,
) {
    let Some((thread_id, turn_id)) = notification_scope(&notification) else {
        return;
    };
    let Some(sink) = sinks.get(thread_id) else {
        return;
    };
    if sink.turn_id.as_deref() != Some(turn_id) {
        return;
    }
    let Some(agent_event) = map_notification(&notification, thread_id, turn_id) else {
        return;
    };
    let is_terminal = agent_event.is_terminal();
    let sender = sink.sender.clone();
    let thread_id = thread_id.to_string();
    let _ = sender.send(agent_event).await;
    if is_terminal && let Some(sink) = sinks.remove(&thread_id) {
        clear_active_guard(&sink);
    }
}

fn clear_active_guard(sink: &ActiveSink) {
    *sink
        .active_turn_id
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    sink.active.store(false, Ordering::Release);
}

/// Handle a server→client request. Dynamic tool calls are dispatched to the
/// registered invoker; approval requests are auto-decided (SDK 0.0 is
/// non-interactive) and surfaced to the host as `ApprovalDecided`.
async fn dispatch_server_request(
    request: ServerRequest,
    tools: Option<&ToolDispatch>,
    sinks: &HashMap<String, ActiveSink>,
    resolve_tx: &mpsc::Sender<ResolveRequest>,
) {
    match request {
        ServerRequest::DynamicToolCall { request_id, params } => {
            dispatch_tool_call(request_id, params, tools, resolve_tx).await;
        }
        ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
            let thread_id = params.thread_id.clone();
            emit_approval(
                sinks,
                &params.thread_id,
                ApprovalRequest {
                    kind: ApprovalKind::CommandExecution,
                    item_id: params.item_id,
                    reason: params.reason,
                    command: params.command,
                    decision: ApprovalDecision::Decline,
                },
            )
            .await;
            send_resolution(
                resolve_tx,
                request_id,
                &CommandExecutionRequestApprovalResponse {
                    decision: CommandExecutionApprovalDecision::Decline,
                },
                Some(thread_id),
            )
            .await;
        }
        ServerRequest::FileChangeRequestApproval { request_id, params } => {
            let thread_id = params.thread_id.clone();
            emit_approval(
                sinks,
                &params.thread_id,
                ApprovalRequest {
                    kind: ApprovalKind::FileChange,
                    item_id: params.item_id,
                    reason: params.reason,
                    command: None,
                    decision: ApprovalDecision::Decline,
                },
            )
            .await;
            send_resolution(
                resolve_tx,
                request_id,
                &FileChangeRequestApprovalResponse {
                    decision: FileChangeApprovalDecision::Decline,
                },
                Some(thread_id),
            )
            .await;
        }
        unsupported => resolve_unsupported_request(unsupported, resolve_tx).await,
    }
}

async fn resolve_unsupported_request(
    request: ServerRequest,
    resolve_tx: &mpsc::Sender<ResolveRequest>,
) {
    let request_id = request.id().clone();
    tracing::warn!(target: "omnix::request", ?request, "rejecting unsupported server request");
    let _ = resolve_tx
        .send(ResolveRequest::Reject {
            request_id,
            error: JSONRPCErrorError {
                code: -32601,
                message: "server request is unsupported by omnix-sdk 0.0".to_string(),
                data: None,
            },
            thread_id: None,
        })
        .await;
}

/// Emit an `ApprovalDecided` audit event to the matching thread's run sink.
async fn emit_approval(
    sinks: &HashMap<String, ActiveSink>,
    thread_id: &str,
    request: ApprovalRequest,
) {
    if let Some(sink) = sinks.get(thread_id) {
        let _ = sink.sender.send(AgentEvent::ApprovalDecided(request)).await;
    }
}

/// Serialize a typed response and queue it for resolution.
async fn send_resolution<T: serde::Serialize>(
    resolve_tx: &mpsc::Sender<ResolveRequest>,
    request_id: RequestId,
    response: &T,
    thread_id: Option<String>,
) {
    if let Ok(result) = serde_json::to_value(response) {
        let _ = resolve_tx
            .send(ResolveRequest::Resolve {
                request_id,
                result,
                thread_id,
            })
            .await;
    }
}

async fn fail_resolution(
    sinks: &mut HashMap<String, ActiveSink>,
    thread_id: Option<&str>,
    error: &std::io::Error,
) {
    let Some(thread_id) = thread_id else {
        tracing::warn!(target: "omnix::request", %error, "failed to resolve server request");
        return;
    };
    let Some(sink) = sinks.remove(thread_id) else {
        return;
    };
    clear_active_guard(&sink);
    let _ = sink
        .sender
        .send(AgentEvent::Failed(AgentFailure {
            message: format!("failed to resolve server request: {error}"),
            turn_id: sink.turn_id,
        }))
        .await;
}

/// Send a failure to every registered run and clear them.
async fn fail_all(sinks: &mut HashMap<String, ActiveSink>, message: &str) {
    for (thread_id, sink) in sinks.drain() {
        clear_active_guard(&sink);
        let _ = sink
            .sender
            .send(AgentEvent::Failed(AgentFailure {
                message: message.to_string(),
                turn_id: Some(thread_id),
            }))
            .await;
    }
}

#[cfg(test)]
#[path = "consumer_tests.rs"]
mod tests;
