# General Agent Runtime Technical Design

## 1. Objective

Build a provider-neutral general Agent Runtime that can be embedded into different business
applications. The first usable release is a process-based worker, not an FFI library:

- `codex exec` remains a lightweight smoke/debug entry point.
- `codex app-server` provides JSON-RPC over stdio for desktop applications.
- The same app-server protocol can be hosted behind an enterprise gateway for multiple users.
- Tool execution, skills, local plugins, MCP, compaction, session persistence, and streaming events
  are the core product capabilities.
- Coding, realtime, image, migration, analytics, TUI, V8, Bazel, and other domain-specific features
  must not be compiled unless explicitly requested.

The app-server protocol remains compatible enough for current clients. Product-specific legacy
methods may remain in the schema but must be inert when their implementation is not compiled.

## 2. Architecture

```text
Desktop Application                     Enterprise Gateway
        | JSON-RPC / stdio                       | worker transport
        +-------------------+--------------------+
                            |
                    codex-app-server
                            |
       +--------------------+--------------------+
       |                    |                    |
  Agent Kernel       Capability Modules    Host Services
  turn loop          tools / skills        config / secrets
  model stream       plugins / MCP         session storage
  context            filesystem            tenant isolation
  compact            shell / git
  events
```

### Process Boundary

The first release uses a child process and JSON-RPC instead of Rust FFI. This preserves crash
isolation, avoids ABI and ownership problems, works across desktop languages, and is easier to
inspect with logs and captured protocol messages. FFI is deferred until measurements show process
communication is a material bottleneck.

### Compatibility Boundary

`codex-app-server-protocol` remains the wire contract. Existing ChatGPT, remote-plugin, and other
legacy types may remain for compatibility, but the minimal worker does not compile their product
clients. Calls to unavailable methods return a stable `unsupported` error.

## 3. Crate Classification

### Tier A: Agent Kernel, Always Compiled

These crates form the smallest useful general agent:

| Crate | Responsibility |
|---|---|
| `codex-protocol` | Internal event, item, operation, and model-neutral types |
| `codex-api` | Responses and Chat Completions HTTP/SSE adaptation |
| `codex-client` | HTTP client primitives |
| `codex-model-provider-info` | Provider configuration and wire API selection |
| `codex-model-provider` | Provider request execution |
| `codex-models-manager` | Model metadata and selection |
| `codex-config` | Runtime configuration layering |
| `codex-core` | Current agent turn loop and orchestration implementation |
| `codex-context-fragments` | Bounded model-visible context fragments |
| `codex-prompts` | Prompt assembly primitives |
| `codex-tools` | Tool definitions and dispatch contracts |
| `codex-state` | Runtime state primitives |
| `codex-rollout` | Session event persistence |
| `codex-thread-store` | Thread/session storage abstraction |
| `codex-login` | API-key/external bearer credential loading only |

`codex-core` remains the runtime implementation for the first usable release. No new facade or
worker crate is introduced during the cutting phase. Business applications communicate through
`codex-app-server`; a stable runtime facade may be extracted only after the retained dependency
graph and behavior are verified.

### Tier B: Default General-Agent Capabilities

These are enabled in the recommended worker profile:

| Feature | Crates |
|---|---|
| `skills` | `codex-skills`, `codex-core-skills`, `codex-skills-extension` |
| `plugins` | `codex-plugin`, `codex-core-plugins`, `codex-utils-plugins` |
| `mcp` | `codex-mcp`, `codex-rmcp-client`, `codex-mcp-extension` |
| `sessions` | `codex-rollout`, `codex-state`, `codex-thread-store` |
| `compact` | Existing compact implementation inside `codex-core` |
| `hooks` | `codex-hooks` |

### Tier C: Optional Business Capabilities

These must be Cargo features and optional dependencies:

| Feature | Crates | Default |
|---|---|---|
| `filesystem` | `codex-file-system`, `codex-file-search`, `codex-file-watcher` | desktop profile |
| `shell` | `codex-exec-server`, `codex-shell-command`, `codex-execpolicy` | off |
| `patch` | `codex-apply-patch` | off |
| `git` | `codex-git-utils` | off |
| `sandbox` | `codex-sandboxing`, platform sandbox crates | off |
| `connectors` | `codex-connectors`, `codex-connectors-extension` | off |
| `memory` | `codex-memories-read`, `codex-memories-write`, extension | off |
| `web-search` | `codex-web-search-extension` | off |
| `image-generation` | `codex-image-generation-extension` | off |
| `guardian` | `codex-guardian` | off |
| `goal` | `codex-goal-extension`, `codex-agent-graph-store` | off |
| `telemetry` | `codex-analytics`, `codex-otel`, `codex-feedback` | off |

### Tier D: Excluded From The First Product Build

These may remain in the repository but are not dependencies or default workspace build members:

