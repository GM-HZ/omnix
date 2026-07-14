//! Opt-in process integration for single-binary embedded hosts.

use codex_arg0::Arg0DispatchPaths;
use codex_arg0::Arg0PathEntryGuard;

/// Keeps Codex helper aliases alive and records the executable paths that an
/// embedded host explicitly initialized.
///
/// Call [`initialize_embedded_process`] at the very beginning of `main`, before
/// creating async runtimes or other threads, and move the result into the SDK
/// builder. Helper invocations may terminate the process during initialization.
pub struct EmbeddedProcess {
    guard: Arg0PathEntryGuard,
}

impl EmbeddedProcess {
    pub(crate) fn paths(&self) -> &Arg0DispatchPaths {
        self.guard.paths()
    }
}

/// Initialize arg0 helper dispatch for a single-binary host.
///
/// Returns `None` when helper aliases cannot be prepared; text inference and
/// host-provided dynamic tools remain usable, but built-in command sandbox
/// helpers are unavailable.
pub fn initialize_embedded_process() -> Option<EmbeddedProcess> {
    codex_arg0::arg0_dispatch().map(|guard| EmbeddedProcess { guard })
}
