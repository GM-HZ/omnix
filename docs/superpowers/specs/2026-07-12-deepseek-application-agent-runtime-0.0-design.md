# DeepSeek Application Agent Runtime 0.0 Technical Design

## 1. Status

This document freezes the scope and release contract for Runtime 0.0.

Runtime 0.0 is not a general multi-provider platform, a coding-agent product, or a knowledge
application. It is a reusable, single-tenant Agent runtime that multiple independent business
applications can embed and specialize.

Existing redundant modules may remain compiled and present as long as they do not alter the
supported DeepSeek path, perform unexpected product-network calls, or prevent the release gate from
passing.

## 2. Product Definition

DeepSeek Application Agent Runtime 0.0 is a single-tenant Agent worker for embedding
DeepSeek-powered tool-using agents into independent business applications.

The runtime owns domain-neutral Agent mechanics:

- multi-turn conversation and Agent loop;
- streamed text and reasoning;
- tool discovery, invocation, and result continuation;
- dynamic tools, skills, local plugins, and MCP;
- context accounting and automatic compaction;
- persisted threads and resume;
- token and prompt-cache accounting;
- app-server stdio and in-process host boundaries;
- codex exec as a smoke and debugging surface.

Each business application owns:

- system and developer instructions;
- business tools and their authorization;
- skills and plugins;
- application data and storage;
- UI and workflow semantics;
- domain-specific output interpretation;
- user identity and business permissions;
- application-level auditing and policy.

A knowledge application is one reference consumer. Customer support, document processing,
analytics, workflow automation, internal assistants, and other applications use the same runtime
contract without changes to the Agent kernel.

## 3. Deployment Boundary

Runtime 0.0 supports one tenant and one application execution boundary per worker:

    Application A -> Worker A -> isolated home/workspace/credentials/state
    Application B -> Worker B -> isolated home/workspace/credentials/state
    Application C -> Worker C -> isolated home/workspace/credentials/state

Multiple applications may use the same runtime distribution. A single worker is not required to
provide process-internal tenant isolation.

Supported hosts:

1. App-server over stdio is the recommended desktop and service-worker boundary. The host owns
   process startup, shutdown, restart, and health supervision.
2. In-process app-server is supported for Rust hosts that need lower transport overhead. It uses the
   same app-server semantics and bounded in-memory queues.
3. codex exec is supported only as smoke, debugging, and acceptance tooling. It is not the
   application integration contract.

FFI, shared multi-tenant workers, and a new public runtime facade are outside Runtime 0.0.

## 4. Supported Model Contract

### 4.1 Required Model

The only formally supported and release-gated model slug is deepseek-v4-flash.

Provider contract:

    Provider family: DeepSeek
    Base URL: configurable; default https://api.deepseek.com/v1
    Authentication: DEEPSEEK_API_KEY or host-provided bearer credential
    Wire API: POST /chat/completions
    Streaming: SSE
    Model slug: deepseek-v4-flash
    Raw context window: 1,000,000 tokens
    Effective input window: 950,000 tokens
    Automatic compact threshold: 850,000 tokens

deepseek-v4-pro may work through the same protocol and data path, but Runtime 0.0 does not maintain
a separate compatibility promise, implementation branch, or release matrix for it.

Other providers and model slugs are best-effort only. Their existing code may remain, but failures
specific to them do not block Runtime 0.0.

### 4.2 Thinking Mapping

Runtime 0.0 freezes the existing reasoning-effort mapping:

| Runtime effort | DeepSeek request |
| --- | --- |
| none, minimal, low | thinking.type = disabled |
| medium | thinking.type = enabled |
| high, max | thinking.type = annotated, budget 8,192 |
| xhigh, ultra | thinking.type = annotated, budget 16,384 |

The response adapter must preserve reasoning_content across ordinary responses and tool-call turns.
Reasoning is internal Agent history and must not be duplicated as final assistant text.

### 4.3 Tool Calling

The supported path must handle:

- one function call;
- multiple sequential function calls;
- multiple tool calls in one streamed response;
- incremental function name and argument chunks;
- reasoning followed by a tool call;
- tool result continuation;
- reasoning followed by final text;
- malformed or incomplete tool arguments as a controlled turn error;
- cancellation while a tool or model stream is active.

ResponseEvent remains the internal compatibility boundary. DeepSeek adaptation stays below that
boundary in the Chat Completions request and response adapters.

## 5. One-Million-Token Context Policy

### 5.1 Window Definitions

Runtime 0.0 defines three explicit limits:

    raw model window       = 1,000,000
    effective input window =   950,000
    automatic compact      =   850,000

