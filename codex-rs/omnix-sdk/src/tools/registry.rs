//! Tool registry: collects host tools and adapts them into the runtime's
//! object-safe [`ToolInvoker`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use omnix_runtime::ToolDescriptor;
use omnix_runtime::ToolInvocation;
use omnix_runtime::ToolInvocationOutput;
use omnix_runtime::ToolInvoker;

use super::AgentTool;
use super::ToolCallContext;
use super::erased::ErasedTool;
use super::erased::ToolShim;
use crate::error::OmnixError;
use crate::error::OmnixErrorKind;

/// A registry of host-provided tools.
///
/// Register tools with [`ToolRegistry::register`]; the runtime advertises their
/// specifications to the model and routes calls back to them. Passed to
/// [`crate::OmnixBuilder::tools`].
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ErasedTool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. Returns [`OmnixErrorKind::InvalidConfig`] on a name
    /// conflict.
    pub fn register<T: AgentTool>(&mut self, tool: T) -> Result<&mut Self, OmnixError> {
        let spec = tool.specification();
        if self.tools.contains_key(&spec.name) {
            return Err(OmnixError::new(
                OmnixErrorKind::InvalidConfig,
                format!("a tool named `{}` is already registered", spec.name),
            ));
        }
        self.tools.insert(spec.name, Arc::new(ToolShim(tool)));
        Ok(self)
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Adapt into a runtime tool invoker, or `None` when empty.
    pub(crate) fn into_invoker(self) -> Option<Arc<dyn ToolInvoker>> {
        if self.tools.is_empty() {
            return None;
        }
        Some(Arc::new(RegistryInvoker { tools: self.tools }))
    }
}

/// Runtime-facing adapter over the registered tools.
struct RegistryInvoker {
    tools: HashMap<String, Arc<dyn ErasedTool>>,
}

impl ToolInvoker for RegistryInvoker {
    fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools
            .values()
            .map(|tool| {
                let spec = tool.specification();
                ToolDescriptor {
                    name: spec.name,
                    description: spec.description,
                    input_schema: spec.input_schema,
                }
            })
            .collect()
    }

    fn invoke(
        &self,
        invocation: ToolInvocation,
    ) -> Pin<Box<dyn Future<Output = ToolInvocationOutput> + Send + '_>> {
        let tool = self.tools.get(&invocation.tool).cloned();
        Box::pin(async move {
            let Some(tool) = tool else {
                return ToolInvocationOutput::error(format!("unknown tool `{}`", invocation.tool));
            };
            let context = ToolCallContext {
                session_id: invocation.thread_id,
                call_id: invocation.call_id,
            };
            match tool.call(invocation.arguments, context).await {
                Ok(output) => ToolInvocationOutput::ok(output.text),
                Err(err) => ToolInvocationOutput::error(err.message),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::OmnixErrorKind;
    use crate::tools::AgentTool;
    use crate::tools::ToolOutput;
    use crate::tools::ToolSpecification;

    struct NamedTool(&'static str);

    impl AgentTool for NamedTool {
        fn specification(&self) -> ToolSpecification {
            ToolSpecification {
                name: self.0.to_string(),
                description: "test".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            }
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: super::ToolCallContext,
        ) -> Result<ToolOutput, crate::tools::ToolError> {
            Ok(ToolOutput::text("ok"))
        }
    }

    #[test]
    fn register_detects_name_conflicts() {
        let mut registry = ToolRegistry::new();
        registry
            .register(NamedTool("dup"))
            .expect("first registers");
        let err = registry
            .register(NamedTool("dup"))
            .map(|_| ())
            .expect_err("duplicate must be rejected");
        assert_eq!(err.kind(), OmnixErrorKind::InvalidConfig);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn empty_registry_yields_no_invoker() {
        assert!(ToolRegistry::new().into_invoker().is_none());
    }

    #[test]
    fn non_empty_registry_advertises_descriptors() {
        let mut registry = ToolRegistry::new();
        registry.register(NamedTool("a")).unwrap();
        registry.register(NamedTool("b")).unwrap();
        let invoker = registry.into_invoker().expect("invoker");
        let mut names: Vec<String> = invoker.descriptors().into_iter().map(|d| d.name).collect();
        names.sort();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }
}
