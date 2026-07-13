//! Public streaming run handle.

use omnix_runtime::Run;

use crate::event::AgentEvent;

/// A single agent run (one turn). Yields [`AgentEvent`]s in order until a
/// terminal event or the stream closes.
///
/// ```no_run
/// # async fn drive(mut run: omnix_sdk::AgentRun) -> Result<(), omnix_sdk::OmnixError> {
/// while let Some(event) = run.next().await {
///     match event {
///         omnix_sdk::AgentEvent::MessageDelta { delta, .. } => print!("{delta}"),
///         omnix_sdk::AgentEvent::Completed(_) => break,
///         _ => {}
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct AgentRun {
    inner: Run,
}

impl AgentRun {
    pub(crate) fn new(inner: Run) -> Self {
        Self { inner }
    }

    /// The server-assigned turn id for this run.
    pub fn turn_id(&self) -> &str {
        self.inner.turn_id()
    }

    /// Await the next event, or `None` once the run is finished.
    pub async fn next(&mut self) -> Option<AgentEvent> {
        self.inner.next().await.map(AgentEvent::from)
    }
}
