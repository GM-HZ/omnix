# Detach ChatGPT/OpenAI Bindings Implementation Plan v2

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trim this Codex fork into a provider-neutral local harness while preserving Codex session orchestration, tool execution, sandboxing, patch application, MCP, and thread history.

**Architecture:** Execute the refactor in discovery-gated, compile-preserving phases. Remove product entry points first, neutralize auth/provider defaults behind compatibility facades, then clean protocol/schema/UI/dead code with tests at each phase.

**Tech Stack:** Rust workspace under `codex-rs`, Cargo, Bazel lockfile at repo root, `just` tasks from repo root, app-server v2 schema generation, TUI `insta` snapshots.

---

## Scope And Fixed Decisions

This v2 plan replaces the previous plan. The previous plan is obsolete.

Fixed decisions for this pass:

- Keep OpenAI-compatible Responses wire shape as a generic model-adapter protocol.
- Keep the `codex-login` crate name, `AuthManager` facade, and `CodexAuth` compatibility shell for this pass.
- Keep `protocol/src/openai_models.rs` as a file name for this pass; add a neutral `model_catalog` re-export instead of renaming 600+ imports immediately.
- Keep `cloud-requirements`, `CloudRequirementsLoader`, and config layering types for this pass; change remote/cloud loading to an empty managed-config loader.
- Remove ChatGPT/OpenAI product login, cloud task UI, remote ChatGPT marketplace/share/sync, first-party backend calls, GPT default models, OpenAI default provider, and Bedrock default provider.
- Default provider id is `local`.
- Default provider name is `Local Model Provider`.
- Default provider base URL is `http://localhost:11434/v1`.
- Default model is `local-default`.
- `MODULE.bazel.lock` is `/Users/gongmeng/dev/code/omnix/MODULE.bazel.lock`.
- Run `just` commands from `/Users/gongmeng/dev/code/omnix`, not from `codex-rs`.
- Avoid broad dependency refreshes; use normal `cargo check`/`cargo test` to drive minimal lockfile changes.

## File Structure

Implementation owners:

- Workspace graph:
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/Cargo.toml`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/Cargo.lock`
  - Modify: `/Users/gongmeng/dev/code/omnix/MODULE.bazel.lock`
- Product crate removals:
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/chatgpt`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/responses-api-proxy`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/backend-client`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/cloud-tasks`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/cloud-tasks-client`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/cloud-tasks-mock-client`
- Auth:
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/lib.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/auth/mod.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/auth/manager.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/auth/storage.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/auth/default_client.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/auth_env_telemetry.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/device_code_auth.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/pkce.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/server.rs`
- Provider/model catalog:
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider-info/src/lib.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider-info/src/model_provider_info_tests.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider/src/auth.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider/src/bearer_auth_provider.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider/src/provider.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider/src/models_endpoint.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider/src/amazon_bedrock`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/models-manager/src/manager.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/models-manager/src/manager_tests.rs`
- Protocol/config/schema:
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/protocol/src/lib.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/protocol/src/account.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/protocol/src/auth.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/protocol/src/error.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server-protocol/src/protocol/common.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server-protocol/src/protocol/v2/account.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server-protocol/src/protocol/v2/config.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/config/src/config_toml.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/config/src/profile_toml.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/config/mod.rs`
  - Modify generated schema under `/Users/gongmeng/dev/code/omnix/codex-rs/app-server-protocol/schema`
  - Modify generated config schema `/Users/gongmeng/dev/code/omnix/codex-rs/core/config.schema.json`
