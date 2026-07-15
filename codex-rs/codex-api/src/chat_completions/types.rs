use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

/// Request body for the Chat Completions API (`/v1/chat/completions`).
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Qwen3: enable thinking/reasoning mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    /// Qwen3: budget for thinking tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
    /// DeepSeek: thinking configuration (`{ type: "enabled"|"disabled"|"annotated", budget?: int }`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<DeepSeekThinking>,
}

/// Output format supported by the Chat Completions provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ChatCompletionsResponseFormat {
    /// Require the provider to return a syntactically valid JSON object.
    JsonObject,
}

/// Optional provider-specific fields for a Chat Completions request.
///
/// Keeping these options separate preserves the stable request payload type
/// while allowing callers to opt into capabilities that are not universally
/// supported by OpenAI-compatible providers.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ChatCompletionsOptions {
    pub(crate) response_format: Option<ChatCompletionsResponseFormat>,
}

impl ChatCompletionsOptions {
    /// Require a syntactically valid JSON object from providers that support
    /// the OpenAI-compatible JSON mode.
    pub fn json_object() -> Self {
        Self {
            response_format: Some(ChatCompletionsResponseFormat::JsonObject),
        }
    }
}

/// DeepSeek thinking/reasoning control.
/// Maps to the `thinking` field in Chat Completions requests.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeepSeekThinking {
    /// Disable thinking entirely (fast mode).
    Disabled,
    /// Enable thinking with default budget.
    Enabled,
    /// Enable thinking with explicit token budget.
    Annotated { budget: u32 },
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

/// A single chunk from a streamed Chat Completions response.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    #[serde(default)]
    pub usage: Option<ChunkUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChunkDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ChunkDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    /// DeepSeek-R1 reasoning content field.
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ChunkDeltaToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkDeltaToolCall {
    pub index: u32,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<ChunkDeltaFunction>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkDeltaFunction {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
    /// DeepSeek: prompt tokens served from the provider-side prompt cache.
    #[serde(default)]
    pub prompt_cache_hit_tokens: u32,
    /// DeepSeek: prompt tokens that missed the provider-side prompt cache.
    #[serde(default)]
    pub prompt_cache_miss_tokens: u32,
}
