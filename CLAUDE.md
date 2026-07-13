# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

This is **OpenAI Codex** — a coding agent that runs locally. The repo contains the Rust workspace (`codex-rs/`), the CLI entry point (`codex-cli/`), TypeScript/Python SDKs (`sdk/`), and Bazel-based build infrastructure. The main binary is `codex` (Rust), which provides both an interactive TUI and a non-interactive `codex exec` mode, plus an app-server daemon that powers IDE integrations and SDKs.

## Build, test, and lint

All commands assume `cd codex-rs` first (the justfile sets `working-directory := "codex-rs"`).

### Build and run

```sh
cargo build                          # build everything
cargo run --bin codex                # run the interactive TUI
cargo run --bin codex -- exec "..."  # run non-interactively
```

### Formatting and linting

```sh
just fmt                             # cargo fmt + Python SDK ruff
just fix -p <crate>                  # clippy --fix, scoped to one crate
just fix                             # workspace-wide clippy --fix (slow; only for shared-crate changes)
just clippy -p <crate>              # clippy checks, scoped to one crate
just argument-comment-lint           # run argument-comment-lint via Bazel
```

### Running tests

```sh
cargo test -p codex-tui              # tests for a specific crate
just test                            # full suite via cargo-nextest (if installed)
cargo test                           # complete cargo test suite
# Avoid --all-features for routine runs; it expands the build matrix and target/ disk usage.
```

### Snapshot tests (insta)

Used heavily in `codex-tui` for UI output validation. After UI changes:

```sh
cargo test -p codex-tui
cargo insta pending-snapshots -p codex-tui
cargo insta show -p codex-tui path/to/file.snap.new
cargo insta accept -p codex-tui   # only if you intend to accept ALL new snapshots
```

### Bazel (CI and release builds)

```sh
just bazel-codex <args>              # build and run via Bazel
just bazel-test                       # run all Bazel tests (excluding argument-comment-lint)
just bazel-clippy                     # clippy via Bazel
just bazel-lock-check                 # check MODULE.bazel.lock drift (CI gate)
just build-for-release                # release binaries via remote execution
```

After changing Cargo.toml/Cargo.lock: run `just bazel-lock-update` and include the lockfile update.

### Schemas and codegen

```sh
just write-config-schema                      # update codex-rs/core/config.schema.json
just write-app-server-schema                  # regenerate app-server protocol fixtures
just write-app-server-schema --experimental   # include experimental API fixtures
just write-hooks-schema                       # hooks schema fixtures
```

## High-level architecture

### Directory layout

| Directory | Purpose |
|---|---|
| `codex-rs/` | Rust workspace (~80+ crates). The heart of the codebase. |
| `codex-cli/` | TypeScript CLI wrapper installed by end users (`@openai/codex`). Downloads and spawns the Rust `codex` binary. |
| `sdk/python/` | Experimental Python SDK wrapping the app-server via JSON-RPC v2 over stdio. |
| `sdk/python-runtime/` | Python runtime for app-server sandbox execution. |
| `sdk/typescript/` | TypeScript SDK (`@openai/codex-sdk`) wrapping the CLI via JSONL over stdin/stdout. |
| `docs/` | User-facing docs (install, config, execpolicy, skills, etc.). Not for general architectural docs. |
| `tools/` | Custom tooling including `argument-comment-lint` (Dylint-based). |
| `patches/` | Patches for Bazel module dependencies. |
| `third_party/v8/` | V8 engine for sandboxed JS execution (code-mode). |

### Key crate categories

**Entry points / binaries:**
- `cli` (`codex-cli`) — the `codex` binary (TUI + non-interactive exec)
- `tui` (`codex-tui`) — the interactive terminal UI using ratatui
- `exec-server` (`codex-exec-server`) — long-running daemon for sandboxed command execution
- `app-server` (`codex-app-server`) — JSON-RPC v2 server for IDE integrations and SDKs
- `app-server-daemon` — long-running app-server process management
- `mcp-server` (`codex-mcp-server`) — MCP (Model Context Protocol) server

**Core / shared:**
- `core` (`codex-core`) — **largest crate**; the central hub for agent logic. Resists adding new code here; prefer extracting to purpose-built crates.
- `core-api` (`codex-core-api`) — public API surface for core
- `protocol` (`codex-protocol`) — core protocol types (API messages, tool definitions)
- `config` (`codex-config`) — configuration loading, `ConfigToml` parsing
- `backend-client` — HTTP client for OpenAI API communication
- `codex-client` (`codex-client`) — typed client for the Responses API
- `codex-api` (`codex-api`) — lower-level API bindings

**App-server protocol:**
- `app-server-protocol` — types and wire format for the app-server JSON-RPC v2 API
- `app-server-transport` — transport layer (stdio, UDS)
- `app-server-client` — typed client for the app-server API
- `app-server-test-client` — integration test client for app-server

