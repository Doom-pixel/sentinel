//! # sentinel-host — HITL Pre-flight Verification Bridge
//!
//! Implements the Human-in-the-Loop protocol. High-risk actions
//! generate an `ExecutionManifest` that is displayed to the user
//! and must be cryptographically signed before execution proceeds.

use ed25519_dalek::{Signer, SigningKey, VerifyingKey, Signature, Verifier};
use rand::rngs::OsRng;
use sentinel_shared::{ExecutionManifest, ManifestSignature, RiskLevel, SentinelError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Approval status for a manifest.
#[derive(Debug, Clone)]
pub enum ApprovalStatus {
    Pending,
    Approved(ManifestSignature),
    Rejected(String),
    TimedOut,
}

/// The HITL bridge — manages manifest submission, user approval, and signing.
pub struct HitlBridge {
    /// Ed25519 signing key for the host (generated on startup).
    signing_key: SigningKey,
    /// Corresponding verification key.
    verifying_key: VerifyingKey,
    /// Pending and resolved manifests.
    manifests: Arc<RwLock<HashMap<String, (ExecutionManifest, ApprovalStatus)>>>,
}

impl HitlBridge {
    /// Create a new HITL bridge with a fresh Ed25519 keypair.
    pub fn new() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        info!("HITL bridge initialized with Ed25519 keypair");
        Self {
            signing_key,
            verifying_key,
            manifests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Submit a manifest for user approval.
    pub async fn submit_manifest(
        &self,
        manifest: ExecutionManifest,
    ) -> Result<ApprovalStatus, SentinelError> {
        let manifest_id = manifest.id.clone();
        info!(
            manifest_id = %manifest_id,
            risk = ?manifest.risk_level,
            action = %manifest.action_description,
            "HITL: Manifest submitted for approval"
        );

        // Store the manifest as pending
        {
            let mut manifests = self.manifests.write().await;
            manifests.insert(
                manifest_id.clone(),
                (manifest.clone(), ApprovalStatus::Pending),
            );
        }

        // Display manifest to user and collect decision
        let approved = self.prompt_user(&manifest).await;

        if approved {
            let signature = self.sign_manifest(&manifest)?;
            let status = ApprovalStatus::Approved(signature);
            self.manifests
                .write()
                .await
                .get_mut(&manifest_id)
                .map(|(_, s)| *s = status.clone());
            info!(manifest_id = %manifest_id, "HITL: Manifest APPROVED");
            Ok(status)
        } else {
            let reason = "User rejected the action".to_string();
            let status = ApprovalStatus::Rejected(reason.clone());
            self.manifests
                .write()
                .await
                .get_mut(&manifest_id)
                .map(|(_, s)| *s = status.clone());
            warn!(manifest_id = %manifest_id, "HITL: Manifest REJECTED");
            Ok(status)
        }
    }

    /// Check the approval status of a previously submitted manifest.
    pub async fn check_status(&self, manifest_id: &str) -> Option<ApprovalStatus> {
        self.manifests
            .read()
            .await
            .get(manifest_id)
            .map(|(_, status)| status.clone())
    }

    /// Verify that a signature is valid for a given manifest.
    pub fn verify_signature(
        &self,
        manifest: &ExecutionManifest,
        signature: &ManifestSignature,
    ) -> Result<bool, SentinelError> {
        let manifest_bytes = serde_json::to_vec(manifest)?;

        let sig_bytes: [u8; 64] = signature
            .signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| SentinelError::InvalidSignature)?;

        let sig = Signature::from_bytes(&sig_bytes);

        let key_bytes: [u8; 32] = signature
            .signer_public_key
            .as_slice()
            .try_into()
            .map_err(|_| SentinelError::InvalidSignature)?;

        let verifying_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| SentinelError::InvalidSignature)?;

        match verifying_key.verify(&manifest_bytes, &sig) {
            Ok(()) => Ok(true),
            Err(_) => {
                error!(manifest_id = %manifest.id, "HITL: Signature verification FAILED");
                Ok(false)
            }
        }
    }

    /// Get the host's public verification key.
    pub fn public_key(&self) -> Vec<u8> {
        self.verifying_key.to_bytes().to_vec()
    }

    // ── Internal helpers ────────────────────────────────────────────────

    /// Sign a manifest with the host's Ed25519 key.
    fn sign_manifest(
        &self,
        manifest: &ExecutionManifest,
    ) -> Result<ManifestSignature, SentinelError> {
        let manifest_bytes = serde_json::to_vec(manifest)?;
        let signature = self.signing_key.sign(&manifest_bytes);

        Ok(ManifestSignature {
            manifest_id: manifest.id.clone(),
            signature_bytes: signature.to_bytes().to_vec(),
            signer_public_key: self.verifying_key.to_bytes().to_vec(),
        })
    }

    /// Display a manifest to the user and prompt for approval.
    async fn prompt_user(&self, manifest: &ExecutionManifest) -> bool {
        let risk_indicator = match manifest.risk_level {
            RiskLevel::Low => "🟢 LOW",
            RiskLevel::Medium => "🟡 MEDIUM",
            RiskLevel::High => "🟠 HIGH",
            RiskLevel::Critical => "🔴 CRITICAL",
        };

        println!();
        println!("╔══════════════════════════════════════════════════════════╗");
        println!("║          SENTINEL — Pre-flight Verification             ║");
        println!("╠══════════════════════════════════════════════════════════╣");
        println!("║ Manifest ID: {:<43}║", manifest.id);
        println!("║ Risk Level:  {:<43}║", risk_indicator);
        println!("╠══════════════════════════════════════════════════════════╣");
        println!("║ Action:                                                 ║");
        for line in textwrap_simple(&manifest.action_description, 54) {
            println!("║   {:<55}║", line);
        }
        println!("╠══════════════════════════════════════════════════════════╣");
        println!("║ Parameters:                                             ║");
        let params_str = serde_json::to_string_pretty(&manifest.parameters)
            .unwrap_or_else(|_| "{}".to_string());
        for line in params_str.lines().take(10) {
            println!("║   {:<55}║", truncate_str(line, 55));
        }
        println!("╚══════════════════════════════════════════════════════════╝");
        println!();

        use std::io::{self, Write};
        print!("  Approve this action? [y/N]: ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().eq_ignore_ascii_case("y")
    }
}

fn textwrap_simple(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    for word in text.split_whitespace() {
        if current_line.len() + word.len() + 1 > width {
            lines.push(current_line.clone());
            current_line.clear();
        }
        if !current_line.is_empty() {
            current_line.push(' ');
        }
        current_line.push_str(word);
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}
