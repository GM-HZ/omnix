# Staffroom Slim Core Merge Readiness Plan

> Baseline commit: `ad551c6f4 refactor: slim core to native Chat Completions harness`
>
> Branch: `codex/staffroom-slim-core`
>
> Objective: retain the provider-neutral Codex harness, native Chat Completions compatibility,
> workspace/file/tool/plugin foundations, and remove reachable ChatGPT/OpenAI product services.

## 1. Current State

### Completed

- Deleted product crates: `chatgpt`, `backend-client`, `cloud-tasks`,
  `cloud-tasks-client`, `cloud-tasks-mock-client`, `responses-api-proxy`, and
  `codex-backend-openapi-models`.
- Removed CLI cloud-task, ChatGPT apply, and responses-proxy commands.
- Removed public browser OAuth, device-code login, PKCE server, login HTML assets, and TUI
  ChatGPT onboarding. API-key login remains.
- Removed remote plugin backend, remote bundle download, sharing, checkout, catalog cache, and
  remote installed-plugin synchronization implementation.
- Removed TUI remote marketplace background requests and ChatGPT-specific remote error guidance.
- Disabled automatic OpenAI curated plugin repository synchronization at startup.
- Replaced backend connector reads with local connector projection.
- Removed Amazon Bedrock provider implementation and product cloud account/rate-limit calls.
- Preserved native Chat Completions adapter and legacy OpenAI-compatible provider support without
  changing the `ResponseEvent` contract.
- Updated Cargo and Bazel lockfiles.

### Verified

- `cargo check` passed for `codex-login`, `codex-core-plugins`, `codex-app-server`, `codex-cli`,
  and `codex-tui`.
- `just test -p codex-login`: 126/126 passed.
- `just test -p codex-core-plugins`: functional failures fixed; the target snapshot test passed.
- App-server local API-key login, local plugin list, and local plugin uninstall focused tests passed.
- Scoped Clippy completed successfully for the affected crates.
- `cargo build -p codex-cli` passed.
- `./target/debug/codex exec "say hi"` completed successfully with the DeepSeek provider.

### Not Merge Ready Yet

- The app-server full suite still registers tests for removed remote plugin/share behavior.
- Some tests still encode removed Bedrock, cloud config, and provider-reservation assumptions.
- Several unrelated integration tests fail or time out in the current local environment.
- CLI full nextest scheduling stalled after compilation and was interrupted after about nine minutes.
- No clean post-pruning full affected-crate test run has been captured.

## 2. Remaining Work Summary

| ID | Task | Priority | Dependency | Estimate |
|---|---|---:|---|---:|
| M1 | Prune obsolete remote plugin app-server tests | P0 | none | 0.5-1 day |
| M2 | Add local-harness app-server integration coverage | P0 | M1 | 0.5 day |
| M3 | Reconcile provider and removed Bedrock tests | P0 | none | 0.25-0.5 day |
| M4 | Reconcile cloud-config fallback tests | P0 | none | 0.25-0.5 day |
| M5 | Audit remaining reachable product network paths | P0 | M1-M4 | 0.5-1 day |
| M6 | Separate protocol compatibility shells from runtime features | P1 | M5 | 0.5 day |
| M7 | Stabilize CLI/TUI test execution | P0 | M1-M4 | 0.5 day |
| M8 | Run final merge verification matrix | P0 | M1-M7 | 0.5-1 day |
| M9 | Review and split the oversized commit | P1 | M8 | 0.5 day |

Expected remaining engineering time: approximately 3.5-5.5 focused days. If only the minimum
merge gate is required and unrelated baseline failures are documented instead of fixed, expect
approximately 2-3 days.

## 3. Independent Implementation Tasks

### M1: Prune Obsolete Remote Plugin App-Server Tests

**Goal:** tests must no longer require deleted remote catalog, sharing, bundle sync, or remote
uninstall behavior.

**Files:**

- `codex-rs/app-server/tests/suite/v2/mod.rs`
- `codex-rs/app-server/tests/suite/v2/plugin_share.rs`
- `codex-rs/app-server/tests/suite/v2/plugin_list.rs`
- `codex-rs/app-server/tests/suite/v2/plugin_uninstall.rs`
- `codex-rs/app-server/tests/suite/v2/recommended_plugins.rs`
- `codex-rs/app-server/tests/suite/v2/skills_list.rs`

**Steps:**

