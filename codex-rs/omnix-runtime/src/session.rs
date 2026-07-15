//! Session orchestration: create/resume a thread, and start runs on it.
//!
//! A `Session` maps 1:1 to an app-server thread. It borrows the runtime's
//! cloneable request handle and the consumer command channel; a run registers
//! its event sink with the consumer before issuing `turn/start`.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;

use codex_app_server_client::InProcessAppServerRequestHandle;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::DynamicToolFunctionSpec;
use codex_app_server_protocol::DynamicToolSpec;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadCompactStartParams;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadSource;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::error::RuntimeError;
use crate::events::AgentEvent;
use crate::events::consumer::ConsumerCommand;
use crate::events::consumer::RunSink;
use crate::run::Run;
use crate::run::RunControl;
use crate::runtime::SessionDefaults;
use crate::spec::OMNIX_PROVIDER_ID;
use crate::tools::ToolDescriptor;

/// Per-session creation options reserved for future additive settings.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct SessionConfig {}

/// Per-run options lowered into one app-server turn.
#[derive(Debug, Default, Clone)]
pub struct RunConfig {
    /// Optional JSON Schema used to guide the final assistant message.
    pub output_schema: Option<serde_json::Value>,
}

/// A live agent session bound to one thread.
pub struct Session {
    request_handle: InProcessAppServerRequestHandle,
    consumer_tx: mpsc::Sender<ConsumerCommand>,
    thread_id: String,
    model: String,
    request_ids: Arc<AtomicI64>,
    active_turn_id: Arc<Mutex<Option<String>>>,
    /// True while a run is in flight. Enforces one active run per session (§12).
    active_run: Arc<AtomicBool>,
}

/// Non-sensitive session metadata (§8.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetadata {
    /// The session (thread) id.
    pub id: String,
    /// The model bound to this session.
    pub model: String,
}

