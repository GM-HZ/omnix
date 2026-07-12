# DeepSeek Cache Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure and explain DeepSeek prompt-cache behavior without changing agent request semantics or public protocols.

**Architecture:** Parse DeepSeek cache usage into the existing token accounting path, compute privacy-safe request-layout fingerprints in a focused `codex-core` module, and compare a direct API control against the real Codex harness through an explicitly invoked live test. Keep default tests offline and segment measurements at context-window resets.

**Tech Stack:** Rust, serde, tracing, SHA-256, tokio, nextest, just, DeepSeek Chat Completions SSE.

---

## Constraints

- Do not change `ResponseEvent`, app-server protocol, prompt content/order, tool order, or compaction.
- Do not log raw messages, instructions, tool schemas, API keys, or full thread IDs.
- Do not run the paid live test from default CI or `just test`.
- Run Rust commands from `codex-rs` unless a step explicitly says repository root.
- If dependencies change, update root `MODULE.bazel.lock` per repository policy.

## File Map

- Modify `codex-rs/codex-api/src/chat_completions/types.rs`: deserialize cache usage.
- Modify `codex-rs/codex-api/src/chat_completions/sse_parser.rs`: map and diagnose usage.
- Create `codex-rs/core/src/chat_completions_observability.rs`: hashing and comparison.
- Create `codex-rs/core/src/chat_completions_observability_tests.rs`: focused unit tests.
- Modify `codex-rs/core/src/lib.rs`: register the private module.
- Modify `codex-rs/core/src/client.rs`: observe the exact outgoing request.
- Modify `codex-rs/core/src/client_tests.rs`: prove request semantics are unchanged.
- Create `codex-rs/core/tests/live_deepseek_cache.rs`: ignored control/harness benchmark.
- Modify root `justfile`: explicit paid-test recipe.
- Modify Cargo/Bazel lockfiles only if a new direct dependency is actually required.

### Task 1: Parse DeepSeek Cache Usage

**Files:**
- Modify: `codex-rs/codex-api/src/chat_completions/types.rs`
- Modify: `codex-rs/codex-api/src/chat_completions/sse_parser.rs`

- [x] **Step 1: Add the failing SSE usage test**

Add a final SSE chunk containing:

```json
{"id":"cache-1","choices":[],"usage":{"prompt_tokens":1000,"prompt_cache_hit_tokens":900,"prompt_cache_miss_tokens":100,"completion_tokens":20,"total_tokens":1020}}
```

Collect the `ResponseEvent::Completed` usage and compare the entire object:

```rust
assert_eq!(
    completed_usage,
    TokenUsage {
        input_tokens: 1000,
        cached_input_tokens: 900,
        output_tokens: 20,
        reasoning_output_tokens: 0,
        total_tokens: 1020,
    }
);
```

- [x] **Step 2: Verify the test fails for the intended reason**

```bash
just test -p codex-api sse_parser
```

Expected: cached input is `0`.

- [x] **Step 3: Extend `ChunkUsage` compatibly**

Add exactly:

```rust
#[serde(default)]
pub prompt_cache_hit_tokens: u32,
#[serde(default)]
pub prompt_cache_miss_tokens: u32,
```

Keep existing usage fields unchanged.

- [x] **Step 4: Map hit tokens to existing protocol accounting**

Replace the parser's hard-coded zero with:

```rust
cached_input_tokens: i64::from(u.prompt_cache_hit_tokens),
```

Keep `input_tokens` sourced from `prompt_tokens`; do not reconstruct it from hit plus miss.

- [x] **Step 5: Add compatibility cases**

Add a fixture without cache fields and expect zero cached input. Add a fixture where
`hit + miss != prompt_tokens` and verify the turn still completes with the provider-reported hit
count.

- [x] **Step 6: Verify and commit**

```bash
just test -p codex-api
git add codex-rs/codex-api/src/chat_completions/types.rs   codex-rs/codex-api/src/chat_completions/sse_parser.rs
git commit -m "feat(api): report DeepSeek cached prompt tokens"
```

Expected: all `codex-api` tests pass.

### Task 2: Implement Privacy-Safe Fingerprints

**Files:**
- Create: `codex-rs/core/src/chat_completions_observability.rs`
- Create: `codex-rs/core/src/chat_completions_observability_tests.rs`
- Modify: `codex-rs/core/src/lib.rs`
- Modify only if needed: `codex-rs/core/Cargo.toml`, `codex-rs/Cargo.lock`, `MODULE.bazel.lock`

- [x] **Step 1: Write canonicalization tests**

Register the sibling file with:

```rust
#[cfg(test)]
#[path = "chat_completions_observability_tests.rs"]
mod tests;
```

