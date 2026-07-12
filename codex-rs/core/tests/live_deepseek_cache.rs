//! DeepSeek prompt-cache benchmark: a *control* baseline that measures cache
//! behavior by driving the production Chat Completions client directly.
//!
//! Almost all of this file is offline-testable and runs in the default suite:
//! configuration parsing, profile sizing, deterministic content generation, and
//! report aggregation. Only the single network body is `#[ignore]` and reads
//! `DEEPSEEK_API_KEY` at runtime; it never appears in default CI or `just test`.
//!
//! The control isolates provider-side cache behavior from anything the Codex
//! harness does, so a later harness scenario (Task 6) can be compared against
//! it: if the control hits cache but the harness does not, the harness is
//! breaking the prefix; if neither hits, the provider is the variable.

#![allow(clippy::expect_used)]
#![allow(clippy::print_stdout)]

use std::time::Duration;

// ------------------------------------------------------------------------
// Configuration (offline-testable)
// ------------------------------------------------------------------------

const ENV_BASE_URL: &str = "DEEPSEEK_BASE_URL";
const ENV_MODEL: &str = "DEEPSEEK_MODEL";
const ENV_PROFILE: &str = "DEEPSEEK_CACHE_PROFILE";
const ENV_ROUNDS: &str = "DEEPSEEK_CACHE_ROUNDS";
const ENV_WARMUP_SECONDS: &str = "DEEPSEEK_CACHE_WARMUP_SECONDS";

const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
const DEFAULT_MODEL: &str = "deepseek-chat";

/// Stable-prefix sizing per profile, in approximate tokens. We approximate four
/// characters per token when generating deterministic content.
const APPROX_CHARS_PER_TOKEN: usize = 4;

/// Effective context window we refuse to exceed with the stable prefix, so a
/// misconfigured profile cannot silently overflow and force a truncation that
/// would itself look like a cache reset.
const MAX_EFFECTIVE_CONTEXT_WINDOW_TOKENS: usize = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheProfile {
    Smoke,
    Normal,
    Long,
}

impl CacheProfile {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "smoke" => Ok(Self::Smoke),
            "normal" => Ok(Self::Normal),
            "long" => Ok(Self::Long),
            other => Err(format!("unknown DEEPSEEK_CACHE_PROFILE: {other:?}")),
        }
    }

    /// Approximate stable-prefix token target for this profile.
    fn stable_prefix_tokens(self) -> usize {
        match self {
            Self::Smoke => 2_000,
            Self::Normal => 8_000,
            Self::Long => 32_000,
        }
    }
}

#[derive(Debug, Clone)]
struct BenchmarkConfig {
    base_url: String,
    model: String,
    profile: CacheProfile,
    /// Number of steady-state rounds (excludes the single warm-up round).
    steady_state_rounds: u32,
    warmup: Duration,
}

impl BenchmarkConfig {
    /// Parse configuration from an explicit key/value lookup so tests can drive
    /// it without touching the process environment.
    fn parse(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let base_url = get(ENV_BASE_URL).unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = get(ENV_MODEL).unwrap_or_else(|| DEFAULT_MODEL.to_string());

        let profile = match get(ENV_PROFILE) {
            Some(raw) => CacheProfile::parse(&raw)?,
            None => CacheProfile::Normal,
        };

        let steady_state_rounds = match get(ENV_ROUNDS) {
            Some(raw) => raw
                .parse::<u32>()
                .map_err(|_| format!("invalid {ENV_ROUNDS}: {raw:?}"))?,
            None => 3,
        };
        if steady_state_rounds < 2 {
            return Err(format!(
                "{ENV_ROUNDS} must be at least 2 steady-state rounds, got {steady_state_rounds}"
            ));
        }

        let warmup_seconds = match get(ENV_WARMUP_SECONDS) {
            Some(raw) => raw
                .parse::<u64>()
                .map_err(|_| format!("invalid {ENV_WARMUP_SECONDS}: {raw:?}"))?,
            None => 5,
        };
        if warmup_seconds > 120 {
            return Err(format!(
                "{ENV_WARMUP_SECONDS} is unreasonably large: {warmup_seconds}s"
            ));
        }

        let config = Self {
            base_url,
            model,
            profile,
            steady_state_rounds,
            warmup: Duration::from_secs(warmup_seconds),
        };

        let approx_tokens = config.profile.stable_prefix_tokens();
        if approx_tokens > MAX_EFFECTIVE_CONTEXT_WINDOW_TOKENS {
            return Err(format!(
                "profile prefix {approx_tokens} tokens exceeds effective window {MAX_EFFECTIVE_CONTEXT_WINDOW_TOKENS}"
            ));
        }

        Ok(config)
    }
}

