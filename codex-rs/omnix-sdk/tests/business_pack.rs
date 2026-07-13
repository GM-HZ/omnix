//! A Business Pack's bounded instructions preserve the base harness and reach
//! the outgoing Chat Completions request as additive policy.

use core_test_support::chat_completions::cc_text_turn;
use core_test_support::chat_completions::mount_chat_completions_sequence;
use core_test_support::chat_completions::start_mock_chat_completions_server;
use omnix_sdk::BusinessPack;
use omnix_sdk::Credentials;
use omnix_sdk::ModelConfig;
use omnix_sdk::Omnix;
use omnix_sdk::RuntimeConfig;
use omnix_sdk::RuntimeScope;

#[tokio::test]
async fn business_pack_instructions_reach_the_request() {
    let server = start_mock_chat_completions_server().await;
    mount_chat_completions_sequence(&server, vec![cc_text_turn("cc-1", "ok", 8, 2)]).await;

    let home = tempfile::tempdir().expect("temp dir");
    let mut config = RuntimeConfig {
        scope: RuntimeScope::Application(home.path().to_path_buf()),
        model: ModelConfig::default(),
        context: Default::default(),
        permissions: Default::default(),
        tools: Default::default(),
    };
    config.model.base_url = server.uri();
    config.model.model = "mock-model".to_string();

    // A distinctive instruction string we can look for in the request body.
    const MARKER: &str = "OMNIX-PACK-METHODOLOGY-MARKER";
    let pack = BusinessPack::new("test-pack", "0.0")
        .with_inline_instruction(format!("Follow this methodology: {MARKER}"));

    let runtime = Omnix::builder()
        .config(config)
        .credentials(Credentials::from_api_key("test-key"))
        .business_pack(pack)
        .build()
        .await
        .expect("runtime builds with a pack");

    let mut session = runtime
        .sessions()
        .create(Default::default())
        .await
        .expect("session");
    let mut run = session.run("hi").await.expect("run");
    while run.next().await.is_some() {}

    let requests = server.received_requests().await.expect("recorded requests");
    let chat = requests
        .iter()
        .find(|r| r.url.path().ends_with("/chat/completions"))
        .expect("a chat/completions request");
    let body = String::from_utf8_lossy(&chat.body);
    assert!(
        body.contains(MARKER),
        "pack instructions must reach the request as additive instructions; body: {body}"
    );

    runtime.shutdown().await.expect("shutdown");
}
