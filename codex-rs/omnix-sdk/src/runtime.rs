//! Public runtime handle.

use omnix_runtime::Runtime;

use crate::error::OmnixError;
use crate::error::OmnixErrorKind;
use crate::session::Sessions;

/// Health of the runtime, surfaced to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeHealth {
    Ready,
    ShutDown,
}

impl From<omnix_runtime::RuntimeHealth> for RuntimeHealth {
    fn from(value: omnix_runtime::RuntimeHealth) -> Self {
        match value {
            omnix_runtime::RuntimeHealth::Ready => RuntimeHealth::Ready,
            omnix_runtime::RuntimeHealth::ShutDown => RuntimeHealth::ShutDown,
        }
    }
}

/// Statically-known runtime capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub wire_api: &'static str,
    pub reasoning: bool,
    pub tools: bool,
    pub host_tools: bool,
    pub built_in_tools: bool,
    pub persistence: bool,
    pub compaction: bool,
}

impl From<omnix_runtime::Capabilities> for Capabilities {
    fn from(value: omnix_runtime::Capabilities) -> Self {
        Self {
            wire_api: value.wire_api,
            reasoning: value.reasoning,
            tools: value.tools,
            host_tools: value.host_tools,
            built_in_tools: value.built_in_tools,
            persistence: value.persistence,
            compaction: value.compaction,
        }
    }
}

/// The embedded Omnix runtime — the single entry point for a host application.
///
/// Created via [`crate::Omnix::builder`]. Owns the in-process app-server for its
/// lifetime; drop or [`OmnixRuntime::shutdown`] releases it.
pub struct OmnixRuntime {
    inner: Runtime,
}

impl OmnixRuntime {
    pub(crate) fn new(inner: Runtime) -> Self {
        Self { inner }
    }

    /// Current runtime health.
    pub fn health(&self) -> RuntimeHealth {
        self.inner.health().into()
    }

    /// Statically-known capabilities of this runtime.
    pub fn capabilities(&self) -> Capabilities {
        self.inner.capabilities().into()
    }

    /// Access the session factory for creating and resuming sessions.
    pub fn sessions(&self) -> Sessions<'_> {
        Sessions::new(&self.inner)
    }

    /// Gracefully shut down the runtime and its in-process app-server.
    pub async fn shutdown(self) -> Result<(), OmnixError> {
        self.inner.shutdown().await.map_err(|e| {
            OmnixError::new(
                OmnixErrorKind::RuntimeUnavailable,
                "runtime shutdown failed",
            )
            .with_source(e)
        })
    }
}
