use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use rand::Rng;
use tracing::error;

const INITIAL_DELAY_MS: u64 = 200;
const BACKOFF_FACTOR: f64 = 2.0;

/// Stub: feedback_tags macro no-ops — Sentry feedback removed per slim-agent-loop design.
#[macro_export]
macro_rules! feedback_tags {
    ($( $key:ident = $value:expr ),+ $(,)?) => {};
}

/// Stub: no longer emits feedback auth recovery tags (Sentry removed).
pub(crate) fn emit_feedback_auth_recovery_tags(
    _auth_recovery_mode: &str,
    _auth_recovery_phase: &str,
    _auth_recovery_outcome: &str,
    _auth_request_id: Option<&str>,
    _auth_cf_ray: Option<&str>,
    _auth_error: Option<&str>,
    _auth_error_code: Option<&str>,
) {}

pub fn backoff(attempt: u64) -> Duration {
    let exp = BACKOFF_FACTOR.powi(attempt.saturating_sub(1) as i32);
    let base = (INITIAL_DELAY_MS as f64 * exp) as u64;
    let jitter = rand::rng().random_range(0.9..1.1);
    Duration::from_millis((base as f64 * jitter) as u64)
}

pub(crate) fn error_or_panic(message: impl std::string::ToString) {
    if cfg!(debug_assertions) {
        panic!("{}", message.to_string());
    } else {
        error!("{}", message.to_string());
    }
}

pub fn resolve_path(base: &Path, path: &PathBuf) -> PathBuf {
    if path.is_absolute() {
        path.clone()
    } else {
        base.join(path)
    }
}

/// Trim a thread name and return `None` if it is empty after trimming.
pub fn normalize_thread_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