- `codex-tui`
- `codex-v8-poc`
- `codex-realtime-webrtc`
- `codex-code-mode-host`
- `codex-external-agent-migration`
- `codex-external-agent-sessions`
- `codex-app-server-test-client`
- `codex-thread-manager-sample`
- `codex-lmstudio`
- `codex-ollama`
- Bazel targets and lock generation

Bazel remains an optional upstream-maintenance path. The Staffroom release gate is Cargo-only.

## 4. Compile Profiles

Cargo features are additive, so the minimal profile must use `default = []`. Named bundles are
convenience aliases, not mutually exclusive modes.

### Existing Entry Crates

- `codex-app-server` is the production process entry point and desktop/enterprise protocol host.
- `codex-cli` remains the debug entry point; its default build is reduced to `exec` and app-server
  launch support.
- `codex-core` remains the internal agent implementation.
- No new runtime, worker, SDK, or FFI crate is added until cutting and verification are complete.

### Feature Bundles

The same leaf features are propagated through `codex-core`, `codex-app-server`, and `codex-cli`.
The app-server owns the named bundles:

```toml
[features]
default = ["profile-general"]

skills = ["codex-core/skills"]
plugins = ["codex-core/plugins"]
mcp = ["codex-core/mcp"]
hooks = ["codex-core/hooks"]
filesystem = ["codex-core/filesystem"]
shell = ["codex-core/shell"]
patch = ["codex-core/patch"]
git = ["codex-core/git"]
sandbox = ["codex-core/sandbox"]
connectors = ["codex-core/connectors"]
memory = ["codex-core/memory"]
web-search = ["codex-core/web-search"]
image-generation = ["codex-core/image-generation"]
guardian = ["codex-core/guardian"]
goal = ["codex-core/goal"]
telemetry = ["codex-core/telemetry"]

profile-minimal = []
profile-general = ["skills", "plugins", "mcp", "hooks"]
profile-desktop = ["profile-general", "filesystem"]
profile-automation = ["profile-desktop", "shell", "patch", "git", "sandbox"]
profile-full = [
  "profile-automation",
  "connectors",
  "memory",
  "web-search",
  "image-generation",
  "guardian",
  "goal",
  "telemetry",
]
```

### Build Commands

```bash
# Smallest provider + turn-loop app-server
cargo build -p codex-app-server --no-default-features --features profile-minimal

# Recommended general business agent
cargo build -p codex-app-server --no-default-features --features profile-general

# Desktop knowledge application
cargo build -p codex-app-server --no-default-features --features profile-desktop

# Coding/automation worker
cargo build -p codex-app-server --no-default-features --features profile-automation

# Explicit custom combination
cargo build -p codex-app-server --no-default-features \
  --features skills,plugins,mcp,filesystem,memory
```

The root workspace sets:

```toml
[workspace]
default-members = [
  "app-server",
  "app-server-protocol",
  "cli",
]
```

Therefore plain `cargo build` no longer compiles TUI, V8, samples, test clients, or optional
extensions.

## 5. Required Dependency Refactoring

Adding features only at the app-server is insufficient because `codex-core` currently declares
many hard dependencies. Feature propagation must continue through the existing layers:

```text
codex-cli/app-server feature
  -> codex-core feature
     -> optional capability dependency
```

The same rule applies to app-server handlers. A handler for an optional capability is compiled
under `#[cfg(feature = "...")]`; when absent, protocol dispatch returns `unsupported` without
linking the capability crate.

The first refactoring target is `codex-core/Cargo.toml`:

- keep model, context, compact, event, session, and generic tool contracts mandatory;
- mark plugins, skills, MCP, file, shell, Git, memory, image, web, guardian, goal, telemetry, and
  coding dependencies optional;
- place extension registration in feature-scoped modules;
- avoid `cfg(any())` placeholders.

## 6. Runtime Behavior

### Minimal Profile

The minimal worker supports:

- API-key or externally supplied bearer credentials;
- Responses and Chat Completions providers;
- streaming model output;
- agent turn loop;
- host-registered in-process or protocol tools;
- context compaction;
- thread start, read, resume, and persisted rollout;
- app-server JSON-RPC events.

It does not automatically access ChatGPT, OpenAI product backends, curated plugin repositories,
cloud configuration, analytics, or remote plugin services.

### Capability Discovery

At startup the worker reports compiled capabilities in the initialize response. Runtime config may
disable a compiled capability, but it cannot enable one that was not compiled. Calls to missing
capabilities return:

```json
{
  "code": -32601,
  "message": "capability 'image-generation' is not compiled into this worker"
}
```

This makes build composition observable to desktop and enterprise hosts.

## 7. Enterprise Hosting

The first enterprise deployment uses process isolation:

