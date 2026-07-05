use std::sync::Arc;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
pub(crate) async fn run_remote_compact_task(_s: Arc<Session>, _c: Arc<TurnContext>) -> Result<(), std::io::Error> { Err(std::io::Error::other("remote compact removed")) }
