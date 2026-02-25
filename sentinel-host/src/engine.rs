//! # sentinel-host — Wasmtime Engine Setup
//!
//! Initializes the Wasmtime engine, configures resource limits,
//! creates the `Store` with fuel metering, and sets up the `Linker`
//! with the host-call function implementations using `bindgen!`.

use anyhow::{Context, Result};
use std::sync::Arc;
use wasmtime::*;
use tracing::{info, error};

use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView, ResourceTable};

use crate::capabilities::CapabilityManager;
use crate::config::SentinelConfig;
use crate::hitl::HitlBridge;
use crate::host_calls::HostCallHandler;

wasmtime::component::bindgen!({
    path: "../wit/sentinel.wit",
    world: "sentinel-guest",
    async: true,
});

/// Per-instance state stored in the Wasmtime `Store`.
///
/// This is the "jail" — all guest execution happens within the
/// constraints defined here.
pub struct SentinelState {
    pub limits: StoreLimits,
    pub host_calls: Arc<HostCallHandler>,
    pub hitl: Arc<HitlBridge>,
    pub llm: Arc<Box<dyn crate::llm::LlmBackend>>,
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub log_sender: Option<tokio::sync::mpsc::Sender<(String, String, String)>>,
}

impl WasiView for SentinelState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

#[async_trait::async_trait]
impl sentinel::agent::capabilities::Host for SentinelState {
    async fn request_fs_read(&mut self, path: String, justification: String) -> sentinel::agent::capabilities::CapabilityResult {
        let res = self.host_calls.request_fs_read(path, justification).await;
        match res {
            Ok(id) => sentinel::agent::capabilities::CapabilityResult::Granted(sentinel::agent::capabilities::CapabilityToken { id, is_valid: true }),
            Err(e) => sentinel::agent::capabilities::CapabilityResult::Denied(e.to_string()),
        }
    }

    async fn request_fs_write(&mut self, path: String, justification: String) -> sentinel::agent::capabilities::CapabilityResult {
        let res = self.host_calls.request_fs_write(path, justification).await;
        match res {
            Ok(id) => sentinel::agent::capabilities::CapabilityResult::Granted(sentinel::agent::capabilities::CapabilityToken { id, is_valid: true }),
            Err(e) => sentinel::agent::capabilities::CapabilityResult::Denied(e.to_string()),
        }
    }

    async fn request_net_outbound(&mut self, url: String, method: String, justification: String) -> sentinel::agent::capabilities::CapabilityResult {
        let res = self.host_calls.request_net_outbound(url, method, justification).await;
        match res {
            Ok(id) => sentinel::agent::capabilities::CapabilityResult::Granted(sentinel::agent::capabilities::CapabilityToken { id, is_valid: true }),
            Err(e) => sentinel::agent::capabilities::CapabilityResult::Denied(e.to_string()),
        }
    }

    async fn request_ui_observe(&mut self) -> sentinel::agent::capabilities::CapabilityResult {
        let res = self.host_calls.request_ui_observe().await;
        match res {
            Ok(id) => sentinel::agent::capabilities::CapabilityResult::Granted(sentinel::agent::capabilities::CapabilityToken { id, is_valid: true }),
            Err(e) => sentinel::agent::capabilities::CapabilityResult::Denied(e.to_string()),
        }
    }

    async fn request_ui_dispatch(&mut self, event_type: String) -> sentinel::agent::capabilities::CapabilityResult {
        let res = self.host_calls.request_ui_dispatch(event_type).await;
        match res {
            Ok(id) => sentinel::agent::capabilities::CapabilityResult::Granted(sentinel::agent::capabilities::CapabilityToken { id, is_valid: true }),
            Err(e) => sentinel::agent::capabilities::CapabilityResult::Denied(e.to_string()),
        }
    }

    async fn release_capability(&mut self, token_id: String) -> bool {
        self.host_calls.release_capability(token_id).await
    }

