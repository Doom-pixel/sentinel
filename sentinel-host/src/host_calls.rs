//! # sentinel-host — Host Call Implementations

use crate::capabilities::CapabilityManager;
use crate::config::SentinelConfig;
use sentinel_shared::{CapabilityScope, SentinelError};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn, error};

/// Host-side implementations of the WIT `capabilities` interface.
pub struct HostCallHandler {
    pub capability_manager: Arc<CapabilityManager>,
    pub config: SentinelConfig,
}

impl HostCallHandler {
    pub fn new(capability_manager: Arc<CapabilityManager>, config: SentinelConfig) -> Self {
        Self {
            capability_manager,
            config,
        }
    }

    // ── Capability Request Handlers ─────────────────────────────────────

    pub async fn request_fs_read(
        &self,
        path: String,
        justification: String,
    ) -> Result<String, SentinelError> {
        info!(path = %path, justification = %justification, "Guest requesting fs.read capability");

        let canonical = self.canonicalize_and_validate_path(&path)?;

        let scope = CapabilityScope::FsPath {
            allowed_pattern: canonical.to_string_lossy().to_string(),
            read_only: true,
        };

        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn request_net_outbound(
        &self,
        url: String,
        method: String,
        justification: String,
    ) -> Result<String, SentinelError> {
        info!(url = %url, method = %method, justification = %justification,
              "Guest requesting net.outbound capability");

        let scope = CapabilityScope::NetUrl {
            allowed_url_pattern: url.clone(),
            methods: vec![method],
        };

        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn request_ui_observe(&self) -> Result<String, SentinelError> {
        info!("Guest requesting ui.observe capability");
        let scope = CapabilityScope::UiObserve;
        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn request_ui_dispatch(
        &self,
        event_type: String,
    ) -> Result<String, SentinelError> {
        info!(event_type = %event_type, "Guest requesting ui.dispatch capability");
        let scope = CapabilityScope::UiDispatch {
            allowed_event_types: vec![event_type],
        };
        let token = self.capability_manager.mint_token(scope).await?;
        Ok(token.id)
    }

    pub async fn release_capability(&self, token_id: String) -> bool {
        info!(token_id = %token_id, "Guest releasing capability");
        self.capability_manager.revoke_token(&token_id).await
    }

    // ── Token-Gated Operations ──────────────────────────────────────────

    pub async fn fs_read(
        &self,
        token_id: String,
        path: String,
    ) -> Result<Vec<u8>, SentinelError> {
        self.capability_manager
            .validate_token(&token_id, &path)
            .await?;

        let canonical = self.canonicalize_and_validate_path(&path)?;

        let metadata = tokio::fs::metadata(&canonical).await.map_err(|e| {
            SentinelError::GuestError {
                message: format!("Cannot stat file: {e}"),
            }
        })?;

        if metadata.len() as usize > self.config.filesystem.max_read_size {
            return Err(SentinelError::ResourceExhausted {
                resource: format!(
                    "File size {} exceeds limit {}",
                    metadata.len(),
                    self.config.filesystem.max_read_size
                ),
            });
        }

        let contents = tokio::fs::read(&canonical).await.map_err(|e| {
            SentinelError::GuestError {
                message: format!("Cannot read file: {e}"),
            }
        })?;

        info!(path = %path, size = contents.len(), "fs.read completed");
        Ok(contents)
    }

    pub async fn net_request(
        &self,
        token_id: String,
        url: String,
        method: String,
        _headers: Vec<(String, String)>,
        _body: Option<Vec<u8>>,
    ) -> Result<NetResponse, SentinelError> {
        self.capability_manager
            .validate_token(&token_id, &url)
            .await?;

        info!(url = %url, method = %method, "net.request — validated (stub response)");

        Ok(NetResponse {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: b"{}".to_vec(),
        })
    }

    pub async fn ui_get_state(
        &self,
        token_id: String,
    ) -> Result<String, SentinelError> {
        self.capability_manager
            .validate_token(&token_id, "ui:observe")
            .await?;

        info!("ui.observe — returning stub state");
        Ok(r#"{"screen": "main", "elements": []}"#.to_string())
    }

    pub async fn ui_send_event(
        &mut self,
        token_id: String,
        event_type: String,
        _payload: String,
    ) -> Result<bool, SentinelError> {
        self.capability_manager
            .validate_token(&token_id, &format!("ui:dispatch:{event_type}"))
            .await?;

        info!(event_type = %event_type, "ui.dispatch — event sent (stub)");
        Ok(true)
    }

    // ── Internal Helpers ────────────────────────────────────────────────

    fn canonicalize_and_validate_path(
        &self,
        path: &str,
    ) -> Result<std::path::PathBuf, SentinelError> {
        let requested = Path::new(path);

        let canonical = requested.canonicalize().map_err(|_| {
            SentinelError::PathEscapeAttempt {
                path: path.to_string(),
            }
        })?;

        let is_allowed = self
            .config
            .filesystem
            .allowed_read_dirs
            .iter()
            .any(|dir| {
                let d = dir.canonicalize().unwrap_or_else(|_| dir.clone());
                tracing::warn!("Checking if canonical {} starts with allowed {}", canonical.display(), d.display());
                canonical.starts_with(&d)
            });

        if !is_allowed {
            warn!(
                path = %path,
                canonical = %canonical.display(),
                "Path escape attempt blocked"
            );
            return Err(SentinelError::PathEscapeAttempt {
                path: canonical.to_string_lossy().to_string(),
            });
        }

        Ok(canonical)
    }
}

#[derive(Debug, Clone)]
pub struct NetResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
