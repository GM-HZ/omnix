//! Stubs for removed codex_chatgpt crate functionality.
//! 
//! The chatgpt crate provided two modules used by app-server:
//! 1. connectors — mostly re-exports from codex_core::connectors
//! 2. workspace_settings — ChatGPT-specific workspace beta settings

pub mod workspace_settings {
    use std::sync::{Arc, RwLock};

    #[derive(Default, Debug)]
    pub struct WorkspaceSettingsCache {
        _entry: RwLock<()>,
    }

    impl Clone for WorkspaceSettingsCache {
        fn clone(&self) -> Self { Self::default() }
    }

    /// Always returns true (plugins enabled) since there's no ChatGPT workspace.
    pub async fn codex_plugins_enabled_for_workspace(
        _config: &codex_core::config::Config,
        _auth: Option<&codex_login::CodexAuth>,
        _cache: Option<&Arc<WorkspaceSettingsCache>>,
    ) -> Result<bool, std::io::Error> {
        Ok(true)
    }
}

pub mod connectors {
    use codex_connectors::AppInfo;
    use std::io;

    // Wrappers around codex_core::connectors functions (accept any types)
    pub async fn list_cached_accessible_connectors_from_mcp_tools<T>(
        _config: &T,
    ) -> io::Result<Vec<AppInfo>> { Ok(vec![]) }

    pub async fn list_accessible_connectors_from_mcp_tools_with_mcp_manager<T, U, V, W>(
        _mcp: &T,
        _config: &U,
        _unified_exec: &V,
        _plugins: &W,
    ) -> io::Result<Vec<AppInfo>> { Ok(vec![]) }

    pub async fn with_app_enabled_state<T>(
        apps: Vec<AppInfo>,
        _config: &T,
    ) -> io::Result<Vec<AppInfo>> { Ok(apps) }

    // Functions that were chatgpt-specific wrappers
    pub async fn list_cached_all_connectors(
        _config: &codex_core::config::Config,
        _plugin_apps: &[codex_plugin::AppConnectorId],
    ) -> io::Result<Vec<AppInfo>> {
        Ok(vec![])
    }

    pub async fn list_all_connectors_with_options(
        _config: &codex_core::config::Config,
        _force_refetch: bool,
        _plugin_apps: &[codex_plugin::AppConnectorId],
    ) -> io::Result<Vec<AppInfo>> {
        Ok(vec![])
    }

    pub fn connectors_for_plugin_apps(
        connectors: Vec<AppInfo>,
        _plugin_apps: &[codex_plugin::AppConnectorId],
    ) -> Vec<AppInfo> {
        connectors
    }

    pub fn merge_connectors_with_accessible(
        all: Vec<AppInfo>,
        _accessible: Vec<AppInfo>,
        _all_connectors_loaded: bool,
    ) -> Vec<AppInfo> {
        all
    }
}