1. Remove the complete `plugin_share` test module; sharing is removed behavior, not a negative path.
2. Delete `plugin_list` cases that mount `/wham/*`, workspace plugin catalogs, featured plugin IDs,
   shared-with-me, created-by-me, or remote installed bundle endpoints.
3. Delete remote-ID uninstall tests and remote detail/cache namespace helpers.
4. Remove recommended-plugin tests that require external ChatGPT login or a remote catalog warm-up.
5. Remove runtime `remote_plugin` toggle tests from skills-list coverage.
6. Keep local marketplace parsing, installed/enabled state, interface asset path, invalid manifest,
   relative-CWD rejection, and local uninstall tests.
7. Run `just fmt`.

**Verification:**

```bash
just test -p codex-app-server plugin_list
just test -p codex-app-server plugin_uninstall
rg -n "plugin_share|shared_with_me|created_by_me|remote_installed|remote_plugin" \
  codex-rs/app-server/tests/suite/v2
```

**Done when:** retained plugin integration tests cover only local/configured marketplaces and all
selected tests pass.

**Commit:** `test(app-server): remove obsolete remote plugin coverage`

**Status: DONE** — commit `7c6ee5753`.

- Deleted `plugin_share.rs` (13 sharing tests) and `recommended_plugins.rs` (1 ChatGPT-login
  test) wholesale; removed both from `mod.rs`.
- `plugin_list.rs`: 39 → 12 tests (4186 → ~1180 lines). Kept the 12 local-only tests
  (invalid marketplace, installed+suggestions, ignores-cache-without-catalog, relative-CWD
  rejection, keeps-valid-marketplaces, alternate-discoverable-manifest, omitted-cwds,
  install/enabled state, home-config enabled state, interface asset paths, legacy string
  prompt, installed git-source interface). Removed all remote-marketplace/catalog-cache/
  featured/shared-with-me/created-by-me/workspace-directory/bundle-sync cases and the two
  workspace-settings-gated cases (handler `workspace_codex_plugins_enabled` now returns
  `true` with no backend call). Removed the share-context-from-local-mapping test
  (`share_context` is always `None` now). Cleaned unused imports/consts/helpers.
- `plugin_uninstall.rs`: 10 → 2 tests. Kept local uninstall + analytics; removed all
  remote-ID uninstall/catalog-detail cases (handler is now `"invalid local plugin id"`,
  no network).
- `skills_list.rs`: 9 → 6 tests. Removed `runtime_remote_plugin_toggle_*`,
  `skills_list_loads_remote_installed_plugin_skills_from_cache`, and the
  `workspace_codex_plugins_disabled` case (backend `/accounts/{id}/settings` gating removed).
- Verification: `cargo test -p codex-app-server --test all -- v2::plugin_list::
  v2::plugin_uninstall:: v2::skills_list::` → **20 passed, 590 filtered out**. `just fmt`
  clean. rg audit leaves only benign `remote_plugin_id: None` / `"remote_plugin_id": null`
  wire-compat field assertions (allowed per M6).

**Follow-up discovered (next round, NOT done in M1):** `plugin_read.rs` (not in M1's file
list) has the same obsolete-remote-test problem — ~11 of its 21 tests use
`remote_marketplace_name: Some(...)` / `write_remote_plugin_catalog_config` /
`write_plugin_share_local_path_mapping` and would fail at runtime (handler rejects
`remote_marketplace_name.is_some()`; `share_context` is always `None`). Its
`marketplace_path: Some(...) / remote_marketplace_name: None` tests are genuine local reads
worth keeping. Recommend folding a `plugin_read.rs` prune into M2 or a dedicated M1b before
running the full app-server suite in M8.


### M2: Add Local-Harness App-Server Integration Coverage

**Goal:** replace deleted product tests with compact tests for the behavior Staffroom depends on.

**Files:**

- `codex-rs/app-server/tests/suite/v2/plugin_list.rs`
- `codex-rs/app-server/tests/suite/v2/plugin_uninstall.rs`
- `codex-rs/app-server/tests/suite/v2/account.rs`
- `codex-rs/app-server/src/request_processors/local_connectors_tests.rs`

**Steps:**

1. Add one API-key login round-trip test that verifies notification and account read behavior.
2. Add one local marketplace list test with installed/enabled/plugin-interface assertions.
3. Add one local plugin uninstall test that verifies cache and config removal.
4. Add one test proving plugin list performs no backend product request.
5. Add one local connector projection test proving only requested plugin apps are exposed.
6. Avoid assertions on protocol fields that are statically `None`.

**Verification:**

