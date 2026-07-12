# DeepSeek Cache Observability Design

## Status

Approved for planning. This phase adds measurement only. It must not change prompt contents,
message ordering, tool ordering, compaction behavior, `ResponseEvent`, app-server protocol, or the
default human-readable `codex exec` output.

## Goal

Make DeepSeek prompt-cache behavior measurable end to end so a later optimization can identify
whether cache misses come from the provider, system instructions, tool schemas, message history,
or a context-window reset.

The first optimization target is a steady-state token-weighted cache hit rate of at least 95% for
ordinary multi-turn conversations. This phase does not claim that target; it establishes a trusted
baseline and explains each material miss.

## Background

DeepSeek automatically caches complete prompt-prefix units. A later request hits only prefix units
that were persisted and that still match completely. Persistence is best effort and may take a few
seconds. Consequently, a useful benchmark must distinguish warm-up requests, normal steady-state
requests, and requests after compaction or another intentional context reset.

The current Chat Completions implementation already requests streamed usage and converts the final
usage chunk into `codex_protocol::protocol::TokenUsage`. However, `ChunkUsage` discards DeepSeek's
`prompt_cache_hit_tokens` and `prompt_cache_miss_tokens`, and the parser always emits zero cached
input tokens. The existing protocol, app-server, exec JSONL, and final token display already carry
`cached_input_tokens`, so no public API expansion is necessary.

## Scope

### In Scope

- Deserialize DeepSeek cache hit and miss token counts without breaking other OpenAI-compatible
  providers.
- Populate the existing `TokenUsage.cached_input_tokens` field.
- Emit privacy-safe structured diagnostics for request layout and final cache usage.
- Add deterministic offline parser and fingerprint tests.
- Add an explicit, paid, networked DeepSeek cache benchmark invoked through `just`.
- Report context-window usage, compaction boundaries, and comparable control/harness results.

### Out Of Scope

- Reordering tools or messages.
- Moving skill, plugin, MCP, or extension injections.
- Adding provider-specific cache keys.
- Changing model context-window metadata or compact thresholds.
- Adding fields to app-server v2 or `ResponseEvent`.
- Making the live benchmark part of normal CI.
- Logging raw prompts, tool schemas, API keys, or full thread identifiers.

## Architecture

### Usage Parsing

Extend `codex_api::chat_completions::types::ChunkUsage` with two defaulted fields:

```rust
#[serde(default)]
pub prompt_cache_hit_tokens: u32,
#[serde(default)]
pub prompt_cache_miss_tokens: u32,
```

The SSE parser maps `prompt_cache_hit_tokens` to the existing
`TokenUsage.cached_input_tokens`. `prompt_tokens` remains the authoritative total input count.
Miss tokens are retained in the wire usage type for diagnostics, but are not added to public
protocol types because `input_tokens - cached_input_tokens` already represents non-cached input.
Provider inconsistencies must not fail a turn. Diagnostics flag inconsistent totals while runtime
accounting preserves the provider values.

### Request-Layout Fingerprints

Create a focused private module in `codex-core` rather than growing `client.rs`. It computes a
`ChatCompletionsRequestLayout` from the fully serialized request immediately before transport.

The layout contains:

```text
message_count
tool_count
system_fingerprint
tools_fingerprint
message_prefix_fingerprints
request_fingerprint
```

Fingerprints use SHA-256 over canonical JSON bytes and are rendered as the first 16 lowercase hex
characters. SHA-256 is stable across processes and platforms; Rust's default hasher is not an
acceptable persistent diagnostic identifier.

Canonicalization recursively sorts JSON object keys while preserving array order. Array order is
semantically important for messages and potentially important for tools, so it must not be sorted.
`message_prefix_fingerprints[n]` hashes messages `0..=n`; comparing the current list with the
previous request identifies the longest unchanged message prefix.

The diagnostic module never retains or emits canonical JSON. It returns counts and hashes only.

### Per-Session Comparison State

Keep the previous Chat Completions layout in existing model-client session state, scoped to a single
Codex thread/client session. Do not use global state. For each request, derive:

```text
system_changed
tools_changed
longest_matching_message_prefix
previous_message_count
cache_reset_reason
```

`cache_reset_reason` is diagnostic metadata with these values:

- `none`: an ordinary append-only request.
- `first_request`: no previous layout exists.
- `system_changed`: system fingerprint changed.
- `tools_changed`: tools fingerprint changed.
- `history_rewritten`: no append-only message-prefix relationship exists.
- `compaction`: request metadata identifies a compaction boundary.
- `model_changed`: model slug changed.

If current plumbing cannot distinguish compaction without expanding unrelated interfaces, log
`history_rewritten` in Phase 1 and correlate it with the existing compaction trace. Do not modify
`ResponseEvent` to force this classification.

### Structured Diagnostics

Emit two tracing events at `debug` level.

Before transport:

```text
event="chat_completions.request_layout"
model
request_kind
message_count
tool_count
system_fingerprint
tools_fingerprint
request_fingerprint
longest_matching_message_prefix
previous_message_count
cache_reset_reason
model_context_window
estimated_context_tokens
```

After final usage:

```text
event="chat_completions.cache_usage"
prompt_tokens
cache_hit_tokens
cache_miss_tokens
cache_hit_ratio
usage_consistent
```

The response-side event does not repeat private request content. If correlating request and response
requires an identifier, use an opaque per-request diagnostic ID generated locally and carried only
through internal stream state. Never log credentials or full user-controlled data.

The token-weighted hit ratio is:

```text
cache_hit_tokens / (cache_hit_tokens + cache_miss_tokens)
```

If DeepSeek omits both fields, report the ratio as unavailable rather than `0%`. If their sum is
inconsistent with `prompt_tokens`, report both raw values and set `usage_consistent=false`.

