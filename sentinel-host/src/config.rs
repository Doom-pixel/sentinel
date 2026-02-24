//! # sentinel-host — Configuration
//!
//! Runtime configuration for the SENTINEL host, defining resource
//! limits, capability scopes, and security policy thresholds.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentinelConfig {
    pub engine: EngineConfig,
    pub filesystem: FsConfig,
    pub network: NetConfig,
    pub hitl: HitlConfig,
    pub llm: crate::llm::LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub max_memory_bytes: usize,
    pub max_tables: u32,
    pub max_table_elements: u32,
    pub fuel_limit: Option<u64>,
    pub guest_module_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsConfig {
    pub allowed_read_dirs: Vec<PathBuf>,
    pub allowed_write_dirs: Vec<PathBuf>,
    pub max_read_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetConfig {
    pub url_whitelist: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub request_timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlConfig {
    pub approval_threshold: ApprovalThreshold,
    pub approval_timeout: Duration,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ApprovalThreshold {
    None,
    High,
    Critical,
    All,
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig {
                max_memory_bytes: 256 * 1024 * 1024,
                max_tables: 10,
                max_table_elements: 10_000,
                fuel_limit: Some(1_000_000_000),
                guest_module_path: PathBuf::from("guest.wasm"),
            },
            filesystem: FsConfig {
                allowed_read_dirs: vec![std::env::current_dir().unwrap_or_default()],
                allowed_write_dirs: vec![],
                max_read_size: 10 * 1024 * 1024,
            },
            network: NetConfig {
                url_whitelist: vec![],
                allowed_methods: vec!["GET".into(), "POST".into(), "PUT".into(), "DELETE".into()],
                request_timeout: Duration::from_secs(30),
            },
            hitl: HitlConfig {
                approval_threshold: ApprovalThreshold::High,
                approval_timeout: Duration::from_secs(300),
            },
            llm: crate::llm::LlmConfig::default(),
        }
    }
}