```bash
just test -p codex-app-server login_account_api_key
just test -p codex-app-server plugin_list_local_harness
just test -p codex-app-server plugin_uninstall_local_harness
just test -p codex-app-server local_connectors
```

**Done when:** each retained local product behavior has an app-server public JSON-RPC integration
test and no mock product backend is required.

**Commit:** `test(app-server): cover local harness plugin flows`

### M3: Reconcile Provider And Removed Bedrock Tests

**Goal:** tests match the provider-neutral model configuration and deleted Bedrock implementation.

**Files:**

- `codex-rs/app-server/src/config_manager_service_tests.rs`
- `codex-rs/app-server/tests/suite/v2/thread_start.rs`
- `codex-rs/model-provider-info/src/model_provider_info_tests.rs`
- `codex-rs/model-provider/src/provider.rs`

**Steps:**

1. Decide and document whether built-in provider IDs may be overridden by user config.
2. If override is intentionally supported for old OpenAI-compatible endpoints, replace the old
   rejection test with a round-trip test for the override.
3. Remove `thread_start_provider_model_fallback_uses_bedrock_static_catalog`.
4. Search all test fixtures for removed `aws` fields and Bedrock provider IDs.
5. Verify custom base URL, API key env var, Responses wire API, and Chat Completions wire API.

**Verification:**

```bash
just test -p codex-model-provider-info
just test -p codex-model-provider
just test -p codex-app-server reserved_builtin_provider
just test -p codex-app-server thread_start_provider_model_fallback
```

**Done when:** no test requires Bedrock and custom OpenAI-compatible provider configuration is
explicitly covered.

**Commit:** `test: align provider coverage with slim harness`

### M4: Reconcile Cloud-Config Fallback Tests

**Goal:** removed first-party cloud bundle loading must not leave tests waiting for backend errors.

**Files:**

- `codex-rs/cloud-config/src/service_tests.rs`
- `codex-rs/app-server/tests/suite/v2/thread_start.rs`
- `codex-rs/app-server/tests/suite/v2/thread_resume.rs`
- `codex-rs/app-server/tests/suite/v2/thread_fork.rs`

**Steps:**

1. Remove tests that require backend cloud bundle fetch failures to propagate.
2. Add or retain tests proving local config layers load for start, resume, and fork.
3. Verify the no-op cloud config service cannot make network requests.
4. Ensure local managed requirements behavior remains deterministic.

**Verification:**

```bash
just test -p codex-cloud-config
just test -p codex-app-server thread_start_local_config
just test -p codex-app-server thread_resume_local_config
just test -p codex-app-server thread_fork_local_config
```

**Done when:** thread lifecycle tests no longer depend on a first-party cloud endpoint.

**Commit:** `test: remove cloud bundle assumptions from local harness`

### M5: Audit Remaining Reachable Product Network Paths

**Goal:** prove startup and normal local execution do not contact ChatGPT/OpenAI product services.

**Files:** discovered by the audit; likely areas include `login`, `core-plugins`, `app-server`,
`cloud-config`, `models-manager`, and `core`.

**Steps:**

1. Search for first-party URLs, backend routes, workspace settings, remote catalogs, and OAuth.
2. Classify every hit as runtime reachable, protocol compatibility, test fixture, or documentation.
3. Remove or disable every runtime-reachable product service call.
4. Retain provider-configured OpenAI-compatible URLs only when selected by user configuration.
5. Verify plugin startup does not start curated repository synchronization.
6. Add a startup test with a deny-all mock/proxy that asserts zero product requests.
7. Record allowed residual identifiers in the plan: protocol auth variants, persisted auth parsing,
   and wire-compatible optional fields.

**Audit command:**

```bash
rg -n "chatgpt|api\.openai\.com|auth\.openai\.com|backend-api|wham/|workspace_settings|\
sync_openai_plugins_repo|remote_plugin|plugin_share" codex-rs --glob '!target'
```

**Done when:** every remaining hit has a written compatibility justification and zero implicit
first-party requests occur during startup or `codex exec`.

**Commit:** `refactor: close remaining first-party runtime paths`

### M6: Separate Compatibility Shells From Runtime Features

**Goal:** make retained legacy protocol fields visibly inert and prevent accidental reactivation.

**Files:**

- `codex-rs/app-server-protocol/src/protocol/v2.rs`
- `codex-rs/app-server/src/request_processors/plugins.rs`
- `codex-rs/protocol/src/account.rs`
- `codex-rs/login/src/auth/manager.rs`
- `codex-rs/features/src/lib.rs`