Cover object-key stability and array sensitivity:

```rust
assert_eq!(
    fingerprint(&json!({"a": 1, "b": 2})),
    fingerprint(&json!({"b": 2, "a": 1}))
);
assert_ne!(fingerprint(&json!([1, 2])), fingerprint(&json!([2, 1])));
```

Also prove tool-array order changes the tool fingerprint.

- [x] **Step 2: Run the test and observe compile failure**

```bash
just test -p codex-core chat_completions_observability
```

Expected: fingerprint functions and types do not exist.

- [x] **Step 3: Define focused private types**

```rust
pub(crate) struct ChatCompletionsRequestLayout {
    pub(crate) model: String,
    pub(crate) message_count: usize,
    pub(crate) tool_count: usize,
    pub(crate) system_fingerprint: Option<String>,
    pub(crate) tools_fingerprint: String,
    pub(crate) message_prefix_fingerprints: Vec<String>,
    pub(crate) request_fingerprint: String,
}

pub(crate) struct ChatCompletionsLayoutComparison {
    pub(crate) system_changed: bool,
    pub(crate) tools_changed: bool,
    pub(crate) longest_matching_message_prefix: usize,
    pub(crate) previous_message_count: usize,
    pub(crate) reset_reason: CacheResetReason,
}
```

Define an exhaustive private `CacheResetReason` enum for `FirstRequest`, `ModelChanged`,
`SystemChanged`, `ToolsChanged`, `HistoryRewritten`, and `None`.

- [x] **Step 4: Implement canonical SHA-256**

Recursively sort JSON object keys, preserve array order, serialize, SHA-256 hash, and expose only the
first 16 lowercase hex characters. Prefer an existing workspace hashing utility. If a new direct
dependency is unavoidable, use workspace `sha2`.

- [x] **Step 5: Implement cumulative message-prefix comparison**

Hash messages `0..=n` for every index. Compare previous/current vectors until the first mismatch.
Return the exact number of unchanged messages.

- [x] **Step 6: Add privacy tests**

Use sentinel prompt and tool secrets. Assert every exposed diagnostic string has the expected
lowercase-hex shape and contains neither sentinel. Do not expose canonical JSON from the module.

- [x] **Step 7: Verify, lint, and format**

```bash
just test -p codex-core chat_completions_observability
just fix -p codex-core
just fmt
```

If Cargo dependencies changed, run from repository root:

```bash
just bazel-lock-update
```

Expected: focused tests pass; lockfile changes are limited to the selected hashing dependency.

- [x] **Step 8: Commit**

```bash
git add codex-rs/core/src/chat_completions_observability.rs   codex-rs/core/src/chat_completions_observability_tests.rs   codex-rs/core/src/lib.rs
git add codex-rs/core/Cargo.toml codex-rs/Cargo.lock MODULE.bazel.lock
git commit -m "feat(core): fingerprint chat completion request layout"
```

Stage dependency files only when changed.

### Task 3: Trace Request Layout Without Semantic Drift

**Files:**
- Modify: `codex-rs/core/src/client.rs`
- Modify: `codex-rs/core/src/client_tests.rs`

- [x] **Step 1: Add comparison tests**

Build two requests with identical system/tools and one appended user message. Expect:

```rust
assert!(!comparison.system_changed);
assert!(!comparison.tools_changed);
assert_eq!(
    comparison.longest_matching_message_prefix,
    first_request.messages.len()
);
```

Add separate cases changing only tools and rewriting an earlier message.

- [x] **Step 2: Verify failure**

```bash
just test -p codex-core client_tests
```

Expected: request observation is not wired into client state.

- [x] **Step 3: Add session-scoped previous-layout state**

Store `Option<ChatCompletionsRequestLayout>` beside existing mutable model-client session state,
using the neighboring synchronization pattern. Do not use global state or rollout persistence.

- [x] **Step 4: Observe the final request**

In `stream_chat_completions`, after `build_chat_completions_request` and before
`stream_request`, compute the layout from the exact request that will be serialized. Compare and
replace previous state, then emit a debug event named `chat_completions.request_layout`.

Include model, message/tool counts, system/tool/request fingerprints, previous message count,
longest matching prefix, reset reason, and effective context window. Include an estimated token
count only if an existing token estimator is available at this call site; never label bytes as
tokens.

- [x] **Step 5: Keep reset classification conservative**

Use `HistoryRewritten` when the message prefix is not append-only. Do not add broad plumbing merely
to label compaction; correlate with existing compaction traces until the request kind is naturally
available.

- [x] **Step 6: Prove request JSON is unchanged**

Serialize a request before and after observation and compare complete `serde_json::Value` objects.
Observation must not alter model, messages, tools, options, or their ordering.

