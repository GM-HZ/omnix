//! Map internal `omnix_runtime::RuntimeError` into the stable [`OmnixError`].

use omnix_runtime::RuntimeError;

use crate::error::OmnixError;
use crate::error::OmnixErrorKind;

/// Convert a runtime error into the public error, choosing a stable category.
pub(crate) fn map_runtime_error(err: RuntimeError) -> OmnixError {
    let kind = match &err {
        RuntimeError::Dirs { .. } => OmnixErrorKind::StorageUnavailable,
        RuntimeError::ConfigBuild { .. } => OmnixErrorKind::InvalidConfig,
        RuntimeError::Environment { .. } | RuntimeError::Start { .. } => {
            OmnixErrorKind::RuntimeUnavailable
        }
        RuntimeError::IncompatibleState { .. } => OmnixErrorKind::SessionIncompatible,
        RuntimeError::Manifest { .. } => OmnixErrorKind::StorageUnavailable,
        RuntimeError::Request { .. } => classify_request_error(&err),
        RuntimeError::Turn { .. } => OmnixErrorKind::Internal,
        RuntimeError::RunAlreadyActive => OmnixErrorKind::SessionIncompatible,
        RuntimeError::EventStreamClosed | RuntimeError::Unavailable => {
            OmnixErrorKind::RuntimeUnavailable
        }
    };
    OmnixError::new(kind, err.to_string()).with_source(err)
}

/// Best-effort classification of a JSON-RPC request failure from its message.
fn classify_request_error(err: &RuntimeError) -> OmnixErrorKind {
    let msg = err.to_string().to_lowercase();
    if msg.contains("rate limit") || msg.contains("429") {
        OmnixErrorKind::ProviderRateLimited
    } else if msg.contains("not found") {
        OmnixErrorKind::SessionNotFound
    } else if msg.contains("context") && msg.contains("limit") {
        OmnixErrorKind::ContextLimitExceeded
    } else {
        OmnixErrorKind::Internal
    }
}
