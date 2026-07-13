//! `Omnix::builder()` — the ergonomic entry point for constructing a runtime.

use std::path::PathBuf;

use omnix_runtime::Runtime;

use crate::config::Credentials;
use crate::config::RuntimeConfig;
use crate::config::RuntimeScope;
use crate::error::OmnixError;
use crate::error::OmnixErrorKind;
use crate::error_map::map_runtime_error;
use crate::pack::BusinessPack;
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

    /// Attach a Business Pack. Its instructions are resolved and become the
    /// runtime's `base_instructions` (unless `base_instructions` is set
    /// explicitly, which then takes precedence). File-backed instruction
    /// fragments resolve relative to [`OmnixBuilder::pack_root`] when set.
    pub fn business_pack(mut self, pack: BusinessPack) -> Self {
        self.business_pack = Some(pack);
        self
    }

    /// Root directory for resolving a pack's file-backed assets.
    pub fn pack_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.pack_root = Some(root.into());
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

        // Resolve base instructions: an explicit builder value wins; otherwise
        // fold in the Business Pack's composed instructions.
        let base_instructions = match self.base_instructions {
            Some(explicit) => Some(explicit),
            None => match &self.business_pack {
                Some(pack) => pack
                    .resolve_base_instructions(self.pack_root.as_deref())
                    .map_err(|e| {
                        OmnixError::new(OmnixErrorKind::InvalidConfig, e.to_string()).with_source(e)
                    })?,
                None => None,
            },
        };

        let tool_invoker = self.tools.and_then(ToolRegistry::into_invoker);

        let spec = config.into_spec(
            &credentials,
            base_instructions,
            self.developer_instructions,
            tool_invoker,
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
        skills: Default::default(),
        plugins: Default::default(),
        persistence: Default::default(),
        observability: Default::default(),
    }
}
