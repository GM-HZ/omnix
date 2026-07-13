//! Bounds for host-supplied text inserted into model-visible instruction items.

use crate::error::OmnixError;
use crate::error::OmnixErrorKind;

/// Conservative byte ceiling below the repository's 10K-token per-item cap.
///
/// Bytes are used instead of character estimates so non-English and tokenizer
/// byte-fallback input cannot evade the bound.
pub(crate) const MAX_MODEL_VISIBLE_INSTRUCTION_BYTES: usize = 8 * 1024;

pub(crate) fn validate_instruction(label: &str, value: &str) -> Result<(), OmnixError> {
    if value.len() > MAX_MODEL_VISIBLE_INSTRUCTION_BYTES {
        return Err(OmnixError::new(
            OmnixErrorKind::InvalidConfig,
            format!(
                "{label} exceeds the {MAX_MODEL_VISIBLE_INSTRUCTION_BYTES} byte model-visible item limit"
            ),
        ));
    }
    Ok(())
}
