//! In-process runtime handle: owns the app-server lifecycle and orchestrates
//! sessions/runs over the background event consumer.
//!
//! The event consumer task owns the `InProcessAppServerClient` (because
//! `next_event()` needs `&mut`), while this handle keeps a cloneable
//! `InProcessAppServerRequestHandle` for issuing thread/turn requests.

use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessAppServerRequestHandle;
use tokio::sync::mpsc;

use crate::config_translate::build_config;
use crate::dirs::RuntimePaths;
use crate::dirs::prepare_runtime_dirs;
use crate::error::RuntimeError;
use crate::events::consumer::ConsumerCommand;
use crate::events::consumer::ToolDispatch;
use crate::events::consumer::spawn_consumer;
use crate::session::Session;
use crate::spec::RuntimeSpec;
use crate::start_args::build_start_args;
use crate::tools::ToolDescriptor;
use crate::tools::ToolLimits;

/// Health of the embedded runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeHealth {
    /// The in-process app-server is initialized and accepting requests.
    Ready,
    /// The runtime has been shut down.
    ShutDown,
}

/// Statically-known capabilities of Runtime 0.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub wire_api: &'static str,
    pub reasoning: bool,
    pub tools: bool,
    pub persistence: bool,
    pub compaction: bool,
}

impl Capabilities {
    fn runtime_0_0() -> Self {
        Self {
            wire_api: "chat_completions",
            reasoning: true,
            tools: true,
            persistence: true,
            compaction: true,
        }
    }
}

/// The embedded runtime.
pub struct Runtime {
    request_handle: InProcessAppServerRequestHandle,
    consumer_tx: mpsc::Sender<ConsumerCommand>,
    consumer_handle: tokio::task::JoinHandle<InProcessAppServerClient>,
    paths: RuntimePaths,
    health: RuntimeHealth,
    /// Tool descriptors advertised on every thread (empty when no tools).
    tool_descriptors: Vec<ToolDescriptor>,
    /// Defaults applied to every thread started by this runtime.
    session_defaults: SessionDefaults,
}

/// Runtime-level defaults applied when starting a thread. These must be set on
/// `ThreadStartParams` (not just the initial `Config`) because the app-server
/// re-resolves config on `thread/start` and does not carry the SDK's harness
/// overrides — but it DOES honor per-thread params.
#[derive(Clone)]
pub(crate) struct SessionDefaults {
    pub model: String,
    pub base_instructions: Option<String>,
    pub developer_instructions: Option<String>,
}

impl Runtime {
    /// Prepare directories, build the in-memory config, start the in-process
    /// app-server (initialize handshake included), and spawn the event consumer.
    pub async fn start(spec: RuntimeSpec) -> Result<Self, RuntimeError> {
        let started_at = std::time::Instant::now();
        let paths =
            prepare_runtime_dirs(&spec.scope).map_err(|source| RuntimeError::Dirs { source })?;

        // Persist + compatibility-check the non-sensitive runtime manifest
        // (§14). Refuses to start against a newer, unmigratable on-disk state.
        let scope_label = spec.scope.label();
        crate::manifest::reconcile_manifest(&paths.codex_home, &spec.model.model, scope_label)?;

        let translated = build_config(&spec, &paths).await?;
        let start_args = build_start_args(translated.config, translated.cli_overrides).await?;
        let client = InProcessAppServerClient::start(start_args)
            .await
            .map_err(|source| RuntimeError::Start { source })?;

        let request_handle = client.request_handle();

        let session_defaults = SessionDefaults {
            model: spec.model.model.clone(),
            base_instructions: spec.base_instructions.clone(),
            developer_instructions: spec.developer_instructions.clone(),
        };

        // Wire tool dispatch, if the host registered any tools.
        let (tool_dispatch, tool_descriptors) = match spec.tool_invoker {
            Some(invoker) => {
                let descriptors = invoker.descriptors();
                let max_concurrency = spec.tools.max_concurrency.max(1);
                let dispatch = ToolDispatch {
                    invoker,
                    limits: ToolLimits {
                        call_timeout: spec.tools.call_timeout,
                        max_concurrency,
                    },
                    semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency)),
                };
                (Some(dispatch), descriptors)
            }
            None => (None, Vec::new()),
        };

        let (consumer_tx, consumer_handle) = spawn_consumer(client, tool_dispatch);

        tracing::info!(
            target: "omnix::runtime",
            init_ms = started_at.elapsed().as_millis() as u64,
            model = %session_defaults.model,
            scope = scope_label,
            tools = tool_descriptors.len(),
            "omnix runtime initialized"
        );

        Ok(Self {
            request_handle,
            consumer_tx,
            consumer_handle,
            paths,
            health: RuntimeHealth::Ready,
            tool_descriptors,
            session_defaults,
        })
    }

    /// Current health.
    pub fn health(&self) -> RuntimeHealth {
        self.health.clone()
    }

    /// Statically-known runtime capabilities.
    pub fn capabilities(&self) -> Capabilities {
        Capabilities::runtime_0_0()
    }

    /// Resolved on-disk paths (`.omnix` home and workspace).
    pub fn paths(&self) -> &RuntimePaths {
        &self.paths
    }

    /// Create a new agent session (a new thread).
    pub async fn create_session(
        &self,
        config: crate::session::SessionConfig,
    ) -> Result<Session, RuntimeError> {
        Session::create(
            self.request_handle.clone(),
            self.consumer_tx.clone(),
            self.tool_descriptors.clone(),
            self.session_defaults.clone(),
            config,
        )
        .await
    }

    /// Resume an existing session by its id (thread id).
    pub async fn resume_session(&self, session_id: String) -> Result<Session, RuntimeError> {
        Session::resume(
            self.request_handle.clone(),
            self.consumer_tx.clone(),
            self.tool_descriptors.clone(),
            self.session_defaults.clone(),
            session_id,
        )
        .await
    }

    /// Gracefully shut down the runtime: stop the consumer, recover the client,
    /// and shut it down.
    pub async fn shutdown(mut self) -> Result<(), RuntimeError> {
        self.health = RuntimeHealth::ShutDown;
        // Ask the consumer to stop; ignore send errors if it already exited.
        let _ = self.consumer_tx.send(ConsumerCommand::Stop).await;
        let client = self
            .consumer_handle
            .await
            .map_err(|_| RuntimeError::EventStreamClosed)?;
        let result = client
            .shutdown()
            .await
            .map_err(|source| RuntimeError::Start { source });
        tracing::info!(
            target: "omnix::runtime",
            complete = result.is_ok(),
            "omnix runtime shut down"
        );
        result
    }
}
