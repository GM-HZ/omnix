use crate::chat_completions::types::ChatCompletionChunk;
use crate::chat_completions::types::ChunkDeltaToolCall;
use crate::chat_completions::types::ChunkUsage;
use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use codex_client::ByteStream;
use codex_client::StreamResponse;
use codex_protocol::OmnixMessage;
use codex_protocol::OmnixToolCall;
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
                    .send(Err(ApiError::Stream(format!(
                        "SSE stream error: {e}"
                    ))))
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
            if !item_added_emitted
                && (delta.content.is_some() || delta.reasoning_content.is_some())
            {
                use codex_protocol::models::ResponseItem;
                let placeholder = ResponseItem::Message {
                    id: Some(chunk.id.clone()),
                    role: "assistant".to_string(),
                    content: Vec::new(),
                    phase: None,
                    internal_chat_message_metadata_passthrough: None,
                };
                let _ = tx.send(Ok(ResponseEvent::OutputItemAdded(placeholder))).await;
                item_added_emitted = true;
            }

            // Content text delta
            if let Some(ref text) = delta.content {
                content_acc.push_str(text);
                let _ = tx.send(Ok(ResponseEvent::OutputTextDelta(text.clone()))).await;
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
                        flush_tool_calls(
                            &mut tool_slots,
                            &content_acc,
                            &thinking_acc,
                            &tx,
                        )
                        .await;
                        content_acc.clear();
                        thinking_acc.clear();
                    }
                    "stop" | "length" => {
                        flush_text_response(
                            &response_id,
                            &content_acc,
                            &thinking_acc,
                            &usage,
                            &tx,
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        }
    }

    // End of stream — flush any remaining content
    if !content_acc.is_empty() || !thinking_acc.is_empty() {
        flush_text_response(
            &response_id,
            &content_acc,
            &thinking_acc,
            &usage,
            &tx,
        )
        .await;
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

    let mut omnix_tool_calls: Vec<OmnixToolCall> = Vec::new();
    for slot in tool_slots.drain(..) {
        if slot.id.is_empty() {
            continue;
        }
        omnix_tool_calls.push(OmnixToolCall::new(
            slot.id,
            slot.name,
            slot.arguments,
        ));
    }

    if has_content || has_thinking {
        let msg = if has_content {
            OmnixMessage::assistant_with_thinking(
                Some(content.to_string()),
                thinking.to_string(),
            )
        } else {
            OmnixMessage::assistant_with_thinking(None, thinking.to_string())
        };
        let _ = tx.send(Ok(ResponseEvent::ChatOutputItemDone(msg))).await;
    }

    if !omnix_tool_calls.is_empty() {
        let msg = OmnixMessage::assistant_tool_calls(omnix_tool_calls);
        let _ = tx.send(Ok(ResponseEvent::ChatOutputItemDone(msg))).await;
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

    let msg = if !content.is_empty() {
        OmnixMessage::assistant_with_thinking(
            Some(content.to_string()),
            thinking.to_string(),
        )
    } else {
        OmnixMessage::assistant_with_thinking(None, thinking.to_string())
    };

    let _ = tx.send(Ok(ResponseEvent::ChatOutputItemDone(msg))).await;
}
