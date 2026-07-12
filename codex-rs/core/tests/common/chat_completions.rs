//! Mock Chat Completions (DeepSeek-style) SSE server for offline tests.
//!
//! Mirrors [`super::responses`] but speaks the Chat Completions wire format: a
//! stream of bare `data: {json}\n\n` lines terminated by `data: [DONE]\n\n`
//! (no `event:` type prefix), posted to `/v1/chat/completions`. This is the
//! foundation that lets both the core and app-server suites exercise the
//! supported DeepSeek path — not just the Responses API.
//!
//! Chunk fixtures produce the exact shapes the parser in
//! `codex-api/src/chat_completions/sse_parser.rs` consumes. Compose them with
//! [`cc_sse`] into a stream body, then mount with [`mount_chat_completions_once`]
//! or [`mount_chat_completions_sequence`].

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use serde_json::Value;
use serde_json::json;
use wiremock::Match;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

/// A captured Chat Completions request, exposing its parsed JSON body.
#[derive(Clone, Debug)]
pub struct ChatCompletionsRequest(pub wiremock::Request);

impl ChatCompletionsRequest {
    pub fn body_json(&self) -> Value {
        serde_json::from_slice(&self.0.body).expect("request body should be valid JSON")
    }

    /// The `messages` array from the request body.
    pub fn messages(&self) -> Vec<Value> {
        self.body_json()
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    /// The `tools` array from the request body (empty if omitted).
    pub fn tools(&self) -> Vec<Value> {
        self.body_json()
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }
}

/// Captures every Chat Completions request the mock receives, for assertions
/// about outgoing history/tools (e.g. reasoning_content and tool_call_id
/// round-tripping on continuation).
#[derive(Clone)]
pub struct ChatMock {
    requests: Arc<Mutex<Vec<ChatCompletionsRequest>>>,
}

impl ChatMock {
    fn new() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn requests(&self) -> Vec<ChatCompletionsRequest> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn last_request(&self) -> Option<ChatCompletionsRequest> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last()
            .cloned()
    }

    pub fn request_count(&self) -> usize {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

impl Match for ChatMock {
    fn matches(&self, request: &wiremock::Request) -> bool {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(ChatCompletionsRequest(request.clone()));
        true
    }
}

/// Serialize chunks into a Chat Completions SSE body: one bare `data: {json}`
/// line per chunk, terminated by `data: [DONE]`.
pub fn cc_sse(chunks: Vec<Value>) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for chunk in chunks {
        write!(&mut out, "data: {chunk}\n\n").expect("write chunk");
    }
    out.push_str("data: [DONE]\n\n");
    out
}

/// First chunk of an assistant text response: role + initial content delta.
pub fn cc_role_content(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": null}]
    })
}

/// A continuation content delta (no role).
pub fn cc_content_delta(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": null}]
    })
}

/// A reasoning_content delta (DeepSeek thinking mode).
pub fn cc_reasoning(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "choices": [{"index": 0, "delta": {"reasoning_content": text}, "finish_reason": null}]
    })
}

/// Opening chunk for a tool call at `index`: id, name, and (optional) initial
/// arguments.
pub fn cc_tool_call_open(
    id: &str,
    index: u32,
    call_id: &str,
    name: &str,
    arguments: &str,
) -> Value {
    json!({
        "id": id,
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{
                "index": index,
                "id": call_id,
                "type": "function",
                "function": {"name": name, "arguments": arguments}
            }]},
            "finish_reason": null
        }]
    })
}

/// An arguments continuation chunk for the tool call at `index`.
pub fn cc_tool_call_args(id: &str, index: u32, arguments: &str) -> Value {
    json!({
        "id": id,
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{"index": index, "function": {"arguments": arguments}}]},
            "finish_reason": null
        }]
    })
}

/// A terminal chunk carrying only a `finish_reason` (e.g. "stop" or
/// "tool_calls"), no delta content.
pub fn cc_finish(id: &str, finish_reason: &str) -> Value {
    json!({
        "id": id,
        "choices": [{"index": 0, "delta": {}, "finish_reason": finish_reason}]
    })
}

/// A final usage-only chunk (empty choices), mirroring how DeepSeek reports
/// usage — including prompt-cache hit/miss counts.
pub fn cc_usage(id: &str, prompt: u32, hit: u32, miss: u32, completion: u32) -> Value {
    json!({
        "id": id,
        "choices": [],
        "usage": {
            "prompt_tokens": prompt,
            "prompt_cache_hit_tokens": hit,
            "prompt_cache_miss_tokens": miss,
            "completion_tokens": completion,
            "total_tokens": prompt + completion
        }
    })
}

/// Convenience: a complete assistant text turn (role+content, stop, usage).
pub fn cc_text_turn(id: &str, text: &str, prompt: u32, completion: u32) -> String {
    cc_sse(vec![
        cc_role_content(id, text),
        cc_finish(id, "stop"),
        cc_usage(id, prompt, /*hit*/ 0, /*miss*/ prompt, completion),
    ])
}

fn cc_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_raw(body, "text/event-stream")
}

fn base_chat_mock() -> (wiremock::MockBuilder, ChatMock) {
    let chat_mock = ChatMock::new();
    let mock = Mock::given(method("POST"))
        .and(path_regex(".*/chat/completions$"))
        .and(chat_mock.clone());
    (mock, chat_mock)
}

/// Mount a single Chat Completions SSE response.
pub async fn mount_chat_completions_once(server: &MockServer, body: String) -> ChatMock {
    let (mock, chat_mock) = base_chat_mock();
    mock.respond_with(cc_response(body))
        .up_to_n_times(1)
        .mount(server)
        .await;
    chat_mock
}

/// Mount a sequence of Chat Completions SSE responses, one per POST, in order.
/// Panics if more requests arrive than bodies provided.
pub async fn mount_chat_completions_sequence(server: &MockServer, bodies: Vec<String>) -> ChatMock {
    struct SeqResponder {
        num_calls: AtomicUsize,
        responses: Vec<String>,
    }

    impl Respond for SeqResponder {
        fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
            let call_num = self.num_calls.fetch_add(1, Ordering::SeqCst);
            let body = self
                .responses
                .get(call_num)
                .unwrap_or_else(|| panic!("no chat completions response for call {call_num}"));
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body.clone())
        }
    }

    let num_calls = bodies.len() as u64;
    let responder = SeqResponder {
        num_calls: AtomicUsize::new(0),
        responses: bodies,
    };

    let (mock, chat_mock) = base_chat_mock();
    mock.respond_with(responder)
        .up_to_n_times(num_calls)
        .expect(num_calls)
        .mount(server)
        .await;
    chat_mock
}

/// Start a hermetic mock server with a default `/models` response (reused from
/// the Responses harness) plus whatever chat-completions mounts a test adds.
pub async fn start_mock_chat_completions_server() -> MockServer {
    super::responses::start_mock_server().await
}
