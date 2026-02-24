//! # sentinel-host — Capability Manager
//!
//! Implements the capability-based security model. The Guest must request
//! ephemeral tokens from this manager before accessing any host resource.
//! Tokens are scoped, time-limited, and revocable.

use sentinel_shared::{CapabilityScope, CapabilityToken, SentinelError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::SentinelConfig;

/// The capability manager — mints, validates, and revokes tokens.
pub struct CapabilityManager {
    /// Active tokens indexed by ID.
    tokens: Arc<RwLock<HashMap<String, CapabilityToken>>>,
    /// Used nonces to prevent replay attacks.
    used_nonces: Arc<RwLock<std::collections::HashSet<[u8; 32]>>>,
    /// Host configuration for policy enforcement.
    config: SentinelConfig,
    /// Default token TTL.
    default_ttl: Duration,
}

impl CapabilityManager {
    /// Create a new capability manager.
    pub fn new(config: SentinelConfig) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            used_nonces: Arc::new(RwLock::new(std::collections::HashSet::new())),
            config,
            default_ttl: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Mint a new capability token for the given scope.
    ///
    /// Returns `Err` if the requested scope violates policy.
    pub async fn mint_token(
        &self,
        scope: CapabilityScope,
    ) -> Result<CapabilityToken, SentinelError> {
        // Validate the scope against policy
        self.validate_scope(&scope)?;

        let token = CapabilityToken {
            id: generate_token_id(),
            scope,
            issued_at: SystemTime::now(),
            ttl: self.default_ttl,
            revoked: false,
        };

        info!(token_id = %token.id, "Capability token minted");
        self.tokens.write().await.insert(token.id.clone(), token.clone());

        Ok(token)
    }

    /// Validate that a token is still active and covers the requested operation.
    pub async fn validate_token(
        &self,
        token_id: &str,
        requested_resource: &str,
    ) -> Result<CapabilityToken, SentinelError> {
        let tokens = self.tokens.read().await;
        let token = tokens.get(token_id).ok_or_else(|| SentinelError::CapabilityDenied {
            reason: format!("Unknown token: {token_id}"),
        })?;

        if token.revoked {
            return Err(SentinelError::TokenRevoked {
                token_id: token_id.to_string(),
            });
        }

        if !token.is_valid() {
            return Err(SentinelError::TokenExpired {
                token_id: token_id.to_string(),
            });
        }

        // Validate the requested resource against the token scope
        self.check_resource_against_scope(&token.scope, requested_resource)?;

        Ok(token.clone())
    }

    /// Revoke a token immediately.
    pub async fn revoke_token(&self, token_id: &str) -> bool {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.get_mut(token_id) {
            token.revoked = true;
            warn!(token_id = %token_id, "Capability token revoked");
            true
        } else {
            false
        }
    }

    /// Record a nonce as used (replay prevention).
    pub async fn record_nonce(&self, nonce: [u8; 32]) -> Result<(), SentinelError> {
        let mut nonces = self.used_nonces.write().await;
        if !nonces.insert(nonce) {
            return Err(SentinelError::NonceReuse);
        }
        Ok(())
    }

    /// Purge expired tokens (should be called periodically).
    pub async fn purge_expired(&self) -> usize {
        let mut tokens = self.tokens.write().await;
        let before = tokens.len();
        tokens.retain(|_, t| t.is_valid());
        let purged = before - tokens.len();
        if purged > 0 {
            info!(count = purged, "Purged expired capability tokens");
        }
        purged
    }

    // ── Internal helpers ────────────────────────────────────────────────

    /// Check that a requested scope is allowed by policy.
    fn validate_scope(&self, scope: &CapabilityScope) -> Result<(), SentinelError> {
        match scope {
            CapabilityScope::FsPath { allowed_pattern, .. } => {
                // Ensure the requested path pattern falls within allowed directories
                let requested = std::path::Path::new(allowed_pattern);
                let is_allowed = self.config.filesystem.allowed_read_dirs.iter().any(|dir| {
                    let dir_canon = dir.canonicalize().unwrap_or_else(|_| dir.clone());
                    requested.starts_with(&dir_canon)
                });
                if !is_allowed {
                    return Err(SentinelError::PathEscapeAttempt {
                        path: allowed_pattern.clone(),
                    });
                }
            }
            CapabilityScope::NetUrl { allowed_url_pattern, .. } => {
                let is_whitelisted = self
                    .config
                    .network
                    .url_whitelist
                    .iter()
                    .any(|wl| url_matches_pattern(allowed_url_pattern, wl));
                if !is_whitelisted {
                    return Err(SentinelError::UrlNotWhitelisted {
                        url: allowed_url_pattern.clone(),
                    });
                }
            }
            CapabilityScope::UiObserve | CapabilityScope::UiDispatch { .. } => {
                // UI capabilities are always allowed at the scope level;
                // individual operations are checked at dispatch time.
            }
        }
        Ok(())
    }

    /// Verify that a specific resource access is covered by a token scope.
    fn check_resource_against_scope(
        &self,
        scope: &CapabilityScope,
        resource: &str,
    ) -> Result<(), SentinelError> {
        match scope {
            CapabilityScope::FsPath { allowed_pattern, .. } => {
                // Canonicalize and check path containment
                let resource_path = std::path::Path::new(resource).canonicalize().map_err(|_| {
                    SentinelError::PathEscapeAttempt {
                        path: resource.to_string(),
                    }
                })?;
                let scope_path = std::path::Path::new(allowed_pattern);
                if !resource_path.starts_with(&scope_path) {
                    return Err(SentinelError::PathEscapeAttempt {
                        path: resource.to_string(),
                    });
                }
            }
            CapabilityScope::NetUrl { allowed_url_pattern, .. } => {
                if !url_matches_pattern(resource, allowed_url_pattern) {
                    return Err(SentinelError::UrlNotWhitelisted {
                        url: resource.to_string(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// ─── Utility Functions ──────────────────────────────────────────────────────

/// Generate a cryptographically random token ID.
fn generate_token_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    hex_encode(&bytes)
}

/// Simple hex encoding.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Simple URL pattern matching (supports trailing `*` wildcard).
fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        url.starts_with(prefix)
    } else {
        url == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_matches_pattern() {
        assert!(url_matches_pattern(
            "https://api.example.com/v1/chat",
            "https://api.example.com/*"
        ));
        assert!(!url_matches_pattern(
            "https://evil.com/steal",
            "https://api.example.com/*"
        ));
        assert!(url_matches_pattern(
            "https://exact.com/path",
            "https://exact.com/path"
        ));
    }

    #[test]
    fn test_hex_encode() {
        let bytes = [0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(hex_encode(&bytes), "deadbeef");
    }
}
