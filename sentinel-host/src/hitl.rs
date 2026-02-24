//! # sentinel-host — HITL Pre-flight Verification Bridge
//!
//! Supports two approval modes:
//! - **Terminal**: Interactive stdin prompt (default, CLI mode)
//! - **Channel**: Async oneshot channel (for Tauri/Web UI integration)

use ed25519_dalek::{Signer, SigningKey, VerifyingKey, Signature, Verifier};
use rand::rngs::OsRng;
use sentinel_shared::{ExecutionManifest, ManifestSignature, RiskLevel, SentinelError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tracing::{info, warn, error};

#[derive(Debug, Clone)]
pub enum ApprovalStatus {
    Pending,
    Approved(ManifestSignature),
    Rejected(String),
    TimedOut,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ManifestInfo {
    pub id: String,
    pub action_description: String,
    pub parameters_json: String,
    pub risk_level: String,
}

impl From<&ExecutionManifest> for ManifestInfo {
    fn from(m: &ExecutionManifest) -> Self {
        Self {
            id: m.id.clone(),
            action_description: m.action_description.clone(),
            parameters_json: serde_json::to_string_pretty(&m.parameters).unwrap_or_default(),
            risk_level: format!("{:?}", m.risk_level),
        }
    }
}

pub type ApprovalCallback = Box<
    dyn Fn(ManifestInfo) -> tokio::sync::oneshot::Receiver<bool> + Send + Sync,
>;

pub struct HitlBridge {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    manifests: Arc<RwLock<HashMap<String, (ExecutionManifest, ApprovalStatus)>>>,
    approval_callback: Arc<Mutex<Option<ApprovalCallback>>>,
}

impl HitlBridge {
    pub fn new() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        info!("HITL bridge initialized with Ed25519 keypair");
        Self {
            signing_key, verifying_key,
            manifests: Arc::new(RwLock::new(HashMap::new())),
            approval_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_approval_callback(&self, callback: ApprovalCallback) {
        *self.approval_callback.lock().await = Some(callback);
        info!("HITL: External approval callback set (UI mode)");
    }

    pub async fn get_pending_manifests(&self) -> Vec<ManifestInfo> {
        self.manifests.read().await.iter()
            .filter(|(_, (_, s))| matches!(s, ApprovalStatus::Pending))
            .map(|(_, (m, _))| ManifestInfo::from(m))
            .collect()
    }

    pub async fn resolve_manifest(&self, manifest_id: &str, approved: bool) -> Result<ApprovalStatus, SentinelError> {
        let manifest = self.manifests.read().await.get(manifest_id).map(|(m, _)| m.clone());
        let manifest = manifest.ok_or_else(|| SentinelError::GuestError { message: format!("Manifest not found: {}", manifest_id) })?;

        if approved {
            let signature = self.sign_manifest(&manifest)?;
            let status = ApprovalStatus::Approved(signature);
            self.manifests.write().await.get_mut(manifest_id).map(|(_, s)| *s = status.clone());
            info!(manifest_id = %manifest_id, "HITL: Manifest APPROVED (external)");
            Ok(status)
        } else {
            let status = ApprovalStatus::Rejected("User rejected via UI".into());
            self.manifests.write().await.get_mut(manifest_id).map(|(_, s)| *s = status.clone());
            warn!(manifest_id = %manifest_id, "HITL: Manifest REJECTED (external)");
            Ok(status)
        }
    }

    pub async fn submit_manifest(&self, manifest: ExecutionManifest) -> Result<ApprovalStatus, SentinelError> {
        let manifest_id = manifest.id.clone();
        info!(manifest_id = %manifest_id, risk = ?manifest.risk_level, action = %manifest.action_description, "HITL: Manifest submitted");

        self.manifests.write().await.insert(manifest_id.clone(), (manifest.clone(), ApprovalStatus::Pending));

        let approved = {
            let cb = self.approval_callback.lock().await;
            if let Some(ref callback) = *cb {
                let info = ManifestInfo::from(&manifest);
                let rx = callback(info);
                drop(cb);
                match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
                    Ok(Ok(result)) => result,
                    Ok(Err(_)) => false,
                    Err(_) => {
                        let status = ApprovalStatus::TimedOut;
                        self.manifests.write().await.get_mut(&manifest_id).map(|(_, s)| *s = status.clone());
                        return Ok(status);
                    }
                }
            } else {
                drop(cb);
                self.prompt_terminal(&manifest).await
            }
        };

        if approved {
            let signature = self.sign_manifest(&manifest)?;
            let status = ApprovalStatus::Approved(signature);
            self.manifests.write().await.get_mut(&manifest_id).map(|(_, s)| *s = status.clone());
            info!(manifest_id = %manifest_id, "HITL: Manifest APPROVED");
            Ok(status)
        } else {
            let status = ApprovalStatus::Rejected("User rejected the action".into());
            self.manifests.write().await.get_mut(&manifest_id).map(|(_, s)| *s = status.clone());
            warn!(manifest_id = %manifest_id, "HITL: Manifest REJECTED");
            Ok(status)
        }
    }

    pub async fn check_status(&self, manifest_id: &str) -> Option<ApprovalStatus> {
        self.manifests.read().await.get(manifest_id).map(|(_, s)| s.clone())
    }

    pub fn verify_signature(&self, manifest: &ExecutionManifest, signature: &ManifestSignature) -> Result<bool, SentinelError> {
        let manifest_bytes = serde_json::to_vec(manifest)?;
        let sig_bytes: [u8; 64] = signature.signature_bytes.as_slice().try_into().map_err(|_| SentinelError::InvalidSignature)?;
        let sig = Signature::from_bytes(&sig_bytes);
        let key_bytes: [u8; 32] = signature.signer_public_key.as_slice().try_into().map_err(|_| SentinelError::InvalidSignature)?;
        let vk = VerifyingKey::from_bytes(&key_bytes).map_err(|_| SentinelError::InvalidSignature)?;
        match vk.verify(&manifest_bytes, &sig) {
            Ok(()) => Ok(true),
            Err(_) => { error!(manifest_id = %manifest.id, "HITL: Signature verification FAILED"); Ok(false) }
        }
    }

    pub fn public_key(&self) -> Vec<u8> { self.verifying_key.to_bytes().to_vec() }

    fn sign_manifest(&self, manifest: &ExecutionManifest) -> Result<ManifestSignature, SentinelError> {
        let manifest_bytes = serde_json::to_vec(manifest)?;
        let signature = self.signing_key.sign(&manifest_bytes);
        Ok(ManifestSignature {
            manifest_id: manifest.id.clone(),
            signature_bytes: signature.to_bytes().to_vec(),
            signer_public_key: self.verifying_key.to_bytes().to_vec(),
        })
    }

    async fn prompt_terminal(&self, manifest: &ExecutionManifest) -> bool {
        let risk = format!("{:?}", manifest.risk_level);
        println!("\n========================================================");
        println!("       SENTINEL \u2014 Pre-flight Verification");
        println!("========================================================");
        println!(" Manifest ID: {}", manifest.id);
        println!(" Risk Level:  {}", risk);
        println!("--------------------------------------------------------");
        println!(" Action: {}", manifest.action_description);
        println!("--------------------------------------------------------");
        let params_str = serde_json::to_string_pretty(&manifest.parameters).unwrap_or_default();
        for line in params_str.lines().take(10) { println!("   {}", line); }
        println!("========================================================\n");

        use std::io::{self, Write};
        print!("  Approve this action? [y/N]: ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().eq_ignore_ascii_case("y")
    }
}
