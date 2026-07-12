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
    /// Effective context window reported for the model, when known.
    effective_context_window: Option<i64>,
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

/// Render a human-readable table of all rows.
fn render_table(rows: &[CacheRow]) -> String {
    let mut out = String::new();
    out.push_str(
        "scenario        phase         round  prompt    hit       miss      hit_rate  eff_ctx\n",
    );
    for row in rows {
        let hit_rate = row
            .hit_rate()
            .map(|r| format!("{r:.3}"))
            .unwrap_or_else(|| "n/a".to_string());
        let eff_ctx = row
            .effective_context_window
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
            eff_ctx,
        ));
    }
    out
}

/// Render all rows plus the steady-state aggregate as a JSON object.
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
                "effective_context_window": row.effective_context_window,
            })
        })
        .collect();
    serde_json::json!({
        "rows": row_values,
        "steady_state_hit_rate": aggregate_steady_state_hit_rate(rows),
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
            effective_context_window: Some(60_000),
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

        let json = render_json(&rows);
        assert_eq!(json["rows"].as_array().expect("rows array").len(), 2);
        assert!(json["steady_state_hit_rate"].is_number());
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

    /// Effective context window we report for the control rows. The control has
    /// no ModelInfo, so we surface the profile guardrail as the window bound.
    const CONTROL_EFFECTIVE_CONTEXT_WINDOW: i64 = MAX_EFFECTIVE_CONTEXT_WINDOW_TOKENS as i64;

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

        // Read the key only at runtime; never fold it into any error or report.
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .expect("DEEPSEEK_API_KEY must be set for the paid control benchmark");

        let transport = ReqwestTransport::new(reqwest::Client::new());
        let auth: Arc<dyn codex_api::AuthProvider> = Arc::new(BearerAuthProvider::new(api_key));
        let client = ChatCompletionsClient::new(transport, provider(&config.base_url), auth);

        let document = generate_stable_document(config.profile.stable_prefix_tokens());
        let mut rows: Vec<CacheRow> = Vec::new();

        // Warm-up round: populate the provider cache, then wait for persistence.
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
            effective_context_window: Some(CONTROL_EFFECTIVE_CONTEXT_WINDOW),
        });
        tokio::time::sleep(config.warmup).await;

        // Steady-state rounds: these should hit the cache if the provider is
        // caching the stable prefix.
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
                effective_context_window: Some(CONTROL_EFFECTIVE_CONTEXT_WINDOW),
            });
        }

        println!("\n{}", render_table(&rows));
        println!(
            "{}",
            serde_json::to_string_pretty(&render_json(&rows)).expect("json renders")
        );

        // A low hit rate is report data, not a failure. Only structural
        // problems (handled via expect above) fail the benchmark.
    }
}