The 50K raw-window reserve protects provider output and model-side overhead. The additional 100K
between compact and effective limits protects:

- compaction request and summary output;
- tool schemas and tool results;
- reasoning output;
- token estimation error;
- context injected between the threshold check and the next model request.

The runtime must never intentionally wait until 950K before starting compaction.

### 5.2 Accounting Source

Provider-reported prompt usage is authoritative after a completed request. Local estimates are used
before provider usage exists and for proactive threshold checks.

Reports must distinguish prompt input, cached input, non-cached input, output, reasoning output when
available, model context window, tokens remaining before compaction, and context segment identity.

Cached input tokens still occupy model context. Cache accounting affects cost and latency, not
context-window occupancy.

### 5.3 Compaction Behavior

At or above 850K active context tokens:

1. stop starting an ordinary sampling request;
2. construct a bounded compaction input;
3. preserve required system/developer context, tool continuity, and real user intent;
4. install the compacted history as a new context segment;
5. recompute context usage;
6. continue the interrupted turn;
7. mark the first post-compaction request as a cache reset;
8. persist enough metadata for resume to reconstruct the same segment.

Compaction success requires the next ordinary turn to complete. Producing a summary alone is not
sufficient.

If compaction fails, the runtime returns a controlled error and preserves the last durable history.
It must not silently drop history and continue.

### 5.4 Long-Context Acceptance Levels

| Level | Target prompt size | Purpose |
| --- | ---: | --- |
| Small | 2K-8K | fast behavior and cache regression |
| Medium | approximately 100K | application context |
| Large | approximately 300K | sustained business workflow |
| Very large | approximately 700K | long-context stability |
| Compact boundary | 840K-870K | automatic compact transition |
| Effective boundary | below 950K | rejection and headroom validation |

Normal CI uses deterministic fixtures and mock endpoints. Paid DeepSeek tests are explicit and may
use fewer repetitions when cost would otherwise be excessive.

## 6. Context And Cache Invariants

Runtime 0.0 requires:

1. Conversation history grows append-only within one context segment.
2. System instructions remain stable unless the application explicitly changes them.
3. Tool schemas remain stable within a turn unless dynamic tool configuration changes.
4. Context changes are appended as bounded fragments.
5. No individual injected context fragment may exceed 10K tokens.
6. Context fragments that may exceed 1K tokens require a hard cap and manual review.
7. Compaction is the only expected history-rewrite boundary during ordinary operation.
8. Resume reconstructs the same logical history and context segment.
9. Cache is measured separately for warm-up, steady state, and post-compaction requests.
10. A post-compaction miss is excluded from ordinary steady-state cache aggregation.

The initial DeepSeek warm-cache baseline is:

    normal harness steady-state hit rate: approximately 98%
    system changed: false
    tools changed: false
    history rewritten: false

This is a regression baseline, not a permanent provider guarantee. Runtime code is investigated when
the same workload repeatedly falls below 90% or layout fingerprints show unexpected instability.

## 7. Runtime Capability Contract

### 7.1 Required Capabilities

Runtime 0.0 keeps these paths working:

- app-server initialize and initialized handshake;
- thread start, read, list, and resume;
- turn start, streaming events, cancellation, and completion;
- dynamic tool registration;
- built-in tool routing;
- local skill discovery and bounded skill instructions;
- local plugin discovery and enabled-state handling;
- MCP stdio/HTTP connection and tool invocation;
- automatic and explicit compaction;
- rollout and state persistence;
- token/cache usage notifications;
- clean and bounded shutdown.

### 7.2 Non-Goals

Runtime 0.0 does not require:

- compile-time capability profiles;
- minimal dependency closure;
- enterprise quota enforcement;
- process-internal multi-tenancy;
- additional model providers;
- TUI completeness;
- realtime audio;
- V8/code mode;
- remote plugin catalogs or sharing;
- ChatGPT accounts or OpenAI product backends;
- Bazel release parity;
- SDK regeneration without a protocol change.

Existing non-goal modules become blockers only when they are reached by the supported path, perform
unexpected product-network requests, change supported app-server behavior, materially destabilize
build/startup, expose credentials, or cross application boundaries.

## 8. Error And Retry Contract

The supported DeepSeek path must classify authentication errors, rate limits, provider 5xx,
transport failures, malformed SSE, stream idle timeout, context overflow, invalid tool calls, tool
failure, cancellation, and compaction failure.

Retries are bounded. A retry must not duplicate completed tool side effects. If a stream fails after
a tool call is surfaced, continuation must use persisted call identity rather than blindly replaying
the business action.

App-server errors must be structured and must exclude API keys, authorization headers, raw tool
secrets, and full private prompts.

