//! Dynamic host-tool execution for server requests.

use std::sync::Arc;

use codex_app_server_protocol::DynamicToolCallOutputContentItem;
use codex_app_server_protocol::DynamicToolCallParams;
use codex_app_server_protocol::DynamicToolCallResponse;
use codex_app_server_protocol::RequestId;
use tokio::sync::mpsc;

use super::consumer::ResolveRequest;
use crate::tools::ToolInvocation;
use crate::tools::ToolInvoker;
use crate::tools::ToolLimits;

/// Optional host-tool wiring passed to the event consumer.
pub(crate) struct ToolDispatch {
    pub invoker: Arc<dyn ToolInvoker>,
    pub limits: ToolLimits,
    pub semaphore: Arc<tokio::sync::Semaphore>,
}

/// Dispatch a dynamic tool call without blocking event consumption.
pub(super) async fn dispatch_tool_call(
    request_id: RequestId,
    params: DynamicToolCallParams,
    tools: Option<&ToolDispatch>,
    resolve_tx: &mpsc::Sender<ResolveRequest>,
) {
    let response_thread_id = params.thread_id.clone();
    let Some(dispatch) = tools else {
        send_response(
            resolve_tx,
            request_id,
            failure_response("no tools are registered"),
            response_thread_id,
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

    tokio::spawn(async move {
        let _permit = semaphore.acquire_owned().await;
        let started_at = std::time::Instant::now();
        let invocation = ToolInvocation {
            thread_id: thread_id.clone(),
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
        send_response(&resolve_tx, request_id, response, thread_id).await;
    });
}

async fn send_response(
    resolve_tx: &mpsc::Sender<ResolveRequest>,
    request_id: RequestId,
    response: DynamicToolCallResponse,
    thread_id: String,
) {
    if let Ok(result) = serde_json::to_value(response) {
        let _ = resolve_tx
            .send(ResolveRequest::Resolve {
                request_id,
                result,
                thread_id: Some(thread_id),
            })
            .await;
    }
}

fn failure_response(message: &str) -> DynamicToolCallResponse {
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText {
            text: message.to_string(),
        }],
        success: false,
    }
}
