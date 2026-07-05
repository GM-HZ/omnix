use std::sync::Arc;
use crate::session::session::Session;
use codex_protocol::protocol::{ConversationAudioParams, ConversationSpeechParams, ConversationStartParams, ConversationTextParams};
use codex_protocol::error::CodexErr;

type CodexResult<T> = Result<T, CodexErr>;

pub(crate) struct RealtimeConversationManager;
impl RealtimeConversationManager {
    pub(crate) fn new() -> Self { Self }
    pub(crate) async fn clear_active_handoff(&self) {}
    pub(crate) async fn handoff_complete(&self) -> CodexResult<()> { Ok(()) }
    pub(crate) async fn handoff_out(&self, _text: String, _phase: Option<codex_protocol::models::MessagePhase>) -> CodexResult<()> { Ok(()) }
    pub(crate) async fn running_state(&self) -> Option<()> { None }
    pub(crate) async fn shutdown(&self) {}
    pub(crate) async fn audio_in(&self, _f: codex_protocol::protocol::RealtimeAudioFrame) -> CodexResult<()> { Ok(()) }
}

pub(crate) async fn handle_start(
    _sess: &Arc<Session>,
    _sub_id: String,
    _params: ConversationStartParams,
) -> CodexResult<()> { Err(CodexErr::UnsupportedOperation("realtime removed".into())) }

pub(crate) async fn handle_audio(
    _sess: &Arc<Session>,
    _sub_id: String,
    _params: ConversationAudioParams,
) {}

pub(crate) async fn handle_text(
    _sess: &Arc<Session>,
    _sub_id: String,
    _params: ConversationTextParams,
) {}

pub(crate) async fn handle_speech(
    _sess: &Arc<Session>,
    _sub_id: String,
    _params: ConversationSpeechParams,
) {}

pub(crate) async fn handle_close(_sess: &Arc<Session>, _sub_id: String) {}
