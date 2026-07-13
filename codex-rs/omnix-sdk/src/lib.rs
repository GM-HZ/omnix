//! # Omnix Embedded Agent SDK
//!
//! `omnix-sdk` is the single stable dependency a Rust business application needs
//! to embed the Omnix agent runtime (DeepSeek / Chat Completions, Runtime 0.0).
//! It hides the in-process app-server, its JSON-RPC protocol, and the many
//! internal `codex-*` crates behind a small, strongly-typed API.
//!
//! ## Quick start
//!
//! ```no_run
//! use omnix_sdk::{Omnix, Credentials};
//!
//! # async fn run() -> Result<(), omnix_sdk::OmnixError> {
//! let runtime = Omnix::builder()
//!     .application_root("/path/to/app-data")
//!     .credentials(Credentials::from_api_key("sk-..."))
//!     .build()
//!     .await?;
//!
//! // ... create sessions and run turns (see later phases) ...
//!
//! runtime.shutdown().await?;
//! # Ok(())
//! # }
//! ```
//!
//! Startup requires only a data root and credentials; all other settings use
//! Runtime 0.0 defaults. No `config.toml` is read. The API key is held in memory
//! only and never written to disk.

mod builder;
mod config;
mod error;
mod error_map;
mod event;
mod pack;
mod run;
mod runtime;
mod session;
mod tools;

pub use builder::OmnixBuilder;
pub use config::ApprovalPolicy;
pub use config::ContextConfig;
pub use config::Credentials;
pub use config::DEFAULT_BASE_URL;
pub use config::DEFAULT_MODEL;
pub use config::ModelConfig;
pub use config::ObservabilityConfig;
pub use config::PermissionConfig;
pub use config::PersistenceConfig;
pub use config::PluginConfig;
pub use config::RuntimeConfig;
pub use config::RuntimeScope;
pub use config::SandboxPolicy;
pub use config::SkillConfig;
pub use config::ToolConfig;
pub use config::WireApi;
pub use error::Correlation;
pub use error::OmnixError;
pub use error::OmnixErrorKind;
pub use error::OmnixResult;
pub use event::AgentEvent;
pub use event::AgentFailure;
pub use event::ApprovalDecision;
pub use event::ApprovalKind;
pub use event::ApprovalRequest;
pub use event::RunResult;
pub use event::RunStatus;
pub use event::Usage;
pub use pack::BusinessPack;
pub use pack::InstructionSource;
pub use pack::PackError;
pub use pack::PluginSource;
pub use pack::SkillSource;
pub use run::AgentRun;
pub use runtime::Capabilities;
pub use runtime::OmnixRuntime;
pub use runtime::RuntimeHealth;
pub use session::SessionConfig;
pub use session::SessionHandle;
pub use session::SessionMetadata;
pub use session::Sessions;
pub use tools::AgentTool;
pub use tools::ToolCallContext;
pub use tools::ToolError;
pub use tools::ToolOutput;
pub use tools::ToolRegistry;
pub use tools::ToolSpecification;

/// Entry point for constructing an [`OmnixRuntime`].
pub struct Omnix;

impl Omnix {
    /// Start building a runtime.
    pub fn builder() -> OmnixBuilder {
        OmnixBuilder::new()
    }
}
