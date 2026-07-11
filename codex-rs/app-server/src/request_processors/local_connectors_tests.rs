use super::*;
use codex_connectors::metadata::connector_install_url;
use pretty_assertions::assert_eq;

fn app(id: &str) -> AppInfo {
    AppInfo {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        logo_url: None,
        logo_url_dark: None,
        icon_assets: None,
        icon_dark_assets: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: None,
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }
}

fn merged_app(id: &str, is_accessible: bool) -> AppInfo {
    AppInfo {
        install_url: Some(connector_install_url(id, id)),
        is_accessible,
        ..app(id)
    }
}

#[test]
fn local_connector_projection_keeps_only_requested_plugin_apps_in_order() {
    let connectors = connectors_for_plugin_apps(
        vec![app("alpha"), app("beta")],
        &[
            AppConnectorId("gmail".to_string()),
            AppConnectorId("alpha".to_string()),
            AppConnectorId("gmail".to_string()),
        ],
    );

    assert_eq!(
        connectors,
        vec![merged_app("gmail", /*is_accessible*/ false), app("alpha")]
    );
}

#[test]
fn accessibility_merge_drops_nonlocal_connectors_after_local_catalog_loads() {
    let merged = merge_connectors_with_accessible(
        vec![app("alpha")],
        vec![app("alpha"), app("remote-only")],
        /*all_connectors_loaded*/ true,
    );

    assert_eq!(merged, vec![merged_app("alpha", /*is_accessible*/ true)]);
}
