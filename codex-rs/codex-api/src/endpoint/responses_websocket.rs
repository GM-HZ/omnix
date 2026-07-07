// Stub: Responses WebSocket removed per slim-agent-loop design.
// Type definitions preserved for client.rs API compatibility.

use crate::auth::SharedAuthProvider;
use crate::common::ResponseStream;
use crate::common::ResponsesWsRequest;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::telemetry::WebsocketTelemetry;
use http::HeaderMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

pub struct ResponsesWebsocketConnection {
    pub connection_reused: bool,
}

impl std::fmt::Debug for ResponsesWebsocketConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponsesWebsocketConnection").finish()
    }
}

pub struct ResponsesWebsocketClient {
    _provider: Provider,
    _auth: SharedAuthProvider,
}

pub struct ResponsesWebsocketClose {
    pub code: String,
    pub reason: String,
}

pub struct ResponsesWebsocketProbe {
    pub url: String,
    pub status: u16,
    pub reasoning_included: bool,
    pub models_etag_present: bool,
    pub server_model_present: bool,
    pub immediate_close: Option<ResponsesWebsocketClose>,
}

impl ResponsesWebsocketClient {
    pub fn new(provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            _provider: provider,
            _auth: auth,
        }
    }

    #[tracing::instrument(
        name = "responses_websocket.connect",
        level = "info",
        skip_all,
        fields(transport = "responses_websocket", api.path = "responses")
    )]
    pub async fn connect(
        &self,
        _extra_headers: HeaderMap,
        _default_headers: HeaderMap,
        _turn_state: Option<Arc<std::sync::OnceLock<String>>>,
        _telemetry: Option<Arc<dyn WebsocketTelemetry>>,
    ) -> Result<ResponsesWebsocketConnection, ApiError> {
        Err(ApiError::Stream(
            "Responses WebSocket removed per slim-agent-loop design".into(),
        ))
    }

    pub async fn probe_handshake(
        &self,
        _extra_headers: HeaderMap,
        _default_headers: HeaderMap,
        _immediate_close_timeout: std::time::Duration,
    ) -> Result<ResponsesWebsocketProbe, ApiError> {
        Err(ApiError::Stream(
            "Responses WebSocket removed per slim-agent-loop design".into(),
        ))
    }
}

impl ResponsesWebsocketConnection {
    pub async fn is_closed(&self) -> bool {
        true
    }

    pub async fn close(self) -> Result<ResponsesWebsocketClose, ApiError> {
        Ok(ResponsesWebsocketClose {
            code: "1000".into(),
            reason: "WebSocket removed".into(),
        })
    }

    pub async fn ws_id(&self) -> Option<String> {
        None
    }

    pub async fn ws_probe(&self) -> Option<ResponsesWebsocketProbe> {
        None
    }

    pub async fn stream_request(
        &self,
        _request: ResponsesWsRequest,
        _connection_reused: bool,
        _turn_state: Option<Arc<std::sync::OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        Err(ApiError::Stream(
            "Responses WebSocket removed per slim-agent-loop design".into(),
        ))
    }
}
