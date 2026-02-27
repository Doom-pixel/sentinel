//! SENTINEL Host — CLI Entry Point
//!
//! Boots the engine and starts the task execution.

use clap::Parser;
use std::sync::Arc;
use anyhow::Result;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    task: String,
    #[arg(short, long)]
    target: String,
    #[arg(short, long, default_value = "read_report")]
    autonomy: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    println!("🛡️ SENTINEL Host starting...");
    println!("Task: {}", args.task);
    println!("Target: {}", args.target);
    println!("Autonomy: {}", args.autonomy);

    let engine = sentinel_host::Engine::new()?;
    let hitl_bridge = Arc::new(sentinel_host::HitlBridge {
        callback_url: "http://localhost:9876".to_string(),
    });
    let capability_manager = Arc::new(sentinel_host::CapabilityManager {
        autonomy: args.autonomy,
    });

    // Mock WASM for demonstration
    let wasm_bytes = vec![]; 
    let agent_id = "agent-123".to_string();
    let context_json = format!(r#"{{"task": "{}", "target": "{}"}}"#, args.task, args.target);

    engine.run_agent(
        &wasm_bytes,
        agent_id,
        args.target,
        context_json,
        hitl_bridge,
        capability_manager,
    ).await?;

    Ok(())
}
