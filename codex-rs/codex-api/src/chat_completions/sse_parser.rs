use crate::chat_completions::types::ChatCompletionChunk;
use crate::chat_completions::types::ChunkDeltaToolCall;
use crate::chat_completions::types::ChunkUsage;
use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use codex_client::ByteStream;
use codex_client::StreamResponse;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

const REQUEST_ID_HEADER: &str = "x-request-id";

/// Accumulated state for a single tool call being streamed incrementally.
#[derive(Debug, Default)]
struct ToolCallSlot {
    id: String,
    name: String,
    arguments: String,
}

/// Spawn a background task that reads SSE from a Chat Completions stream
/// and forwards parsed events through the returned `ResponseStream`.
pub fn spawn_chat_completions_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
) -> ResponseStream {
    let upstream_request_id = stream_response
        .headers
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);

    tokio::spawn(async move {
        process_chat_completions_sse(stream_response.bytes, tx_event, idle_timeout).await;
    });

    ResponseStream {
        rx_event,
        upstream_request_id,
    }
}

async fn process_chat_completions_sse(
    bytes: ByteStream,
    tx: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
) {
    let mut content_acc = String::new();
    let mut thinking_acc = String::new();
    let mut tool_slots: Vec<ToolCallSlot> = Vec::new();
    let mut response_id = String::new();
    let mut usage: Option<ChunkUsage> = None;
    let mut item_added_emitted = false;

    let mut sse_stream = bytes.eventsource();
    let mut deadline = Instant::now() + idle_timeout;

    loop {
        let next = timeout(deadline.duration_since(Instant::now()), sse_stream.next()).await;

        let event = match next {
            Ok(Some(Ok(event))) => {
                deadline = Instant::now() + idle_timeout;
                event
            }
            Ok(Some(Err(e))) => {
                let _ = tx
                    .send(Err(ApiError::Stream(format!("SSE stream error: {e}"))))
                    .await;
                return;
            }
            Ok(None) => break,
            Err(_) => {
                let _ = tx
                    .send(Err(ApiError::Stream(
                        "Chat completions stream idle timeout".to_string(),
                    )))
                    .await;
                return;
            }
        };

        let data = event.data.trim().to_string();
        if data == "[DONE]" {
            break;
        }
        if data.is_empty() {
            continue;
        }

        let chunk: ChatCompletionChunk = match serde_json::from_str(&data) {
            Ok(c) => c,
            Err(e) => {
                debug!("Failed to parse chat completion chunk: {e}");
                trace!("Malformed chunk data: {data}");
                continue;
            }
        };

        if response_id.is_empty() {
            response_id.clone_from(&chunk.id);
        }
        if chunk.usage.is_some() {
            usage = chunk.usage.clone();
        }

        for choice in &chunk.choices {
            let delta = &choice.delta;

            // Ensure OutputItemAdded is emitted before any delta event so the
            // turn loop has an "active item" to attach deltas to.
            if !item_added_emitted && (delta.content.is_some() || delta.reasoning_content.is_some())
            {
                use codex_protocol::models::ResponseItem;
                let placeholder = ResponseItem::Message {
                    id: Some(chunk.id.clone()),
                    role: "assistant".to_string(),
                    content: Vec::new(),
                    phase: None,
                    internal_chat_message_metadata_passthrough: None,
                };
                let _ = tx
                    .send(Ok(ResponseEvent::OutputItemAdded(placeholder)))
                    .await;
                item_added_emitted = true;
            }

            // Content text delta
            if let Some(ref text) = delta.content {
                content_acc.push_str(text);
                let _ = tx
                    .send(Ok(ResponseEvent::OutputTextDelta(text.clone())))
                    .await;
            }

            // Reasoning/thinking delta (DeepSeek-R1 style)
            if let Some(ref reasoning) = delta.reasoning_content {
                thinking_acc.push_str(reasoning);
                let _ = tx
                    .send(Ok(ResponseEvent::ReasoningContentDelta {
                        delta: reasoning.clone(),
                        content_index: 0,
                    }))
                    .await;
            }

            // Tool call deltas
            if let Some(ref tool_calls) = delta.tool_calls {
                for tc_delta in tool_calls {
                    accumulate_tool_call(&mut tool_slots, tc_delta);
                }
            }

            // Finish reason handling
            if let Some(ref reason) = choice.finish_reason {
                match reason.as_str() {
                    "tool_calls" => {
                        flush_tool_calls(&mut tool_slots, &content_acc, &thinking_acc, &tx).await;
                        content_acc.clear();
                        thinking_acc.clear();
                    }
                    "stop" | "length" => {
                        flush_text_response(&response_id, &content_acc, &thinking_acc, &usage, &tx)
                            .await;
                        content_acc.clear();
                        thinking_acc.clear();
                    }
                    _ => {}
                }
            }
        }
    }

    // End of stream — flush any remaining content
    if !content_acc.is_empty() || !thinking_acc.is_empty() {
        flush_text_response(&response_id, &content_acc, &thinking_acc, &usage, &tx).await;
    }

    // Emit final usage
    if let Some(ref u) = usage {
        let _ = tx
            .send(Ok(ResponseEvent::Completed {
                response_id: response_id.clone(),
                token_usage: Some(TokenUsage {
                    input_tokens: u.prompt_tokens as i64,
                    cached_input_tokens: 0,
                    output_tokens: u.completion_tokens as i64,
                    reasoning_output_tokens: 0,
                    total_tokens: u.total_tokens as i64,
                }),
                end_turn: Some(true),
            }))
            .await;
    }
}