// ------------------------------------------------------------------------
// Deterministic stable content (offline-testable)
// ------------------------------------------------------------------------

/// Build a deterministic block of numbered ASCII paragraphs targeting
/// approximately `target_tokens`. The output is byte-identical for a given
/// target, so the cacheable prefix is stable across rounds.
fn generate_stable_document(target_tokens: usize) -> String {
    let target_chars = target_tokens * APPROX_CHARS_PER_TOKEN;
    let mut doc = String::with_capacity(target_chars + 128);
    let mut paragraph = 0u32;
    while doc.len() < target_chars {
        paragraph += 1;
        // Fixed template; only the paragraph ordinal varies, and it varies
        // deterministically, so repeated generation is byte-identical.
        doc.push_str(&format!(
            "Paragraph {paragraph:06}: reference material for prompt cache measurement. \
             This sentence is intentionally verbose and stable so the leading bytes \
             of the request remain identical across benchmark rounds.\n"
        ));
    }
    doc
}

/// The system text is fixed for the whole run.
fn system_text() -> String {
    "You are a benchmark assistant. Answer with a single short sentence.".to_string()
}

/// A per-round question. Only this trailing text varies between rounds; the
/// system message and document prefix stay byte-identical.
fn round_question(round: u32) -> String {
    format!("Round {round}: reply with exactly the word ACK.")
}

// ------------------------------------------------------------------------
// Reports (offline-testable)
// ------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Warmup,
    SteadyState,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::SteadyState => "steady_state",
        }
    }
}

#[derive(Debug, Clone)]
struct CacheRow {
    scenario: String,
    phase: Phase,
    round: u32,
    prompt_tokens: u64,
    hit_tokens: u64,
    miss_tokens: u64,
    /// The model's real context window when known (harness rows). `None` for
    /// the control scenario, which has no `ModelInfo` — the control's sizing
    /// guardrail is reported separately as `profile_guardrail_tokens`, never
    /// mixed into this column.
    model_context_window: Option<i64>,
}

impl CacheRow {
    fn hit_rate(&self) -> Option<f64> {
        let denom = self.hit_tokens + self.miss_tokens;
        (denom > 0).then(|| self.hit_tokens as f64 / denom as f64)
    }
}

/// Aggregate steady-state rows (warm-up excluded) by summing token counts
/// *before* computing the rate, so large rounds are weighted correctly.
fn aggregate_steady_state_hit_rate(rows: &[CacheRow]) -> Option<f64> {
    let mut hit = 0u64;
    let mut miss = 0u64;
    for row in rows.iter().filter(|r| r.phase == Phase::SteadyState) {
        hit += row.hit_tokens;
        miss += row.miss_tokens;
    }
    let denom = hit + miss;
    (denom > 0).then(|| hit as f64 / denom as f64)
}

/// Aggregate steady-state hit rate for a single scenario ("control" or
/// "harness"), warm-up excluded.
fn scenario_steady_state_hit_rate(rows: &[CacheRow], scenario: &str) -> Option<f64> {
    let scoped: Vec<CacheRow> = rows
        .iter()
        .filter(|r| r.scenario == scenario)
        .cloned()
        .collect();
    aggregate_steady_state_hit_rate(&scoped)
}

/// Total steady-state miss tokens for a single scenario, warm-up excluded.
///
/// Miss tokens are the comparability-safe cross-scenario signal: unlike hit
/// *percentages* (which depend on each scenario's stable-prefix length), an
/// absolute count of prompt tokens that had to be re-processed is meaningful on
/// its own and can be compared per turn.
fn scenario_steady_state_miss_tokens(rows: &[CacheRow], scenario: &str) -> u64 {
    rows.iter()
        .filter(|r| r.scenario == scenario && r.phase == Phase::SteadyState)
        .map(|r| r.miss_tokens)
        .sum()
}

/// Observed layout stability during the harness run, correlated from the
/// `chat_completions.request_layout` diagnostics. These flags let the diagnosis
/// distinguish "provider is flaky" from "our harness is breaking the prefix".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HarnessLayoutSignals {
    tools_changed_during_turns: bool,
    system_changed_during_turns: bool,
    history_rewritten_during_turns: bool,
    /// A steady-state sample immediately followed a compaction (context-window
    /// generation changed), so a miss there is an expected reset.
    post_compaction_sample: bool,
}

