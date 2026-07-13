//! Minimal generic Business Pack example.
//!
//! Shows how a host composes a pack (here, inline instructions) and starts a
//! runtime with it. Deliberately business-neutral — a real application (e.g.
//! Staffroom) supplies its own methodology instructions, skills, and tools.
//!
//! Run with a real key to exercise it live:
//!   DEEPSEEK_API_KEY=... cargo run -p omnix-sdk --example minimal_pack

use omnix_sdk::BusinessPack;
use omnix_sdk::Credentials;
use omnix_sdk::Omnix;
use omnix_sdk::OmnixError;

#[tokio::main]
async fn main() -> Result<(), OmnixError> {
    let Ok(api_key) = std::env::var("DEEPSEEK_API_KEY") else {
        eprintln!("set DEEPSEEK_API_KEY to run this example");
        return Ok(());
    };

    let data_root = std::env::temp_dir().join("omnix-minimal-pack-example");

    // A generic pack: just a short system instruction, composed inline.
    let pack = BusinessPack::new("generic-assistant", "0.0")
        .with_inline_instruction("You are a concise, helpful assistant.");

    let runtime = Omnix::builder()
        .application_root(&data_root)
        .credentials(Credentials::from_api_key(api_key))
        .business_pack(pack)
        .build()
        .await?;

    let mut session = runtime.sessions().create(Default::default()).await?;
    let mut run = session.run("Say hello in one short sentence.").await?;

    while let Some(event) = run.next().await {
        if let omnix_sdk::AgentEvent::MessageCompleted { text, .. } = event {
            println!("assistant: {text}");
        }
    }

    runtime.shutdown().await?;
    Ok(())
}
