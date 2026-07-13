//! `omnix-runtime` — internal adapter over the in-process Omnix app-server.
//!
//! This crate is NOT a stable dependency surface. It exists so `omnix-sdk` can
//! present a small, stable API while reusing the existing app-server execution
//! chain unchanged. It lowers a validated [`RuntimeSpec`] into an in-memory
//! `Config`, starts the in-process app-server, and (in later phases) maps
//! sessions/runs/events onto threads/turns/notifications.
//!
//! Business applications must depend on `omnix-sdk`, never on this crate
//! directly.

mod config_translate;
mod dirs;
mod error;
mod events;
mod manifest;
mod run;
mod runtime;
mod session;
mod spec;
mod start_args;
mod tools;

pub use dirs::RuntimePaths;
pub use dirs::RuntimeScope;
pub use error::RuntimeError;
pub use events::AgentEvent;
pub use events::AgentFailure;
pub use events::ApprovalDecision;
pub use events::ApprovalKind;
pub use events::ApprovalRequest;
pub use events::RunResult;
pub use events::RunStatus;
pub use events::Usage;
pub use manifest::RuntimeManifest;
pub use run::Run;
pub use runtime::Capabilities;
pub use runtime::Runtime;
pub use runtime::RuntimeHealth;
pub use session::Session;
pub use session::SessionConfig;
pub use session::SessionMetadata;
pub use spec::ContextSpec;
pub use spec::ModelSpec;
pub use spec::OMNIX_PROVIDER_ID;
pub use spec::PermissionSpec;
pub use spec::RuntimeSpec;
pub use spec::ToolSpec;
pub use tools::ToolDescriptor;
pub use tools::ToolInvocation;
pub use tools::ToolInvocationOutput;
pub use tools::ToolInvoker;
pub use tools::ToolLimits;

// Re-export the provider wire protocol + permission enums so `omnix-sdk` can
// build a `RuntimeSpec` without depending on the underlying codex crates.
pub use codex_model_provider_info::WireApi;
pub use codex_protocol::config_types::SandboxMode;
pub use codex_protocol::protocol::AskForApproval;
