//! Public, strongly-typed runtime configuration.
//!
//! `RuntimeConfig` is the SDK's configuration contract (design §7). It is
//! validated and lowered into an `omnix_runtime::RuntimeSpec` at build time. No
//! `config.toml` is involved at startup.

mod model;
mod subconfigs;

use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

pub use model::Credentials;
pub use model::DEFAULT_BASE_URL;
pub use model::DEFAULT_MODEL;
pub use model::ModelConfig;
pub use model::WireApi;
pub use subconfigs::ApprovalPolicy;
pub use subconfigs::ContextConfig;
pub use subconfigs::ObservabilityConfig;
pub use subconfigs::PermissionConfig;
pub use subconfigs::PersistenceConfig;
pub use subconfigs::PluginConfig;
pub use subconfigs::SandboxPolicy;
pub use subconfigs::SkillConfig;
pub use subconfigs::ToolConfig;

use crate::error::OmnixError;
use crate::error::OmnixErrorKind;

/// How the host data root is interpreted (design §6).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "root")]
pub enum RuntimeScope {
    /// Host application data root; `.omnix` lives beneath it, root is the workspace.
    Application(PathBuf),
    /// Conventional project directory; `.omnix` at the project root.
    Project(PathBuf),
}

impl RuntimeScope {
    fn to_runtime(&self) -> omnix_runtime::RuntimeScope {
        match self {
            RuntimeScope::Application(root) => {
                omnix_runtime::RuntimeScope::Application { root: root.clone() }
            }
            RuntimeScope::Project(root) => {
                omnix_runtime::RuntimeScope::Project { root: root.clone() }
            }
        }
    }
}

/// The full runtime configuration contract.
///
/// Constructed via [`crate::Omnix::builder`] (or [`RuntimeConfig::from_json`] /
/// [`RuntimeConfig::from_value`] for host-side persistence). The `scope` field
/// has no default and must be provided.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub scope: RuntimeScope,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub permissions: PermissionConfig,
    #[serde(default)]
    pub tools: ToolConfig,
    #[serde(default)]
    pub skills: SkillConfig,
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default)]
    pub persistence: PersistenceConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

impl RuntimeConfig {
    /// Deserialize from a JSON string (for host-side config persistence).
    pub fn from_json(json: &str) -> Result<Self, OmnixError> {
        serde_json::from_str(json)
            .map_err(|e| OmnixError::new(OmnixErrorKind::InvalidConfig, e.to_string()))
    }

    /// Deserialize from a JSON value.
    pub fn from_value(value: serde_json::Value) -> Result<Self, OmnixError> {
        serde_json::from_value(value)
            .map_err(|e| OmnixError::new(OmnixErrorKind::InvalidConfig, e.to_string()))
    }

    /// Validate the context-window policy and other invariants.
    pub(crate) fn validate(&self) -> Result<(), OmnixError> {
        let ctx = &self.context;
        if !(ctx.auto_compact_tokens <= ctx.effective_guardrail_tokens
            && ctx.effective_guardrail_tokens <= ctx.model_context_tokens)
        {
            return Err(OmnixError::new(
                OmnixErrorKind::InvalidConfig,
                "context policy must satisfy auto_compact <= effective_guardrail <= model_context",
            ));
        }
        if self.model.model.trim().is_empty() {
            return Err(OmnixError::new(
                OmnixErrorKind::InvalidConfig,
                "model must not be empty",
            ));
        }
        if self.model.base_url.trim().is_empty() {
            return Err(OmnixError::new(
                OmnixErrorKind::InvalidConfig,
                "provider base_url must not be empty",
            ));
        }
        Ok(())
    }

