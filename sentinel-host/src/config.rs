//! # sentinel-host — Configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Top-level configuration for the SENTINEL host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentinelConfig {
    /// Wasm engine resource limits.
    pub engine: EngineConfig,
    /// Filesystem capability constraints.
    pub filesystem: FsConfig,
    /// Network capability constraints.
    pub network: NetConfig,
    /// HITL approval settings.
    pub hitl: HitlConfig,
    /// LLM provider settings.
    pub llm: crate::llm::LlmConfig,
}

/// Resource limits for the Wasmtime engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Maximum linear memory in bytes (default: 256 MiB).
    pub max_memory_bytes: usize,
    /// Maximum number of Wasm tables.
    pub max_tables: u32,
    /// Maximum table elements.
    pub max_table_elements: u32,
    /// Fuel limit for execution metering (None = unlimited).
    pub fuel_limit: Option<u64>,
    /// Path to the guest Wasm module.
    pub guest_module_path: PathBuf,
}

/// Filesystem access constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsConfig {
    /// Directories the guest is allowed to read from.
    /// All paths are canonicalized and checked with starts_with().
    pub allowed_read_dirs: Vec<PathBuf>,
    /// Maximum file size the guest can read (bytes).
    pub max_read_size: usize,
}

/// Network access constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetConfig {
    /// URL patterns the guest is allowed to access.
    /// Supports simple wildcard matching (e.g., "https://api.example.com/*").
    pub url_whitelist: Vec<String>,
    /// Allowed HTTP methods.
    pub allowed_methods: Vec<String>,
    /// Request timeout.
    pub request_timeout: Duration,
}

/// HITL approval configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlConfig {
    /// Minimum risk level that triggers HITL approval.
    pub approval_threshold: ApprovalThreshold,
    /// Timeout for waiting for user approval.
    pub approval_timeout: Duration,
}

/// When to require user approval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ApprovalThreshold {
    /// Approve everything automatically (DANGEROUS — testing only).
    None,
    /// Require approval for High and Critical actions.
    High,
    /// Require approval for Critical actions only.
    Critical,
    /// Require approval for all actions (paranoid mode).
    All,
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig {
                max_memory_bytes: 256 * 1024 * 1024, // 256 MiB
                max_tables: 10,
                max_table_elements: 10_000,
                fuel_limit: Some(1_000_000_000), // ~1 billion instructions
                guest_module_path: PathBuf::from("guest.wasm"),
            },
            filesystem: FsConfig {
                allowed_read_dirs: vec![std::env::current_dir().unwrap_or_default()],
                max_read_size: 10 * 1024 * 1024, // 10 MiB
            },
            network: NetConfig {
                url_whitelist: vec![],
                allowed_methods: vec![
                    "GET".into(),
                    "POST".into(),
                    "PUT".into(),
                    "DELETE".into(),
                ],
                request_timeout: Duration::from_secs(30),
            },
            hitl: HitlConfig {
                approval_threshold: ApprovalThreshold::High,
                approval_timeout: Duration::from_secs(300), // 5 minutes
            },
            llm: crate::llm::LlmConfig::default(),
        }
    }
}