**Agent / execution:**
- `tools` — built-in tool implementations (shell, file ops, etc.)
- `exec` — command execution primitives
- `sandboxing` — sandbox abstraction layer
- `linux-sandbox` — Linux sandbox (bubblewrap + Landlock + seccomp)
- `execpolicy` — execution policy engine for sandboxed commands
- `code-mode` — JS code execution via V8
- `apply-patch` — diff/patch utilities
- `file-search` — file system search (ripgrep-based)
- `file-watcher` — filesystem event notifications
- `shell-escalation` — shell privilege escalation handling
- `process-hardening` — process security hardening

**MCP / integration:**
- `codex-mcp` (`codex-mcp`) — MCP client connection management; prefer `mcp_connection_manager.rs` for tool call mutations
- `rmcp-client` — RMCP client for real-time MCP
- `connectors` — external service connectors
- `plugin` — plugin system for extensions
- `core-plugins` — core plugin implementations
- `ext/extension-api` — extension API types
- `ext/guardian` — guardian extension for policy enforcement
- `ext/memories` — memories extension

**Persistence / state:**
- `state` (`codex-state`) — SQLite-backed state management (thread store, messages, config)
- `thread-store` — conversation thread persistence
- `agent-graph-store` — agent graph representation storage
- `message-history` — message history storage

**Infrastructure / utilities:**
- `utils/` — many small utility crates (absolute-path, cargo-bin, cache, home-dir, pty, etc.)
- `async-utils` — async helpers and primitives
- `ansi-escape` — ANSI escape sequence parsing
- `otel` — OpenTelemetry integration
- `analytics` — usage analytics
- `rollout` / `rollout-trace` — feature rollout and tracing
- `login` — authentication flow (ChatGPT / API key)
- `secrets` — secrets management (keyring integration)
- `features` — feature flag system

### App-server v2 protocol

New API surface must go into v2 (not v1). Key rules:
- Payload naming: `*Params` (request), `*Response` (response), `*Notification` (notification)
- RPC method pattern: `<resource>/<method>` with singular resource names
- Wire format: camelCase via `#[serde(rename_all = "camelCase")]` + matching `#[ts(rename = "...")]`
- **Exception**: config RPC payloads use snake_case to mirror config.toml keys
- Optionals in `*Params` must use `#[ts(optional = nullable)]`; do NOT use this on responses
- Timestamps: `i64` Unix seconds, named `*_at`
- Pagination: cursor-based by default (`cursor: Option<String>`, `limit: Option<u32>` → `next_cursor: Option<String>`)
- Validate with `cargo test -p codex-app-server-protocol`

### Build system: dual Cargo + Bazel

The repo builds both via Cargo (local dev) and Bazel (CI, remote execution, release). Important differences:
- Bazel does not auto-expose source-tree files to Rust; if you add `include_str!`, `sqlx::migrate!`, etc., update the crate's `BUILD.bazel` (`compile_data`, `build_script_data`, or test data).
- Tests: use `codex_utils_cargo_bin::cargo_bin("...")` and `codex_utils_cargo_bin::find_resource!` instead of `assert_cmd::Command::cargo_bin` or `env!("CARGO_MANIFEST_DIR")` for Bazel compatibility.

## Rust code conventions

See **AGENTS.md** (`codex-rs/` Rust section) for the full convention reference. Key highlights:

- Crate naming: `codex-<name>` prefix (exception: the embedded SDK crates `omnix-sdk` and `omnix-runtime` use the `omnix-` prefix; business apps depend only on `omnix-sdk`)
- Edition: 2024 (workspace-wide default)
- Clippy is strict (many `deny` lints); run `just fix -p <crate>` before finalizing
- Collapse if statements, inline format! args, use method references over closures
- Avoid `#[async_trait]` — prefer native RPITIT with explicit `Send` bounds
- Resist adding code to `codex-core`; extract to new/existing crates instead
- Keep modules under ~500 LoC, extract at ~800 LoC
- Avoid large orchestration files (`chatwidget.rs`, `app.rs`, etc.); prefer new modules
- Sandbox env vars: never add/modify `CODEX_SANDBOX_NETWORK_DISABLED` or `CODEX_SANDBOX` logic

### TUI conventions

- Styling: use ratatui's `Stylize` trait (`"text".dim()`, `.cyan()`, `.red()`, `.green()`, `.magenta()`, `.bold()`)
- Text wrapping: use `textwrap::wrap` for plain strings, `tui/src/wrapping.rs` helpers for ratatui lines
- See `codex-rs/tui/styles.md` for full color/header conventions
- Any user-visible UI change must include `insta` snapshot coverage
