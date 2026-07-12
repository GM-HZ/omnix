//! Privacy-safe fingerprinting of the Chat Completions request layout.
//!
//! DeepSeek (and compatible providers) serve a prompt from cache only when the
//! leading bytes of the request match a previous request byte-for-byte. This
//! module derives compact, content-free fingerprints of the outgoing request so
//! we can *observe* how stable that cacheable prefix is across turns — without
//! logging any prompt text, tool schema, or key material, and without changing
//! the request itself.
//!
//! Two hashing views are kept deliberately distinct:
//!
//! - [`fingerprint`] is *canonical*: object keys are sorted before hashing, so
//!   logically-equal requests hash identically. This drives reset
//!   classification, which should be robust to incidental key reordering.
//! - [`wire_fingerprint`] hashes the *exact serialized bytes*. Because the
//!   shipped binary enables `serde_json/preserve_order`, object-key insertion
//!   order reaches the wire, and a reordering that leaves the canonical
//!   fingerprint unchanged can still silently invalidate the provider cache.
//!   Divergence between the two views is exactly that silent cache-kill.

use std::fmt::Write as _;

use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use tracing::debug;

/// Why the cacheable request prefix changed relative to the previous request.
///
/// Variants are mutually exclusive and evaluated in priority order (see
/// [`ChatCompletionsLayoutComparison::new`]): a model change dominates a system
/// change, which dominates a tools change, which dominates a history rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheResetReason {
    /// No previous request to compare against.
    FirstRequest,
    /// The model slug changed; nothing before it can be reused.
    ModelChanged,
    /// The leading system message changed.
    SystemChanged,
    /// The tools array changed (content or order).
    ToolsChanged,
    /// An earlier message was edited/removed, so the prefix is not append-only.
    HistoryRewritten,
    /// The prefix grew by appending; the previous prefix is fully preserved.
    None,
}

/// Content-free fingerprints describing one outgoing Chat Completions request.
///
/// Every field is either a count or a 16-hex-character digest; none of them can
/// reconstruct prompt or tool content.
///
/// Fingerprints are computed **per message** (each message hashed once), not
/// per cumulative prefix. Comparing per-message fingerprint vectors index by
/// index yields the identical longest-matching-prefix result as hashing every
/// growing prefix would — the first differing message is the cutoff either way
/// — but costs O(n) instead of O(n²), so a long conversation does not re-hash
/// its entire history on every turn.
#[derive(Debug, Clone)]
pub(crate) struct ChatCompletionsRequestLayout {
    pub(crate) model: String,
    pub(crate) message_count: usize,
    pub(crate) tool_count: usize,
    /// Canonical fingerprint of the leading system message, if present.
    pub(crate) system_fingerprint: Option<String>,
    /// Canonical fingerprint of the whole tools array (order-sensitive).
    pub(crate) tools_fingerprint: String,
    /// Canonical fingerprint of each message, in order.
    pub(crate) message_fingerprints: Vec<String>,
    /// Wire (exact-bytes) fingerprint of each message, in order.
    pub(crate) message_wire_fingerprints: Vec<String>,
    /// Canonical fingerprint of the entire `{model, messages, tools}` layout,
    /// folded from the per-component fingerprints (O(n), no full re-serialize).
    pub(crate) request_fingerprint: String,
    /// Wire (exact-bytes) fingerprint of the entire layout, folded from the
    /// per-component wire fingerprints.
    pub(crate) wire_fingerprint: String,
}

impl ChatCompletionsRequestLayout {
    /// Derive a layout from the exact pieces that will be serialized onto the
    /// wire: the model slug, the ordered `messages` array, and the ordered
    /// `tools` array.
    pub(crate) fn from_request(model: &str, messages: &[Value], tools: &[Value]) -> Self {
        let system_fingerprint = messages.first().and_then(|first| {
            (first.get("role").and_then(Value::as_str) == Some("system"))
                .then(|| fingerprint(first))
        });

        let tools_value = Value::Array(tools.to_vec());
        let tools_fingerprint = fingerprint(&tools_value);
        let tools_wire_fingerprint = wire_fingerprint(&tools_value);

        // O(n): hash each message exactly once.
        let message_fingerprints: Vec<String> = messages.iter().map(fingerprint).collect();
        let message_wire_fingerprints: Vec<String> =
            messages.iter().map(wire_fingerprint).collect();

        // Fold the whole-request fingerprints from the components rather than
        // re-serializing the entire request. Each folded fingerprint changes
        // iff any component changes and is order-sensitive.
        let request_fingerprint =
            fold_fingerprints(model, &tools_fingerprint, &message_fingerprints);
        let wire_fingerprint_value =
            fold_fingerprints(model, &tools_wire_fingerprint, &message_wire_fingerprints);

        Self {
            model: model.to_string(),
            message_count: messages.len(),
            tool_count: tools.len(),
            system_fingerprint,
            tools_fingerprint,
            message_fingerprints,
            message_wire_fingerprints,
            request_fingerprint,
            wire_fingerprint: wire_fingerprint_value,
        }
    }
}

/// Result of comparing a request layout to the immediately preceding one within
/// the same session.
pub(crate) struct ChatCompletionsLayoutComparison {
    pub(crate) system_changed: bool,
    pub(crate) tools_changed: bool,
    /// Number of leading messages whose canonical fingerprints are unchanged.
    pub(crate) longest_matching_message_prefix: usize,
    pub(crate) previous_message_count: usize,
    pub(crate) reset_reason: CacheResetReason,
    /// The canonical prefix is stable further than the wire bytes are — i.e. a
    /// serialization-order change is invalidating the cache while the semantic
    /// layout appears unchanged.
    pub(crate) serialization_reordered: bool,
}

