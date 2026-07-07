// Stub: Responses API removed per slim-agent-loop design.
// Type definitions preserved for client.rs API compatibility.

use crate::auth::SharedAuthProvider;
use crate::common::ResponseStream;
use crate::common::ResponsesApiRequest;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::requests::Compression;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use http::HeaderMap;
use std::sync::Arc;
use std::sync::OnceLock;

pub struct ResponsesClient<T: HttpTransport> {
    _transport: T,
    _provider: Provider,
    _auth: SharedAuthProvider,
    _telemetry: Option<Arc<dyn RequestTelemetry>>,
    _sse_telemetry: Option<Arc<dyn crate::telemetry::SseTelemetry>>,
}

#[derive(Default)]
pub struct ResponsesOptions {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub session_source: Option<codex_protocol::protocol::SessionSource>,
    pub extra_headers: HeaderMap,
    pub compression: Compression,
    pub turn_state: Option<Arc<OnceLock<String>>>,
}

impl<T: HttpTransport> ResponsesClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            _transport: transport,
            _provider: provider,
            _auth: auth,
            _telemetry: None,
            _sse_telemetry: None,
        }
    }

    pub fn with_telemetry(
        self,
        _request: Option<Arc<dyn RequestTelemetry>>,
        _sse: Option<Arc<dyn crate::telemetry::SseTelemetry>>,
    ) -> Self {
        Self { _sse_telemetry: _sse, ..self }
    }

    pub async fn stream_request(
        &self,
        _request: ResponsesApiRequest,
        _options: ResponsesOptions,
    ) -> Result<ResponseStream, ApiError> {
        Err(ApiError::Stream("Responses API removed per slim-agent-loop design".into()))
    }

    pub async fn stream(
        &self,
        _body: serde_json::Value,
        _extra_headers: HeaderMap,
        _compression: Compression,
        _turn_state: Option<Arc<OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        Err(ApiError::Stream("Responses API removed per slim-agent-loop design".into()))
    }

    async fn stream_encoded(
        &self,
        _body: codex_client::EncodedJsonBody,
        _extra_headers: HeaderMap,
        _compression: Compression,
        _turn_state: Option<Arc<OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        Err(ApiError::Stream("Responses API removed per slim-agent-loop design".into()))
    }
}