## 9. Persistence And Resume Contract

A persisted thread retains:

- model slug and provider identity;
- logical conversation items;
- reasoning required for DeepSeek tool continuation;
- tool call IDs and results;
- context segment metadata;
- compacted summary and replacement-history boundary;
- application instructions and dynamic tool metadata needed for safe resume;
- token usage needed for context accounting.

Acceptance sequence:

1. complete at least two turns;
2. execute at least one tool call;
3. persist and stop the worker;
4. start a new worker;
5. resume the same thread;
6. submit another turn;
7. verify the outgoing DeepSeek history remains valid and the response completes.

Resume must not duplicate a tool side effect or inject the same context update twice.

## 10. Application Integration Contract

Applications provide an isolated runtime home, workspace roots, DeepSeek credential, fixed model
selection, application instructions, tool implementations, approval policy, plugin/skill/MCP
configuration, and worker supervision.

The runtime provides stable thread/turn lifecycle, streamed domain-neutral events, tool invocation
envelopes, cancellation, persisted resume, context/usage reporting, and overload errors.

Runtime 0.0 does not define business object schemas. Applications translate Agent messages and tool
calls into their own domain models.

## 11. Verification Matrix

### 11.1 Offline Required Tests

Every supported-path change keeps these deterministic tests green:

- Chat Completions text response;
- reasoning plus text;
- reasoning plus tool call;
- streamed tool argument accumulation;
- sequential tool loop;
- multiple tool calls;
- usage and cache parsing;
- request-prefix stability;
- cancellation and stream failure;
- context accounting;
- automatic compact boundary;
- compact then continue;
- persist/resume then continue;
- app-server thread/turn lifecycle;
- local skill/plugin/MCP smoke;
- in-process overload and bounded shutdown.

### 11.2 Paid DeepSeek Tests

The existing cache test remains:

    DEEPSEEK_MODEL=deepseek-v4-flash just test-deepseek-cache

The 0.0 release suite additionally contains explicit live scenarios for text, reasoning, tool call
and continuation, cancellation, persisted resume, and one selected long-context checkpoint.

Paid tests do not run in default CI. Sanitized summaries are attached to the release record.

### 11.3 Domain-Neutral Reference Smokes

Use at least two fixtures so the runtime is not optimized for one application:

1. Document workflow: provide a deterministic document, ask several questions, invoke one source
   tool, persist/resume, and continue.
2. Business operation workflow: provide structured state, call a read tool, call one approved write
   tool, receive final text, and verify the write is not duplicated after retry or resume.

These are runtime reference tests, not bundled products.

## 12. Release Artifacts

Runtime 0.0 produces:

- existing codex binary with exec and app-server launch;
- existing codex-app-server binary when distributed separately;
- existing in-process Rust API;
- recommended DeepSeek configuration;
- supported capability and limitation document;
- sanitized live-test results;
- known-issues list;
- source commit and release tag.

No binary-size target is imposed for 0.0. Binary size, startup time, and idle memory are recorded for
future comparison but do not trigger feature-gating work in this release.

## 13. Release Gate

Runtime 0.0 is releasable when:

    [ ] deepseek-v4-flash is the configured and documented release model.
    [ ] Raw/effective/compact limits are 1,000,000 / 950,000 / 850,000.
    [ ] Text, reasoning, tool, usage, and cache paths pass offline tests.
    [ ] DeepSeek text, reasoning, and tool live smokes pass.
    [ ] Cache steady-state behavior remains healthy on the reference workload.
    [ ] 100K, 300K, and 700K checkpoints have evidence or a documented cost exception.
    [ ] The 850K compact transition is tested deterministically.
    [ ] Compact-then-continue passes.
    [ ] Persist-stop-resume-continue passes.
    [ ] app-server stdio and in-process smokes pass.
    [ ] No supported flow contacts ChatGPT/OpenAI product services implicitly.
    [ ] Secrets and private prompt/tool content do not appear in reports.
    [ ] Known limitations are documented.
    [ ] The release commit is tagged.

## 14. Change Policy After 0.0

Until Runtime 0.1 planning begins:

- accept fixes for DeepSeek, context correctness, tools, compaction, persistence, app-server
  lifecycle, security, and data loss;
- accept upstream fixes only when they directly protect those paths;
- do not merge upstream wholesale;
- do not add provider-specific branches for other vendors;
- do not begin feature-gated dependency cutting;
- do not add enterprise multi-tenancy;
- do not change app-server protocol without an application requirement;
- require a Runtime 0.0 regression test for every supported-path behavior change.

The next version starts from measured application needs, not unused upstream capability.

