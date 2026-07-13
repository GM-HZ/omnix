//! Runtime-side tool abstraction.
//!
//! `omnix-sdk` owns the ergonomic `AgentTool` trait (RPITIT); it adapts a
//! registry into an object-safe [`ToolInvoker`] that this crate can hold behind
//! `Arc<dyn ...>` and drive from the event consumer. Keeping the boundary here
//! (rather than referencing `omnix-sdk`) preserves the one-way crate
//! dependency.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// A tool the runtime advertises to the model, mapped 1:1 to a
/// `DynamicToolSpec::Function`.
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A single tool invocation request routed from the agent.
#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub thread_id: String,
    pub call_id: String,
    pub tool: String,
    pub arguments: serde_json::Value,
}

/// The result of a tool invocation, lowered into a `DynamicToolCallResponse`.
#[derive(Debug, Clone)]
pub struct ToolInvocationOutput {
    /// Text content returned to the model. Empty is allowed.
    pub text: String,
    /// Whether the tool call succeeded.
    pub success: bool,
}

impl ToolInvocationOutput {
    /// A successful text result.
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            success: true,
        }
    }

    /// A failed result carrying a diagnostic message.
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            success: false,
        }
    }
}

/// Object-safe tool dispatch surface consumed by the runtime.
pub trait ToolInvoker: Send + Sync {
    /// Descriptors for every registered tool (advertised on thread start).
    fn descriptors(&self) -> Vec<ToolDescriptor>;

    /// Invoke a tool. The returned future is boxed so the trait stays
    /// object-safe; the SDK's RPITIT `AgentTool` future is erased into it.
    fn invoke(
        &self,
        invocation: ToolInvocation,
    ) -> Pin<Box<dyn Future<Output = ToolInvocationOutput> + Send + '_>>;
}

/// Execution limits for tool dispatch, derived from the runtime `ToolSpec`.
#[derive(Debug, Clone, Copy)]
pub struct ToolLimits {
    pub call_timeout: Duration,
    pub max_concurrency: usize,
}
