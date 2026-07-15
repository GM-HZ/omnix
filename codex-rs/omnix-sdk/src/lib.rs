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
//! let process = Omnix::initialize_embedded_process();
//! let builder = Omnix::builder()
//!     .application_root("/path/to/app-data")
//!     .credentials(Credentials::from_api_key("sk-..."));
//! let builder = match process {
//!     Some(process) => builder.embedded_process(process),
//!     None => builder,
//! };
//! let runtime = builder.build().await?;
//!
//! let mut session = runtime.sessions().create(Default::default()).await?;
//! let schema = serde_json::json!({
//!     "type": "object",
//!     "properties": { "summary": { "type": "string" } },
//!     "required": ["summary"],
//!     "additionalProperties": false
//! });
//! let mut run = session
//!     .run_with_config(
//!         "Summarize the source as JSON.",
//!         omnix_sdk::RunConfig::json(schema),
//!     )
//!     .await?;
//! while run.next().await.is_some() {}
//!
//! runtime.shutdown().await?;
//! # Ok(())
//! # }
//! ```
//!
//! Startup requires only a data root and credentials; all other settings use
//! Runtime 0.0 defaults. No `config.toml` is read. The API key is held in memory
//! only and never written to disk. A single-binary host should call
//! [`Omnix::initialize_embedded_process`] before starting its async runtime to
//! enable built-in command/file tools; host-registered tools and inference do
//! not depend on that helper dispatch.
//!
//! DeepSeek JSON Output guarantees a syntactically valid JSON object. A schema
//! supplied through [`RunConfig`] is bounded model guidance, not provider-side
//! strict validation; applications must validate the result before committing
//! business data.

mod builder;
mod config;
mod error;
mod error_map;
mod event;
mod instruction;
mod pack;
mod process;
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
pub use config::PermissionConfig;
pub use config::RuntimeConfig;
pub use config::RuntimeScope;
pub use config::SandboxPolicy;
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
pub use process::EmbeddedProcess;
pub use run::AgentRun;
pub use run::RunConfig;
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
    /// Initialize helper dispatch for a single-binary embedded host.
    ///
    /// Call this at the very beginning of `main`, before creating threads or an
    /// async runtime, then pass the returned value to
    /// [`OmnixBuilder::embedded_process`].
    pub fn initialize_embedded_process() -> Option<EmbeddedProcess> {
        omnix_runtime::initialize_embedded_process().map(|inner| EmbeddedProcess { inner })
    }

    /// Start building a runtime.
    pub fn builder() -> OmnixBuilder {
        OmnixBuilder::new()
    }
}
