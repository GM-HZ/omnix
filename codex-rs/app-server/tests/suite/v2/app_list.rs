use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::TestAppServer;
use app_test_support::to_response;
use app_test_support::write_chatgpt_auth;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::http::header::AUTHORIZATION;
use axum::routing::get;
use codex_app_server_protocol::AppInfo;
use codex_app_server_protocol::AppsListParams;
use codex_app_server_protocol::AppsListResponse;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_config::types::AuthCredentialsStoreMode;
use codex_login::AuthDotJson;
use codex_login::AuthKeyringBackendKind;
use codex_login::save_auth;
use codex_protocol::auth::AuthMode;
use rmcp::handler::server::ServerHandler;
use rmcp::model::JsonObject;
use rmcp::model::ListToolsResult;
use rmcp::model::Meta;
use rmcp::model::ServerCapabilities;
use rmcp::model::ServerInfo;
use rmcp::model::Tool;
use rmcp::model::ToolAnnotations;
use rmcp::transport::StreamableHttpServerConfig;
use rmcp::transport::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use serde_json::json;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::timeout;

// Bazel CI can spend tens of seconds starting app-server subprocesses or
// processing app-list RPCs under load.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::test]
async fn list_apps_returns_empty_when_connectors_disabled() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            limit: Some(50),
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let AppsListResponse { data, next_cursor } = to_response(response)?;

    assert!(data.is_empty());
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_apps_returns_empty_with_api_key_auth() -> Result<()> {
    let connectors = vec![AppInfo {
        id: "beta".to_string(),
        name: "Beta".to_string(),
        description: Some("Beta connector".to_string()),
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
    }];
    let tools = vec![connector_tool("beta", "Beta App")?];
    let (server_url, server_handle) =
        start_apps_server_with_delays(connectors, tools, Duration::ZERO, Duration::ZERO).await?;

    let codex_home = TempDir::new()?;
    write_connectors_config(codex_home.path(), &server_url)?;
    save_auth(
        codex_home.path(),
        &AuthDotJson {
            auth_mode: Some(AuthMode::ApiKey),
            openai_api_key: Some("test-api-key".to_string()),
            tokens: None,
            last_refresh: None,
            agent_identity: None,
            personal_access_token: None,
            bedrock_api_key: None,
        },
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?;

    let mut mcp = TestAppServer::new(codex_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            limit: Some(50),
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let AppsListResponse { data, next_cursor } = to_response(response)?;
    assert!(data.is_empty());
    assert!(next_cursor.is_none());

    server_handle.abort();
    let _ = server_handle.await;
    Ok(())
}

#[tokio::test]
async fn list_apps_returns_empty_when_workspace_codex_plugins_disabled() -> Result<()> {
    let connectors = vec![AppInfo {
        id: "beta".to_string(),
        name: "Beta".to_string(),
        description: Some("Beta connector".to_string()),
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
    }];
    let tools = vec![connector_tool("beta", "Beta App")?];
    let (server_url, server_handle) = start_apps_server_with_workspace_plugins_enabled(
        connectors, tools, /*workspace_plugins_enabled*/ false,
    )
    .await?;

    let codex_home = TempDir::new()?;
    write_connectors_config(codex_home.path(), &server_url)?;
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123")
            .plan_type("team"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = TestAppServer::new_without_managed_config(codex_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            limit: Some(50),
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let AppsListResponse { data, next_cursor } = to_response(response)?;
    assert!(data.is_empty());
    assert!(next_cursor.is_none());

    server_handle.abort();
    let _ = server_handle.await;
    Ok(())
}

#[tokio::test]
async fn list_apps_includes_plugin_apps_for_chatgpt_auth() -> Result<()> {
    let (server_url, server_handle) =
        start_apps_server_with_delays(Vec::new(), Vec::new(), Duration::ZERO, Duration::ZERO)
            .await?;

    let codex_home = TempDir::new()?;
    write_connectors_and_plugins_config(codex_home.path(), &server_url)?;
    write_plugin_app_fixture(codex_home.path(), "sample", "connector_sample")?;
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-plugin-apps")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = TestAppServer::new(codex_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let AppsListResponse { data, next_cursor } = to_response(response)?;

    assert!(data.iter().any(|app| app.id == "connector_sample"));
    assert!(next_cursor.is_none());

    server_handle.abort();
    let _ = server_handle.await;
    Ok(())
}


#[derive(Clone)]
struct AppsServerState {
    expected_bearer: String,
    expected_account_id: String,
    response: Arc<StdMutex<serde_json::Value>>,
    directory_delay: Duration,
    workspace_plugins_enabled: bool,
}

#[derive(Clone)]
struct AppListMcpServer {
    tools: Arc<StdMutex<Vec<Tool>>>,
    tools_delay: Duration,
}

impl AppListMcpServer {
    fn new(tools: Arc<StdMutex<Vec<Tool>>>, tools_delay: Duration) -> Self {
        Self { tools, tools_delay }
    }
}

#[derive(Clone)]
struct AppsServerControl {
    // Retained so start_apps_server can hand back a live handle to the mock
    // state; the remaining app_list tests don't mutate it after startup.
    #[allow(dead_code)]
    response: Arc<StdMutex<serde_json::Value>>,
    #[allow(dead_code)]
    tools: Arc<StdMutex<Vec<Tool>>>,
}

impl ServerHandler for AppListMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_
    {
        let tools = self.tools.clone();
        let tools_delay = self.tools_delay;
        async move {
            if tools_delay > Duration::ZERO {
                tokio::time::sleep(tools_delay).await;
            }
            let tools = tools
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            Ok(ListToolsResult {
                tools,
                next_cursor: None,
                meta: None,
            })
        }
    }
}

pub(super) async fn start_apps_server_with_delays(
    connectors: Vec<AppInfo>,
    tools: Vec<Tool>,
    directory_delay: Duration,
    tools_delay: Duration,
) -> Result<(String, JoinHandle<()>)> {
    let (server_url, server_handle, _server_control) =
        start_apps_server_with_delays_and_control(connectors, tools, directory_delay, tools_delay)
            .await?;
    Ok((server_url, server_handle))
}

async fn start_apps_server_with_workspace_plugins_enabled(
    connectors: Vec<AppInfo>,
    tools: Vec<Tool>,
    workspace_plugins_enabled: bool,
) -> Result<(String, JoinHandle<()>)> {
    let (server_url, server_handle, _server_control) =
        start_apps_server_with_delays_and_control_inner(
            connectors,
            tools,
            Duration::ZERO,
            Duration::ZERO,
            workspace_plugins_enabled,
        )
        .await?;
    Ok((server_url, server_handle))
}

async fn start_apps_server_with_delays_and_control(
    connectors: Vec<AppInfo>,
    tools: Vec<Tool>,
    directory_delay: Duration,
    tools_delay: Duration,
) -> Result<(String, JoinHandle<()>, AppsServerControl)> {
    start_apps_server_with_delays_and_control_inner(
        connectors,
        tools,
        directory_delay,
        tools_delay,
        /*workspace_plugins_enabled*/ true,
    )
    .await
}

async fn start_apps_server_with_delays_and_control_inner(
    connectors: Vec<AppInfo>,
    tools: Vec<Tool>,
    directory_delay: Duration,
    tools_delay: Duration,
    workspace_plugins_enabled: bool,
) -> Result<(String, JoinHandle<()>, AppsServerControl)> {
    let response = Arc::new(StdMutex::new(
        json!({ "apps": connectors, "next_token": null }),
    ));
    let tools = Arc::new(StdMutex::new(tools));
    let state = AppsServerState {
        expected_bearer: "Bearer chatgpt-token".to_string(),
        expected_account_id: "account-123".to_string(),
        response: response.clone(),
        directory_delay,
        workspace_plugins_enabled,
    };
    let state = Arc::new(state);
    let server_control = AppsServerControl {
        response,
        tools: tools.clone(),
    };

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let mcp_service = StreamableHttpService::new(
        {
            let tools = tools.clone();
            move || Ok(AppListMcpServer::new(tools.clone(), tools_delay))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let router = Router::new()
        .route("/connectors/directory/list", get(list_directory_connectors))
        .route(
            "/connectors/directory/list_workspace",
            get(list_directory_connectors),
        )
        .route(
            "/accounts/account-123/settings",
            get(workspace_settings_response),
        )
        .with_state(state)
        .nest_service("/api/codex/ps/mcp", mcp_service);

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    Ok((format!("http://{addr}"), handle, server_control))
}

async fn workspace_settings_response(
    State(state): State<Arc<AppsServerState>>,
    headers: HeaderMap,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let bearer_ok = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_bearer);
    let account_ok = headers
        .get("chatgpt-account-id")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_account_id);

    if !bearer_ok || !account_ok {
        Err(StatusCode::UNAUTHORIZED)
    } else {
        Ok(Json(json!({
            "beta_settings": {
                "enable_plugins": state.workspace_plugins_enabled
            }
        })))
    }
}

async fn list_directory_connectors(
    State(state): State<Arc<AppsServerState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    if state.directory_delay > Duration::ZERO {
        tokio::time::sleep(state.directory_delay).await;
    }

    let bearer_ok = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_bearer);
    let account_ok = headers
        .get("chatgpt-account-id")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_account_id);
    let external_logos_ok = uri
        .query()
        .is_some_and(|query| query.split('&').any(|pair| pair == "external_logos=true"));

    if !bearer_ok || !account_ok {
        Err(StatusCode::UNAUTHORIZED)
    } else if !external_logos_ok {
        Err(StatusCode::BAD_REQUEST)
    } else {
        let response = state
            .response
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        Ok(Json(response))
    }
}

pub(super) fn connector_tool(connector_id: &str, connector_name: &str) -> Result<Tool> {
    let schema: JsonObject = serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false
    }))?;
    let mut tool = Tool::new(
        Cow::Owned(format!("connector_{connector_id}")),
        Cow::Borrowed("Connector test tool"),
        Arc::new(schema),
    );
    tool.annotations = Some(ToolAnnotations::new().read_only(true));

    let mut meta = Meta::new();
    meta.0
        .insert("connector_id".to_string(), json!(connector_id));
    meta.0
        .insert("connector_name".to_string(), json!(connector_name));
    tool.meta = Some(meta);
    Ok(tool)
}

fn write_connectors_config(codex_home: &std::path::Path, base_url: &str) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
chatgpt_base_url = "{base_url}"
mcp_oauth_credentials_store = "file"

[features]
connectors = true
"#
        ),
    )
}

fn write_connectors_and_plugins_config(codex_home: &Path, base_url: &str) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
chatgpt_base_url = "{base_url}"
mcp_oauth_credentials_store = "file"

[features]
connectors = true
plugins = true

[plugins."sample@test"]
enabled = true
"#
        ),
    )
}

fn write_plugin_app_fixture(codex_home: &Path, plugin_name: &str, app_id: &str) -> Result<()> {
    let plugin_root = codex_home
        .join("plugins/cache")
        .join("test")
        .join(plugin_name)
        .join("local");
    std::fs::create_dir_all(plugin_root.join(".codex-plugin"))?;
    std::fs::write(
        plugin_root.join(".codex-plugin/plugin.json"),
        format!(r#"{{"name":"{plugin_name}"}}"#),
    )?;
    std::fs::write(
        plugin_root.join(".app.json"),
        serde_json::to_vec_pretty(&json!({
            "apps": {
                plugin_name: { "id": app_id }
            }
        }))?,
    )?;
    Ok(())
}