impl Session {
    /// Create a new session (starts a thread).
    ///
    /// `request_ids` is the global, connection-scoped JSON-RPC request-id
    /// counter shared across all sessions. The in-process app-server
    /// correlates request→response by ID on a single connection-wide map,
    /// so per-session counters would collide (review finding #3).
    pub(crate) async fn create(
        request_handle: InProcessAppServerRequestHandle,
        consumer_tx: mpsc::Sender<ConsumerCommand>,
        tool_descriptors: Vec<ToolDescriptor>,
        defaults: SessionDefaults,
        request_ids: Arc<AtomicI64>,
        _config: SessionConfig,
    ) -> Result<Self, RuntimeError> {
        let dynamic_tools = dynamic_tool_specs(&tool_descriptors);
        let model = defaults.model.clone();
        // Per-session overrides fall back to the runtime defaults. Model and
        // instructions MUST be set on the thread params: the app-server
        // re-resolves config on thread/start and does not carry our harness
        // overrides, but it honors these per-thread fields.
        let params = ThreadStartParams {
            model: Some(model.clone()),
            // Pin the thread to the in-memory Omnix provider. Without this the
            // app-server falls back to its own default provider resolution
            // (e.g. local ollama on the Responses API) instead of the DeepSeek
            // Chat Completions provider we injected into the Config.
            model_provider: Some(OMNIX_PROVIDER_ID.to_string()),
            base_instructions: defaults.base_instructions.clone(),
            developer_instructions: defaults.developer_instructions.clone(),
            thread_source: Some(ThreadSource::User),
            dynamic_tools,
            ..ThreadStartParams::default()
        };
        let response: ThreadStartResponse = request_handle
            .request_typed(ClientRequest::ThreadStart {
                request_id: next_request_id(&request_ids),
                params,
            })
            .await?;

        tracing::info!(
            target: "omnix::session",
            session_id = %response.thread.id,
            model = %model,
            tools = tool_descriptors.len(),
            "session created"
        );

        Ok(Self {
            request_handle,
            consumer_tx,
            thread_id: response.thread.id,
            model,
            request_ids,
            active_turn_id: Arc::new(Mutex::new(None)),
            active_run: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Resume an existing session by thread id.
    ///
    /// Dynamic tools are persisted with the thread at creation time, so they are
    /// not re-sent here (`ThreadResumeParams` has no `dynamic_tools` field); the
    /// descriptors are accepted for API symmetry.
    pub(crate) async fn resume(
        request_handle: InProcessAppServerRequestHandle,
        consumer_tx: mpsc::Sender<ConsumerCommand>,
        _tool_descriptors: Vec<ToolDescriptor>,
        defaults: SessionDefaults,
        request_ids: Arc<AtomicI64>,
        thread_id: String,
    ) -> Result<Self, RuntimeError> {
        let params = ThreadResumeParams {
            thread_id: thread_id.clone(),
            model: Some(defaults.model),
            model_provider: Some(OMNIX_PROVIDER_ID.to_string()),
            base_instructions: defaults.base_instructions,
            developer_instructions: defaults.developer_instructions,
            ..ThreadResumeParams::default()
        };
        let response: ThreadResumeResponse = request_handle
            .request_typed(ClientRequest::ThreadResume {
                request_id: next_request_id(&request_ids),
                params,
            })
            .await?;

        tracing::info!(
            target: "omnix::session",
            session_id = %response.thread.id,
            "session resumed"
        );

        Ok(Self {
            request_handle,
            consumer_tx,
            thread_id: response.thread.id,
            model: response.model,
            request_ids,
            active_turn_id: Arc::new(Mutex::new(None)),
            active_run: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The session id (thread id), stable across restarts for resume.
    pub fn id(&self) -> &str {
        &self.thread_id
    }

    /// Non-sensitive metadata for this session (§8.2).
    pub fn metadata(&self) -> SessionMetadata {
        SessionMetadata {
            id: self.thread_id.clone(),
            model: self.model.clone(),
        }
    }

    /// Start a run (one turn) with plain text input. Registers the event sink
    /// with the consumer BEFORE issuing `turn/start` so no early notification is
    /// missed, then returns a [`Run`] streaming the mapped events.
    ///
    /// Rejects a second concurrent run on the same session (§12): only one run
    /// may be active at a time. The guard clears when the previous run reaches a
    /// terminal event or is dropped.
    pub async fn run(&mut self, input: impl Into<String>) -> Result<Run, RuntimeError> {
        self.run_with_config(input, RunConfig::default()).await
    }

    /// Start a run with explicit per-turn options.
    pub async fn run_with_config(
        &mut self,
        input: impl Into<String>,
        config: RunConfig,
    ) -> Result<Run, RuntimeError> {
        // Claim the single-active-run slot. `swap` returns the previous value;
        // if it was already `true`, another run is in flight.
        if self.active_run.swap(true, Ordering::AcqRel) {
            return Err(RuntimeError::RunAlreadyActive);
        }

        let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);
        let (registered_tx, registered_rx) = oneshot::channel();
        if self
            .consumer_tx
            .send(ConsumerCommand::RegisterRun {
                sink: RunSink {
                    thread_id: self.thread_id.clone(),
                    sender: event_tx,
                    active: Arc::clone(&self.active_run),
                    active_turn_id: Arc::clone(&self.active_turn_id),
                },
                registered: registered_tx,
            })
            .await
            .is_err()
        {
            self.active_run.store(false, Ordering::Release);
            return Err(RuntimeError::Unavailable);
        }
        if registered_rx.await.is_err() {
            self.active_run.store(false, Ordering::Release);
            return Err(RuntimeError::Unavailable);
        }

        let params = TurnStartParams {
            thread_id: self.thread_id.clone(),
            input: vec![UserInput::Text {
                text: input.into(),
                text_elements: Vec::new(),
            }],
            output_schema: config.output_schema,
            ..TurnStartParams::default()
        };
        let response: TurnStartResponse = match self
            .request_handle
            .request_typed(ClientRequest::TurnStart {
                request_id: next_request_id(&self.request_ids),
                params,
            })
            .await
        {
            Ok(response) => response,
            Err(err) => {
                let (unregistered_tx, unregistered_rx) = oneshot::channel();
                let _ = self
                    .consumer_tx
                    .send(ConsumerCommand::UnregisterRun {
                        thread_id: self.thread_id.clone(),
                        unregistered: Some(unregistered_tx),
                    })
                    .await;
                let _ = unregistered_rx.await;
                self.active_run.store(false, Ordering::Release);
                return Err(err.into());
            }
        };

        let (bound_tx, bound_rx) = oneshot::channel();
        self.consumer_tx
            .send(ConsumerCommand::BindRun {
                thread_id: self.thread_id.clone(),
                turn_id: response.turn.id.clone(),
                bound: bound_tx,
            })
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        bound_rx.await.map_err(|_| RuntimeError::Unavailable)?;

        *self
            .active_turn_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(response.turn.id.clone());
        tracing::debug!(
            target: "omnix::run",
            session_id = %self.thread_id,
            turn_id = %response.turn.id,
            "run started"
        );
        Ok(Run::new(
            response.turn.id,
            event_rx,
            RunControl {
                active: Arc::clone(&self.active_run),
                active_turn_id: Arc::clone(&self.active_turn_id),
                thread_id: self.thread_id.clone(),
                consumer_tx: self.consumer_tx.clone(),
                request_handle: self.request_handle.clone(),
                request_ids: Arc::clone(&self.request_ids),
            },
        ))
    }

    /// Interrupt the active run, if any.
    pub async fn interrupt(&self) -> Result<(), RuntimeError> {
        let Some(turn_id) = self
            .active_turn_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        else {
            return Ok(());
        };
        let _: codex_app_server_protocol::TurnInterruptResponse = self
            .request_handle
            .request_typed(ClientRequest::TurnInterrupt {
                request_id: next_request_id(&self.request_ids),
                params: TurnInterruptParams {
                    thread_id: self.thread_id.clone(),
                    turn_id,
                },
            })
            .await?;
        Ok(())
    }

    /// Request context compaction for this session.
    pub async fn compact(&self) -> Result<(), RuntimeError> {
        let _: codex_app_server_protocol::ThreadCompactStartResponse = self
            .request_handle
            .request_typed(ClientRequest::ThreadCompactStart {
                request_id: next_request_id(&self.request_ids),
                params: ThreadCompactStartParams {
                    thread_id: self.thread_id.clone(),
                },
            })
            .await?;
        Ok(())
    }

    /// Archive this session.
    pub async fn archive(&self) -> Result<(), RuntimeError> {
        let _: codex_app_server_protocol::ThreadArchiveResponse = self
            .request_handle
            .request_typed(ClientRequest::ThreadArchive {
                request_id: next_request_id(&self.request_ids),
                params: ThreadArchiveParams {
                    thread_id: self.thread_id.clone(),
                },
            })
            .await?;
        Ok(())
    }
}

/// Allocate the next monotonically-increasing JSON-RPC request id.
pub(crate) fn next_request_id(counter: &AtomicI64) -> RequestId {
    RequestId::Integer(counter.fetch_add(1, Ordering::Relaxed))
}

/// Convert runtime tool descriptors into thread-start dynamic tool specs.
/// Returns `None` when there are no tools so the field is omitted on the wire.
fn dynamic_tool_specs(descriptors: &[ToolDescriptor]) -> Option<Vec<DynamicToolSpec>> {
    if descriptors.is_empty() {
        return None;
    }
    Some(
        descriptors
            .iter()
            .map(|d| {
                DynamicToolSpec::Function(DynamicToolFunctionSpec {
                    name: d.name.clone(),
                    description: d.description.clone(),
                    input_schema: d.input_schema.clone(),
                    defer_loading: false,
                })
            })
            .collect(),
    )
}
