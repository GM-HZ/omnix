//! Stable public error type for the Omnix SDK.
//!
//! Business apps match on [`OmnixErrorKind`] categories rather than JSON-RPC
//! error codes or app-server internals. Every error carries a displayable
//! message and optional correlation ids; the low-level `source` is retained for
//! logging only and never rendered in `Display`. API keys, full prompts, and
//! sensitive tool arguments must never be placed in an `OmnixError`.

use std::fmt;

/// Stable error categories (design §11). New variants are additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmnixErrorKind {
    InvalidConfig,
    InvalidCredentials,
    ProviderUnavailable,
    ProviderRateLimited,
    ContextLimitExceeded,
    SessionNotFound,
    SessionIncompatible,
    ToolFailed,
    ToolTimedOut,
    PermissionDenied,
    StorageUnavailable,
    RuntimeUnavailable,
    Cancelled,
    Internal,
}

impl OmnixErrorKind {
    /// A stable, machine-friendly slug for logs and telemetry.
    pub fn as_str(&self) -> &'static str {
        match self {
            OmnixErrorKind::InvalidConfig => "invalid_config",
            OmnixErrorKind::InvalidCredentials => "invalid_credentials",
            OmnixErrorKind::ProviderUnavailable => "provider_unavailable",
            OmnixErrorKind::ProviderRateLimited => "provider_rate_limited",
            OmnixErrorKind::ContextLimitExceeded => "context_limit_exceeded",
            OmnixErrorKind::SessionNotFound => "session_not_found",
            OmnixErrorKind::SessionIncompatible => "session_incompatible",
            OmnixErrorKind::ToolFailed => "tool_failed",
            OmnixErrorKind::ToolTimedOut => "tool_timed_out",
            OmnixErrorKind::PermissionDenied => "permission_denied",
            OmnixErrorKind::StorageUnavailable => "storage_unavailable",
            OmnixErrorKind::RuntimeUnavailable => "runtime_unavailable",
            OmnixErrorKind::Cancelled => "cancelled",
            OmnixErrorKind::Internal => "internal",
        }
    }

    /// Whether retrying the same operation may succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            OmnixErrorKind::ProviderUnavailable | OmnixErrorKind::ProviderRateLimited
        )
    }
}

/// Correlation ids for tracing an error back to a request/session/run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Correlation {
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub request_id: Option<String>,
}

/// The SDK's unified error.
pub struct OmnixError {
    kind: OmnixErrorKind,
    message: String,
    correlation: Correlation,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl OmnixError {
    /// Construct an error with a category and a user-displayable message.
    pub fn new(kind: OmnixErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            correlation: Correlation::default(),
            source: None,
        }
    }

    /// Attach a low-level source (retained for logging only).
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Attach correlation ids.
    pub fn with_correlation(mut self, correlation: Correlation) -> Self {
        self.correlation = correlation;
        self
    }

    pub fn kind(&self) -> OmnixErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn correlation(&self) -> &Correlation {
        &self.correlation
    }

    /// Retry advice derived from the error category.
    pub fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }
}

impl fmt::Display for OmnixError {
    // Deliberately omits `source` to avoid leaking sensitive details.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.kind.as_str(), self.message)
    }
}

impl fmt::Debug for OmnixError {
    // Includes the source for logs, but callers should still avoid logging at
    // untrusted sinks. Kind + message + correlation, plus the boxed source.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OmnixError")
            .field("kind", &self.kind)
            .field("message", &self.message)
            .field("correlation", &self.correlation)
            .field("source", &self.source.as_ref().map(ToString::to_string))
            .finish()
    }
}

impl std::error::Error for OmnixError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Convenience result alias.
pub type OmnixResult<T> = Result<T, OmnixError>;
