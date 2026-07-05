use std::sync::Arc;
use codex_protocol::account::PlanType as AccountPlanType;
use codex_protocol::protocol::SessionSource;
use thiserror::Error;
use crate::outbound_proxy::AuthRouteConfig;
use super::storage::AgentIdentityAuthRecord;

pub struct AgentIdentityKey<'a> { pub agent_runtime_id: &'a str, pub private_key_pkcs8_base64: &'a str }
#[derive(Debug, Clone)] pub enum ChatGptEnvironment { Production, Staging }
impl Default for ChatGptEnvironment { fn default() -> Self { Self::Production } }
impl ChatGptEnvironment {
    pub fn from_chatgpt_base_url(_url: &str) -> Result<Self, std::io::Error> { Ok(Self::Production) }
    pub fn agent_identity_authapi_base_url(&self) -> &str { "https://auth.openai.com" }
    pub fn chatgpt_base_url(&self) -> &str { "https://chatgpt.com" }
}
#[derive(Debug, Clone)] pub struct AgentIdentityAuth { record: Arc<AgentIdentityAuthRecord> }
impl AgentIdentityAuth {
    pub fn record(&self) -> &AgentIdentityAuthRecord { &self.record }
    pub fn account_id(&self) -> &str { &self.record.account_id }
    pub fn chatgpt_user_id(&self) -> &str { &self.record.chatgpt_user_id }
    pub fn email(&self) -> Option<&str> { self.record.email.as_deref() }
    pub fn is_fedramp_account(&self) -> bool { self.record.chatgpt_account_is_fedramp }
    pub fn plan_type(&self) -> AccountPlanType { self.record.plan_type }
    pub fn run_task_id(&self) -> String { String::new() }
    pub async fn from_jwt(_jwt: &str, _base_url: &str, _authapi_url: &str, _route: Option<&AuthRouteConfig>) -> Result<Self, AgentIdentityAuthError> { Err(AgentIdentityAuthError::Unsupported) }
    pub async fn from_record(_r: AgentIdentityAuthRecord, _authapi_url: &str, _route: Option<&AuthRouteConfig>) -> Result<Self, AgentIdentityAuthError> { Err(AgentIdentityAuthError::Unsupported) }
}
#[derive(Debug, Clone, Error)] pub enum AgentIdentityAuthError {
    #[error("unsupported")] Unsupported,
    #[error("bootstrap unavailable: {operation} ({attempts}): {message}")] BootstrapUnavailable { operation: String, attempts: usize, message: String },
}
impl AgentIdentityAuthError {
    pub fn bootstrap_unavailable(error: &std::io::Error) -> Option<&Self> { None }
    pub fn is_retryable(&self) -> bool { false }
    pub fn cloned(&self) -> Self { self.clone() }
}
impl From<AgentIdentityAuthError> for std::io::Error { fn from(e: AgentIdentityAuthError) -> Self { std::io::Error::other(e) } }
impl IntoIterator for AgentIdentityAuthError { type Item = AgentIdentityAuthError; type IntoIter = std::vec::IntoIter<AgentIdentityAuthError>; fn into_iter(self) -> Self::IntoIter { vec![self].into_iter() } }
#[derive(Debug, Clone)] pub struct ManagedChatGptAgentIdentityBinding {
    pub account_id: String, pub chatgpt_user_id: String, pub email: Option<String>,
    pub plan_type: AccountPlanType, pub chatgpt_account_is_fedramp: bool, pub access_token: String,
}
pub(super) const MAX_AGENT_IDENTITY_BOOTSTRAP_ATTEMPTS: usize = 3;
pub(super) fn agent_identity_authapi_base_url(_url: Option<&str>) -> std::io::Result<String> { Err(std::io::Error::other("removed")) }
pub(super) fn require_agent_identity_authapi_base_url(_url: Option<&str>) -> std::io::Result<&str> { Err(std::io::Error::other("removed")) }
pub fn classify_bootstrap_error(_op: &str, _err: AgentIdentityAuthError) -> std::io::Error { std::io::Error::other("removed") }
pub fn record_matches_managed_chatgpt_binding(_r: &AgentIdentityAuthRecord, _b: &ManagedChatGptAgentIdentityBinding) -> bool { false }
pub fn record_needs_task_registration(_r: &AgentIdentityAuthRecord) -> bool { false }
pub async fn register_managed_chatgpt_agent_identity(_b: ManagedChatGptAgentIdentityBinding, _authapi_url: &str, _source: SessionSource, _route: Option<&AuthRouteConfig>) -> Result<AgentIdentityAuth, AgentIdentityAuthError> { Err(AgentIdentityAuthError::Unsupported) }
pub async fn verified_record_from_jwt(_jwt: &str, _authapi_url: &str, _route: Option<&AuthRouteConfig>) -> Result<AgentIdentityAuthRecord, AgentIdentityAuthError> { Err(AgentIdentityAuthError::Unsupported) }
pub fn agent_identity_jwks_url(_base_url: &str) -> String { String::new() }
pub fn agent_registration_url(_base_url: &str) -> String { String::new() }
pub fn agent_task_registration_url(_base_url: &str, _agent_runtime_id: &str) -> String { String::new() }
pub fn build_abom(_source: SessionSource, _agent_runtime_id: &str, _task_id: &str) -> serde_json::Value { serde_json::Value::Null }
pub fn decode_agent_identity_jwt(_jwt: &str, _jwks: Option<&str>) -> Result<AgentIdentityJwtClaims, std::io::Error> { Err(std::io::Error::other("removed")) }
pub async fn fetch_agent_identity_jwks(_base_url: &str) -> Result<serde_json::Value, std::io::Error> { Err(std::io::Error::other("removed")) }
pub fn generate_agent_key_material() -> Result<AgentIdentityKeyMaterial, std::io::Error> { Err(std::io::Error::other("removed")) }
pub struct AgentIdentityKeyMaterial { pub private_key_pkcs8_base64: String, pub public_key_ssh: String }
pub fn is_retryable_registration_error(_err: &str) -> bool { false }
pub fn public_key_ssh_from_private_key_pkcs8_base64(_private_key: &str) -> Result<String, std::io::Error> { Err(std::io::Error::other("removed")) }
pub async fn register_agent_identity(_key: &AgentIdentityKey<'_>, _base_url: &str, _source: SessionSource) -> Result<String, std::io::Error> { Err(std::io::Error::other("removed")) }
pub async fn register_agent_task(_key: &AgentIdentityKey<'_>, _base_url: &str, _agent_runtime_id: &str, _task_id: &str) -> Result<(), std::io::Error> { Err(std::io::Error::other("removed")) }
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentIdentityJwtClaims { pub agent_runtime_id: String, pub agent_private_key: String, pub account_id: String, pub chatgpt_user_id: String, pub email: Option<String>, pub plan_type: AccountPlanType, pub chatgpt_account_is_fedramp: bool }