    /// Lower into the internal runtime spec. `credentials`, pack-derived
    /// instructions, and the tool invoker are supplied by the builder.
    pub(crate) fn into_spec(
        self,
        credentials: &Credentials,
        base_instructions: Option<String>,
        developer_instructions: Option<String>,
        tool_invoker: Option<std::sync::Arc<dyn omnix_runtime::ToolInvoker>>,
    ) -> omnix_runtime::RuntimeSpec {
        omnix_runtime::RuntimeSpec {
            scope: self.scope.to_runtime(),
            model: omnix_runtime::ModelSpec {
                model: self.model.model,
                base_url: self.model.base_url,
                wire_api: match self.model.wire_api {
                    WireApi::ChatCompletions => omnix_runtime::WireApi::ChatCompletions,
                    WireApi::Responses => omnix_runtime::WireApi::Responses,
                },
                api_key: credentials.api_key().to_string(),
            },
            context: omnix_runtime::ContextSpec {
                model_context_tokens: self.context.model_context_tokens,
                effective_guardrail_tokens: self.context.effective_guardrail_tokens,
                auto_compact_tokens: self.context.auto_compact_tokens,
            },
            permissions: omnix_runtime::PermissionSpec {
                approval_policy: match self.permissions.approval_policy {
                    ApprovalPolicy::Never => omnix_runtime::AskForApproval::Never,
                    ApprovalPolicy::OnRequest => omnix_runtime::AskForApproval::OnRequest,
                    ApprovalPolicy::UnlessTrusted => omnix_runtime::AskForApproval::UnlessTrusted,
                },
                sandbox_mode: match self.permissions.sandbox_policy {
                    SandboxPolicy::ReadOnly => omnix_runtime::SandboxMode::ReadOnly,
                    SandboxPolicy::WorkspaceWrite => omnix_runtime::SandboxMode::WorkspaceWrite,
                    SandboxPolicy::DangerFullAccess => omnix_runtime::SandboxMode::DangerFullAccess,
                },
            },
            tools: omnix_runtime::ToolSpec {
                call_timeout: self.tools.call_timeout(),
                max_concurrency: self.tools.max_concurrency,
            },
            base_instructions,
            developer_instructions,
            tool_invoker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_config() -> RuntimeConfig {
        default_config_with_scope(RuntimeScope::Application("/tmp/app".into()))
    }

    fn default_config_with_scope(scope: RuntimeScope) -> RuntimeConfig {
        RuntimeConfig {
            scope,
            model: Default::default(),
            context: Default::default(),
            permissions: Default::default(),
            tools: Default::default(),
            skills: Default::default(),
            plugins: Default::default(),
            persistence: Default::default(),
            observability: Default::default(),
        }
    }

    #[test]
    fn defaults_match_runtime_0_0_contract() {
        let c = app_config();
        assert_eq!(c.model.model, DEFAULT_MODEL);
        assert_eq!(c.model.base_url, DEFAULT_BASE_URL);
        assert_eq!(c.context.model_context_tokens, 1_000_000);
        assert_eq!(c.context.effective_guardrail_tokens, 950_000);
        assert_eq!(c.context.auto_compact_tokens, 850_000);
        assert!(c.persistence.enabled);
        c.validate().expect("default config is valid");
    }

    #[test]
    fn rejects_inverted_context_policy() {
        let mut c = app_config();
        c.context.auto_compact_tokens = 999_999; // > effective guardrail
        let err = c.validate().expect_err("must reject");
        assert_eq!(err.kind(), OmnixErrorKind::InvalidConfig);
    }

    #[test]
    fn rejects_empty_model() {
        let mut c = app_config();
        c.model.model = "   ".to_string();
        assert_eq!(
            c.validate().expect_err("must reject").kind(),
            OmnixErrorKind::InvalidConfig
        );
    }

    #[test]
    fn from_json_roundtrips_scope_and_defaults() {
        let json = r#"{"scope":{"kind":"application","root":"/data/app"}}"#;
        let c = RuntimeConfig::from_json(json).expect("parse");
        match c.scope {
            RuntimeScope::Application(ref p) => assert_eq!(p.to_str(), Some("/data/app")),
            _ => panic!("expected application scope"),
        }
        // Omitted sections fall back to defaults.
        assert_eq!(c.model.model, DEFAULT_MODEL);
    }

    #[test]
    fn into_spec_carries_credentials_and_provider_selection() {
        let c = app_config();
        let creds = Credentials::from_api_key("sk-secret");
        let spec = c.into_spec(&creds, Some("sys".into()), None, None);
        assert_eq!(spec.model.api_key, "sk-secret");
        assert_eq!(spec.base_instructions.as_deref(), Some("sys"));
        assert!(matches!(
            spec.model.wire_api,
            omnix_runtime::WireApi::ChatCompletions
        ));
    }
}