- one worker per user session or tenant execution slot;
- gateway owns authentication, tenant routing, quotas, and worker lifecycle;
- worker receives an isolated home directory, workspace roots, provider credentials, and feature
  policy;
- no global mutable authentication or plugin state is shared between tenants;
- protocol logs include tenant-safe correlation IDs but never credentials.

Multi-tenant state inside a single Rust process is deferred. Process isolation is simpler and safer
for the first usable service.

## 8. Verification Strategy

### Layer 1: Compile Matrix

Every supported app-server profile must compile independently:

```bash
cargo check -p codex-app-server --no-default-features --features profile-minimal
cargo check -p codex-app-server --no-default-features --features profile-general
cargo check -p codex-app-server --no-default-features --features profile-desktop
cargo check -p codex-app-server --no-default-features --features profile-automation
```

CI also checks one custom combination to prevent bundle-only coupling:

```bash
cargo check -p codex-app-server --no-default-features \
  --features skills,plugins,mcp,filesystem,memory
```

### Layer 2: CLI Smoke

The existing command remains the fastest model/turn-loop check:

```bash
./target/debug/codex exec "say hi"
```

Expected:

- configured non-OpenAI provider is selected;
- one model turn completes;
- output is emitted;
- process exits with status 0;
- no ChatGPT/OpenAI product endpoint is contacted implicitly.

### Layer 3: Agent Capability Smokes

The general profile adds deterministic fixtures:

1. Register a local test tool and verify tool call/result round-trip.
2. Load a test skill and verify its bounded instructions reach the model request.
3. Load a local plugin and verify its tool/skill capability becomes available.
4. Connect a local MCP fixture and invoke one tool.
5. Force context compaction and verify the next turn succeeds.
6. Persist, stop, resume, and continue a thread.

These tests use mock model endpoints and do not require internet access.

### Layer 4: App-Server End-to-End Smoke

A small protocol driver performs:

1. `initialize` and compiled-capability inspection;
2. `thread/start`;
3. `turn/start`;
4. streaming item and turn notifications;
5. one tool request/result cycle;
6. `thread/read`;
7. process restart and `thread/resume`;
8. clean shutdown.

The same script runs against stdio locally and against the enterprise worker transport in CI.

### Layer 5: Product-Network Denial Test

Run the minimal and general workers with all outbound requests routed through a recording deny
proxy. Allow only the configured mock model endpoint. The test fails on requests to ChatGPT auth,
OpenAI backend, workspace settings, remote plugins, curated repositories, cloud config, telemetry,
or feedback endpoints.

### Layer 6: Artifact Measurements

CI records but initially does not hard-fail on:

- clean build wall-clock time;
- incremental build time after changing `codex-core` code;
- app-server and CLI binary size;
- number of compiled crates via Cargo build timings;
- startup time to `initialize`;
- idle resident memory.

After two stable releases, budgets are set from observed baselines. The target is that
`profile-general` compiles materially fewer crates and produces a smaller artifact than the current
unconditional app-server build.

## 9. Delivery Phases

### Phase 1: Crate-Level Cutting

- Keep the existing `codex-core`, `codex-app-server`, and `codex-cli` entry path.
- Remove dependencies outside the first general-agent release.
- Set Cargo `default-members` to app-server, protocol, and CLI.
- Remove TUI, V8, realtime, migration, samples, and Bazel from release build commands.
- Preserve a continuously working `codex exec "say hi"` smoke.

### Phase 2: Feature-Gated Dependency Reduction

- Convert core and app-server capability dependencies to optional dependencies.
- Add feature-scoped registration and unavailable handlers.
- Establish compile-matrix CI and artifact measurements.
- Enable minimal, general, desktop, automation, and custom feature combinations.

### Phase 3: Verification And Usable Release

- Pass tool, skill, plugin, MCP, compact, session, and app-server end-to-end smokes.
- Verify product-network denial and artifact measurements.
- Publish the existing app-server process contract used by desktop and enterprise hosts.

### Phase 4: Optional Boundary Extraction

- Evaluate the stable retained dependency graph.
- Extract a runtime facade only if multiple Rust hosts need a direct library API.
- Extract a dedicated worker only if app-server still carries substantial non-runtime behavior.
- Consider FFI only after profiling demonstrates a process-communication bottleneck.

## 10. Acceptance Criteria For The First Usable Release

- Cargo builds the app-server without TUI, V8, realtime, migration, sample, or Bazel work.
- DeepSeek/Qwen or another configured OpenAI-compatible provider completes `say hi`.
- Tool, skill, local plugin, MCP, compact, persist, and resume smokes pass.
- Desktop client can communicate with app-server over stdio.
- Legacy unsupported product RPCs fail explicitly without product network calls.
- A clean `profile-general` build and its artifact measurements are recorded.
- The release process does not require a workspace-wide test or build.
