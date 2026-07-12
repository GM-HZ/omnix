use super::*;
use serde_json::json;

/// A fingerprint must expose only lowercase hex and be 16 characters wide so it
/// never leaks prompt/tool content.
fn assert_hex16(fp: &str) {
    assert_eq!(fp.len(), 16, "fingerprint must be 16 hex chars: {fp}");
    assert!(
        fp.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "fingerprint must be lowercase hex: {fp}"
    );
}

/// Canonicalization sorts object keys, so logically-equal objects fingerprint
/// identically regardless of key insertion order.
#[test]
fn object_key_order_does_not_change_fingerprint() {
    assert_eq!(
        fingerprint(&json!({"a": 1, "b": 2})),
        fingerprint(&json!({"b": 2, "a": 1}))
    );
}

/// Arrays are order-sensitive: element order is part of the identity.
#[test]
fn array_order_changes_fingerprint() {
    assert_ne!(fingerprint(&json!([1, 2])), fingerprint(&json!([2, 1])));
}

/// Reordering the tools array must change the tools fingerprint, since tool
/// order is part of the on-the-wire prefix DeepSeek caches against.
#[test]
fn tool_order_changes_tools_fingerprint() {
    let tool_a = json!({"type": "function", "function": {"name": "shell"}});
    let tool_b = json!({"type": "function", "function": {"name": "read"}});

    let layout_ab = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &[],
        &[tool_a.clone(), tool_b.clone()],
    );
    let layout_ba =
        ChatCompletionsRequestLayout::from_request("deepseek-chat", &[], &[tool_b, tool_a]);

    assert_ne!(layout_ab.tools_fingerprint, layout_ba.tools_fingerprint);
}

/// The canonical fingerprint is insensitive to object-key insertion order, but
/// the wire fingerprint hashes the exact serialized bytes. When the build
/// serializes objects in insertion order (preserve_order), reordered keys
/// produce identical canonical fingerprints yet distinct wire fingerprints —
/// which is exactly the silent cache-kill this guard exists to catch. The
/// assertion is phrased as an invariant so it holds in either build config.
#[test]
fn wire_fingerprint_tracks_serialized_bytes() {
    let a = json!({"x": 1, "y": 2});
    let b = json!({"y": 2, "x": 1});

    assert_eq!(fingerprint(&a), fingerprint(&b));

    let same_bytes = serde_json::to_string(&a).expect("serialize a")
        == serde_json::to_string(&b).expect("serialize b");
    assert_eq!(wire_fingerprint(&a) == wire_fingerprint(&b), same_bytes);
}

/// A first request has no predecessor: the comparison reports FirstRequest and
/// makes no false "changed" claims.
#[test]
fn first_request_reports_first_request_reset() {
    let current = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &[json!({"role": "system", "content": "sys"})],
        &[],
    );
    let comparison = ChatCompletionsLayoutComparison::new(None, &current);

    assert_eq!(comparison.reset_reason, CacheResetReason::FirstRequest);
    assert!(!comparison.system_changed);
    assert!(!comparison.tools_changed);
    assert_eq!(comparison.longest_matching_message_prefix, 0);
    assert_eq!(comparison.previous_message_count, 0);
    assert!(!comparison.serialization_reordered);
}

/// Appending a user message keeps the entire previous prefix intact: no reset,
/// full prefix match.
#[test]
fn appended_message_keeps_full_prefix() {
    let messages_before = vec![
        json!({"role": "system", "content": "sys"}),
        json!({"role": "user", "content": "first"}),
    ];
    let mut messages_after = messages_before.clone();
    messages_after.push(json!({"role": "assistant", "content": "reply"}));

    let previous =
        ChatCompletionsRequestLayout::from_request("deepseek-chat", &messages_before, &[]);
    let current = ChatCompletionsRequestLayout::from_request("deepseek-chat", &messages_after, &[]);
    let comparison = ChatCompletionsLayoutComparison::new(Some(&previous), &current);

    assert!(!comparison.system_changed);
    assert!(!comparison.tools_changed);
    assert_eq!(comparison.reset_reason, CacheResetReason::None);
    assert_eq!(
        comparison.longest_matching_message_prefix,
        messages_before.len()
    );
    assert_eq!(comparison.previous_message_count, messages_before.len());
}

