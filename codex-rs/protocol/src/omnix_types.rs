//! New internal message types for the Omnix agent.
//!
//! `OmnixMessage` is designed to map directly to the Chat Completions API
//! `messages[]` wire format, serving as both the internal conversation
//! representation and the serialization target for Qwen/DeepSeek/OpenAI-
//! compatible providers.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use ts_rs::TS;

/// A single content part inside a user message (text or image).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OmnixContentPart {
    Text {
        text: String,
    },
    #[serde(rename = "image_url")]
    ImageUrl {
        image_url: OmnixImageUrl,
    },
}

/// Image URL reference with optional detail level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct OmnixImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A tool call issued by the assistant, matching the Chat Completions
/// `tool_calls[]` wire format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct OmnixToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: OmnixFunctionCall,
}

impl OmnixToolCall {
    pub fn new(id: String, name: String, arguments: String) -> Self {
        Self {
            id,
            kind: "function".to_string(),
            function: OmnixFunctionCall { name, arguments },
        }
    }
}

/// Function name and arguments within a tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct OmnixFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// User content: either a plain string or an array of content parts
/// (text + images). Serializes as a JSON string when there is only text,
/// or as an array when multimodal.
#[derive(Debug, Clone, PartialEq, JsonSchema, TS)]
pub enum OmnixUserContent {
    Text(String),
    Parts(Vec<OmnixContentPart>),
}

impl Serialize for OmnixUserContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(text) => serializer.serialize_str(text),
            Self::Parts(parts) => parts.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for OmnixUserContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(s) => Ok(Self::Text(s)),
            serde_json::Value::Array(_) => {
                let parts: Vec<OmnixContentPart> =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Parts(parts))
            }
            _ => Err(serde::de::Error::custom(
                "expected string or array for user content",
            )),
        }
    }
}

/// A single message in the conversation history.
///
/// Maps directly to one entry in the Chat Completions `messages[]` array.
/// The `#[serde(tag = "role")]` attribute produces the `"role": "..."` field
/// that providers expect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum OmnixMessage {
    System {
        content: String,
    },
    User {
        content: OmnixUserContent,
    },
    Assistant {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<OmnixToolCall>,
        /// Thinking/reasoning text from models like DeepSeek-R1 or Qwen3.
        ///
        /// Serialized as `reasoning_content` to match the Chat Completions
        /// wire field. DeepSeek's thinking mode requires the reasoning that
        /// preceded a tool call to be passed back on the assistant message
        /// that carries those `tool_calls`, otherwise the follow-up request
        /// is rejected with `invalid_request_error`.
        #[serde(
            default,
            rename = "reasoning_content",
            skip_serializing_if = "Option::is_none"
        )]
        thinking: Option<String>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

