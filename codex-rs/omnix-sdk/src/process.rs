//! Host-process integration for single-binary applications.

/// Opaque helper-dispatch context initialized at host process startup.
///
/// Keeping this value alive inside [`crate::OmnixRuntime`] preserves the helper
/// aliases used by built-in command execution without exposing internal Codex
/// path types to business applications.
pub struct EmbeddedProcess {
    pub(crate) inner: omnix_runtime::EmbeddedProcess,
}

impl EmbeddedProcess {
    pub(crate) fn into_runtime(self) -> omnix_runtime::EmbeddedProcess {
        self.inner
    }
}