- Cloud requirements:
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/cloud-requirements/src/lib.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/config_manager.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/lib.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/debug_config.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/bottom_pane/hooks_browser_view.rs`
- Entry points and runtime:
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/cli/src/main.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/cli/src/login.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/cli/src/doctor.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/exec/src/lib.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/client.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/connectors.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/session/mod.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/session/mcp.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/message_processor.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/request_processors.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/request_processors/account_processor.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/request_processors/catalog_processor.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/request_processors/apps_processor.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/request_processors/plugins.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/src/analytics_utils.rs`
- TUI/plugins/skills:
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/lib.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/chatwidget.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/chatwidget/model_popups.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/chatwidget/plugins.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/chatwidget/rate_limits.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/bottom_pane/app_link_view.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/onboarding/mod.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/local_chatgpt_auth.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/onboarding/auth`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/core-plugins/src/remote.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/core-plugins/src/remote/remote_installed_plugin_sync.rs`
  - Delete: `/Users/gongmeng/dev/code/omnix/codex-rs/core-plugins/src/remote/share.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/core-skills/src/loader.rs`
  - Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/utils/plugins/src/mcp_connector.rs`
- SDK:
  - Inspect and update generated-type consumers under `/Users/gongmeng/dev/code/omnix/sdk`

---

### Task 1: Baseline Discovery And Branch

**Files:**
- No source edits.

- [ ] **Step 1: Create the execution branch**

Run:

```bash
git switch -c codex/detach-chatgpt-openai-bindings
```

Expected: branch switches successfully.

- [ ] **Step 2: Confirm current status**

Run:

```bash
git status --short
```

Expected: only the plan document is uncommitted, or a clean worktree if the plan was already committed.

- [ ] **Step 3: Capture product-binding references**

Run:

```bash
rg -n "codex-chatgpt|codex_cloud_tasks|codex-cloud-tasks|codex_backend_client|codex-backend-client|codex_responses_api_proxy|codex-responses-api-proxy|ChatGPT|chatgpt|api.openai.com|OPENAI_API_KEY|gpt-" codex-rs sdk --glob '!target'
```

Expected: many references. Save this command output in terminal scrollback for comparison; do not write a file.

- [ ] **Step 4: Capture compile baseline**

Run:

```bash
cargo check -p codex-cli
cargo check -p codex-app-server
cargo check -p codex-tui
```

Expected: PASS. Toolchain download failures are environment setup failures and must be resolved before code edits.

- [ ] **Step 5: Commit the plan document**

Run:

```bash
git add docs/superpowers/plans/2026-05-16-detach-chatgpt-openai-bindings.md
git commit -m "docs: plan ChatGPT OpenAI binding removal"
```

Expected: commit succeeds.

---

### Task 2: Remove Product Entry Crates Without Leaving Broken Imports

**Files:**
- Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/Cargo.toml`
- Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/cli/Cargo.toml`
- Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/Cargo.toml`
- Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/tui/Cargo.toml`
- Delete: product crate directories listed in File Structure.

- [ ] **Step 1: Remove CLI imports and subcommands**

In `/Users/gongmeng/dev/code/omnix/codex-rs/cli/src/main.rs`, remove imports for:

```rust
use codex_chatgpt::apply_command::ApplyCommand;
use codex_chatgpt::apply_command::run_apply_command;
use codex_cloud_tasks::Cli as CloudTasksCli;
use codex_responses_api_proxy::Args as ResponsesApiProxyArgs;
```

Remove enum variants and match arms for ChatGPT apply, cloud tasks, and responses API proxy. Keep local `exec`, `tui`, `mcp-server`, `app-server`, config, sandbox, and debug subcommands.

- [ ] **Step 2: Remove app-server first-party imports**

In app-server request processors, remove imports and usage of:

```rust
use codex_backend_client::Client as BackendClient;
use codex_chatgpt::connectors;
use codex_chatgpt::workspace_settings;
```

Replace workspace-settings gates with local config gates that default to enabled for local plugin/MCP configuration. Remove BackendClient account fetch calls instead of replacing them with network calls.

- [ ] **Step 3: Remove TUI ChatGPT imports**

Remove `codex_chatgpt` imports from:

- `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/chatwidget.rs`
- `/Users/gongmeng/dev/code/omnix/codex-rs/tui/src/lib.rs`
- onboarding auth files

Do not remove `codex-cloud-requirements` in this task.

- [ ] **Step 4: Remove manifest dependencies**

Remove product dependencies from:

- `/Users/gongmeng/dev/code/omnix/codex-rs/cli/Cargo.toml`
- `/Users/gongmeng/dev/code/omnix/codex-rs/app-server/Cargo.toml`
- `/Users/gongmeng/dev/code/omnix/codex-rs/tui/Cargo.toml`

Remove workspace members and workspace dependency aliases for:

```toml
backend-client
chatgpt
cloud-tasks
cloud-tasks-client
cloud-tasks-mock-client
responses-api-proxy
```

- [ ] **Step 5: Delete product crate directories**

Run:

```bash
git rm -r codex-rs/chatgpt codex-rs/responses-api-proxy codex-rs/backend-client codex-rs/cloud-tasks codex-rs/cloud-tasks-client codex-rs/cloud-tasks-mock-client
```

Expected: directories are staged for deletion.

- [ ] **Step 6: Verify compile**

Run:

```bash
cargo check -p codex-cli
cargo check -p codex-app-server
cargo check -p codex-tui
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add codex-rs/Cargo.toml codex-rs/cli codex-rs/app-server codex-rs/tui codex-rs/Cargo.lock
git commit -m "refactor: remove ChatGPT product entry crates"
```

Expected: commit succeeds.

---

### Task 3: Make Auth Provider-Neutral Behind Existing Facades

**Files:**
- Modify: login auth files listed in File Structure.
- Delete: `device_code_auth.rs`, `pkce.rs`, `server.rs`.

- [ ] **Step 1: Reduce `CodexAuth` variants**

In `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/auth/manager.rs`, change `CodexAuth` so the only usable variants are:

```rust
pub enum CodexAuth {
    ApiKey(String),
    ExternalBearer(ExternalAuth),
}
```

Delete `Chatgpt`, `ChatgptAuthTokens`, and `AgentIdentity` variants and all methods that only read ChatGPT email, account id, plan type, refresh tokens, or FedRAMP state.

- [ ] **Step 2: Keep `AuthManager` API shape**

Keep these public methods available with provider-neutral behavior:

- `AuthManager::shared_from_config`
- `AuthManager::from_auth_for_testing`
- `AuthManager::auth`
- `AuthManager::auth_cached`
- `AuthManager::external_bearer_only`

Make refresh/recovery methods return no recovery action for generic bearer auth.

- [ ] **Step 3: Remove OAuth/device-code exports**

In `/Users/gongmeng/dev/code/omnix/codex-rs/login/src/lib.rs`, remove exports for device code, PKCE server, login server, refresh URL, revoke URL, and ChatGPT login restriction helpers.

- [ ] **Step 4: Remove OpenAI env handling**

Remove `OPENAI_API_KEY_ENV_VAR` and `read_openai_api_key_from_env`. Keep `CODEX_API_KEY_ENV_VAR` and `CODEX_ACCESS_TOKEN_ENV_VAR`.

- [ ] **Step 5: Update auth storage**

In `auth/storage.rs`, deserialize old ChatGPT auth records as unsupported legacy auth and return `None` from manager auth lookup. Do not panic on old files.

- [ ] **Step 6: Verify auth tests**

Run:

```bash
cargo test -p codex-login
cargo check -p codex-model-provider
cargo check -p codex-core
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add codex-rs/login codex-rs/model-provider codex-rs/core
git commit -m "refactor(auth): make login provider-neutral"
```

---

### Task 4: Replace OpenAI And Bedrock Provider Defaults

**Files:**
- Modify/delete provider and models-manager files listed in File Structure.

- [ ] **Step 1: Replace provider constants**

In `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider-info/src/lib.rs`, define:

```rust
const LOCAL_PROVIDER_NAME: &str = "Local Model Provider";
pub const LOCAL_PROVIDER_ID: &str = "local";
pub const DEFAULT_LOCAL_BASE_URL: &str = "http://localhost:11434/v1";
```

Remove `OPENAI_PROVIDER_NAME`, `OPENAI_PROVIDER_ID`, `CHATGPT_CODEX_BASE_URL`, `AMAZON_BEDROCK_PROVIDER_ID`, and Bedrock model constants.

- [ ] **Step 2: Replace `create_openai_provider`**

Replace `create_openai_provider` with `create_local_provider` using `LOCAL_PROVIDER_NAME`, `LOCAL_PROVIDER_ID`, and `DEFAULT_LOCAL_BASE_URL`.

- [ ] **Step 3: Rename provider auth flag**

Rename `requires_openai_auth` to `requires_provider_auth` across provider info, config structs, tests, and schema-facing types.

- [ ] **Step 4: Replace built-ins**

Make `built_in_model_providers()` return exactly:

- `local`
- `ollama`
- `lmstudio`

Remove Bedrock from the built-in map and delete Bedrock override handling in `merge_configured_model_providers`.

- [ ] **Step 5: Disable remote compaction by default**

In `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider-info/src/lib.rs`, change `ModelProviderInfo::supports_remote_compaction()` to:

```rust
pub fn supports_remote_compaction(&self) -> bool {
    false
}
```

Then update tests in `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider-info/src/model_provider_info_tests.rs` so OpenAI/Azure-specific positive cases are removed or changed to assert `false`.

Verify the call site in `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/compact.rs` still compiles and now routes all providers away from remote compaction. Do not delete `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/compact_remote_v2.rs` in this task; it becomes dead-code cleanup only after `cargo test -p codex-core` confirms no remaining references.

- [ ] **Step 6: Delete Bedrock provider runtime**

Delete `/Users/gongmeng/dev/code/omnix/codex-rs/model-provider/src/amazon_bedrock` and remove module references, dependencies on `codex-aws-auth`, and Bedrock tests.

- [ ] **Step 7: Make request auth generic**

In `bearer_auth_provider.rs`, keep only `Authorization: Bearer <token>` injection. Remove `ChatGPT-Account-ID` and `X-OpenAI-Fedramp`.

- [ ] **Step 8: Rename model endpoint/manager types**

Rename only remote-fetching model manager types:

- `OpenAiModelsEndpoint` to `RemoteModelsEndpoint`
- `OpenAiModelsManager` to `RemoteModelsManager`

Keep these names unchanged:

- `ModelsManager`
- `ModelsEndpointClient`
- `ModelsManagerConfig`
- `StaticModelsManager`

`StaticModelsManager` remains the provider-neutral implementation used when a static model catalog is supplied.

- [ ] **Step 9: Verify**

Run:

```bash
cargo test -p codex-model-provider-info
cargo test -p codex-model-provider
cargo test -p codex-models-manager
```

Expected: PASS.

- [ ] **Step 10: Commit**

Run:

```bash
git add codex-rs/model-provider-info codex-rs/model-provider codex-rs/models-manager codex-rs/Cargo.toml codex-rs/Cargo.lock
git commit -m "refactor(provider): use local provider defaults"
```

---

### Task 5: Update Protocol, Config, Schema, And SDK Consumers

**Files:**
- Modify protocol/config/schema/SDK files listed in File Structure.

- [ ] **Step 1: Add neutral model catalog export**

In `/Users/gongmeng/dev/code/omnix/codex-rs/protocol/src/lib.rs`, keep:

```rust
pub mod openai_models;
```

and add:

```rust
pub use openai_models as model_catalog;
```

- [ ] **Step 2: Simplify protocol account types**

In `/Users/gongmeng/dev/code/omnix/codex-rs/protocol/src/account.rs`, remove ChatGPT plan/account types and make `ProviderAccount` generic:

```rust
pub enum ProviderAccount {
    ApiKey,
    ExternalBearer,
}
```

Update `protocol/src/error.rs` to remove plan-specific usage-limit copy.

- [ ] **Step 3: Simplify app-server auth modes**

In `app-server-protocol/src/protocol/common.rs`, make `AuthMode` contain:

- `ApiKey`
- `ExternalBearer`

Remove `Chatgpt`, `ChatgptAuthTokens`, and `AgentIdentity`.

- [ ] **Step 4: Remove ChatGPT account APIs**

In `v2/account.rs`, remove ChatGPT login, device-code, external token variants, and `ChatgptAuthTokensRefresh*` types. Remove `account/chatgptAuthTokens/refresh` from `common.rs`.

- [ ] **Step 5: Remove ChatGPT config fields**

Remove `chatgpt_base_url`, `forced_chatgpt_workspace_id`, and ChatGPT forced login parsing from:

- `config/src/config_toml.rs`
- `config/src/profile_toml.rs`
- `app-server-protocol/src/protocol/v2/config.rs`
- `core/src/config/mod.rs`

- [ ] **Step 6: Replace default model values**

Replace test/default model strings `gpt-5`, `gpt-5.2`, `gpt-5.3-codex`, `gpt-5.4`, and `gpt-5.4-mini` with `local-default`, `local-fast`, or `test-model`.

- [ ] **Step 7: Regenerate schemas**

Run from `/Users/gongmeng/dev/code/omnix`:

```bash
just write-config-schema
just write-app-server-schema
```

Expected: generated schemas no longer include ChatGPT auth/config types.

- [ ] **Step 8: Update SDK consumers**

Run:

```bash
rg -n "Chatgpt|ChatGPT|chatgptAuthTokens|forced_chatgpt_workspace_id|chatgpt_base_url" sdk codex-rs/app-server-protocol/schema
```

Update SDK tests/fixtures and generated type consumers so the search has no product API dependencies.

- [ ] **Step 9: Verify**

Run:

```bash
cargo test -p codex-protocol
cargo test -p codex-app-server-protocol
```

Expected: PASS.

- [ ] **Step 10: Commit**

Run:

```bash
git add codex-rs/protocol codex-rs/app-server-protocol codex-rs/config codex-rs/core/config.schema.json sdk
git commit -m "refactor(protocol): remove ChatGPT account API"
```

---

### Task 6: Make Cloud Requirements An Empty Managed Config Loader

**Files:**
- Modify cloud-requirements and cloud-requirements call sites listed in File Structure.

- [ ] **Step 1: Make loader functions return empty loaders**

In `/Users/gongmeng/dev/code/omnix/codex-rs/cloud-requirements/src/lib.rs`, make `cloud_requirements_loader` and `cloud_requirements_loader_for_storage` return `CloudRequirementsLoader::default()` behavior without network calls.

- [ ] **Step 2: Remove ChatGPT base URL plumbing**

In `app-server/src/config_manager.rs`, change `replace_cloud_requirements_loader` so it accepts no `chatgpt_base_url` and stores the empty loader.

- [ ] **Step 3: Remove app-server refresh calls**

In `app-server/src/lib.rs` and `account_processor.rs`, remove calls that rebuild cloud requirements from ChatGPT auth/base URL.

- [ ] **Step 4: Rename UI source label**

Change TUI/debug labels from cloud/admin ChatGPT wording to `Managed config`.

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p codex-cloud-requirements
cargo test -p codex-core
cargo test -p codex-exec
cargo test -p codex-app-server
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add codex-rs/cloud-requirements codex-rs/app-server codex-rs/tui codex-rs/exec codex-rs/core
git commit -m "refactor(config): neutralize cloud requirements loader"
```