impl OmnixMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
        }
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self::User {
            content: OmnixUserContent::Text(text.into()),
        }
    }

    pub fn user_parts(parts: Vec<OmnixContentPart>) -> Self {
        Self::User {
            content: OmnixUserContent::Parts(parts),
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::Assistant {
            content: Some(text.into()),
            tool_calls: Vec::new(),
            thinking: None,
        }
    }

    pub fn assistant_with_thinking(text: Option<String>, thinking: impl Into<String>) -> Self {
        Self::Assistant {
            content: text,
            tool_calls: Vec::new(),
            thinking: Some(thinking.into()),
        }
    }

    pub fn assistant_tool_calls(tool_calls: Vec<OmnixToolCall>) -> Self {
        Self::Assistant {
            content: None,
            tool_calls,
            thinking: None,
        }
    }

    /// Assistant message carrying tool calls plus the reasoning that preceded
    /// them. DeepSeek's thinking mode rejects tool-call turns whose
    /// `reasoning_content` is not passed back, so the agentic loop must
    /// reattach it here.
    pub fn assistant_tool_calls_with_thinking(
        tool_calls: Vec<OmnixToolCall>,
        thinking: Option<String>,
    ) -> Self {
        Self::Assistant {
            content: None,
            tool_calls,
            thinking,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }

    /// Returns true if this is an assistant message that only carries
    /// thinking/reasoning content (no text, no tool calls). These messages
    /// should be filtered out before building API requests.
    pub fn is_thinking_only(&self) -> bool {
        matches!(
            self,
            Self::Assistant {
                content: None,
                tool_calls,
                thinking: Some(_),
            } if tool_calls.is_empty()
        )
    }

    /// Returns the role name as a string.
    pub fn role(&self) -> &'static str {
        match self {
            Self::System { .. } => "system",
            Self::User { .. } => "user",
            Self::Assistant { .. } => "assistant",
            Self::Tool { .. } => "tool",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_message_serialization() {
        let msg = OmnixMessage::system("You are a helpful assistant.");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "system");
        assert_eq!(json["content"], "You are a helpful assistant.");
    }

    #[test]
    fn test_user_text_serialization() {
        let msg = OmnixMessage::user_text("Hello");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn test_user_parts_serialization() {
        let msg = OmnixMessage::user_parts(vec![
            OmnixContentPart::Text {
                text: "What is this?".into(),
            },
            OmnixContentPart::ImageUrl {
                image_url: OmnixImageUrl {
                    url: "https://example.com/img.png".into(),
                    detail: Some("high".into()),
                },
            },
        ]);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert!(json["content"].is_array());
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][1]["type"], "image_url");
    }

    #[test]
    fn test_assistant_text_serialization() {
        let msg = OmnixMessage::assistant_text("Hello there!");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"], "Hello there!");
        assert!(json.get("tool_calls").is_none());
        assert!(json.get("reasoning_content").is_none());
    }

    #[test]
    fn test_assistant_tool_calls_serialization() {
        let msg = OmnixMessage::assistant_tool_calls(vec![OmnixToolCall::new(
            "call_123".into(),
            "shell".into(),
            r#"{"command":"ls"}"#.into(),
        )]);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert!(json["content"].is_null());
        assert_eq!(json["tool_calls"][0]["id"], "call_123");
        assert_eq!(json["tool_calls"][0]["type"], "function");
        assert_eq!(json["tool_calls"][0]["function"]["name"], "shell");
    }

    #[test]
    fn test_tool_result_serialization() {
        let msg = OmnixMessage::tool_result("call_123", "file1.txt\nfile2.txt");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_123");
        assert_eq!(json["content"], "file1.txt\nfile2.txt");
    }

    #[test]
    fn test_assistant_with_thinking() {
        let msg =
            OmnixMessage::assistant_with_thinking(Some("The answer is 4.".into()), "2+2=4...");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"], "The answer is 4.");
        assert_eq!(json["reasoning_content"], "2+2=4...");
    }

    #[test]
    fn test_assistant_tool_calls_with_thinking_serialization() {
        let msg = OmnixMessage::assistant_tool_calls_with_thinking(
            vec![OmnixToolCall::new(
                "call_1".into(),
                "shell".into(),
                r#"{"command":["ls"]}"#.into(),
            )],
            Some("I should list the files.".into()),
        );
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["tool_calls"][0]["id"], "call_1");
        // DeepSeek thinking mode requires the reasoning on the tool-call turn.
        assert_eq!(json["reasoning_content"], "I should list the files.");
    }

    #[test]
    fn test_thinking_only_detection() {
        let thinking = OmnixMessage::assistant_with_thinking(None, "Let me think about this...");
        assert!(thinking.is_thinking_only());

        let text = OmnixMessage::assistant_text("Hello");
        assert!(!text.is_thinking_only());

        let tool_calls = OmnixMessage::assistant_tool_calls(vec![OmnixToolCall::new(
            "c1".into(),
            "f".into(),
            "{}".into(),
        )]);
        assert!(!tool_calls.is_thinking_only());
    }

    #[test]
    fn test_user_text_roundtrip() {
        let msg = OmnixMessage::user_text("Hello world");
        let json_str = serde_json::to_string(&msg).unwrap();
        let deserialized: OmnixMessage = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_assistant_tool_calls_roundtrip() {
        let msg = OmnixMessage::assistant_tool_calls(vec![
            OmnixToolCall::new("c1".into(), "shell".into(), r#"{"cmd":"ls"}"#.into()),
            OmnixToolCall::new("c2".into(), "read".into(), r#"{"path":"/"}"#.into()),
        ]);
        let json_str = serde_json::to_string(&msg).unwrap();
        let deserialized: OmnixMessage = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_tool_result_roundtrip() {
        let msg = OmnixMessage::tool_result("call_abc", "output data");
        let json_str = serde_json::to_string(&msg).unwrap();
        let deserialized: OmnixMessage = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, deserialized);
    }
}
