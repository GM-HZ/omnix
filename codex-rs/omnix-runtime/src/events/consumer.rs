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

use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessServerEvent;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::DynamicToolCallOutputContentItem;
use codex_app_server_protocol::DynamicToolCallParams;
use codex_app_server_protocol::DynamicToolCallResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use tokio::sync::mpsc;

use crate::events::AgentEvent;
use crate::events::AgentFailure;
use crate::events::ApprovalDecision;
use crate::events::ApprovalKind;
use crate::events::ApprovalRequest;
use crate::events::mapping::map_notification;
use crate::tools::ToolInvocation;
use crate::tools::ToolInvoker;
use crate::tools::ToolLimits;

/// A sink for one run's events, plus the thread id it is scoped to.
pub(crate) struct RunSink {
    pub thread_id: String,
    pub sender: mpsc::Sender<AgentEvent>,
}

/// Commands the consumer task accepts from runtime handles.
pub(crate) enum ConsumerCommand {
    /// Register a run's event sink. The consumer routes events whose `thread_id`
    /// matches until a terminal event is delivered.
    RegisterRun(RunSink),
    /// Stop the consumer loop (runtime shutdown).
    Stop,
}

/// Optional tool dispatch wiring passed to the consumer.
pub(crate) struct ToolDispatch {
    pub invoker: Arc<dyn ToolInvoker>,
    pub limits: ToolLimits,
    /// Bounds the number of concurrently-running tool tasks (§12).
    pub semaphore: Arc<tokio::sync::Semaphore>,
}

/// A completed server-request resolution ready to send back to the app-server.
struct ResolveRequest {
    request_id: RequestId,
    result: serde_json::Value,
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
        let mut sinks: HashMap<String, mpsc::Sender<AgentEvent>> = HashMap::new();

        loop {
            tokio::select! {
                // Prefer draining commands so a run is registered before its
                // events arrive.
                biased;

                cmd = cmd_rx.recv() => match cmd {
                    Some(ConsumerCommand::RegisterRun(sink)) => {
                        sinks.insert(sink.thread_id, sink.sender);
                    }
                    Some(ConsumerCommand::Stop) | None => break,
                },

                // A tool/approval finished: resolve its server request.
                Some(resolve) = resolve_rx.recv() => {
                    let _ = client.resolve_server_request(resolve.request_id, resolve.result).await;
                }

                event = client.next_event() => match event {
                    Some(event) => {
                        route_event(&mut sinks, event, tools.as_ref(), &resolve_tx).await;
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
    sinks: &mut HashMap<String, mpsc::Sender<AgentEvent>>,
    event: InProcessServerEvent,
    tools: Option<&ToolDispatch>,
    resolve_tx: &mpsc::Sender<ResolveRequest>,
) {
    match event {
        InProcessServerEvent::ServerNotification(notification) => {
            let mut terminal_thread: Option<String> = None;
            for (thread_id, sender) in sinks.iter() {
                if let Some(agent_event) = map_notification(&notification, thread_id) {
                    let is_terminal = agent_event.is_terminal();
                    let _ = sender.send(agent_event).await;
                    if is_terminal {
                        terminal_thread = Some(thread_id.clone());
                    }
                    break;
                }
            }
            if let Some(thread_id) = terminal_thread {
                sinks.remove(&thread_id);
            }
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

/// Handle a server→client request. Dynamic tool calls are dispatched to the
/// registered invoker; approval requests are auto-decided (SDK 0.0 is
/// non-interactive) and surfaced to the host as `WaitingForApproval`.
async fn dispatch_server_request(
    request: ServerRequest,
    tools: Option<&ToolDispatch>,
    sinks: &HashMap<String, mpsc::Sender<AgentEvent>>,
    resolve_tx: &mpsc::Sender<ResolveRequest>,
) {
    match request {
        ServerRequest::DynamicToolCall { request_id, params } => {
            dispatch_tool_call(request_id, params, tools, resolve_tx).await;
        }
        ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
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
            )
            .await;
        }
        ServerRequest::FileChangeRequestApproval { request_id, params } => {
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
            )
            .await;
        }
        _ => {
            // Other server requests (permissions grant, MCP elicitation, etc.)
            // are outside the SDK 0.0 non-interactive surface; leaving them
            // unresolved lets the app-server apply its own default handling.
        }
    }
}

/// Emit a `WaitingForApproval` event to the matching thread's run sink.
async fn emit_approval(
    sinks: &HashMap<String, mpsc::Sender<AgentEvent>>,
    thread_id: &str,
    request: ApprovalRequest,
) {
    if let Some(sender) = sinks.get(thread_id) {
        let _ = sender.send(AgentEvent::WaitingForApproval(request)).await;
    }
}

/// Serialize a typed response and queue it for resolution.
async fn send_resolution<T: serde::Serialize>(
    resolve_tx: &mpsc::Sender<ResolveRequest>,
    request_id: RequestId,
    response: &T,
) {
    if let Ok(result) = serde_json::to_value(response) {
        let _ = resolve_tx.send(ResolveRequest { request_id, result }).await;
    }
}

/// Dispatch a dynamic tool call to the registered invoker on its own task.
async fn dispatch_tool_call(
    request_id: RequestId,
    params: DynamicToolCallParams,
    tools: Option<&ToolDispatch>,
    resolve_tx: &mpsc::Sender<ResolveRequest>,
) {
    let Some(dispatch) = tools else {
        send_resolution(
            resolve_tx,
            request_id,
            &failure_response("no tools are registered"),
        )
        .await;
        return;
    };

    let DynamicToolCallParams {
        thread_id,
        call_id,
        tool,
        arguments,
        ..
    } = params;
    let invoker = Arc::clone(&dispatch.invoker);
    let timeout = dispatch.limits.call_timeout;
    let semaphore = Arc::clone(&dispatch.semaphore);
    let resolve_tx = resolve_tx.clone();

    // Spawn so a slow tool never blocks the event loop.
    tokio::spawn(async move {
        // Enforce the global concurrency cap (§12): acquire before invoking.
        let _permit = semaphore.acquire_owned().await;
        let started_at = std::time::Instant::now();
        let invocation = ToolInvocation {
            thread_id,
            call_id,
            tool: tool.clone(),
            arguments,
        };
        let response = match tokio::time::timeout(timeout, invoker.invoke(invocation)).await {
            Ok(output) => DynamicToolCallResponse {
                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                    text: output.text,
                }],
                success: output.success,
            },
            Err(_) => failure_response(&format!("tool `{tool}` timed out")),
        };
        tracing::debug!(
            target: "omnix::tool",
            tool = %tool,
            success = response.success,
            duration_ms = started_at.elapsed().as_millis() as u64,
            "tool call completed"
        );
        if let Ok(result) = serde_json::to_value(&response) {
            let _ = resolve_tx.send(ResolveRequest { request_id, result }).await;
        }
    });
}

/// A `DynamicToolCallResponse` carrying a failure diagnostic.
fn failure_response(message: &str) -> DynamicToolCallResponse {
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText {
            text: message.to_string(),
        }],
        success: false,
    }
}

/// Send a failure to every registered run and clear them.
async fn fail_all(sinks: &mut HashMap<String, mpsc::Sender<AgentEvent>>, message: &str) {
    for (thread_id, sender) in sinks.drain() {
        let _ = sender
            .send(AgentEvent::Failed(AgentFailure {
                message: message.to_string(),
                turn_id: Some(thread_id),
            }))
            .await;
    }
}
