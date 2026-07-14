//! Object-safety shim for [`AgentTool`].
//!
//! RPITIT traits are not object-safe, so a registry cannot store
//! `Box<dyn AgentTool>`. `ErasedTool` is an internal, object-safe trait whose
//! `call` returns a boxed future; `ToolShim<T>` boxes a concrete `AgentTool`'s
//! future into it. This boxing is a runtime-internal implementation detail — the
//! public `AgentTool` trait stays RPITIT.

use std::future::Future;
use std::pin::Pin;

use super::AgentTool;
use super::ToolCallContext;
use super::ToolError;
use super::ToolOutput;
use super::ToolSpecification;

/// Object-safe counterpart to [`AgentTool`].
pub(crate) trait ErasedTool: Send + Sync {
    fn specification(&self) -> ToolSpecification;

    fn call(
        &self,
        input: serde_json::Value,
        context: ToolCallContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + '_>>;
}

/// Wraps a concrete [`AgentTool`], boxing its future to satisfy [`ErasedTool`].
pub(crate) struct ToolShim<T: AgentTool>(pub(crate) T);

impl<T: AgentTool> ErasedTool for ToolShim<T> {
    fn specification(&self) -> ToolSpecification {
        self.0.specification()
    }

    fn call(
        &self,
        input: serde_json::Value,
        context: ToolCallContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + '_>> {
        Box::pin(self.0.call(input, context))
    }
}