---

### Task 7: Remove First-Party Runtime Branches From Core

**Files:**
- Modify core runtime/test files listed in File Structure.

- [ ] **Step 1: Remove unauthorized recovery branches**

In `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/client.rs`, remove ChatGPT/OpenAI-specific unauthorized recovery and token refresh behavior.

- [ ] **Step 2: Remove cyber fallback product copy**

In `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/session/mod.rs`, remove ChatGPT/OpenAI cyber URLs and `gpt-5.2` fallback copy. Replace with provider-neutral backend error text.

- [ ] **Step 3: Remove host-owned app logic**

In `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/session/mcp.rs`, remove host-owned ChatGPT apps, OpenAI curated suggestions, and ChatGPT auth cache keys.

- [ ] **Step 4: Remove ChatGPT connector discovery**

In `/Users/gongmeng/dev/code/omnix/codex-rs/core/src/connectors.rs`, keep only user-configured connectors/MCP servers.

- [ ] **Step 5: Update tests**

Update core session/client/thread/config/connector tests to use provider-neutral auth and model names.

- [ ] **Step 6: Verify**

Run:

```bash
cargo test -p codex-core
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add codex-rs/core
git commit -m "refactor(core): remove first-party runtime branches"
```

---

### Task 8: Update App-Server Processors And Integration Tests

