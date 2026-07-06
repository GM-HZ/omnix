// Stub: Responses SSE parser removed per slim-agent-loop design.

use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::common::SafetyBuffering;
use crate::common::SafetyBufferingTreatment;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::StreamResponse;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;

/// Stub — Responses API SSE is no longer used.
pub fn spawn_response_stream(
    _stream_response: StreamResponse,
    _idle_timeout: Duration,
    _telemetry: Option<Arc<dyn SseTelemetry>>,
    _turn_state: Option<Arc<OnceLock<String>>>,
) -> ResponseStream {
    let (tx, rx) = mpsc::channel(1);
    let _ = tx.try_send(Err(ApiError::Stream("Responses API removed".into())));
    ResponseStream { rx_event: rx, upstream_request_id: None }
}

/// Stub — no longer processes Responses SSE events.
pub fn process_responses_event(_event: &ResponsesStreamEvent) -> Option<ResponseEvent> { None }

/// Stub — Responses stream event types preserved for API compatibility.
#[derive(Deserialize, Debug)]
pub struct ResponsesStreamEvent {
    #[serde(rename = "type")]
    kind: String,
    headers: Option<Value>,
    metadata: Option<Value>,
    response: Option<Value>,
    item: Option<Value>,
    item_id: Option<String>,
    call_id: Option<String>,
    delta: Option<String>,
    summary_index: Option<i64>,
    content_index: Option<i64>,
    safety_buffering: Option<Value>,
}

impl ResponsesStreamEvent {
    pub fn kind(&self) -> &str { &self.kind }
    pub fn response_model(&self) -> Option<String> { None }
    pub fn turn_state(&self) -> Option<String> { None }
    pub fn model_verifications(&self) -> Option<Vec<codex_protocol::protocol::ModelVerification>> { None }
    pub fn turn_moderation_metadata(&self) -> Option<codex_protocol::protocol::TurnModerationMetadataEvent> { None }
    pub fn safety_buffering(&self, _treatment: &SafetyBufferingTreatment) -> Option<SafetyBuffering> { None }
}
