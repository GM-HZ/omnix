use crate::auth::SharedAuthProvider;
use crate::chat_completions::sse_parser::spawn_chat_completions_stream;
use crate::chat_completions::types::ChatCompletionsOptions;
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
use serde::Serialize;
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
        self.stream_request_with_options(request, extra_headers, ChatCompletionsOptions::default())
            .await
    }

    #[instrument(skip_all, fields(model = %request.model))]
    pub async fn stream_request_with_options(
        &self,
        request: ChatCompletionsRequest,
        extra_headers: HeaderMap,
        options: ChatCompletionsOptions,
    ) -> Result<ResponseStream, ApiError> {
        #[derive(Serialize)]
        struct RequestBody<'a> {
            #[serde(flatten)]
            request: &'a ChatCompletionsRequest,
            #[serde(skip_serializing_if = "Option::is_none")]
            response_format:
                &'a Option<crate::chat_completions::types::ChatCompletionsResponseFormat>,
        }

        let body = RequestBody {
            request: &request,
            response_format: &options.response_format,
        };
        let body = EncodedJsonBody::encode(&body).map_err(|e| ApiError::InvalidRequest {
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
