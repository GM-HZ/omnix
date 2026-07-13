//! Internal error type for the runtime adapter.
//!
//! `omnix-sdk` maps these into its stable public `OmnixError`. Keeping a
//! dedicated internal error keeps the adapter's failure modes explicit without
//! committing the public surface to them.

use codex_app_server_client::TypedRequestError;

/// Failures raised while starting or driving the in-process runtime.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// Preparing the `.omnix` directory tree failed.
    #[error("failed to prepare .omnix runtime directory: {source}")]
    Dirs { source: std::io::Error },

    /// Building the in-memory `Config` failed.
    #[error("failed to build runtime config: {source}")]
    ConfigBuild { source: std::io::Error },

    /// Resolving the exec-server environment failed.
    #[error("failed to initialize runtime environment: {source}")]
    Environment { source: std::io::Error },

    /// Starting the in-process app-server client failed.
    #[error("failed to start in-process app-server: {source}")]
    Start { source: std::io::Error },

    /// The on-disk `.omnix` state was written by a newer, unsupported runtime.
    #[error("incompatible .omnix state schema: found v{found}, this runtime supports v{supported}")]
    IncompatibleState { found: u32, supported: u32 },

    /// Serializing/writing the runtime manifest failed.
    #[error("failed to write runtime manifest: {message}")]
    Manifest { message: String },

    /// A typed JSON-RPC request to the app-server failed.
    #[error("app-server request failed: {source}")]
    Request { source: TypedRequestError },

    /// A turn ended in a failed or interrupted state.
    #[error("turn failed: {message}")]
    Turn { message: String },

    /// The runtime's event stream closed before the operation completed.
    #[error("runtime event stream closed unexpectedly")]
    EventStreamClosed,

    /// A run is already active on this session; only one is allowed at a time.
    #[error("a run is already active on this session")]
    RunAlreadyActive,

    /// The runtime has already been shut down.
    #[error("runtime is unavailable")]
    Unavailable,
}

impl From<TypedRequestError> for RuntimeError {
    fn from(source: TypedRequestError) -> Self {
        RuntimeError::Request { source }
    }
}