    async fn fs_read(&mut self, token_id: String, path: String) -> Result<Vec<u8>, String> {
        match self.host_calls.fs_read(token_id, path).await {
            Ok(content) => Ok(content),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn fs_write(&mut self, token_id: String, path: String, data: Vec<u8>) -> Result<bool, String> {
        match self.host_calls.fs_write(token_id, path, data).await {
            Ok(ok) => Ok(ok),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn fs_list_dir(&mut self, token_id: String, path: String) -> Result<Vec<String>, String> {
        match self.host_calls.fs_list_dir(token_id, path).await {
            Ok(entries) => Ok(entries),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn net_request(
        &mut self,
        token_id: String,
        url: String,
        method: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<sentinel::agent::capabilities::NetResponse, String> {
        match self.host_calls.net_request(token_id, url, method, headers, body).await {
            Ok(resp) => Ok(sentinel::agent::capabilities::NetResponse {
                status: resp.status,
                headers: resp.headers,
                body: resp.body,
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn ui_get_state(&mut self, token_id: String) -> Result<String, String> {
        match self.host_calls.ui_get_state(token_id).await {
            Ok(st) => Ok(st),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn ui_send_event(&mut self, token_id: String, event_type: String, payload: String) -> Result<bool, String> {
        match self.host_calls.ui_send_event(token_id, event_type, payload).await {
            Ok(ok) => Ok(ok),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[async_trait::async_trait]
impl sentinel::agent::hitl::Host for SentinelState {
    async fn submit_manifest(&mut self, manifest: sentinel::agent::hitl::ExecutionManifest) -> sentinel::agent::hitl::ApprovalResult {
        let m = sentinel_shared::ExecutionManifest {
            id: manifest.id,
            action_description: manifest.action_description,
            risk_level: match manifest.risk {
                sentinel::agent::hitl::RiskLevel::Low => sentinel_shared::RiskLevel::Low,
                sentinel::agent::hitl::RiskLevel::Medium => sentinel_shared::RiskLevel::Medium,
                sentinel::agent::hitl::RiskLevel::High => sentinel_shared::RiskLevel::High,
                sentinel::agent::hitl::RiskLevel::Critical => sentinel_shared::RiskLevel::Critical,
            },
            parameters: serde_json::from_str(&manifest.parameters_json).unwrap_or_default(),
            capability_token_id: None,
            created_at: std::time::SystemTime::now(),
            nonce: [0u8; 32],
        };

        let res = self.hitl.submit_manifest(m).await;
        match res {
            Ok(crate::hitl::ApprovalStatus::Approved(sig)) => {
                sentinel::agent::hitl::ApprovalResult::Approved(sentinel::agent::hitl::ManifestApproval {
                    manifest_id: sig.manifest_id,
                    signature: sig.signature_bytes,
                    approver_key: sig.signer_public_key,
                })
            }
            Ok(crate::hitl::ApprovalStatus::Rejected(reason)) => {
                sentinel::agent::hitl::ApprovalResult::Rejected(reason)
            }
            Ok(crate::hitl::ApprovalStatus::TimedOut) => {
                sentinel::agent::hitl::ApprovalResult::TimedOut
            }
            Err(e) => sentinel::agent::hitl::ApprovalResult::Rejected(e.to_string()),
            _ => sentinel::agent::hitl::ApprovalResult::Rejected("Unknown error".into()),
        }
    }

    async fn check_approval(&mut self, manifest_id: String) -> sentinel::agent::hitl::ApprovalResult {
        let res = self.hitl.check_status(&manifest_id).await;
        match res {
            Some(crate::hitl::ApprovalStatus::Approved(sig)) => {
                sentinel::agent::hitl::ApprovalResult::Approved(sentinel::agent::hitl::ManifestApproval {
                    manifest_id: sig.manifest_id,
                    signature: sig.signature_bytes,
                    approver_key: sig.signer_public_key,
                })
            }
            Some(crate::hitl::ApprovalStatus::Rejected(reason)) => {
                sentinel::agent::hitl::ApprovalResult::Rejected(reason)
            }
            Some(crate::hitl::ApprovalStatus::TimedOut) => {
                sentinel::agent::hitl::ApprovalResult::TimedOut
            }
            _ => sentinel::agent::hitl::ApprovalResult::Rejected("Pending or not found".to_string()),
        }
    }
}

#[async_trait::async_trait]
impl sentinel::agent::logging::Host for SentinelState {
    async fn log(&mut self, level: sentinel::agent::logging::LogLevel, target: String, message: String) {
        let level_str = match level {
            sentinel::agent::logging::LogLevel::Trace => { tracing::trace!(target = %target, "{}", message); "trace" },
            sentinel::agent::logging::LogLevel::Debug => { tracing::debug!(target = %target, "{}", message); "debug" },
            sentinel::agent::logging::LogLevel::Info => { tracing::info!(target = %target, "{}", message); "info" },
            sentinel::agent::logging::LogLevel::Warn => { tracing::warn!(target = %target, "{}", message); "warn" },
            sentinel::agent::logging::LogLevel::Error => { tracing::error!(target = %target, "{}", message); "error" },
        };
        
        if let Some(tx) = &self.log_sender {
            let _ = tx.try_send((level_str.to_string(), target, message));
        }
    }
}

#[async_trait::async_trait]
impl sentinel::agent::reasoning::Host for SentinelState {
    async fn complete(&mut self, messages: Vec<sentinel::agent::reasoning::ChatMessage>, max_tokens: Option<u32>, temperature: Option<f32>, response_format_json: Option<String>) -> Result<sentinel::agent::reasoning::CompletionResponse, String> {
        let req_messages = messages.into_iter().map(|m| crate::llm::ChatMessage {
            role: match m.role.as_str() {
                "system" => crate::llm::Role::System,
                "user" => crate::llm::Role::User,
                "assistant" => crate::llm::Role::Assistant,
                _ => crate::llm::Role::User,
            },
            content: m.content,
        }).collect();

        let req = crate::llm::CompletionRequest {
            messages: req_messages,
            max_tokens,
            temperature,
            response_format: response_format_json.and_then(|s| serde_json::from_str(&s).ok()),
        };

        if let Some(tx) = &self.log_sender {
            let provider = self.llm.provider_name();
            let _ = tx.try_send(("info".to_string(), "system".to_string(), format!("THOUGHT: Waiting for LLM response from {}...", provider)));
        }

        match self.llm.complete(req).await {
            Ok(resp) => Ok(sentinel::agent::reasoning::CompletionResponse {
                content: resp.content,
                model: resp.model,
                usage: sentinel::agent::reasoning::TokenUsage {
                    prompt_tokens: resp.usage.prompt_tokens,
                    completion_tokens: resp.usage.completion_tokens,
                    total_tokens: resp.usage.total_tokens,
                },
                finish_reason: resp.finish_reason,
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn get_provider_name(&mut self) -> String {
        self.llm.provider_name().to_string()
    }
}

pub fn create_engine(_config: &SentinelConfig) -> Result<Engine> {
    let mut engine_config = Config::new();
    engine_config.consume_fuel(true);
    engine_config.epoch_interruption(true);
    engine_config.cranelift_opt_level(OptLevel::Speed);
    engine_config.wasm_component_model(true);
    engine_config.async_support(true);

    let engine = Engine::new(&engine_config)
        .context("Failed to create Wasmtime engine")?;

    info!("Wasmtime engine created with async support, fuel metering, and epoch interruption");
    Ok(engine)
}

pub fn create_store(engine: &Engine, config: &SentinelConfig, state: SentinelState) -> Result<Store<SentinelState>> {
    let mut store = Store::new(engine, state);
    store.limiter(|state| &mut state.limits);

    if let Some(fuel) = config.engine.fuel_limit {
        store.set_fuel(fuel).context("Failed to set fuel limit")?;
        info!(fuel = fuel, "Fuel limit set");
    }

    store.set_epoch_deadline(1);
    Ok(store)
}

pub fn build_store_limits(config: &SentinelConfig) -> StoreLimits {
    StoreLimitsBuilder::new()
        .memory_size(config.engine.max_memory_bytes)
        .tables(config.engine.max_tables as usize)
        .table_elements(config.engine.max_table_elements as usize)
        .instances(100)
        .memories(20)
        .build()
}

pub fn load_module(engine: &Engine, config: &SentinelConfig) -> Result<component::Component> {
    let module_path = &config.engine.guest_module_path;
    info!(path = %module_path.display(), "Loading guest component");

    let component = component::Component::from_file(engine, module_path)
        .context(format!(
            "Failed to load guest component from '{}'",
            module_path.display()
        ))?;

    info!(path = %module_path.display(), "Guest component loaded and compiled");
    Ok(component)
}

pub fn setup_linker(engine: &Engine) -> Result<component::Linker<SentinelState>> {
    let mut linker = component::Linker::new(engine);

    wasmtime_wasi::add_to_linker_async(&mut linker)
        .context("Failed to add WASI bindings to linker")?;

    SentinelGuest::add_to_linker(&mut linker, |state: &mut SentinelState| state)
        .context("Failed to add guest bindings to linker")?;

    info!("Linker configured with host-call bindings");
    Ok(linker)
}

pub async fn boot(
    config: SentinelConfig,
    context_json: String,
    log_sender: Option<tokio::sync::mpsc::Sender<(String, String, String)>>,
    capability_manager: Arc<CapabilityManager>,
    hitl: Arc<HitlBridge>
) -> Result<()> {
    info!("SENTINEL boot sequence starting");

    let engine = create_engine(&config)?;
    let limits = build_store_limits(&config);

    let host_calls = Arc::new(HostCallHandler::new(
        capability_manager.clone(),
        config.clone(),
    ));
    
    let llm = Arc::new(crate::llm::create_backend(&config.llm)?);

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_env()
        .build();
    let table = ResourceTable::new();

    let state = SentinelState {
        limits,
        host_calls,
        hitl,
        llm,
        wasi,
        table,
        log_sender,
    };
    let mut store = create_store(&engine, &config, state)?;
    let linker = setup_linker(&engine)?;
    let component = load_module(&engine, &config)?;

    let instance = SentinelGuest::instantiate_async(&mut store, &component, &linker)
        .await
        .context("Failed to instantiate guest module")?;

    info!("Guest module instantiated successfully");

    let result: i32 = instance.call_run(&mut store, &context_json)
        .await
        .context("Guest execution failed")?;

    info!("Guest finished with exit code {}", result);
    info!("SENTINEL boot sequence complete ✓");
    
    Ok(())
}
