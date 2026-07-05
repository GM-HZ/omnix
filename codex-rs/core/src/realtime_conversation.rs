use std::sync::Arc;
use crate::session::session::Session;
use codex_protocol::protocol::{ConversationAudioParams, ConversationSpeechParams, ConversationStartParams, ConversationTextParams};

pub(crate) struct RealtimeConversationManager;
impl RealtimeConversationManager {
    pub(crate) fn new() -> Self { Self }
    pub(crate) fn clear_active_handoff(&self) {}
    pub(crate) fn handoff_complete(&self) {}
    pub(crate) fn handoff_out(&self) {}
    pub(crate) fn running_state(&self) {}
    pub(crate) async fn shutdown(&self) {}
    pub(crate) async fn audio_in(&self, _f: codex_api::RealtimeAudioFrame) -> Result<(), codex_protocol::error::CodexErr> { Ok(()) }
}

pub(crate) async fn handle_start(_s: &Arc<Session>, _i: String, _p: ConversationStartParams) -> Result<(), codex_protocol::error::CodexErr> { Err(codex_protocol::error::CodexErr::UnsupportedOperation("realtime removed".into())) }
pub(crate) async fn handle_audio(_s: &Arc<Session>, _i: String, _p: ConversationAudioParams) {}
pub(crate) async fn handle_text(_s: &Arc<Session>, _i: String, _p: ConversationTextParams) {}
pub(crate) async fn handle_speech(_s: &Arc<Session>, _i: String, _p: ConversationSpeechParams) {}
pub(crate) async fn handle_close(_s: &Arc<Session>, _i: String) {}
