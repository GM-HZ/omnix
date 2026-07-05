pub mod workspace_settings {
    use std::collections::HashMap;
    #[derive(Default)]
    pub struct WorkspaceSettingsCache;
    pub fn codex_plugins_enabled_for_workspace(_cache: &WorkspaceSettingsCache, _config: &codex_core::config::Config) -> bool { true }
}

pub mod connectors {
    use codex_connectors::AppInfo;
    pub fn list_cached_accessible_connectors_from_mcp_tools(_c: &codex_core::config::Config) -> Vec<AppInfo> { vec![] }
    pub fn list_cached_all_connectors(_c: &codex_core::config::Config, _p: &[String]) -> Vec<AppInfo> { vec![] }
    pub fn list_accessible_connectors_from_mcp_tools_with_mcp_manager(_m: &codex_mcp::McpManager, _c: &codex_core::config::Config, _u: &codex_core::exec_env::UnifiedExecManager, _p: &codex_core::plugins::PluginsManager) -> Vec<AppInfo> { vec![] }
    pub fn list_all_connectors_with_options(_c: &codex_core::config::Config, _f: bool, _p: &[String]) -> Vec<AppInfo> { vec![] }
    pub fn with_app_enabled_state(_a: Vec<AppInfo>, _c: &codex_core::config::Config) -> Vec<AppInfo> { vec![] }
    pub fn merge_connectors_with_accessible(_a: Vec<AppInfo>, _b: Vec<AppInfo>, _l: bool) -> Vec<AppInfo> { vec![] }
    pub fn connectors_for_plugin_apps(_c: Vec<AppInfo>, _p: &[String]) -> Vec<AppInfo> { vec![] }
}