/// Changing the model invalidates the whole prefix.
#[test]
fn model_change_reports_model_reset() {
    let messages = vec![json!({"role": "system", "content": "sys"})];
    let previous = ChatCompletionsRequestLayout::from_request("deepseek-chat", &messages, &[]);
    let current = ChatCompletionsRequestLayout::from_request("qwen-max", &messages, &[]);
    let comparison = ChatCompletionsLayoutComparison::new(Some(&previous), &current);

    assert_eq!(comparison.reset_reason, CacheResetReason::ModelChanged);
}

/// Editing the system message reports a system reset (takes priority over any
/// downstream message-prefix analysis).
#[test]
fn system_change_reports_system_reset() {
    let previous = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &[json!({"role": "system", "content": "old"})],
        &[],
    );
    let current = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &[json!({"role": "system", "content": "new"})],
        &[],
    );
    let comparison = ChatCompletionsLayoutComparison::new(Some(&previous), &current);

    assert!(comparison.system_changed);
    assert_eq!(comparison.reset_reason, CacheResetReason::SystemChanged);
}

/// Adding a tool reports a tools reset.
#[test]
fn tools_change_reports_tools_reset() {
    let messages = vec![json!({"role": "system", "content": "sys"})];
    let previous = ChatCompletionsRequestLayout::from_request("deepseek-chat", &messages, &[]);
    let current = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &messages,
        &[json!({"type": "function", "function": {"name": "shell"}})],
    );
    let comparison = ChatCompletionsLayoutComparison::new(Some(&previous), &current);

    assert!(comparison.tools_changed);
    assert_eq!(comparison.reset_reason, CacheResetReason::ToolsChanged);
}

/// Rewriting an earlier (non-terminal) message breaks the append-only prefix
/// and is reported as HistoryRewritten with the exact unchanged prefix length.
#[test]
fn rewritten_history_reports_history_rewrite() {
    let previous = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &[
            json!({"role": "system", "content": "sys"}),
            json!({"role": "user", "content": "first"}),
            json!({"role": "assistant", "content": "reply"}),
        ],
        &[],
    );
    let current = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &[
            json!({"role": "system", "content": "sys"}),
            json!({"role": "user", "content": "EDITED"}),
            json!({"role": "assistant", "content": "reply"}),
        ],
        &[],
    );
    let comparison = ChatCompletionsLayoutComparison::new(Some(&previous), &current);

    assert_eq!(comparison.reset_reason, CacheResetReason::HistoryRewritten);
    // System (index 0) still matches; the edit is at index 1.
    assert_eq!(comparison.longest_matching_message_prefix, 1);
    assert_eq!(comparison.previous_message_count, 3);
}

/// Fingerprints must never contain raw prompt or tool text. Feed sentinels and
/// assert no exposed diagnostic string contains them, and every fingerprint has
/// the hex-16 shape.
#[test]
fn fingerprints_do_not_leak_content() {
    const PROMPT_SECRET: &str = "SUPER_SECRET_SYSTEM_PROMPT";
    const TOOL_SECRET: &str = "SUPER_SECRET_TOOL_NAME";

    let layout = ChatCompletionsRequestLayout::from_request(
        "deepseek-chat",
        &[
            json!({"role": "system", "content": PROMPT_SECRET}),
            json!({"role": "user", "content": "hello"}),
        ],
        &[json!({"type": "function", "function": {"name": TOOL_SECRET}})],
    );

    let mut all_fps = vec![
        layout.tools_fingerprint.clone(),
        layout.request_fingerprint.clone(),
        layout.wire_fingerprint.clone(),
    ];
    if let Some(sys) = &layout.system_fingerprint {
        all_fps.push(sys.clone());
    }
    all_fps.extend(layout.message_prefix_fingerprints.iter().cloned());

    for fp in &all_fps {
        assert_hex16(fp);
        assert!(!fp.contains(PROMPT_SECRET));
        assert!(!fp.contains(TOOL_SECRET));
    }
}
