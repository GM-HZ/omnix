use super::ContextualUserFragment;
use codex_utils_output_truncation::approx_token_count;
use serde_json::Value;

const MAX_FRAGMENT_TOKENS: usize = 1_000;

/// Bounded schema guidance for Chat Completions JSON Output.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JsonOutputSchemaInstructions {
    body: String,
}

impl JsonOutputSchemaInstructions {
    pub(crate) fn new(schema: &Value) -> Option<Self> {
        let body = format!(
            "\nReturn the final answer as a JSON object matching this JSON Schema. Do not wrap the JSON in Markdown fences.\n{schema}\n"
        );
        let (start, end) = Self::type_markers();
        if approx_token_count(&format!("{start}{body}{end}")) > MAX_FRAGMENT_TOKENS {
            return None;
        }
        Some(Self { body })
    }
}

impl ContextualUserFragment for JsonOutputSchemaInstructions {
    fn role(&self) -> &'static str {
        "user"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        ("<json_output_schema>", "</json_output_schema>")
    }

    fn body(&self) -> String {
        self.body.clone()
    }
}