- [x] **Step 7: Verify and commit**

```bash
just test -p codex-core client_tests
just fix -p codex-core
just fmt
git add codex-rs/core/src/client.rs codex-rs/core/src/client_tests.rs
git commit -m "feat(core): trace chat completion cache layout"
```

### Task 4: Add Cache-Usage Diagnostics

**Files:**
- Modify: `codex-rs/codex-api/src/chat_completions/sse_parser.rs`
- Test: existing `sse_parser.rs` test module

- [x] **Step 1: Add ratio tests**

Define expected behavior:

```text
hit=0, miss=0     -> ratio unavailable
hit=90, miss=10   -> 0.9
hit=0, miss=100   -> 0.0
```

Test consistency separately against `prompt_tokens`.

- [x] **Step 2: Verify failure**

```bash
just test -p codex-api cache_usage
```

Expected: ratio and consistency diagnostics are absent.

- [x] **Step 3: Emit response-side diagnostics**

When final usage arrives, emit a debug event named `chat_completions.cache_usage` containing raw
prompt/hit/miss counts, optional token-weighted ratio, and `usage_consistent`.

Use the enclosing tracing span for correlation. Do not modify `ResponseEvent` or add a public
request identifier just for logging.

- [x] **Step 4: Verify and commit**

```bash
just test -p codex-api
just fix -p codex-api
just fmt
git add codex-rs/codex-api/src/chat_completions/sse_parser.rs
git commit -m "feat(api): trace DeepSeek prompt cache usage"
```

### Task 5: Build the Ignored Direct-API Control

**Files:**
- Create: `codex-rs/core/tests/live_deepseek_cache.rs`
- Modify: root `justfile`

- [x] **Step 1: Separate offline configuration from network execution**

Define configuration with these environment variables and defaults:

```text
DEEPSEEK_BASE_URL=https://api.deepseek.com
DEEPSEEK_MODEL=deepseek-chat
DEEPSEEK_CACHE_PROFILE=normal
DEEPSEEK_CACHE_ROUNDS=3
DEEPSEEK_CACHE_WARMUP_SECONDS=5
```

Keep parsing and report aggregation callable from normal offline tests.

- [x] **Step 2: Add profile tests**

Assert `smoke`, `normal`, and `long` target approximately 2K, 8K, and 32K stable-prefix tokens.
Reject unknown profiles, fewer than two steady-state rounds, unreasonable waits, and payloads that
would exceed the configured effective context window.

- [x] **Step 3: Add the paid ignored test**

Mark the network body:

```rust
#[ignore = "requires DEEPSEEK_API_KEY and incurs API cost"]
```

Read the key only at runtime and never include it in errors, debug output, or report structures.

- [x] **Step 4: Generate deterministic stable content**

Generate numbered ASCII paragraphs from a fixed template. Keep system text and document prefix
byte-identical; vary only a short final question per round.

- [x] **Step 5: Implement control requests**

Send one warm-up, sleep the configured persistence interval, then send steady-state rounds directly
through the production Chat Completions client. Require final usage for every completed request.

- [x] **Step 6: Define stable reports**

Each row contains scenario, phase, round, prompt/hit/miss tokens, hit rate, and context data. Print a
human-readable table followed by JSON. Aggregate by summing tokens before calculating the rate;
exclude warm-up.

- [x] **Step 7: Add the explicit root recipe**

Add `test-deepseek-cache` using nextest's ignored-only selection so it runs only this paid test.
Preserve `DEEPSEEK_API_KEY` and do not echo it.

- [x] **Step 8: Verify offline and live smoke modes**

From `codex-rs`:

```bash
just test -p codex-core live_deepseek_cache
```

Expected: offline tests pass and the paid body remains ignored.

From repository root:

```bash
DEEPSEEK_CACHE_PROFILE=smoke just test-deepseek-cache
```

Expected: warm-up plus control rounds, valid table/JSON, no secret output.

- [x] **Step 9: Commit**

```bash
git add codex-rs/core/tests/live_deepseek_cache.rs justfile
git commit -m "test(core): add DeepSeek cache control benchmark"
```

### Task 6: Add the Real Harness Scenario

**Files:**
- Modify: `codex-rs/core/tests/live_deepseek_cache.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs` only if required by the existing registration pattern

- [x] **Step 1: Add dual-scenario aggregation tests**

Construct control/harness rows in memory and compare the entire summary. Verify token-weighted rates,
percentage-point gap, warm-up exclusion, and context-window segmentation.

- [x] **Step 2: Add diagnosis tests**

Cover:

