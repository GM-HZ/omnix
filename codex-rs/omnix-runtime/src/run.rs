//! A single run (one turn) over a session's thread.
//!
//! `Run` wraps the receiving end of the per-run event channel populated by the
//! background consumer task. It yields mapped [`AgentEvent`]s in order until a
//! terminal event (`Completed`/`Failed`) or the channel closes.
//!
//! A run holds an "active" guard shared with its `Session` so the session can
//! enforce one active run at a time (§12); the guard clears when the run reaches
//! a terminal event or is dropped.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;
use std::time::Instant;

use tokio::sync::mpsc;

use codex_app_server_client::InProcessAppServerRequestHandle;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::TurnInterruptParams;

use crate::events::AgentEvent;
use crate::events::consumer::ConsumerCommand;
use crate::session::next_request_id;

/// Streaming handle for one turn's events.
pub struct Run {
    turn_id: String,
    event_rx: mpsc::Receiver<AgentEvent>,
    finished: bool,
    control: RunControl,
    started_at: Instant,
    first_token_logged: bool,
}

pub(crate) struct RunControl {
    pub active: Arc<AtomicBool>,
    pub active_turn_id: Arc<Mutex<Option<String>>>,
    pub thread_id: String,
    pub consumer_tx: mpsc::Sender<ConsumerCommand>,
    pub request_handle: InProcessAppServerRequestHandle,
    pub request_ids: Arc<AtomicI64>,
}

impl Run {
    pub(crate) fn new(
        turn_id: String,
        event_rx: mpsc::Receiver<AgentEvent>,
        control: RunControl,
    ) -> Self {
        Self {
            turn_id,
            event_rx,
            finished: false,
            control,
            started_at: Instant::now(),
            first_token_logged: false,
        }
    }

    /// The server-assigned turn id for this run.
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    /// Await the next event. Returns `None` once the run has produced a terminal
    /// event or the runtime closed the stream.
    pub async fn next(&mut self) -> Option<AgentEvent> {
        if self.finished {
            return None;
        }
        match self.event_rx.recv().await {
            Some(event) => {
                self.observe(&event);
                if event.is_terminal() {
                    self.mark_finished();
                }
                Some(event)
            }
            None => {
                self.mark_finished();
                None
            }
        }
    }

    /// Emit observability signals for `event` (§13): first-token latency, run
    /// duration + usage on completion.
    fn observe(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::MessageDelta { .. } if !self.first_token_logged => {
                self.first_token_logged = true;
                tracing::debug!(
                    target: "omnix::run",
                    turn_id = %self.turn_id,
                    first_token_ms = self.started_at.elapsed().as_millis() as u64,
                    "first token"
                );
            }
            AgentEvent::Usage(usage) => {
                tracing::debug!(
                    target: "omnix::run",
                    turn_id = %self.turn_id,
                    input_tokens = usage.input_tokens,
                    cached_input_tokens = usage.cached_input_tokens,
                    output_tokens = usage.output_tokens,
                    "token usage"
                );
            }
            AgentEvent::CompactCompleted => {
                tracing::info!(target: "omnix::run", turn_id = %self.turn_id, "context compacted");
            }
            AgentEvent::Completed(result) => {
                tracing::info!(
                    target: "omnix::run",
                    turn_id = %self.turn_id,
                    status = ?result.status,
                    duration_ms = self.started_at.elapsed().as_millis() as u64,
                    "run completed"
                );
            }
            AgentEvent::Failed(failure) => {
                tracing::warn!(
                    target: "omnix::run",
                    turn_id = %self.turn_id,
                    duration_ms = self.started_at.elapsed().as_millis() as u64,
                    error = %failure.message,
                    "run failed"
                );
            }
            _ => {}
        }
    }

    fn mark_finished(&mut self) {
        self.finished = true;
        *self
            .control
            .active_turn_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.control.active.store(false, Ordering::Release);
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if self.finished || !self.control.active.load(Ordering::Acquire) {
            return;
        }

        if self
            .control
            .consumer_tx
            .try_send(ConsumerCommand::AbandonRun {
                thread_id: self.control.thread_id.clone(),
                turn_id: self.turn_id.clone(),
                active: Arc::clone(&self.control.active),
                active_turn_id: Arc::clone(&self.control.active_turn_id),
            })
            .is_err()
        {
            *self
                .control
                .active_turn_id
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            self.control.active.store(false, Ordering::Release);
            return;
        }

        let request_handle = self.control.request_handle.clone();
        let request_ids = Arc::clone(&self.control.request_ids);
        let active = Arc::clone(&self.control.active);
        let thread_id = self.control.thread_id.clone();
        let turn_id = self.turn_id.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    let mut last_error = None;
                    for attempt in 0..5 {
                        let result: Result<codex_app_server_protocol::TurnInterruptResponse, _> = request_handle
                            .request_typed(ClientRequest::TurnInterrupt {
                                request_id: next_request_id(&request_ids),
                                params: TurnInterruptParams {
                                    thread_id: thread_id.clone(),
                                    turn_id: turn_id.clone(),
                                },
                            })
                            .await;
                        match result {
                            Ok(_) => return,
                            Err(error) => last_error = Some(error),
                        }
                        if !active.load(Ordering::Acquire) {
                            return;
                        }
                        if attempt < 4 {
                            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                        }
                    }
                    if let Some(error) = last_error {
                        tracing::warn!(target: "omnix::run", %error, "failed to interrupt dropped run after retries");
                    }
                });
            }
            Err(_) => {
                tracing::warn!(target: "omnix::run", "no async runtime available to interrupt dropped run");
            }
        }
    }
}
