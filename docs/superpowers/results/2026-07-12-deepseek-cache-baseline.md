# DeepSeek Prompt-Cache Baseline — 2026-07-12

Sanitized results from the DeepSeek cache observability benchmark
(`just test-deepseek-cache`). No prompt text, tool schemas, or key material —
only token counts, hit rates, and diagnoses.

## Configuration

| Field | Value |
|---|---|
| Model | `deepseek-v4-flash` |
| Base URL | `https://api.deepseek.com` (default) |
| Warm-up | 5 s persistence wait, excluded from steady-state aggregates |
| Recipe | `DEEPSEEK_MODEL=deepseek-v4-flash DEEPSEEK_CACHE_PROFILE=<profile> just test-deepseek-cache` |

The recipe runs only the combined `deepseek_cache_report` test, single-threaded
(`-j1`), so the control and harness scenarios cannot concurrently cross-warm the
shared provider cache.

## Smoke profile (~2K-token stable prefix), 3 steady-state rounds

| scenario | phase | round | prompt | hit | miss | hit_rate | model_ctx |
|---|---|---|---|---|---|---|---|
| control | warmup | 0 | 1373 | 1280 | 93 | 0.932 | n/a |
| control | steady | 1–3 | 1373 | 1280 | 93 | 0.932 | n/a |
| harness | warmup | 0 | 3192 | 1664 | 1528 | 0.521 | 950000 |
| harness | steady | 1 | 3209 | 3072 | 137 | 0.957 | 950000 |
| harness | steady | 2 | 3226 | 3200 | 26 | 0.992 | 950000 |
| harness | steady | 3 | 3243 | 3200 | 43 | 0.987 | 950000 |

- control steady-state hit rate: **0.932**, miss tokens: **279**
- harness steady-state hit rate: **0.979**, miss tokens: **206**
- diagnosis: **WithinTolerance**

Note: the harness warm-up shows a partial hit (0.521) because the control
scenario ran first in the same serialized invocation and left a warm prefix on
the shared account. This is expected and contained now that both scenarios run
in one serialized test rather than two concurrent ones.

## Normal profile (~8K-token stable prefix), 3 warm-cache steady-state runs

Three consecutive runs produced **byte-identical** aggregates:

| metric | run 1 | run 2 | run 3 |
|---|---|---|---|
| control steady-state hit rate | 0.98851 | 0.98851 | 0.98851 |
| control steady-state miss tokens | 183 | 183 | 183 |
| harness steady-state hit rate | 0.98297 | 0.98297 | 0.98297 |
| harness steady-state miss tokens | 366 | 366 | 366 |
| tools_changed_during_turns | false | false | false |
| system_changed_during_turns | false | false | false |
| history_rewritten_during_turns | false | false | false |
| diagnosis | WithinTolerance | WithinTolerance | WithinTolerance |

**These are three warm-cache steady-state runs, not three independent cold
baselines.** DeepSeek's prompt cache persists across requests and across test
invocations, so byte-identical results demonstrate that the *steady-state warm
cache is reproducible* — they do not each re-exercise "cold start → persist →
hit" from scratch. For a true cold baseline, prepend a per-run unique namespace
to the stable prefix so each run misses on its first request.

## Interpretation (decision matrix)

- **The provider caches stable prefixes reliably.** Control hit rate ≥ 0.93
  (smoke) and ≈ 0.99 (normal), well above the 0.90 health bar.
- **The Codex harness does not break the cacheable prefix.** Harness steady-state
  hit rate ≈ 0.98 and clears the same absolute bar on its own. No tool-order,
  system, or history-rewrite instability was observed across turns.
- **Per-scenario figures are trend signals, not cross-scenario comparisons.**
  The control (~1.4K/8K prompt, no tools) and harness (~3.2K prompt with system
  + tools + growing history) carry different system prompts, tools, history, and
  per-round tails. Neither their hit percentages nor their absolute miss counts
  are directly comparable: 366 harness misses vs 183 control misses does **not**
  mean the harness caused 183 extra misses. Each scenario is judged on its own
  absolute health bar and watched for its own trend across runs.

**Conclusion:** evidence falls on the "control healthy AND harness healthy on
its own bar" branch of the decision matrix → **no `codex-core` change is
warranted for prompt caching.** Phase 2 optimization is not indicated by this
data. Re-run after any change to the system prompt, tool set/order, or
history-conversion path.

