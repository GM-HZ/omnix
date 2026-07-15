//! Public session API.
//!
//! A [`SessionHandle`] maps to one persistent agent thread. `Sessions` is the
//! factory obtained from [`crate::OmnixRuntime::sessions`].

use omnix_runtime::Runtime;
use omnix_runtime::Session;
use omnix_runtime::SessionConfig as RuntimeSessionConfig;

pub use omnix_runtime::SessionMetadata;

use crate::error::OmnixError;
use crate::error_map::map_runtime_error;
use crate::run::AgentRun;
use crate::run::RunConfig;

/// Per-session creation options reserved for future additive settings.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct SessionConfig {}

impl SessionConfig {
    fn into_runtime(self) -> RuntimeSessionConfig {
        RuntimeSessionConfig::default()
    }
}

/// Factory for creating and resuming sessions. Borrows the runtime.
pub struct Sessions<'a> {
    runtime: &'a Runtime,
}

impl<'a> Sessions<'a> {
    pub(crate) fn new(runtime: &'a Runtime) -> Self {
        Self { runtime }
    }

    /// Create a new session (a new thread).
    pub async fn create(&self, config: SessionConfig) -> Result<SessionHandle, OmnixError> {
        let session = self
            .runtime
            .create_session(config.into_runtime())
            .await
            .map_err(map_runtime_error)?;
        Ok(SessionHandle::new(session))
    }

    /// Resume an existing session by id.
    pub async fn resume(&self, session_id: impl Into<String>) -> Result<SessionHandle, OmnixError> {
        let session = self
            .runtime
            .resume_session(session_id.into())
            .await
            .map_err(map_runtime_error)?;
        Ok(SessionHandle::new(session))
    }
}

/// A live session bound to one thread.
pub struct SessionHandle {
    inner: Session,
}

impl SessionHandle {
    fn new(inner: Session) -> Self {
        Self { inner }
    }

    /// The session id (thread id), stable across restarts for [`Sessions::resume`].
    pub fn id(&self) -> &str {
        self.inner.id()
    }

    /// Non-sensitive metadata for this session (id, model).
    pub fn metadata(&self) -> SessionMetadata {
        self.inner.metadata()
    }

    /// Start a run with plain text input, returning the streaming [`AgentRun`].
    pub async fn run(&mut self, input: impl Into<String>) -> Result<AgentRun, OmnixError> {
        let run = self.inner.run(input).await.map_err(map_runtime_error)?;
        Ok(AgentRun::new(run))
    }

    /// Start a run with explicit generation options.
    pub async fn run_with_config(
        &mut self,
        input: impl Into<String>,
        config: RunConfig,
    ) -> Result<AgentRun, OmnixError> {
        let config = config.into_runtime()?;
        let run = self
            .inner
            .run_with_config(input, config)
            .await
            .map_err(map_runtime_error)?;
        Ok(AgentRun::new(run))
    }

    /// Interrupt the active run, if any.
    pub async fn interrupt(&self) -> Result<(), OmnixError> {
        self.inner.interrupt().await.map_err(map_runtime_error)
    }

    /// Request context compaction.
    pub async fn compact(&self) -> Result<(), OmnixError> {
        self.inner.compact().await.map_err(map_runtime_error)
    }

    /// Archive this session.
    pub async fn archive(&self) -> Result<(), OmnixError> {
        self.inner.archive().await.map_err(map_runtime_error)
    }
}