/// The first Phase-2 optimization candidate implied by the measured evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheDiagnosis {
    /// Control itself did not hit cache: the provider is the variable, not us.
    ProviderVariability,
    /// Control hits, harness lags, and tool layout is unstable across turns.
    ToolLayoutInstability,
    /// Control hits, harness lags, and earlier history is being rewritten.
    HistoryLayoutInstability,
    /// The lagging sample immediately followed a compaction: expected reset.
    ExpectedCacheReset,
    /// Fingerprints are stable yet a gap persists: suspect serialization or
    /// provider behavior, capture a canonical request diff next.
    SerializationOrProviderBehavior,
    /// Control and harness are within tolerance: no core change warranted.
    WithinTolerance,
}

/// Minimum steady-state hit rate for a scenario to count as caching healthily.
///
/// Both scenarios are judged against this same absolute bar. We deliberately do
/// NOT subtract the harness rate from the control rate: the control sends a
/// short fixed document with no tools (~1.4K prompt tokens) while the harness
/// carries the full Codex system prompt, tools, and a growing history (~3K+),
/// so their hit *percentages* are not directly comparable — a higher harness
/// percentage would not prove the harness avoids extra misses, and a lower one
/// would not prove it causes them. Comparing each against an absolute bar, plus
/// the per-scenario miss-token counts in the report, avoids that trap.
const HEALTHY_HIT_RATE: f64 = 0.90;

/// Apply the decision matrix to measured per-scenario rates and observed layout
/// signals.
///
/// Rates are token-weighted steady-state hit rates in `[0, 1]`. Precedence:
/// 1. If the control cannot cache, nothing is attributable to the harness.
/// 2. A post-compaction sample is an expected reset regardless of rate.
/// 3. If the harness caches healthily on its own bar, we are done — the
///    provider caches and so do we.
/// 4. Otherwise the provider caches but the harness does not: attribute to the
///    observed layout instability (tools, then history, then serialization).
fn diagnose(
    control_hit_rate: Option<f64>,
    harness_hit_rate: Option<f64>,
    signals: HarnessLayoutSignals,
) -> CacheDiagnosis {
    let control = control_hit_rate.unwrap_or(0.0);
    let harness = harness_hit_rate.unwrap_or(0.0);

    // The control establishes whether the provider caches stable prefixes at
    // all. If it does not, nothing can be attributed to the harness.
    if control < HEALTHY_HIT_RATE {
        return CacheDiagnosis::ProviderVariability;
    }

    // A post-compaction sample is an expected reset regardless of rate.
    if signals.post_compaction_sample {
        return CacheDiagnosis::ExpectedCacheReset;
    }

    // The provider caches; judge the harness on its OWN absolute health rather
    // than on a percentage subtracted from the control. If the harness also
    // caches healthily, we are done.
    if harness >= HEALTHY_HIT_RATE {
        return CacheDiagnosis::WithinTolerance;
    }

    // Provider caches, but the harness does not — attribute to layout.
    if signals.tools_changed_during_turns {
        return CacheDiagnosis::ToolLayoutInstability;
    }
    if signals.history_rewritten_during_turns {
        return CacheDiagnosis::HistoryLayoutInstability;
    }
    CacheDiagnosis::SerializationOrProviderBehavior
}

/// Render a human-readable table of all rows.
fn render_table(rows: &[CacheRow]) -> String {
    let mut out = String::new();
    out.push_str(
        "scenario        phase         round  prompt    hit       miss      hit_rate  model_ctx\n",
    );
    for row in rows {
        let hit_rate = row
            .hit_rate()
            .map(|r| format!("{r:.3}"))
            .unwrap_or_else(|| "n/a".to_string());
        let model_ctx = row
            .model_context_window
            .map(|w| w.to_string())
            .unwrap_or_else(|| "n/a".to_string());
        out.push_str(&format!(
            "{:<15} {:<13} {:<6} {:<9} {:<9} {:<9} {:<9} {}\n",
            row.scenario,
            row.phase.as_str(),
            row.round,
            row.prompt_tokens,
            row.hit_tokens,
            row.miss_tokens,
            hit_rate,
            model_ctx,
        ));
    }
    out
}

