//! Stub implementation for Realtime/WebSocket functionality.
//!
//! The Chat Completions API does not support WebSocket-based realtime
//! communication. This module provides stub types so the TUI and connector
//! code that references realtime types can still compile without pulling in
//! the full OpenAI Realtime API dependencies.

use codex_protocol::protocol::RealtimeAudioFrame;
use codex_protocol::protocol::RealtimeEvent;

/// Stub realtime session that does nothing — Chat Completions providers
/// don't support WebSocket realtime.
#[derive(Debug, Default)]
pub struct RealtimeSessionStub {
    _private: (),
}

impl RealtimeSessionStub {
    pub fn new() -> Self {
        Self { _private: () }
    }

    pub fn is_active(&self) -> bool {
        false
    }

    pub fn send_audio(&self, _frame: RealtimeAudioFrame) {
        // no-op: Chat Completions API doesn't support audio
    }

    pub fn send_event(&self, _event: RealtimeEvent) {
        // no-op: Chat Completions API doesn't support realtime events
    }

    pub fn close(&self) {
        // no-op
    }
}

/// Stub realtime connector — always returns "not connected".
#[derive(Debug, Default)]
pub struct RealtimeConnectorStub {
    _private: (),
}

impl RealtimeConnectorStub {
    pub fn new() -> Self {
        Self { _private: () }
    }

    pub fn is_connected(&self) -> bool {
        false
    }

    pub fn connect(&self) -> bool {
        false
    }

    pub fn disconnect(&self) {
        // no-op
    }
}

/// Stub WebSocket connection for realtime audio.
#[derive(Debug, Default)]
pub struct RealtimeWebSocketStub {
    _private: (),
}

impl RealtimeWebSocketStub {
    pub fn new() -> Self {
        Self { _private: () }
    }

    pub fn is_open(&self) -> bool {
        false
    }

    pub fn close(&self) {
        // no-op
    }
}
