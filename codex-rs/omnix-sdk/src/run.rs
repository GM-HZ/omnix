//! Public streaming run handle.

use omnix_runtime::Run;

use crate::OmnixError;
use crate::OmnixErrorKind;
use crate::event::AgentEvent;

const MAX_OUTPUT_SCHEMA_FRAGMENT_TOKENS: usize = 1_000;

/// Per-run generation options.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct RunConfig {
    pub(crate) output_schema: Option<serde_json::Value>,
}

impl RunConfig {
    /// Request a valid JSON object shaped according to `schema`.
    ///
    /// DeepSeek JSON Output guarantees syntactically valid JSON, while the
    /// schema is model guidance rather than provider-side strict validation.
    /// The host application must validate the returned object before commit.
    pub fn json(schema: serde_json::Value) -> Self {
        Self {
            output_schema: Some(schema),
        }
    }

    pub(crate) fn into_runtime(self) -> Result<omnix_runtime::RunConfig, OmnixError> {
        if let Some(schema) = self.output_schema.as_ref() {
            if !schema.is_object() {
                return Err(OmnixError::new(
                    OmnixErrorKind::InvalidConfig,
                    "output schema must be a JSON object",
                ));
            }
            let guidance = format!(
                "<json_output_schema>\nReturn the final answer as a JSON object matching this JSON Schema. Do not wrap the JSON in Markdown fences.\n{schema}\n</json_output_schema>"
            );
            if codex_utils_output_truncation::approx_token_count(&guidance)
                > MAX_OUTPUT_SCHEMA_FRAGMENT_TOKENS
            {
                return Err(OmnixError::new(
                    OmnixErrorKind::InvalidConfig,
                    format!(
                        "output schema guidance exceeds {MAX_OUTPUT_SCHEMA_FRAGMENT_TOKENS} approximate tokens"
                    ),
                ));
            }
        }
        Ok(omnix_runtime::RunConfig {
            output_schema: self.output_schema,
        })
    }
}

/// A single agent run (one turn). Yields [`AgentEvent`]s in order until a
/// terminal event or the stream closes.
///
/// ```no_run
/// # async fn drive(mut run: omnix_sdk::AgentRun) -> Result<(), omnix_sdk::OmnixError> {
/// while let Some(event) = run.next().await {
///     match event {
///         omnix_sdk::AgentEvent::MessageDelta { delta, .. } => print!("{delta}"),
///         omnix_sdk::AgentEvent::Completed(_) => break,
///         _ => {}
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct AgentRun {
    inner: Run,
}

impl AgentRun {
    pub(crate) fn new(inner: Run) -> Self {
        Self { inner }
    }

    /// The server-assigned turn id for this run.
    pub fn turn_id(&self) -> &str {
        self.inner.turn_id()
    }

    /// Await the next event, or `None` once the run is finished.
    pub async fn next(&mut self) -> Option<AgentEvent> {
        self.inner.next().await.map(AgentEvent::from)
    }
}