**Steps:**

1. Inventory `remotePluginId`, share-context, ChatGPT auth, and workspace fields exposed on the wire.
2. Keep fields needed for old clients but mark runtime handlers unavailable consistently.
3. Remove feature flags that no longer gate any executable implementation.
4. Keep persisted-token parsing only where it supports migration or external bearer auth.
5. Add schema comments or internal documentation explaining compatibility-only fields.
6. Regenerate app-server schema only if wire definitions change.

**Verification:**

```bash
just write-app-server-schema
just test -p codex-app-server-protocol
git diff --exit-code -- codex-rs/app-server-protocol/schema
```

**Done when:** protocol compatibility is intentional, documented, and cannot activate deleted
network behavior.

**Commit:** `refactor(protocol): isolate legacy product compatibility`

### M7: Stabilize CLI And TUI Test Execution

**Goal:** obtain reproducible test results rather than relying on compile-only evidence.

**Files:** determined by diagnosis; likely CLI test binaries and TUI plugin fixtures.

**Steps:**

1. Re-run `just test -p codex-cli` with nextest status diagnostics.
2. Determine why nextest stalls after compilation with no child test process.
3. Check stale cargo locks, test enumeration, and package test binary startup.
4. Run CLI library/login tests separately to isolate the blocking binary.
5. Compile and run TUI tests after the CLI issue is isolated.
6. Review TUI pending snapshots and accept only intended local-harness changes.
7. Keep remote-section merge helpers under `#[cfg(test)]`; verify they are absent from release code.

**Verification:**

```bash
just test -p codex-cli
just test -p codex-tui
cargo insta pending-snapshots -p codex-tui
```

**Done when:** both commands terminate normally, pass, and no unreviewed snapshots remain.

**Commit:** `test: stabilize slim CLI and TUI suites`

### M8: Final Merge Verification Matrix

**Goal:** produce one clean, repeatable evidence set for merge approval.

**Steps:**

1. Run `git diff --check` and confirm a clean worktree.
2. Run `just bazel-lock-update`; verify no lock drift.
3. Run affected-crate tests: login, core-plugins, model-provider-info, model-provider,
   cloud-config, app-server, CLI, and TUI.
4. Because core/common changed, run the complete `just test` suite after explicit approval.
5. Run scoped Clippy, then `just fmt`; do not modify source afterward.
6. Build `codex-cli` from a clean target state if practical.
7. Run `./target/debug/codex exec "say hi"` and record provider, exit code, and response.
8. Re-run the product-network audit.

**Merge gate:**

- All task-related tests pass.
- Full-suite failures, if any, are reproduced on `main` and documented as baseline failures.
- Build and smoke pass.
- No reachable implicit ChatGPT/OpenAI product service remains.
- Worktree is clean and commits are reviewable.

**Commit:** no source commit expected; only a verification note if the project keeps one.

### M9: Review And Split The Oversized Commit

**Goal:** make the 31k-line deletion reviewable and reduce merge/revert risk.

**Current size:** 219 files, approximately 947 additions and 31,496 deletions.

**Suggested commit split:**

1. Remove standalone product crates and manifests.
2. Preserve native Chat Completions compatibility and provider-neutral model selection.
3. Remove ChatGPT login/onboarding and product account services.
4. Remove remote plugin backend and use local connector/plugin projection.
5. Remove Bedrock/cloud task/cloud rate-limit paths.
6. Update tests, snapshots, Cargo lock, and Bazel lock.

**Steps:**

1. Create a backup branch at `ad551c6f4`.
2. Rebuild commits without changing the final tree.
3. After each commit, run at least `cargo check -p codex-cli`.
4. Compare the rebuilt branch tree to `ad551c6f4` with `git diff --exit-code`.
5. Run M8 again on the rebuilt branch.

**Done when:** each commit is coherent and the final tree is byte-for-byte equivalent to the
verified implementation.

## 4. Recommended Execution Order

1. M1: remove stale remote plugin tests.
2. M2: establish local app-server coverage.
3. M3 and M4: reconcile provider and cloud-config assumptions.
4. M7: get CLI/TUI suites deterministic.
5. M5: perform final runtime network audit.
6. M6: clean compatibility-only API surfaces.
7. M8: run merge verification.
8. M9: split the large commit only after behavior is stable.

Do not start M9 before M8 has passed once; otherwise failures become harder to distinguish from
history-rewrite mistakes.
