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
        emit_cache_usage_diagnostics(u);
        let _ = tx
            .send(Ok(ResponseEvent::Completed {
                response_id: response_id.clone(),
                token_usage: Some(TokenUsage {
                    input_tokens: u.prompt_tokens as i64,
                    cached_input_tokens: i64::from(u.prompt_cache_hit_tokens),
                    output_tokens: u.completion_tokens as i64,
                    reasoning_output_tokens: 0,
                    total_tokens: u.total_tokens as i64,
                }),
                end_turn: Some(true),
            }))
            .await;
    }
}

/// Response-side prompt-cache diagnostics derived from the provider's final
/// usage report. Content-free (counts and a ratio only), so it is safe to log.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CacheUsageDiagnostics {
    prompt_tokens: u32,
    hit_tokens: u32,
    miss_tokens: u32,
    /// Token-weighted hit ratio `hit / (hit + miss)`, or `None` when there is
    /// no cache signal (both counts zero) so we never report a misleading 0.0.
    hit_ratio: Option<f64>,
    /// Whether `hit + miss == prompt_tokens`. When false, the provider's cache
    /// split does not reconcile with its own prompt count.
    usage_consistent: bool,
}

impl CacheUsageDiagnostics {
    fn from_usage(usage: &ChunkUsage) -> Self {
        let hit = usage.prompt_cache_hit_tokens;
        let miss = usage.prompt_cache_miss_tokens;
        let denom = hit + miss;
        let hit_ratio = (denom > 0).then(|| f64::from(hit) / f64::from(denom));
        Self {
            prompt_tokens: usage.prompt_tokens,
            hit_tokens: hit,
            miss_tokens: miss,
            hit_ratio,
            usage_consistent: denom == usage.prompt_tokens,
        }
    }
}

