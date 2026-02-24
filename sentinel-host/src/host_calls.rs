//! # sentinel-host — Host Call Implementations
//!
//! These are the functions wired into the Wasmtime `Linker` that the
//! Guest invokes through the WIT interface. Every call goes through
//! capability validation before touching any host resource.

use crate::capabilities::CapabilityManager;
use crate::config::SentinelConfig;
use sentinel_shared::{CapabilityScope, SentinelError};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn, error};

pub struct HostCallHandler {
    pub capability_manager: Arc<CapabilityManager>,
    pub config: SentinelConfig,
}

impl HostCallHandler {
    pub fn new(capability_manager: Arc<CapabilityManager>, config: SentinelConfig) -> Self {
        Self { capability_manager, config }
    }

    pub async fn request_fs_read(&self, path: String, justification: String) -> Result<String, SentinelError> {
        info!(path = %path, justification = %justification, "Guest requesting fs.read capability");
        let canonical = self.canonicalize_and_validate_read_path(&path)?;
        let scope = CapabilityScope::FsPath { allowed_pattern: canonical.to_string_lossy().to_string(), read_only: true };
        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn request_fs_write(&self, path: String, justification: String) -> Result<String, SentinelError> {
        info!(path = %path, justification = %justification, "Guest requesting fs.write capability");
        let canonical = self.canonicalize_and_validate_write_path(&path)?;
        let scope = CapabilityScope::FsPath { allowed_pattern: canonical.to_string_lossy().to_string(), read_only: false };
        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn request_net_outbound(&self, url: String, method: String, justification: String) -> Result<String, SentinelError> {
        info!(url = %url, method = %method, justification = %justification, "Guest requesting net.outbound capability");
        let scope = CapabilityScope::NetUrl { allowed_url_pattern: url.clone(), methods: vec![method] };
        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn request_ui_observe(&self) -> Result<String, SentinelError> {
        info!("Guest requesting ui.observe capability");
        let scope = CapabilityScope::UiObserve;
        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn request_ui_dispatch(&self, event_type: String) -> Result<String, SentinelError> {
        info!(event_type = %event_type, "Guest requesting ui.dispatch capability");
        let scope = CapabilityScope::UiDispatch { allowed_event_types: vec![event_type] };
        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn release_capability(&self, token_id: String) -> bool {
        info!(token_id = %token_id, "Guest releasing capability");
        self.capability_manager.revoke_token(&token_id).await
    }

    // ── Token-Gated Operations ──────────────────────────────────────────

    pub async fn fs_read(&self, token_id: String, path: String) -> Result<Vec<u8>, SentinelError> {
        self.capability_manager.validate_token(&token_id, &path).await?;
        let canonical = self.canonicalize_and_validate_read_path(&path)?;

        let metadata = tokio::fs::metadata(&canonical).await.map_err(|e| SentinelError::GuestError { message: format!("Cannot stat file: {e}") })?;
        if metadata.len() as usize > self.config.filesystem.max_read_size {
            return Err(SentinelError::ResourceExhausted { resource: format!("File size {} exceeds limit {}", metadata.len(), self.config.filesystem.max_read_size) });
        }

        let contents = tokio::fs::read(&canonical).await.map_err(|e| SentinelError::GuestError { message: format!("Cannot read file: {e}") })?;
        info!(path = %path, size = contents.len(), "fs.read completed");
        Ok(contents)
    }

    pub async fn fs_write(&self, token_id: String, path: String, data: Vec<u8>) -> Result<bool, SentinelError> {
        self.capability_manager.validate_token(&token_id, &path).await?;

        let target = Path::new(&path);
        let parent = target.parent().unwrap_or(Path::new("."));
        let parent_canon = parent.canonicalize().map_err(|e| SentinelError::GuestError { message: format!("Cannot resolve write directory: {e}") })?;

        let is_allowed = self.config.filesystem.allowed_write_dirs.iter().any(|dir| {
            let d = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            parent_canon.starts_with(&d)
        });

        if !is_allowed {
            warn!(path = %path, "Write denied — directory not in allowed_write_dirs");
            return Err(SentinelError::PathEscapeAttempt { path: path.clone() });
        }

        let write_path = parent_canon.join(target.file_name().unwrap_or_default());
        tokio::fs::write(&write_path, &data).await.map_err(|e| SentinelError::GuestError { message: format!("Cannot write file: {e}") })?;
        info!(path = %write_path.display(), size = data.len(), "fs.write completed");
        Ok(true)
    }

    pub async fn fs_list_dir(&self, token_id: String, path: String) -> Result<Vec<String>, SentinelError> {
        self.capability_manager.validate_token(&token_id, &path).await?;
        let canonical = self.canonicalize_and_validate_read_path(&path)?;

        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&canonical).await.map_err(|e| SentinelError::GuestError { message: format!("Cannot read directory: {e}") })?;

        while let Some(entry) = dir.next_entry().await.map_err(|e| SentinelError::GuestError { message: format!("Error reading dir entry: {e}") })? {
            if let Some(name) = entry.file_name().to_str() {
                entries.push(name.to_string());
            }
        }

        info!(path = %path, count = entries.len(), "fs.list_dir completed");
        Ok(entries)
    }

    pub async fn net_request(&self, token_id: String, url: String, method: String, _headers: Vec<(String, String)>, _body: Option<Vec<u8>>) -> Result<NetResponse, SentinelError> {
        self.capability_manager.validate_token(&token_id, &url).await?;
        info!(url = %url, method = %method, "net.request — validated (stub response)");
        Ok(NetResponse { status: 200, headers: vec![("content-type".into(), "application/json".into())], body: b"{}".to_vec() })
    }

    pub async fn ui_get_state(&self, token_id: String) -> Result<String, SentinelError> {
        self.capability_manager.validate_token(&token_id, "ui:observe").await?;
        info!("ui.observe — returning stub state");
        Ok(r#"{"screen": "main", "elements": []}"#.to_string())
    }

    pub async fn ui_send_event(&self, token_id: String, event_type: String, _payload: String) -> Result<bool, SentinelError> {
        self.capability_manager.validate_token(&token_id, &format!("ui:dispatch:{event_type}")).await?;
        info!(event_type = %event_type, "ui.dispatch — event sent (stub)");
        Ok(true)
    }

    // ── Internal Helpers ────────────────────────────────────────────────

    fn canonicalize_and_validate_read_path(&self, path: &str) -> Result<std::path::PathBuf, SentinelError> {
        let requested = Path::new(path);
        let canonical = requested.canonicalize().map_err(|_| SentinelError::PathEscapeAttempt { path: path.to_string() })?;

        let is_allowed = self.config.filesystem.allowed_read_dirs.iter().any(|dir| {
            let d = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            canonical.starts_with(&d)
        });

        if !is_allowed {
            warn!(path = %path, canonical = %canonical.display(), "Path escape attempt blocked (read)");
            return Err(SentinelError::PathEscapeAttempt { path: canonical.to_string_lossy().to_string() });
        }
        Ok(canonical)
    }

    fn canonicalize_and_validate_write_path(&self, path: &str) -> Result<std::path::PathBuf, SentinelError> {
        let requested = Path::new(path);
        let parent = requested.parent().unwrap_or(Path::new("."));
        let parent_canon = parent.canonicalize().map_err(|_| SentinelError::PathEscapeAttempt { path: path.to_string() })?;

        let is_allowed = self.config.filesystem.allowed_write_dirs.iter().any(|dir| {
            let d = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            parent_canon.starts_with(&d)
        });

        if !is_allowed {
            warn!(path = %path, canonical = %parent_canon.display(), "Path escape attempt blocked (write)");
            return Err(SentinelError::PathEscapeAttempt { path: parent_canon.to_string_lossy().to_string() });
        }
        Ok(parent_canon.join(requested.file_name().unwrap_or_default()))
    }
}

#[derive(Debug, Clone)]
pub struct NetResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