**Files:**
- Modify app-server files and tests listed in File Structure.

- [ ] **Step 1: Remove account login flows**

In `app-server/src/request_processors/account_processor.rs`, remove ChatGPT login, device code, external token refresh, BackendClient account fetch, and ChatGPT base URL usage.

- [ ] **Step 2: Replace workspace setting gates**

In catalog/apps/plugins processors, replace ChatGPT workspace setting gates with local config gates that default to enabled.

- [ ] **Step 3: Remove analytics base URL derivation**

In `app-server/src/analytics_utils.rs`, remove `chatgpt_base_url`-derived analytics endpoint behavior.

- [ ] **Step 4: Update integration fixtures**

Update:

- `app-server/tests/suite/auth.rs`
- `app-server/tests/common/auth_fixtures.rs`
- `app-server/tests/common/mock_model_server.rs`

Remove ChatGPT auth fixtures and use generic API key or external bearer fixtures.

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p codex-app-server
cargo test -p app_test_support
cargo test -p codex-app-server --test all
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add codex-rs/app-server
git commit -m "refactor(app-server): remove ChatGPT account processors"
```

---

### Task 9: Remove ChatGPT TUI And Marketplace UI

**Files:**
- Modify/delete TUI/plugin/skill files listed in File Structure.

- [ ] **Step 1: Remove onboarding auth**

Delete `tui/src/onboarding/auth` and `tui/src/local_chatgpt_auth.rs`. Make first-run flow enter the local harness with provider-neutral config status.

- [ ] **Step 2: Remove ChatGPT account UI state**

Remove `has_chatgpt_account`, ChatGPT rate-limit refresh, and ChatGPT-only model availability branches.

- [ ] **Step 3: Remove ChatGPT app links**

In `bottom_pane/app_link_view.rs`, remove `chatgpt.com` allowlist and copy like `Manage on ChatGPT` and `Install on ChatGPT`.

- [ ] **Step 4: Remove OpenAI curated marketplace tab**

In `chatwidget/plugins.rs`, remove `OPENAI_CURATED_MARKETPLACE_NAME` and special tab behavior.

- [ ] **Step 5: Remove remote plugin sync/share**

Delete remote sync/share implementations in `core-plugins` and remove first-party originator helpers in `utils/plugins/src/mcp_connector.rs`.

- [ ] **Step 6: Keep legacy skill metadata filename**

Keep `openai.yaml` file handling in `core-skills/src/loader.rs` as legacy compatibility. Do not add new OpenAI product behavior.

- [ ] **Step 7: Verify and accept snapshots**

Run:

```bash
cargo test -p codex-tui
cargo insta pending-snapshots -p codex-tui
cargo insta accept -p codex-tui
cargo test -p codex-core-plugins
cargo test -p codex-core-skills
```

Expected: PASS and intended snapshots accepted.

- [ ] **Step 8: Commit**

Run:

```bash
git add codex-rs/tui codex-rs/core-plugins codex-rs/core-skills codex-rs/utils/plugins
git commit -m "refactor(tui): remove ChatGPT marketplace UI"
```

---

### Task 10: Neutralize Analytics And Feedback

**Files:**
- Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/analytics`
- Modify: `/Users/gongmeng/dev/code/omnix/codex-rs/feedback`
- Modify call sites in core/cli/tui/app-server.

