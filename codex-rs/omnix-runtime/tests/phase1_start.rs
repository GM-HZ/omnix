//! Phase 1 exit test: the embedded runtime starts against a mock Chat
//! Completions provider with NO `config.toml`, reports Ready, and shuts down
//! cleanly.

mod common;

use common::test_spec;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use omnix_runtime::Runtime;
use omnix_runtime::RuntimeHealth;

#[tokio::test]
async fn embedded_runtime_starts_without_config_toml() {
    let server = start_mock_chat_completions_server().await;
    let home = tempfile::tempdir().expect("temp dir");
    let root = home.path().to_path_buf();

    let runtime = Runtime::start(test_spec(root.clone(), server.uri()))
        .await
        .expect("runtime should start with an in-memory config");

    assert_eq!(runtime.health(), RuntimeHealth::Ready);

    // The `.omnix` home was derived beneath the application root, and no
    // config.toml was created or required.
    let dot_omnix = root.join(".omnix");
    assert!(dot_omnix.is_dir(), ".omnix directory should exist");
    assert!(
        !dot_omnix.join("config.toml").exists(),
        "startup must not require or create a config.toml"
    );
    // §14: the non-sensitive runtime manifest is written on startup and must not
    // contain the API key.
    let manifest_path = dot_omnix.join("runtime.json");
    assert!(manifest_path.is_file(), "runtime.json should be written");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read manifest");
    assert!(manifest.contains("\"scope\": \"application\""));
    assert!(
        !manifest.contains("test-key"),
        "manifest must not contain the API key"
    );
    assert_eq!(runtime.paths().codex_home, dot_omnix);
    assert_eq!(runtime.paths().workspace, root);

    let caps = runtime.capabilities();
    assert_eq!(caps.wire_api, "chat_completions");
    assert!(caps.persistence);

    runtime.shutdown().await.expect("clean shutdown");
}
