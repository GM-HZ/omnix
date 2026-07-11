use std::collections::HashMap;
use std::collections::HashSet;

use codex_connectors::AppInfo;
use codex_connectors::merge::merge_connectors;
use codex_connectors::merge::merge_plugin_connectors;
use codex_core::config::Config;
pub(super) use codex_core::connectors::list_accessible_connectors_from_mcp_tools_with_mcp_manager;
pub(super) use codex_core::connectors::list_cached_accessible_connectors_from_mcp_tools;
pub(super) use codex_core::connectors::with_app_enabled_state;
use codex_plugin::AppConnectorId;

pub(super) async fn list_cached_all_connectors(
    _config: &Config,
    plugin_apps: &[AppConnectorId],
) -> Option<Vec<AppInfo>> {
    Some(plugin_connectors(plugin_apps))
}

pub(super) async fn list_all_connectors_with_options(
    _config: &Config,
    _force_refetch: bool,
    plugin_apps: &[AppConnectorId],
) -> anyhow::Result<Vec<AppInfo>> {
    Ok(plugin_connectors(plugin_apps))
}

fn plugin_connectors(plugin_apps: &[AppConnectorId]) -> Vec<AppInfo> {
    merge_plugin_connectors(
        Vec::new(),
        plugin_apps
            .iter()
            .map(|connector_id| connector_id.0.clone()),
    )
}

pub(super) fn connectors_for_plugin_apps(
    connectors: Vec<AppInfo>,
    plugin_apps: &[AppConnectorId],
) -> Vec<AppInfo> {
    let connectors = merge_plugin_connectors(
        connectors,
        plugin_apps
            .iter()
            .map(|connector_id| connector_id.0.clone()),
    );
    let mut connectors_by_id = connectors
        .into_iter()
        .map(|connector| (connector.id.clone(), connector))
        .collect::<HashMap<_, _>>();

    plugin_apps
        .iter()
        .filter_map(|connector_id| connectors_by_id.remove(connector_id.0.as_str()))
        .collect()
}

pub(super) fn merge_connectors_with_accessible(
    connectors: Vec<AppInfo>,
    accessible_connectors: Vec<AppInfo>,
    all_connectors_loaded: bool,
) -> Vec<AppInfo> {
    let accessible_connectors = if all_connectors_loaded {
        let connector_ids = connectors
            .iter()
            .map(|connector| connector.id.as_str())
            .collect::<HashSet<_>>();
        accessible_connectors
            .into_iter()
            .filter(|connector| connector_ids.contains(connector.id.as_str()))
            .collect()
    } else {
        accessible_connectors
    };
    merge_connectors(connectors, accessible_connectors)
}

#[cfg(test)]
#[path = "local_connectors_tests.rs"]
mod tests;
