//! `Omnix::builder()` — the ergonomic entry point for constructing a runtime.

use std::path::PathBuf;

use omnix_runtime::Runtime;

use crate::config::Credentials;
use crate::config::RuntimeConfig;
use crate::config::RuntimeScope;
use crate::error::OmnixError;
use crate::error::OmnixErrorKind;
use crate::error_map::map_runtime_error;
use crate::instruction::validate_instruction;
use crate::pack::BusinessPack;
use crate::process::EmbeddedProcess;
use crate::runtime::OmnixRuntime;
use crate::tools::ToolRegistry;

/// Fluent builder for an [`OmnixRuntime`].
///
/// Minimal startup requires only a data root and credentials; everything else
/// uses Runtime 0.0 defaults. A base config can be supplied via
/// [`OmnixBuilder::config`] and then refined.
#[derive(Default)]
pub struct OmnixBuilder {
    scope: Option<RuntimeScope>,
    config: Option<RuntimeConfig>,
    credentials: Option<Credentials>,
    base_instructions: Option<String>,
    developer_instructions: Option<String>,
    tools: Option<ToolRegistry>,
    business_pack: Option<BusinessPack>,
    pack_root: Option<PathBuf>,
    process: Option<EmbeddedProcess>,
}

impl OmnixBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Use an application data root (design §6.1): `.omnix` is created beneath
    /// it and the root itself is the agent workspace.
    pub fn application_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.scope = Some(RuntimeScope::Application(root.into()));
        self
    }

    /// Use a conventional project root (design §6.2).
    pub fn project_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.scope = Some(RuntimeScope::Project(root.into()));
        self
    }

    /// Supply a full base config. If both this and a scope-setter are used, the
    /// explicit `scope` setter wins for the scope field.
    pub fn config(mut self, config: RuntimeConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Provider credentials (in-memory API key).
    pub fn credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// System/methodology instructions (typically from a Business Pack).
    pub fn base_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.base_instructions = Some(instructions.into());
        self
    }

    /// Per-runtime developer instructions.
    pub fn developer_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.developer_instructions = Some(instructions.into());
        self
    }

    /// Register host tools advertised to the model.
    pub fn tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Attach a Business Pack. Its bounded instructions are added to the
    /// runtime's developer instructions, preserving the built-in agent/tool
    /// harness. An explicit [`OmnixBuilder::developer_instructions`] value wins.
    pub fn business_pack(mut self, pack: BusinessPack) -> Self {
        self.business_pack = Some(pack);
        self
    }

    /// Root directory for resolving a pack's file-backed assets.
    pub fn pack_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.pack_root = Some(root.into());
        self
    }

    /// Enable Codex helper dispatch for a single-binary host. Obtain this value
    /// by calling [`crate::Omnix::initialize_embedded_process`] at the very
    /// beginning of `main`, before starting async runtimes or threads.
    pub fn embedded_process(mut self, process: EmbeddedProcess) -> Self {
        self.process = Some(process);
        self
    }

    /// Validate configuration and start the runtime.
    pub async fn build(self) -> Result<OmnixRuntime, OmnixError> {
        let credentials = self.credentials.ok_or_else(|| {
            OmnixError::new(
                OmnixErrorKind::InvalidCredentials,
                "credentials are required to start the runtime",
            )
        })?;
        if !credentials.is_present() {
            return Err(OmnixError::new(
                OmnixErrorKind::InvalidCredentials,
                "API key must not be empty",
            ));
        }

        // Resolve the config: an explicit `config()` is the base; a scope-setter
        // overrides its scope. At least one must provide a scope.
        let mut config = self.config.ok_or(()).or_else(|()| {
            self.scope
                .clone()
                .map(default_config_with_scope)
                .ok_or_else(|| {
                    OmnixError::new(
                        OmnixErrorKind::InvalidConfig,
                        "a data root (application_root/project_root) or config is required",
                    )
                })
        })?;
        if let Some(scope) = self.scope {
            config.scope = scope;
        }

        config.validate()?;

        // `base_instructions` replaces the model's built-in agent/tool-use
        // harness ENTIRELY — only set it when the host explicitly asks for a
        // full replacement. Business Pack instructions go into
        // `developer_instructions` (the additive, safe layer that preserves the
        // harness). An explicit builder value always wins.
        let base_instructions = self.base_instructions;

        let developer_instructions = match self.developer_instructions {
            Some(explicit) => Some(explicit),
            None => match &self.business_pack {
                Some(pack) => pack
                    .resolve_developer_instructions(self.pack_root.as_deref())
                    .map_err(|e| {
                        OmnixError::new(OmnixErrorKind::InvalidConfig, e.to_string()).with_source(e)
                    })?,
                None => None,
            },
        };

        if let Some(instructions) = base_instructions.as_deref() {
            validate_instruction("base instructions", instructions)?;
        }
        if let Some(instructions) = developer_instructions.as_deref() {
            validate_instruction("developer instructions", instructions)?;
        }

        let tool_invoker = self.tools.and_then(ToolRegistry::into_invoker);

        let spec = config.into_spec(
            &credentials,
            base_instructions,
            developer_instructions,
            tool_invoker,
            self.process.map(EmbeddedProcess::into_runtime),
        );

        let runtime = Runtime::start(spec).await.map_err(map_runtime_error)?;
        Ok(OmnixRuntime::new(runtime))
    }
}

/// Build a defaulted config carrying only the given scope.
fn default_config_with_scope(scope: RuntimeScope) -> RuntimeConfig {
    RuntimeConfig {
        scope,
        model: Default::default(),
        context: Default::default(),
        permissions: Default::default(),
        tools: Default::default(),
    }
}
