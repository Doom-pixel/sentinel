//! # SENTINEL Host — Entry Point
//!
//! The Jailer: orchestrates the Wasm sandbox, enforces capabilities,
//! and mediates between the AI agent (Guest) and the outside world.

mod capabilities;
mod config;
mod engine;
mod hitl;
mod host_calls;
mod llm;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// SENTINEL — Secure, Zero-Trust Agent Runtime
#[derive(Parser, Debug)]
#[command(name = "sentinel", version, about = "Zero-trust local-first agent framework")]
struct Cli {
    /// Path to the guest Wasm module.
    #[arg(short, long)]
    module: PathBuf,

    /// LLM provider to use (ollama, openai, anthropic, deepseek, grok, google).
    #[arg(long, default_value = "ollama")]
    provider: String,

    /// Model identifier (e.g., "llama3.1:8b", "gpt-4o", "claude-sonnet-4-20250514").
    #[arg(long, default_value = "llama3.1:8b")]
    model: String,

    /// API key for the selected provider (not needed for Ollama).
    #[arg(long, env = "SENTINEL_API_KEY")]
    api_key: Option<String>,

    /// Directories the guest is allowed to read (can be specified multiple times).
    #[arg(long = "allow-read")]
    allow_read: Vec<PathBuf>,

    /// Directories the guest is allowed to write (can be specified multiple times).
    /// Write operations always trigger HITL approval.
    #[arg(long = "allow-write")]
    allow_write: Vec<PathBuf>,

    /// URLs the guest is allowed to access (can be specified multiple times).
    #[arg(long = "allow-url")]
    allow_url: Vec<String>,

    /// Maximum memory in MiB (default: 256).
    #[arg(long, default_value = "256")]
    max_memory_mib: usize,

    /// Fuel limit for execution (default: 1 billion).
    #[arg(long, default_value = "1000000000")]
    fuel: u64,

    /// Log level filter (default: info).
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&cli.log_level)
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .with_target(true)
        .with_thread_ids(true)
        .init();

    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║         SENTINEL v0.1.0                   ║");
    println!("  ║   Zero-Trust Agent Runtime                ║");
    println!("  ║   Local-First · Defense-in-Depth          ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();

    info!("Configuration:");
    info!("  Module:     {}", cli.module.display());
    info!("  Provider:   {}", cli.provider);
    info!("  Model:      {}", cli.model);
    info!("  Memory:     {} MiB", cli.max_memory_mib);
    info!("  Fuel:       {}", cli.fuel);
    info!("  Read dirs:  {:?}", cli.allow_read);
    info!("  Write dirs: {:?}", cli.allow_write);
    info!("  URL allow:  {:?}", cli.allow_url);

    // Build configuration from CLI args
    let mut config = config::SentinelConfig::default();
    config.engine.guest_module_path = cli.module;
    config.engine.max_memory_bytes = cli.max_memory_mib * 1024 * 1024;
    config.engine.fuel_limit = Some(cli.fuel);
    config.filesystem.allowed_read_dirs = cli.allow_read;
    config.filesystem.allowed_write_dirs = cli.allow_write;
    config.network.url_whitelist = cli.allow_url;

    // Configure LLM provider from CLI
    let api_key = cli.api_key.unwrap_or_default();
    config.llm.model = cli.model;
    config.llm.provider = match cli.provider.to_lowercase().as_str() {
        "ollama" => llm::LlmProvider::Ollama {
            base_url: "http://localhost:11434".into(),
        },
        "openai" => llm::LlmProvider::OpenAi {
            api_key: api_key.clone(),
            org_id: None,
        },
        "anthropic" => llm::LlmProvider::Anthropic {
            api_key: api_key.clone(),
        },
        "deepseek" => llm::LlmProvider::Deepseek {
            api_key: api_key.clone(),
            base_url: None,
        },
        "grok" => llm::LlmProvider::Grok {
            api_key: api_key.clone(),
        },
        "google" => llm::LlmProvider::Google {
            api_key: api_key.clone(),
        },
        other => {
            // Treat as OpenAI-compatible with the provider name as base URL
            llm::LlmProvider::OpenAiCompatible {
                api_key: api_key.clone(),
                base_url: other.to_string(),
            }
        }
    };

    // Boot the engine with default context
    let context_json = r#"{"target_directory": ".", "task_prompt": "Perform the default agent task."}"#.to_string();
    engine::boot(config, context_json, None).await?;

    Ok(())
}
