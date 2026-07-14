//! Minimal generic Business Pack example.
//!
//! Shows how a host composes a pack (here, inline instructions) and starts a
//! runtime with it. Deliberately business-neutral: a real application supplies
//! its own methodology instructions and host tools.
//!
//! Run with a real key to exercise it live:
//!   DEEPSEEK_API_KEY=... cargo run -p omnix-sdk --example minimal_pack

use omnix_sdk::BusinessPack;
use omnix_sdk::Credentials;
use omnix_sdk::EmbeddedProcess;
use omnix_sdk::Omnix;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let process = Omnix::initialize_embedded_process();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(process))
}

async fn run(process: Option<EmbeddedProcess>) -> Result<(), Box<dyn std::error::Error>> {
    let Ok(api_key) = std::env::var("DEEPSEEK_API_KEY") else {
        eprintln!("set DEEPSEEK_API_KEY to run this example");
        return Ok(());
    };

    let data_root = std::env::temp_dir().join("omnix-minimal-pack-example");

    // A generic pack: one short additive methodology instruction.
    let pack = BusinessPack::new("generic-assistant", "0.0")
        .with_inline_instruction("You are a concise, helpful assistant.");

    let builder = Omnix::builder()
        .application_root(&data_root)
        .credentials(Credentials::from_api_key(api_key))
        .business_pack(pack);
    let builder = match process {
        Some(process) => builder.embedded_process(process),
        None => builder,
    };
    let runtime = builder.build().await?;

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