- [ ] **Step 1: Make analytics no-op**

Remove network sending and `codex-login` dependency from analytics. Keep event data structures so call sites compile.

- [ ] **Step 2: Make feedback no-op/local-only**

Remove first-party upload behavior. Keep local error formatting and diagnostics.

- [ ] **Step 3: Verify**

Run:

```bash
cargo test -p codex-analytics
cargo test -p codex-feedback
```

Expected: PASS.

- [ ] **Step 4: Commit**

Run:

```bash
git add codex-rs/analytics codex-rs/feedback
git commit -m "refactor: neutralize analytics and feedback"
```

---

### Task 11: Lockfile, Bazel, Format, And Final Verification

**Files:**
- Modify lockfiles and formatting output.

- [ ] **Step 1: Check product references**

Run:

```bash
rg -n "ChatGPT|chatgpt|chatgptAuthTokens|chatgpt_base_url|forced_chatgpt_workspace_id" codex-rs sdk --glob '!target'
rg -n "api.openai.com|chatgpt.com|OPENAI_API_KEY|requires_openai_auth" codex-rs sdk --glob '!target'
rg -n "gpt-5|gpt-4|gpt-" codex-rs sdk --glob '!target'
```

Expected: output only for intentional legacy comments/tests or the retained `openai_models.rs` compatibility name.