impl ChatCompletionsLayoutComparison {
    pub(crate) fn new(
        previous: Option<&ChatCompletionsRequestLayout>,
        current: &ChatCompletionsRequestLayout,
    ) -> Self {
        let Some(previous) = previous else {
            return Self {
                system_changed: false,
                tools_changed: false,
                longest_matching_message_prefix: 0,
                previous_message_count: 0,
                reset_reason: CacheResetReason::FirstRequest,
                serialization_reordered: false,
            };
        };

        let model_changed = previous.model != current.model;
        let system_changed = previous.system_fingerprint != current.system_fingerprint;
        let tools_changed = previous.tools_fingerprint != current.tools_fingerprint;

        let longest_matching_message_prefix = common_prefix_len(
            &previous.message_fingerprints,
            &current.message_fingerprints,
        );
        let longest_matching_wire_prefix = common_prefix_len(
            &previous.message_wire_fingerprints,
            &current.message_wire_fingerprints,
        );
        let previous_message_count = previous.message_count;

        let reset_reason = if model_changed {
            CacheResetReason::ModelChanged
        } else if system_changed {
            CacheResetReason::SystemChanged
        } else if tools_changed {
            CacheResetReason::ToolsChanged
        } else if longest_matching_message_prefix < previous_message_count {
            CacheResetReason::HistoryRewritten
        } else {
            CacheResetReason::None
        };

        // Only meaningful when the semantic layout looks stable: if the bytes
        // diverge earlier than the canonical prefix does, a serialization
        // reordering is silently breaking the cache.
        let serialization_reordered = !model_changed
            && !system_changed
            && !tools_changed
            && longest_matching_wire_prefix < longest_matching_message_prefix;

        Self {
            system_changed,
            tools_changed,
            longest_matching_message_prefix,
            previous_message_count,
            reset_reason,
            serialization_reordered,
        }
    }
}

/// Length of the shared leading run of two fingerprint vectors.
fn common_prefix_len(a: &[String], b: &[String]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Compute the layout of the exact request about to be serialized, compare it
/// to the previous request in this session, and emit a `chat_completions.request_layout`
/// debug event. Returns the new layout so the caller can store it as the next
/// comparison baseline.
///
/// This is pure observation: it reads `model`/`messages`/`tools` by reference
/// and never mutates them, so the request that streams is byte-identical to
/// what it would have been without this call.
pub(crate) fn observe_request_layout(
    previous: Option<&ChatCompletionsRequestLayout>,
    model: &str,
    messages: &[Value],
    tools: &[Value],
    effective_context_window: Option<i64>,
) -> ChatCompletionsRequestLayout {
    let current = ChatCompletionsRequestLayout::from_request(model, messages, tools);
    let comparison = ChatCompletionsLayoutComparison::new(previous, &current);

    debug!(
        target: "chat_completions.request_layout",
        model = %current.model,
        message_count = current.message_count,
        tool_count = current.tool_count,
        system_fingerprint = current.system_fingerprint.as_deref().unwrap_or("none"),
        tools_fingerprint = %current.tools_fingerprint,
        request_fingerprint = %current.request_fingerprint,
        wire_fingerprint = %current.wire_fingerprint,
        previous_message_count = comparison.previous_message_count,
        longest_matching_message_prefix = comparison.longest_matching_message_prefix,
        system_changed = comparison.system_changed,
        tools_changed = comparison.tools_changed,
        serialization_reordered = comparison.serialization_reordered,
        reset_reason = ?comparison.reset_reason,
        effective_context_window = effective_context_window.unwrap_or(0),
        "observed chat completions request layout"
    );

    current
}

/// Canonical fingerprint: object keys are sorted recursively before hashing so
/// logically-equal values hash identically. Array order is preserved.
pub(crate) fn fingerprint(value: &Value) -> String {
    let mut canonical = String::new();
    write_canonical(value, &mut canonical);
    sha16(canonical.as_bytes())
}

/// Wire fingerprint: hashes the exact serialized bytes, preserving whatever
/// object-key order the value carries.
pub(crate) fn wire_fingerprint(value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_default();
    sha16(serialized.as_bytes())
}

/// Fold the model slug, tools fingerprint, and per-message fingerprints into a
/// single whole-request fingerprint. Order-sensitive (a length-prefixed,
/// delimited stream), so it changes iff any component or the message order
/// changes — without re-serializing the entire request.
fn fold_fingerprints(
    model: &str,
    tools_fingerprint: &str,
    message_fingerprints: &[String],
) -> String {
    let mut acc = String::with_capacity(
        model.len() + tools_fingerprint.len() + message_fingerprints.len() * 17,
    );
    let _ = write!(
        acc,
        "m:{model}|t:{tools_fingerprint}|n:{}",
        message_fingerprints.len()
    );
    for fp in message_fingerprints {
        acc.push('|');
        acc.push_str(fp);
    }
    sha16(acc.as_bytes())
}

/// Serialize `value` with object keys sorted recursively. This is a stable
/// canonical form; it is intentionally not exposed outside the module so raw
/// (content-bearing) JSON never escapes.
fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.push('{');
            for (idx, key) in keys.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_canonical(&map[key.as_str()], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        scalar => {
            // Scalars (null/bool/number/string) have no key order to normalize.
            let _ = out.write_str(&serde_json::to_string(scalar).unwrap_or_default());
        }
    }
}

/// Append `s` as a JSON string literal (quoted and escaped) to `out`.
fn write_json_string(s: &str, out: &mut String) {
    let _ = out.write_str(&serde_json::to_string(s).unwrap_or_default());
}

/// SHA-256 the input and expose only the first 8 bytes as 16 lowercase hex
/// characters — enough to detect change, too little to reconstruct content.
fn sha16(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
#[path = "chat_completions_observability_tests.rs"]
mod tests;