/// Render all rows plus per-scenario aggregates as a JSON object.
///
/// Cross-scenario comparison uses absolute steady-state **miss tokens** and
/// each scenario's own hit rate, never a subtraction of the two hit
/// percentages — the control and harness carry different stable-prefix lengths,
/// so their percentages are not directly comparable.
fn render_json(rows: &[CacheRow]) -> serde_json::Value {
    let row_values: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "scenario": row.scenario,
                "phase": row.phase.as_str(),
                "round": row.round,
                "prompt_tokens": row.prompt_tokens,
                "hit_tokens": row.hit_tokens,
                "miss_tokens": row.miss_tokens,
                "hit_rate": row.hit_rate(),
                "model_context_window": row.model_context_window,
            })
        })
        .collect();
    serde_json::json!({
        "rows": row_values,
        "steady_state_hit_rate": aggregate_steady_state_hit_rate(rows),
        "control_steady_state_miss_tokens": scenario_steady_state_miss_tokens(rows, "control"),
        "harness_steady_state_miss_tokens": scenario_steady_state_miss_tokens(rows, "harness"),
        "comparability_note": "hit rates are per-scenario and not directly comparable across scenarios (different stable-prefix lengths); compare absolute miss tokens instead",
    })
}

// ------------------------------------------------------------------------
// Offline tests
// ------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn from_map(pairs: &[(&str, &str)]) -> Result<BenchmarkConfig, String> {
        BenchmarkConfig::parse(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        })
    }

    #[test]
    fn defaults_are_offline_safe() {
        let config = from_map(&[]).expect("defaults parse");
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.profile, CacheProfile::Normal);
        assert_eq!(config.steady_state_rounds, 3);
        assert_eq!(config.warmup, Duration::from_secs(5));
    }

    #[test]
    fn profiles_target_expected_token_sizes() {
        assert_eq!(CacheProfile::Smoke.stable_prefix_tokens(), 2_000);
        assert_eq!(CacheProfile::Normal.stable_prefix_tokens(), 8_000);
        assert_eq!(CacheProfile::Long.stable_prefix_tokens(), 32_000);
    }

    #[test]
    fn unknown_profile_is_rejected() {
        let err = from_map(&[(ENV_PROFILE, "gigantic")]).expect_err("unknown profile rejected");
        assert!(err.contains("gigantic"), "{err}");
    }

    #[test]
    fn fewer_than_two_rounds_is_rejected() {
        let err = from_map(&[(ENV_ROUNDS, "1")]).expect_err("one round rejected");
        assert!(err.contains(ENV_ROUNDS), "{err}");
    }

    #[test]
    fn unreasonable_warmup_is_rejected() {
        let err = from_map(&[(ENV_WARMUP_SECONDS, "600")]).expect_err("long warmup rejected");
        assert!(err.contains(ENV_WARMUP_SECONDS), "{err}");
    }

    #[test]
    fn generated_document_is_deterministic_and_sized() {
        let a = generate_stable_document(2_000);
        let b = generate_stable_document(2_000);
        assert_eq!(a, b, "document generation must be deterministic");
        // At least the requested character budget was produced.
        assert!(a.len() >= 2_000 * APPROX_CHARS_PER_TOKEN);
    }

    #[test]
    fn only_the_question_varies_between_rounds() {
        assert_eq!(system_text(), system_text());
        assert_ne!(round_question(1), round_question(2));
    }

    fn row(scenario: &str, phase: Phase, round: u32, hit: u64, miss: u64) -> CacheRow {
        CacheRow {
            scenario: scenario.to_string(),
            phase,
            round,
            prompt_tokens: hit + miss,
            hit_tokens: hit,
            miss_tokens: miss,
            model_context_window: Some(60_000),
        }
    }

    #[test]
    fn aggregate_excludes_warmup_and_sums_before_dividing() {
        let rows = vec![
            // Warm-up is a full miss and must be excluded.
            row("control", Phase::Warmup, 0, 0, 1_000),
            row("control", Phase::SteadyState, 1, 900, 100),
            row("control", Phase::SteadyState, 2, 800, 200),
        ];
        // (900 + 800) / (1000 + 1000) = 0.85; warm-up excluded.
        let rate = aggregate_steady_state_hit_rate(&rows).expect("rate available");
        assert!((rate - 0.85).abs() < 1e-9, "unexpected rate {rate}");
    }

    #[test]
    fn aggregate_without_signal_is_none() {
        let rows = vec![row("control", Phase::SteadyState, 1, 0, 0)];
        assert_eq!(aggregate_steady_state_hit_rate(&rows), None);
    }

    #[test]
    fn table_and_json_render_all_rows() {
        let rows = vec![
            row("control", Phase::Warmup, 0, 0, 1_000),
            row("control", Phase::SteadyState, 1, 900, 100),
        ];
        let table = render_table(&rows);
        assert!(table.contains("warmup"));
        assert!(table.contains("steady_state"));
        assert!(table.contains("model_ctx"));

        let json = render_json(&rows);
        assert_eq!(json["rows"].as_array().expect("rows array").len(), 2);
        assert!(json["steady_state_hit_rate"].is_number());
        // Comparability-safe cross-scenario signals are present.
        assert!(json["control_steady_state_miss_tokens"].is_number());
        assert!(json["harness_steady_state_miss_tokens"].is_number());
        assert!(json["comparability_note"].is_string());
        // Rows expose the model window (not a guardrail) under its own key.
        assert!(json["rows"][0].get("model_context_window").is_some());
    }

    #[test]
    fn miss_tokens_are_summed_per_scenario_warmup_excluded() {
        let rows = vec![
            row("control", Phase::Warmup, 0, 0, 500),
            row("control", Phase::SteadyState, 1, 900, 100),
            row("control", Phase::SteadyState, 2, 800, 200),
            row("harness", Phase::SteadyState, 1, 950, 50),
        ];
        // Warm-up 500 excluded; 100 + 200 = 300.
        assert_eq!(scenario_steady_state_miss_tokens(&rows, "control"), 300);
        assert_eq!(scenario_steady_state_miss_tokens(&rows, "harness"), 50);
    }

    #[test]
    fn dual_scenario_aggregation_is_per_scenario_and_warmup_excluded() {
        let rows = vec![
            row("control", Phase::Warmup, 0, 0, 1_000),
            row("control", Phase::SteadyState, 1, 950, 50),
            row("control", Phase::SteadyState, 2, 950, 50),
            row("harness", Phase::Warmup, 0, 0, 1_000),
            row("harness", Phase::SteadyState, 1, 500, 500),
            row("harness", Phase::SteadyState, 2, 500, 500),
        ];

        let control = scenario_steady_state_hit_rate(&rows, "control").expect("control rate");
        let harness = scenario_steady_state_hit_rate(&rows, "harness").expect("harness rate");
        assert!((control - 0.95).abs() < 1e-9, "control {control}");
        assert!((harness - 0.50).abs() < 1e-9, "harness {harness}");
    }

    fn signals(
        tools: bool,
        system: bool,
        history: bool,
        post_compaction: bool,
    ) -> HarnessLayoutSignals {
        HarnessLayoutSignals {
            tools_changed_during_turns: tools,
            system_changed_during_turns: system,
            history_rewritten_during_turns: history,
            post_compaction_sample: post_compaction,
        }
    }

    #[test]
    fn diagnosis_low_control_blames_provider() {
        // Control below the health bar: the provider is not caching stable
        // prefixes, so nothing is attributable to the harness — even with every
        // layout signal set.
        assert_eq!(
            diagnose(Some(0.40), Some(0.10), signals(true, true, true, false)),
            CacheDiagnosis::ProviderVariability
        );
    }

    #[test]
    fn diagnosis_provider_caches_harness_unhealthy_with_tool_change() {
        // Provider caches (control healthy) but the harness misses on its own
        // absolute bar, with tool layout changing across turns.
        assert_eq!(
            diagnose(Some(0.95), Some(0.60), signals(true, false, false, false)),
            CacheDiagnosis::ToolLayoutInstability
        );
    }

    #[test]
    fn diagnosis_provider_caches_harness_unhealthy_with_history_rewrite() {
        assert_eq!(
            diagnose(Some(0.95), Some(0.60), signals(false, false, true, false)),
            CacheDiagnosis::HistoryLayoutInstability
        );
    }

    #[test]
    fn diagnosis_post_compaction_is_expected_reset() {
        // A post-compaction sample is an expected reset and takes precedence
        // over layout-instability classification, even at a low harness rate.
        assert_eq!(
            diagnose(Some(0.95), Some(0.10), signals(true, true, true, true)),
            CacheDiagnosis::ExpectedCacheReset
        );
    }

    #[test]
    fn diagnosis_stable_fingerprints_but_harness_unhealthy_is_serialization_or_provider() {
        assert_eq!(
            diagnose(Some(0.95), Some(0.60), signals(false, false, false, false)),
            CacheDiagnosis::SerializationOrProviderBehavior
        );
    }

    #[test]
    fn diagnosis_both_scenarios_healthy_is_within_tolerance() {
        // Each scenario clears the absolute health bar on its own; no gap
        // subtraction is involved.
        assert_eq!(
            diagnose(Some(0.95), Some(0.92), signals(true, true, true, false)),
            CacheDiagnosis::WithinTolerance
        );
    }

    #[test]
    fn diagnosis_ignores_raw_percentage_gap_between_scenarios() {
        // The harness percentage exceeding the control percentage must NOT be
        // read as proof of anything: the two workloads have different stable
        // prefix lengths. Both healthy on their own bar => WithinTolerance.
        assert_eq!(
            diagnose(Some(0.93), Some(0.98), signals(false, false, false, false)),
            CacheDiagnosis::WithinTolerance
        );
        // And a harness far above an unhealthy control is still ProviderVariability:
        // if the provider itself is not caching the control, the harness number
        // cannot be trusted as evidence the harness is fine.
        assert_eq!(
            diagnose(Some(0.50), Some(0.98), signals(false, false, false, false)),
            CacheDiagnosis::ProviderVariability
        );
    }
}