fn accumulate_tool_call(slots: &mut Vec<ToolCallSlot>, tc: &ChunkDeltaToolCall) {
    let idx = tc.index as usize;
    while slots.len() <= idx {
        slots.push(ToolCallSlot::default());
    }
    let slot = &mut slots[idx];
    if let Some(ref id) = tc.id {
        slot.id.clone_from(id);
    }
    if let Some(ref func) = tc.function {
        if let Some(ref name) = func.name {
            slot.name.clone_from(name);
        }
        if let Some(ref args) = func.arguments {
            slot.arguments.push_str(args);
        }
    }
}

async fn flush_tool_calls(
    tool_slots: &mut Vec<ToolCallSlot>,
    content: &str,
    thinking: &str,
    tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
) {
    // If there was text content before tool calls, emit a message with it first
    let has_content = !content.is_empty();
    let has_thinking = !thinking.is_empty();

    if has_thinking {
        emit_reasoning_item(thinking, tx).await;
    }
    if has_content {
        emit_assistant_message_item(content, tx).await;
    }

    for slot in tool_slots.drain(..) {
        if slot.id.is_empty() {
            continue;
        }
        use codex_protocol::models::ResponseItem;
        let item = ResponseItem::FunctionCall {
            id: None,
            name: slot.name,
            namespace: None,
            arguments: slot.arguments,
            call_id: slot.id,
            internal_chat_message_metadata_passthrough: None,
        };
        let _ = tx.send(Ok(ResponseEvent::OutputItemDone(item))).await;
    }
}

async fn flush_text_response(
    _response_id: &str,
    content: &str,
    thinking: &str,
    _usage: &Option<ChunkUsage>,
    tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
) {
    if content.is_empty() && thinking.is_empty() {
        return;
    }

    if !thinking.is_empty() {
        emit_reasoning_item(thinking, tx).await;
    }
    if !content.is_empty() {
        emit_assistant_message_item(content, tx).await;
    }
}

/// Emit a completed assistant text message as a standard `OutputItemDone` so
/// the core turn loop lands and renders it the same way it does for the
/// Responses API.
async fn emit_assistant_message_item(
    content: &str,
    tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
) {
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;

    let item = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: content.to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    };
    let _ = tx.send(Ok(ResponseEvent::OutputItemDone(item))).await;
}

/// Emit accumulated reasoning/thinking as a standard `Reasoning` item.
async fn emit_reasoning_item(thinking: &str, tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>) {
    use codex_protocol::models::ReasoningItemContent;
    use codex_protocol::models::ResponseItem;

    let item = ResponseItem::Reasoning {
        id: None,
        summary: Vec::new(),
        content: Some(vec![ReasoningItemContent::ReasoningText {
            text: thinking.to_string(),
        }]),
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    };
    let _ = tx.send(Ok(ResponseEvent::OutputItemDone(item))).await;
}
