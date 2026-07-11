use crate::auth::SharedAuthProvider;
use crate::chat_completions::sse_parser::spawn_chat_completions_stream;
use crate::chat_completions::types::ChatCompletionsRequest;
use crate::common::ResponseStream;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use http::HeaderMap;
use http::Method;
use std::sync::Arc;
use std::time::Duration;
use tracing::instrument;

const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

pub struct ChatCompletionsClient<T: HttpTransport> {
    session: EndpointSession<T>,
}

impl<T: HttpTransport> ChatCompletionsClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
        }
    }

    pub fn with_telemetry(self, request: Option<Arc<dyn RequestTelemetry>>) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
        }
    }

    #[instrument(skip_all, fields(model = %request.model))]
    pub async fn stream_request(
        &self,
        request: ChatCompletionsRequest,
        extra_headers: HeaderMap,
    ) -> Result<ResponseStream, ApiError> {
        let body = EncodedJsonBody::encode(&request).map_err(|e| ApiError::InvalidRequest {
            message: format!("Failed to serialize request: {e}"),
        })?;

        let path = "/chat/completions";
        let stream_response = self
            .session
            .stream_encoded_json_with(Method::POST, path, extra_headers, Some(body), |_| {})
            .await?;

        Ok(spawn_chat_completions_stream(
            stream_response,
            DEFAULT_IDLE_TIMEOUT,
        ))
    }
}