/// Emit a `chat_completions.cache_usage` debug event from the provider's final
/// usage. Uses the enclosing tracing span for correlation; does not touch
/// `ResponseEvent` or introduce a public request identifier.
fn emit_cache_usage_diagnostics(usage: &ChunkUsage) {
    let diagnostics = CacheUsageDiagnostics::from_usage(usage);
    debug!(
        target: "chat_completions.cache_usage",
        prompt_tokens = diagnostics.prompt_tokens,
        hit_tokens = diagnostics.hit_tokens,
        miss_tokens = diagnostics.miss_tokens,
        hit_ratio = diagnostics.hit_ratio.unwrap_or(-1.0),
        usage_consistent = diagnostics.usage_consistent,
        "observed chat completions prompt cache usage"
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use codex_client::TransportError;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ReasoningItemContent;
    use codex_protocol::models::ResponseItem;
    use futures::TryStreamExt;
    use tokio::sync::mpsc;
    use tokio_test::io::Builder as IoBuilder;
    use tokio_util::io::ReaderStream;

    /// Feed raw SSE bytes through the Chat Completions parser and collect the
    /// emitted events, mirroring the real streaming path.
    async fn collect_events(chunks: &[&str]) -> Vec<Result<ResponseEvent, ApiError>> {
        let mut builder = IoBuilder::new();
        for chunk in chunks {
            builder.read(chunk.as_bytes());
        }
        let reader = builder.build();
        let stream =
            ReaderStream::new(reader).map_err(|err| TransportError::Network(err.to_string()));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(64);
        tokio::spawn(process_chat_completions_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(5),
        ));

        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        events
    }

    fn ok_events(events: Vec<Result<ResponseEvent, ApiError>>) -> Vec<ResponseEvent> {
        events
            .into_iter()
            .map(|ev| ev.expect("parser emitted an error event"))
            .collect()
    }

    fn output_item_dones(events: &[ResponseEvent]) -> Vec<&ResponseItem> {
        events
            .iter()
            .filter_map(|ev| match ev {
                ResponseEvent::OutputItemDone(item) => Some(item),
                _ => None,
            })
            .collect()
    }

    /// Extract the token usage reported by the terminal `Completed` event.
    fn completed_usage(events: &[ResponseEvent]) -> Option<TokenUsage> {
        events.iter().find_map(|ev| match ev {
            ResponseEvent::Completed { token_usage, .. } => token_usage.clone(),
            _ => None,
        })
    }

    /// DeepSeek reports prompt-cache hits via `prompt_cache_hit_tokens`; those
    /// must surface as `cached_input_tokens` in the terminal usage so the token
    /// accounting path can distinguish cached from fresh prompt tokens.
    #[tokio::test]
    async fn cache_hit_tokens_map_to_cached_input() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: {\"id\":\"cache-1\",\"choices\":[],\"usage\":{\"prompt_tokens\":1000,\"prompt_cache_hit_tokens\":900,\"prompt_cache_miss_tokens\":100,\"completion_tokens\":20,\"total_tokens\":1020}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        assert_eq!(
            completed_usage(&events),
            Some(TokenUsage {
                input_tokens: 1000,
                cached_input_tokens: 900,
                output_tokens: 20,
                reasoning_output_tokens: 0,
                total_tokens: 1020,
            })
        );
    }

    /// Providers that omit the cache fields (e.g. Qwen, older DeepSeek) must
    /// still complete, reporting zero cached input rather than failing to parse.
    #[tokio::test]
    async fn missing_cache_fields_report_zero_cached_input() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        assert_eq!(
            completed_usage(&events),
            Some(TokenUsage {
                input_tokens: 5,
                cached_input_tokens: 0,
                output_tokens: 2,
                reasoning_output_tokens: 0,
                total_tokens: 7,
            })
        );
    }

    /// When `hit + miss != prompt_tokens` we trust the provider-reported hit
    /// count verbatim and keep `input_tokens` sourced from `prompt_tokens`; the
    /// turn must still complete without reconstructing the input count.
    #[tokio::test]
    async fn inconsistent_cache_counts_trust_provider_hit_count() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: {\"id\":\"cache-2\",\"choices\":[],\"usage\":{\"prompt_tokens\":1000,\"prompt_cache_hit_tokens\":700,\"prompt_cache_miss_tokens\":100,\"completion_tokens\":20,\"total_tokens\":1020}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        assert_eq!(
            completed_usage(&events),
            Some(TokenUsage {
                input_tokens: 1000,
                cached_input_tokens: 700,
                output_tokens: 20,
                reasoning_output_tokens: 0,
                total_tokens: 1020,
            })
        );
    }

    /// Regression: an assistant text reply must be emitted exactly once as a
    /// standard `OutputItemDone(Message)`. Previously the parser emitted a
    /// `ChatOutputItemDone(OmnixMessage)` that the core turn loop ignored, so
    /// replies vanished; and the stop-flush plus end-of-stream flush could
    /// duplicate the message once it did land.
    #[tokio::test]
    async fn text_reply_emits_single_assistant_message_item() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"!\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        let items = output_item_dones(&events);
        assert_eq!(
            items.len(),
            1,
            "expected exactly one OutputItemDone, got: {events:?}"
        );
        assert_matches!(
            items[0],
            ResponseItem::Message { role, content, .. }
                if role == "assistant"
                    && content
                        == &vec![ContentItem::OutputText {
                            text: "Hi!".to_string(),
                        }]
        );

        // A streaming placeholder must open the item before deltas so the turn
        // loop has an active item to attach text to.
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, ResponseEvent::OutputItemAdded(_))),
            "expected an OutputItemAdded placeholder"
        );
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, ResponseEvent::Completed { .. })),
            "expected a Completed event"
        );
    }

    /// Reasoning models (e.g. deepseek-v4-flash) stream `reasoning_content`
    /// separately from `content`. Both must land: a `Reasoning` item followed
    /// by the assistant `Message`.
    #[tokio::test]
    async fn reasoning_then_text_emits_reasoning_and_message_items() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c2\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"Think\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Answer\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c2\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        let items = output_item_dones(&events);
        assert_eq!(
            items.len(),
            2,
            "expected reasoning + message, got: {events:?}"
        );
        assert_matches!(
            items[0],
            ResponseItem::Reasoning { content: Some(c), .. }
                if c == &vec![ReasoningItemContent::ReasoningText {
                    text: "Think".to_string(),
                }]
        );
        assert_matches!(
            items[1],
            ResponseItem::Message { role, content, .. }
                if role == "assistant"
                    && content == &vec![ContentItem::OutputText { text: "Answer".to_string() }]
        );
    }

    /// A streamed tool call must land as a standard `OutputItemDone(FunctionCall)`
    /// with the accumulated id/name/arguments.
    #[tokio::test]
    async fn tool_call_emits_function_call_item() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c3\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"shell\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c3\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"command\\\":[\\\"ls\\\"]}\"}}]},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c3\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        let items = output_item_dones(&events);
        assert_eq!(
            items.len(),
            1,
            "expected one function call, got: {events:?}"
        );
        assert_matches!(
            items[0],
            ResponseItem::FunctionCall { name, arguments, call_id, .. }
                if name == "shell"
                    && call_id == "call_1"
                    && arguments == "{\"command\":[\"ls\"]}"
        );
    }

    fn usage(prompt: u32, hit: u32, miss: u32) -> ChunkUsage {
        ChunkUsage {
            prompt_tokens: prompt,
            completion_tokens: 0,
            total_tokens: prompt,
            prompt_cache_hit_tokens: hit,
            prompt_cache_miss_tokens: miss,
        }
    }

    /// With no cache signal at all (hit=miss=0) the ratio must be unavailable,
    /// never a misleading 0.0.
    #[test]
    fn cache_ratio_unavailable_without_signal() {
        let diagnostics = CacheUsageDiagnostics::from_usage(&usage(0, 0, 0));
        assert_eq!(diagnostics.hit_ratio, None);
    }

    /// A 90/10 split yields a 0.9 token-weighted hit ratio.
    #[test]
    fn cache_ratio_reports_token_weighted_hit_rate() {
        let diagnostics = CacheUsageDiagnostics::from_usage(&usage(100, 90, 10));
        assert_eq!(diagnostics.hit_ratio, Some(0.9));
    }

    /// A full miss reports 0.0 (a real, available signal — distinct from the
    /// no-signal case above).
    #[test]
    fn cache_ratio_reports_zero_on_full_miss() {
        let diagnostics = CacheUsageDiagnostics::from_usage(&usage(100, 0, 100));
        assert_eq!(diagnostics.hit_ratio, Some(0.0));
    }

    /// Consistency is judged against `prompt_tokens` independently of the ratio:
    /// hit+miss==prompt is consistent; a mismatch is flagged.
    #[test]
    fn cache_usage_consistency_checks_prompt_tokens() {
        assert!(CacheUsageDiagnostics::from_usage(&usage(100, 90, 10)).usage_consistent);
        assert!(!CacheUsageDiagnostics::from_usage(&usage(1000, 700, 100)).usage_consistent);
    }

    /// Reasoning followed by a tool call in one stream: a `Reasoning` item must
    /// land before the `FunctionCall`, so DeepSeek thinking-mode tool turns keep
    /// their reasoning_content attached to the right call.
    #[tokio::test]
    async fn reasoning_then_tool_call_emits_reasoning_before_function_call() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"Plan\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c4\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"shell\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c4\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        let items = output_item_dones(&events);
        assert_eq!(
            items.len(),
            2,
            "expected reasoning + function call, got: {events:?}"
        );
        assert_matches!(
            items[0],
            ResponseItem::Reasoning { content: Some(c), .. }
                if c == &vec![ReasoningItemContent::ReasoningText { text: "Plan".to_string() }]
        );
        assert_matches!(
            items[1],
            ResponseItem::FunctionCall { name, call_id, .. }
                if name == "shell" && call_id == "call_1"
        );
    }

    /// Two tool calls in a single streamed response (distinct `index` slots)
    /// must both land as separate `FunctionCall` items.
    #[tokio::test]
    async fn multiple_tool_calls_in_one_response_emit_all() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c5\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"type\":\"function\",\"function\":{\"name\":\"read\",\"arguments\":\"{}\"}},{\"index\":1,\"id\":\"call_b\",\"type\":\"function\",\"function\":{\"name\":\"write\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c5\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        let calls: Vec<(&str, &str)> = output_item_dones(&events)
            .into_iter()
            .filter_map(|item| match item {
                ResponseItem::FunctionCall { name, call_id, .. } => {
                    Some((name.as_str(), call_id.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(calls, vec![("read", "call_a"), ("write", "call_b")]);
    }

    /// A malformed `data:` line (invalid JSON) must be skipped, not fatal: the
    /// surrounding well-formed chunks still parse and the turn completes.
    #[tokio::test]
    async fn malformed_chunk_is_skipped_not_fatal() {
        let events = ok_events(
            collect_events(&[
                "data: {\"id\":\"c6\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
                "data: {not valid json}\n\n",
                "data: {\"id\":\"c6\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
                "data: [DONE]\n\n",
            ])
            .await,
        );

        // No error event; the assistant message and Completed still arrive.
        let items = output_item_dones(&events);
        assert_eq!(items.len(), 1);
        assert_matches!(items[0], ResponseItem::Message { .. });
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, ResponseEvent::Completed { .. }))
        );
    }

    /// A transport error mid-stream surfaces as `ApiError::Stream`, not a panic
    /// or a silently truncated success.
    #[tokio::test]
    async fn transport_error_surfaces_as_stream_error() {
        let reader = IoBuilder::new()
            .read(b"data: {\"id\":\"c7\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n")
            .read_error(std::io::Error::other("boom"))
            .build();
        let stream =
            ReaderStream::new(reader).map_err(|err| TransportError::Network(err.to_string()));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(64);
        tokio::spawn(process_chat_completions_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(5),
        ));

        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }

        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, Err(ApiError::Stream(_)))),
            "expected an ApiError::Stream, got: {events:?}"
        );
    }

    /// When the stream stalls past the idle timeout, the parser emits a stream
    /// error rather than hanging.
    #[tokio::test(start_paused = true)]
    async fn idle_timeout_surfaces_as_stream_error() {
        let reader = IoBuilder::new()
            .read(b"data: {\"id\":\"c8\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n")
            .wait(Duration::from_secs(60))
            .build();
        let stream =
            ReaderStream::new(reader).map_err(|err| TransportError::Network(err.to_string()));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(64);
        tokio::spawn(process_chat_completions_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(5),
        ));

        let mut saw_stream_error = false;
        while let Some(ev) = rx.recv().await {
            if matches!(ev, Err(ApiError::Stream(_))) {
                saw_stream_error = true;
            }
        }
        assert!(saw_stream_error, "expected an idle-timeout stream error");
    }
}