```text
control < 90%                                 -> provider_variability
control >= 90%, gap >= 5, tools changed       -> tool_layout_instability
control >= 90%, gap >= 5, history rewritten   -> history_layout_instability
post-compaction sample                        -> expected_cache_reset
stable fingerprints with unexplained gap      -> serialization_or_provider_behavior
```

- [x] **Step 3: Verify failure**

```bash
just test -p codex-core deepseek_cache_report
```

Expected: harness aggregation and diagnoses are absent.

- [x] **Step 4: Build one production-path Codex session**

Use the configured DeepSeek provider and Chat Completions path. Reuse one thread for warm-up and all
steady-state turns. Keep model, cwd, tools, skill/plugin configuration, and MCP inventory fixed.
Capture token usage events, not human-formatted output.

- [x] **Step 5: Attach context-window data**

For every harness row record raw/effective context window, provider prompt tokens, usage percentage,
auto-compact limit/scope, tokens until compaction when available, and a redacted window ordinal.
Start a new aggregate segment whenever the context window generation changes.

- [x] **Step 6: Merge reports and diagnosis**

Print both scenarios in one table and JSON object. A low hit rate is report data, not test failure.
Only configuration, transport, parsing, missing usage, or internal invariant errors return nonzero.

- [x] **Step 7: Run three normal baselines**

From repository root:

```bash
DEEPSEEK_CACHE_PROFILE=normal just test-deepseek-cache
DEEPSEEK_CACHE_PROFILE=normal just test-deepseek-cache
DEEPSEEK_CACHE_PROFILE=normal just test-deepseek-cache
```

Expected: three complete comparable reports. Keep raw results outside version control unless writing
a sanitized baseline document.

- [x] **Step 8: Verify and commit**

```bash
cd codex-rs
just test -p codex-core deepseek_cache
just fix -p codex-core
just fmt
git add codex-rs/core/tests/live_deepseek_cache.rs
git add codex-rs/core/tests/suite/mod.rs
git commit -m "test(core): compare DeepSeek cache through the agent harness"
```

Stage `suite/mod.rs` only if changed.

### Task 7: Final Verification and Optimization Decision

**Files:**
- No runtime files expected
- Optional create: `docs/superpowers/results/YYYY-MM-DD-deepseek-cache-baseline.md`

- [x] **Step 1: Run final lint and formatting**

```bash
cd codex-rs
just fix -p codex-api
just fix -p codex-core
just fmt
```

- [x] **Step 2: Run affected tests**

```bash
just test -p codex-api
just test -p codex-core
```

Expected: both pass. Ask before running complete workspace `just test`, as required by repository
policy because `codex-core` changed.

- [x] **Step 3: Prove no public protocol drift**

From repository root:

```bash
git diff --exit-code -- codex-rs/app-server-protocol codex-rs/protocol
```

Expected: no schema/API changes.

- [ ] **Step 4: Inspect exec JSON usage**

Run one warmed DeepSeek-backed `codex exec --json` command using existing provider configuration.
Confirm final usage exposes nonzero cached input without a new exec flag.

- [x] **Step 5: Classify three normal runs**

Record control/harness rates, gap, changing fingerprints, context usage, and window resets. Do not
start Phase 2 until all three runs are comparable.

- [x] **Step 6: Apply the decision matrix**

| Evidence | First Phase 2 candidate |
| --- | --- |
| Tool fingerprint changes during ordinary turns | Deterministic tool ordering and per-sampling tool snapshot reuse |
| System fingerprint changes | Separate stable base instructions from turn-scoped instructions |
| Earlier message prefix changes without compaction | Repair history conversion or context-update placement |
| Misses occur only after compaction | Treat as expected reset; assess compact threshold separately |
| Control/harness gap is below 5 points | Avoid core changes; accept provider variability |
| Fingerprints stable but gap is at least 5 points | Capture a local canonical request diff and inspect serialization/provider behavior |

- [x] **Step 7: Complete the review checklist**

```text
[ ] Request JSON is unchanged by observation.
[ ] Default tests perform no network requests.
[ ] Live tests require explicit invocation.
[ ] Cache usage reaches existing exec/app-server accounting.
[ ] Fingerprints are stable and privacy-safe.
[ ] Warm-up and post-compaction samples are excluded from steady-state totals.
[ ] Context-window metadata is reported but not modified.
[ ] Low cache rate does not fail the live benchmark.
```

## Review Checkpoints

Request review after Tasks 1, 3, 5, and 7:

- Task 1: usage-field fidelity and provider compatibility.
- Task 3: zero request-semantic drift and privacy.
- Task 5: paid-test isolation, cost controls, and secret handling.
- Task 7: evidence quality and whether Phase 2 selection follows measured data.

