use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CapabilityScope {
    Read(String),        // Path pattern, e.g., "/workspace/src/**"
    Write(String),       // Path pattern
    Network(String),     // URL pattern, e.g., "https://api.github.com/**"
    Shell(String),       // Command pattern
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    pub id: String,
    pub scope: CapabilityScope,
    pub expires_at: SystemTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionManifest {
    pub id: String,
    pub action_description: String,
    pub risk_level: RiskLevel,
    pub parameters: HashMap<String, String>,
    pub capability_token_id: Option<String>,
    pub created_at: SystemTime,
    pub nonce: [u8; 32],
}

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum SentinelError {
    #[error("Capability denied: {0}")]
    CapabilityDenied(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("LLM error: {0}")]
    LlmError(String),
    
    #[error("HITL approval required")]
    ApprovalRequired,
    
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, SentinelError>;
