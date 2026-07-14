//! Public tool API.
//!
//! Hosts implement [`AgentTool`] (an ergonomic RPITIT trait, per the workspace
//! convention of avoiding `#[async_trait]` on public traits) and register
//! instances with a [`ToolRegistry`]. The registry adapts them into the runtime's
//! object-safe `ToolInvoker` via an internal boxing shim (see `erased`).

mod erased;
mod registry;

use std::future::Future;

pub use registry::ToolRegistry;

/// A tool's advertised interface.
#[derive(Debug, Clone)]
pub struct ToolSpecification {
    /// Unique tool name (the model calls this).
    pub name: String,
    /// Human/model-readable description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's input arguments.
    pub input_schema: serde_json::Value,
}

/// Context passed to a tool invocation.
#[derive(Debug, Clone)]
pub struct ToolCallContext {
    /// The session (thread) id the call belongs to.
    pub session_id: String,
    /// The tool call id assigned by the agent.
    pub call_id: String,
}

/// A tool's successful output.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Text returned to the model.
    pub text: String,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// A tool failure. Surfaced to the model as an unsuccessful tool result.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A host-implemented tool.
///
/// The `call` future must be `Send`. This trait is intentionally RPITIT (no
/// `#[async_trait]`); the registry erases it into an object-safe form
/// internally.
pub trait AgentTool: Send + Sync + 'static {
    /// The tool's advertised specification.
    fn specification(&self) -> ToolSpecification;

    /// Invoke the tool with JSON `input` and a call `context`.
    fn call(
        &self,
        input: serde_json::Value,
        context: ToolCallContext,
    ) -> impl Future<Output = Result<ToolOutput, ToolError>> + Send;
}