### Existing Output Surfaces

No new command-line flag is required in Phase 1.

- `codex exec --json` already emits cached input usage through its existing usage structure.
- Human-readable final output already shows cached tokens when the count is positive.
- Detailed diagnosis remains in debug tracing and the dedicated benchmark report.

This keeps normal agent output stable while making automation and investigation possible.

## Benchmark Design

### Invocation

The repository root exposes:

```bash
DEEPSEEK_API_KEY=... just test-deepseek-cache
```

Optional variables:

```text
DEEPSEEK_BASE_URL               default: https://api.deepseek.com
DEEPSEEK_MODEL                  default: deepseek-chat
DEEPSEEK_CACHE_PROFILE          default: normal; smoke|normal|long
DEEPSEEK_CACHE_ROUNDS           default: 3 steady-state rounds
DEEPSEEK_CACHE_WARMUP_SECONDS   default: 5
```

The recipe runs only the ignored live test. Normal `just test` must never call DeepSeek or require
an API key.

### Profiles

Profiles target approximate stable-prefix sizes rather than exact tokenizer counts:

| Profile | Stable prefix | Purpose |
| --- | ---: | --- |
| `smoke` | 2K tokens | Fast connectivity and parsing check |
| `normal` | 8K tokens | Default repeatable comparison |
| `long` | 32K tokens | Manual long-context investigation |

Generated fixture text must be deterministic and content-neutral. The benchmark prints the actual
provider-reported prompt tokens, so conclusions use measured rather than estimated size.

### Control Scenario

Send direct Chat Completions requests with an unchanged system message and stable document prefix,
changing only a short final question. Run one warm-up request, wait the configured persistence
period, then run the steady-state rounds.

This measures DeepSeek's attainable cache rate at that time and account. It is not a Codex quality
test by itself.

### Harness Scenario

Run the same semantic workload through a real Codex session using the configured DeepSeek provider.
Keep the same thread, model, tools, skill/plugin configuration, and working directory for all
rounds. This exercises the production request builder and message conversion path.

The benchmark must not silently disable tools or context injection merely to improve the result.
Its purpose is to expose the cost of the real harness.

### Report

Print one stable table followed by a machine-readable JSON summary. Each row includes:

```text
scenario, phase, round, prompt_tokens, hit_tokens, miss_tokens, hit_rate,
context_window, context_usage_rate, system_changed, tools_changed,
matching_message_prefix, cache_reset_reason
```

The summary includes token-weighted aggregate rates for control and harness steady-state rounds,
their percentage-point gap, and an automatic diagnosis:

- Control below 90%: provider persistence or best-effort variability; rerun before changing Codex.
- Control at least 90%, harness gap at least 5 points, tools changed: investigate tool schemas.
- Control at least 90%, harness gap at least 5 points, history rewritten: investigate context/history.
- Post-compaction miss: expected reset; exclude it from steady-state acceptance.
- Fingerprints stable but harness remains low: inspect serialization and provider behavior.

The live benchmark exits nonzero only for configuration, transport, parsing, or invariant errors.
It does not fail merely because cache rate is below target; Phase 1 is measurement, and DeepSeek
documents caching as best effort.

## Context-Window Adaptation

For every harness row, report:

- Raw model context window.
- Effective window after `effective_context_window_percent`.
- Provider-reported prompt tokens.
- Prompt usage percentage of the effective window.
- Configured auto-compact limit and scope.
- Estimated tokens remaining before compaction when available.
- Current context-window generation/window ID as a redacted ordinal.

Segment aggregates by context window. Never combine pre-compaction and post-compaction requests in
the same steady-state cache rate. A compaction starts a new measurement segment.

Phase 1 validates metadata but does not change it. A mismatch between DeepSeek's documented limit,
configured `ModelInfo`, and observed behavior becomes a separate corrective change with its own
tests.

## Testing

### Offline Tests

- Deserialize complete DeepSeek usage.
- Deserialize usage without cache fields.
- Emit `cached_input_tokens` from the final SSE usage chunk.
- Preserve successful parsing when hit + miss differs from prompt total.
- Canonical JSON produces stable fingerprints independent of object insertion order.
- Message array order and tool array order change fingerprints.
- Prefix comparison returns the exact longest unchanged prefix.
- No diagnostic value contains raw prompt or tool text.

### Live Checks

- Control and harness each complete warm-up plus configured rounds.
- Every completed request has provider usage.
- JSON report can be parsed and aggregate totals equal row totals.
- `smoke`, `normal`, and `long` reject payloads that exceed the effective model window before an
  API call is made.

## Acceptance Criteria

- `just test -p codex-api` passes with cache usage coverage.
- `just test -p codex-core` passes with fingerprint and comparison coverage.
- Default `just test` remains network-free.
- `DEEPSEEK_API_KEY=... just test-deepseek-cache` produces control and harness reports.
- Existing app-server and `ResponseEvent` schemas are unchanged.
- Existing `codex exec --json` reports nonzero cached input when DeepSeek supplies it.
- Logs expose no prompt text, tool definitions, API keys, or full thread IDs.
- The report separates warm-up, steady-state, and post-compaction samples.

## Follow-Up Optimization Gate

Do not start prompt-layout optimization until at least three comparable `normal` benchmark runs are
captured. Select Phase 2 work from evidence:

- Tool fingerprint instability: stabilize tool ordering/snapshots first.
- System instability: isolate and stabilize base instructions.
- History rewrite without compaction: fix Chat Completions conversion or context injection.
- Only compaction resets: tune context/compaction separately; do not distort ordinary prompt layout.
- Small control/harness gap: accept provider behavior and avoid unnecessary core changes.