- [ ] **Step 2: Run focused tests**

Run:

```bash
cargo test -p codex-login
cargo test -p codex-model-provider-info
cargo test -p codex-model-provider
cargo test -p codex-models-manager
cargo test -p codex-protocol
cargo test -p codex-app-server-protocol
cargo test -p codex-core
cargo test -p codex-cli
cargo test -p codex-exec
cargo test -p codex-app-server
cargo test -p codex-app-server --test all
cargo test -p codex-tui
cargo test -p codex-core-plugins
cargo test -p codex-core-skills
cargo test -p codex-analytics
cargo test -p codex-feedback
```

Expected: PASS.

- [ ] **Step 3: Update Bazel lock**

Run from `/Users/gongmeng/dev/code/omnix`:

```bash
just bazel-lock-update
just bazel-lock-check
```

Expected: PASS. `/Users/gongmeng/dev/code/omnix/MODULE.bazel.lock` changes only when Bazel dependency resolution requires a lockfile update.

- [ ] **Step 4: Format**

Run from `/Users/gongmeng/dev/code/omnix`:

```bash
just fmt
```

Expected: PASS.

- [ ] **Step 5: Run scoped fixes**

Run from `/Users/gongmeng/dev/code/omnix`:

```bash
just fix -p codex-cli
just fix -p codex-tui
just fix -p codex-app-server
just fix -p codex-core
just fix -p codex-model-provider
just fix -p codex-model-provider-info
```

Expected: PASS. Do not rerun tests only because `fmt` or `fix` ran.

- [ ] **Step 6: Commit final cleanup**

Run:

```bash
git status --short
git add codex-rs sdk MODULE.bazel.lock
git commit -m "chore: finalize provider-neutral cleanup"
```

Expected: commit succeeds if final cleanup changed files.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-16-detach-chatgpt-openai-bindings.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task, review between tasks, fast iteration.
2. Inline Execution - execute tasks in this session using executing-plans, batch execution with checkpoints.