// ------------------------------------------------------------------------
// Paid live control (network; ignored by default)
// ------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and incurs API cost"]
async fn deepseek_cache_control_benchmark() {
    live::run_control().await;
}

/// Combined benchmark: runs the direct-API control and the production Codex
/// harness against the same profile, then prints both scenarios plus a
/// diagnosis in one report.
#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and incurs API cost"]
async fn deepseek_cache_report() {
    live::run_harness_and_control().await;
}

/// Network-only module. Nothing here runs unless the ignored test above is
/// explicitly selected.
mod live {
    use super::*;
    use codex_api::ChatCompletionsClient;
    use codex_api::ChatCompletionsRequest;
    use codex_api::ResponseEvent;
    use codex_api::StreamOptions;
    use codex_client::ReqwestTransport;
    use codex_model_provider::BearerAuthProvider;
    use futures::StreamExt;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The control has no `ModelInfo`, so its rows carry `model_context_window:
    /// None`; the profile's sizing guardrail is reported separately as
    /// `profile_guardrail_tokens` and never conflated with a model window.
    const PROFILE_GUARDRAIL_TOKENS: i64 = MAX_EFFECTIVE_CONTEXT_WINDOW_TOKENS as i64;

    fn provider(base_url: &str) -> codex_api::Provider {
        codex_api::Provider {
            name: "deepseek-control".to_string(),
            base_url: format!("{}/v1", base_url.trim_end_matches('/')),
            query_params: None,
            headers: http::HeaderMap::new(),
            retry: codex_api::RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(0),
                retry_429: false,
                retry_5xx: false,
                retry_transport: false,
            },
            stream_idle_timeout: Duration::from_secs(120),
        }
    }

    fn build_request(model: &str, document: &str, question: &str) -> ChatCompletionsRequest {
        // System + stable document first (the cacheable prefix), then the
        // per-round question last so only the tail changes.
        let messages = vec![
            serde_json::json!({"role": "system", "content": system_text()}),
            serde_json::json!({"role": "user", "content": document}),
            serde_json::json!({"role": "user", "content": question}),
        ];
        ChatCompletionsRequest {
            model: model.to_string(),
            messages,
            tools: Vec::new(),
            tool_choice: None,
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            temperature: Some(0.0),
            max_tokens: Some(16),
            parallel_tool_calls: None,
            enable_thinking: None,
            thinking_budget: None,
            thinking: None,
        }
    }

    /// Drive one request to completion and return the (prompt, hit, miss)
    /// token counts from the terminal usage. Panics if usage never arrives —
    /// missing usage is a hard error, not report data.
    async fn run_once(
        client: &ChatCompletionsClient<ReqwestTransport>,
        request: ChatCompletionsRequest,
    ) -> (u64, u64, u64) {
        let mut stream = client
            .stream_request(request, http::HeaderMap::new())
            .await
            .expect("control request should start streaming");

        let mut usage = None;
        while let Some(event) = stream.next().await {
            if let ResponseEvent::Completed { token_usage, .. } =
                event.expect("stream event should not error")
            {
                usage = token_usage;
            }
        }

        let usage = usage.expect("control request must report final usage");
        let prompt = usage.input_tokens.max(0) as u64;
        let hit = usage.cached_input_tokens.max(0) as u64;
        let miss = prompt.saturating_sub(hit);
        (prompt, hit, miss)
    }

    pub(super) async fn run_control() {
        let env: HashMap<String, String> = std::env::vars().collect();
        let config = BenchmarkConfig::parse(|key| env.get(key).cloned())
            .expect("benchmark configuration should be valid");

        let mut rows: Vec<CacheRow> = Vec::new();
        collect_control_rows(&config, &mut rows).await;

        println!("\n{}", render_table(&rows));
        println!(
            "{}",
            serde_json::to_string_pretty(&render_json(&rows)).expect("json renders")
        );

        // A low hit rate is report data, not a failure. Only structural
        // problems (handled via expect above) fail the benchmark.
    }

    // --------------------------------------------------------------------
    // Harness scenario: drive a real production Codex session over DeepSeek.
    // --------------------------------------------------------------------

    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use tracing::Event;
    use tracing::Subscriber;
    use tracing::field::Visit;
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context as LayerContext;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::util::SubscriberInitExt;

    /// Captures `chat_completions.request_layout` diagnostics so the harness can
    /// tell whether tools/system/history drifted across turns.
    #[derive(Clone, Default)]
    struct LayoutCaptureLayer {
        signals: Arc<Mutex<HarnessLayoutSignals>>,
    }

    #[derive(Default)]
    struct LayoutVisitor {
        fields: BTreeMap<String, String>,
    }

    impl Visit for LayoutVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
        fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }

    impl<S> Layer<S> for LayoutCaptureLayer
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
            if event.metadata().target() != "chat_completions.request_layout" {
                return;
            }
            let mut visitor = LayoutVisitor::default();
            event.record(&mut visitor);

            let mut signals = self
                .signals
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if visitor.fields.get("tools_changed").map(String::as_str) == Some("true") {
                signals.tools_changed_during_turns = true;
            }
            if visitor.fields.get("system_changed").map(String::as_str) == Some("true") {
                signals.system_changed_during_turns = true;
            }
            if let Some(reset) = visitor.fields.get("reset_reason")
                && reset.contains("HistoryRewritten")
            {
                signals.history_rewritten_during_turns = true;
            }
        }
    }

    /// Extract the terminal `last_token_usage` after a turn completes, plus the
    /// model context window reported alongside it.
    fn usage_to_row(
        scenario: &str,
        phase: Phase,
        round: u32,
        usage: &codex_protocol::protocol::TokenUsage,
        context_window: Option<i64>,
    ) -> CacheRow {
        let prompt = usage.input_tokens.max(0) as u64;
        let hit = usage.cached_input_tokens.max(0) as u64;
        let miss = prompt.saturating_sub(hit);
        CacheRow {
            scenario: scenario.to_string(),
            phase,
            round,
            prompt_tokens: prompt,
            hit_tokens: hit,
            miss_tokens: miss,
            model_context_window: context_window,
        }
    }

    /// Run one turn on the shared thread and return the last-turn usage plus the
    /// model context window. Requires a token-count event; missing usage is a
    /// hard error.
    async fn run_harness_turn(
        codex: &codex_core::CodexThread,
        prompt: &str,
    ) -> (codex_protocol::protocol::TokenUsage, Option<i64>) {
        use codex_protocol::protocol::EventMsg;
        use codex_protocol::protocol::Op;
        use codex_protocol::user_input::UserInput;

        codex
            .submit(Op::UserInput {
                items: vec![UserInput::Text {
                    text: prompt.to_string(),
                    text_elements: Vec::new(),
                }],
                final_output_json_schema: None,
                responsesapi_client_metadata: None,
                additional_context: Default::default(),
                thread_settings: Default::default(),
            })
            .await
            .expect("submit user input");

        let mut last_usage = None;
        let mut context_window = None;
        loop {
            let event = codex.next_event().await.expect("event stream");
            match event.msg {
                EventMsg::TokenCount(ev) => {
                    if let Some(info) = ev.info {
                        last_usage = Some(info.last_token_usage);
                        context_window = info.model_context_window;
                    }
                }
                EventMsg::TurnComplete(_) => break,
                EventMsg::Error(err) => panic!("harness turn errored: {}", err.message),
                _ => {}
            }
        }

        (
            last_usage.expect("harness turn must report token usage"),
            context_window,
        )
    }

    pub(super) async fn run_harness_and_control() {
        use core_test_support::test_codex::test_codex;

        let env: HashMap<String, String> = std::env::vars().collect();
        let config = BenchmarkConfig::parse(|key| env.get(key).cloned())
            .expect("benchmark configuration should be valid");
        // Read the key only at runtime; never fold it into any error or report.
        let _ = std::env::var("DEEPSEEK_API_KEY")
            .expect("DEEPSEEK_API_KEY must be set for the paid harness benchmark");

        // ---- Control scenario (direct API) -----------------------------
        let mut rows: Vec<CacheRow> = Vec::new();
        collect_control_rows(&config, &mut rows).await;

        // ---- Harness scenario (production session) ---------------------
        let capture = LayoutCaptureLayer::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        let _guard = subscriber.set_default();

        let deepseek_base = config.base_url.clone();
        let model = config.model.clone();
        let server = wiremock::MockServer::start().await;
        let test = test_codex()
            .with_config(move |cfg| {
                let mut provider =
                    codex_model_provider_info::ModelProviderInfo::create_deepseek_provider();
                provider.base_url = Some(format!("{}/v1", deepseek_base.trim_end_matches('/')));
                provider.supports_websockets = false;
                cfg.model_provider = provider;
                cfg.model = Some(model);
            })
            .build(&server)
            .await
            .expect("build production DeepSeek session");

        let document = generate_stable_document(config.profile.stable_prefix_tokens());

        // Warm-up turn primes the cache; wait the persistence interval.
        let warmup_prompt = format!("{document}\n\n{}", round_question(0));
        let (usage, ctx) = run_harness_turn(&test.codex, &warmup_prompt).await;
        rows.push(usage_to_row("harness", Phase::Warmup, 0, &usage, ctx));
        tokio::time::sleep(config.warmup).await;

        // Steady-state turns reuse the same thread; only the question varies.
        // Detect compaction from an actual prompt-token DROP between successive
        // rounds: within a single reused thread the prompt only grows as history
        // accumulates, so a decrease means the history was compacted (or an
        // earlier turn was rewritten), which resets the cacheable prefix. The
        // model's context window is capacity and stays constant, so it cannot
        // serve as this signal. A HistoryRewritten layout reset (captured via
        // tracing) corroborates the same event.
        let mut previous_prompt_tokens: Option<u64> = None;
        for round in 1..=config.steady_state_rounds {
            let (usage, ctx) = run_harness_turn(&test.codex, &round_question(round)).await;
            let prompt_tokens = usage.input_tokens.max(0) as u64;
            let compacted = previous_prompt_tokens.is_some_and(|prev| prompt_tokens < prev);
            if compacted {
                capture
                    .signals
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .post_compaction_sample = true;
            }
            previous_prompt_tokens = Some(prompt_tokens);
            rows.push(usage_to_row(
                "harness",
                Phase::SteadyState,
                round,
                &usage,
                ctx,
            ));
        }

        let signals = *capture
            .signals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let control_rate = scenario_steady_state_hit_rate(&rows, "control");
        let harness_rate = scenario_steady_state_hit_rate(&rows, "harness");
        let diagnosis = diagnose(control_rate, harness_rate, signals);

        println!("\n{}", render_table(&rows));
        let mut report = render_json(&rows);
        report["control_steady_state_hit_rate"] = serde_json::json!(control_rate);
        report["harness_steady_state_hit_rate"] = serde_json::json!(harness_rate);
        report["profile_guardrail_tokens"] = serde_json::json!(PROFILE_GUARDRAIL_TOKENS);
        report["tools_changed_during_turns"] =
            serde_json::json!(signals.tools_changed_during_turns);
        report["history_rewritten_during_turns"] =
            serde_json::json!(signals.history_rewritten_during_turns);
        report["post_compaction_sample"] = serde_json::json!(signals.post_compaction_sample);
        report["diagnosis"] = serde_json::json!(format!("{diagnosis:?}"));
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("json renders")
        );

        // A low hit rate is report data, not a failure. Only configuration,
        // transport, parsing, missing usage, or invariant errors fail (handled
        // via expect/panic above).
    }

    /// Shared control-collection used by both the control-only and combined
    /// benchmarks, appending "control" rows to `rows`.
    async fn collect_control_rows(config: &BenchmarkConfig, rows: &mut Vec<CacheRow>) {
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .expect("DEEPSEEK_API_KEY must be set for the paid control benchmark");
        let transport = ReqwestTransport::new(reqwest::Client::new());
        let auth: Arc<dyn codex_api::AuthProvider> = Arc::new(BearerAuthProvider::new(api_key));
        let client = ChatCompletionsClient::new(transport, provider(&config.base_url), auth);
        let document = generate_stable_document(config.profile.stable_prefix_tokens());

        let (prompt, hit, miss) = run_once(
            &client,
            build_request(&config.model, &document, &round_question(0)),
        )
        .await;
        rows.push(CacheRow {
            scenario: "control".to_string(),
            phase: Phase::Warmup,
            round: 0,
            prompt_tokens: prompt,
            hit_tokens: hit,
            miss_tokens: miss,
            model_context_window: None,
        });
        tokio::time::sleep(config.warmup).await;

        for round in 1..=config.steady_state_rounds {
            let (prompt, hit, miss) = run_once(
                &client,
                build_request(&config.model, &document, &round_question(round)),
            )
            .await;
            rows.push(CacheRow {
                scenario: "control".to_string(),
                phase: Phase::SteadyState,
                round,
                prompt_tokens: prompt,
                hit_tokens: hit,
                miss_tokens: miss,
                model_context_window: None,
            });
        }
    }
}
