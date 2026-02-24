use anyhow::{Context, Result};
use wasmtime::{component::*, Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiImpl, WasiView, ResourceTable};

use crate::config::SentinelConfig;
use crate::capabilities::CapabilityManager;
use crate::hitl::HitlBridge;
use crate::host_calls::HostCallHandler;
use std::sync::Arc;
use tracing::info;

bindgen!({
    world: "sentinel-guest",
    path: "../wit",
    async: true
});

pub struct SentinelState {
    pub limits: StoreLimits,
    pub host_calls: Arc<HostCallHandler>,
    pub hitl: Arc<HitlBridge>,
    pub llm: Arc<Box<dyn crate::llm::LlmBackend>>,
    pub wasi: WasiCtx,
    pub table: ResourceTable,
}

impl WasiView for SentinelState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

// Ensure our state implements the generated trait for the host functions
impl exports::sentinel::agent::capabilities::Host for SentinelState {
    async fn request_fs_read(
        &mut self,
        path: String,
        reason: String,
    ) -> wasmtime::Result<exports::sentinel::agent::capabilities::CapabilityResult> {
        let result = self.host_calls.request_fs_read(path, reason).await?;
        Ok(result)
    }

    async fn request_fs_write(
        &mut self,
        path: String,
        reason: String,
    ) -> wasmtime::Result<exports::sentinel::agent::capabilities::CapabilityResult> {
        let result = self.host_calls.request_fs_write(path, reason).await?;
        Ok(result)
    }

    async fn request_network(
        &mut self,
        url: String,
        reason: String,
    ) -> wasmtime::Result<exports::sentinel::agent::capabilities::CapabilityResult> {
        let result = self.host_calls.request_network(url, reason).await?;
        Ok(result)
    }

    async fn release_capability(&mut self, token_id: String) -> wasmtime::Result<()> {
        self.host_calls.release_capability(token_id).await?;
        Ok(())
    }

    async fn fs_read(
        &mut self,
        token_id: String,
        path: String,
    ) -> wasmtime::Result<Result<Vec<u8>, String>> {
        let result = self.host_calls.fs_read(token_id, path).await;
        Ok(result)
    }

    async fn fs_write(
        &mut self,
        token_id: String,
        path: String,
        data: Vec<u8>,
    ) -> wasmtime::Result<Result<(), String>> {
        let result = self.host_calls.fs_write(token_id, path, data).await;
        Ok(result)
    }

    async fn fs_list_dir(
        &mut self,
        token_id: String,
        path: String,
    ) -> wasmtime::Result<Result<Vec<String>, String>> {
        let result = self.host_calls.fs_list_dir(token_id, path).await;
        Ok(result)
    }

    async fn http_get(
        &mut self,
        token_id: String,
        url: String,
    ) -> wasmtime::Result<Result<Vec<u8>, String>> {
        let result = self.host_calls.http_get(token_id, url).await;
        Ok(result)
    }
    
    async fn http_post(
        &mut self,
        token_id: String,
        url: String,
        body: Vec<u8>,
    ) -> wasmtime::Result<Result<Vec<u8>, String>> {
        let result = self.host_calls.http_post(token_id, url, body).await;
        Ok(result)
    }

    async fn log(
        &mut self,
        level: exports::sentinel::agent::capabilities::LogLevel,
        target: String,
        message: String,
    ) -> wasmtime::Result<()> {
        let log_level = match level {
            exports::sentinel::agent::capabilities::LogLevel::Debug => "DEBUG",
            exports::sentinel::agent::capabilities::LogLevel::Info => "INFO",
            exports::sentinel::agent::capabilities::LogLevel::Warn => "WARN",
            exports::sentinel::agent::capabilities::LogLevel::Error => "ERROR",
        };
        // Print safely via host logging
        match level {
            exports::sentinel::agent::capabilities::LogLevel::Debug => {
                tracing::debug!(target: &target, "[{}] {}", log_level, message);
            }
            exports::sentinel::agent::capabilities::LogLevel::Info => {
                tracing::info!(target: &target, "[{}] {}", log_level, message);
            }
            exports::sentinel::agent::capabilities::LogLevel::Warn => {
                tracing::warn!(target: &target, "[{}] {}", log_level, message);
            }
            exports::sentinel::agent::capabilities::LogLevel::Error => {
                tracing::error!(target: &target, "[{}] {}", log_level, message);
            }
        }
        
        Ok(())
    }
}

// Implement LLM bindings
impl exports::sentinel::agent::llm::Host for SentinelState {
    async fn get_provider_name(&mut self) -> wasmtime::Result<String> {
        Ok(self.llm.provider_name())
    }

    async fn complete(
        &mut self,
        messages: Vec<exports::sentinel::agent::llm::ChatMessage>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        stop_sequences: Option<Vec<String>>,
    ) -> wasmtime::Result<Result<String, String>> {
        let host_messages = messages.into_iter().map(|m| crate::llm::ChatMessage {
            role: m.role,
            content: m.content,
        }).collect();

        match self.llm.complete(host_messages, max_tokens, temperature, stop_sequences).await {
            Ok(res) => Ok(Ok(res)),
            Err(e) => Ok(Err(e.to_string())),
        }
    }
}

fn create_engine(config: &SentinelConfig) -> Result<Engine> {
    let mut wasm_config = Config::new();
    
    // Required for WASI and our component
    wasm_config.wasm_component_model(true);
    wasm_config.async_support(true);
    
    // Security: limit memory and execution time
    wasm_config.consume_fuel(config.engine.fuel_limit.is_some());
    // Remove WASM module/instance limits as they block compilation
    
    Engine::new(&wasm_config).context("Failed to create Wasmtime engine")
}

fn build_store_limits(config: &SentinelConfig) -> StoreLimits {
    StoreLimitsBuilder::new()
        .memory_size(config.engine.max_memory_bytes)
        // Set table elements limit high enough for WASI and stdlib
        .table_elements(100_000)
        .instances(1000)
        .memories(100)
        .tables(100)
        .build()
}

fn create_store(engine: &Engine, config: &SentinelConfig, state: SentinelState) -> Result<Store<SentinelState>> {
    let mut store = Store::new(engine, state);
    store.limiter(|state| &mut state.limits);
    
    if let Some(fuel) = config.engine.fuel_limit {
        store.set_fuel(fuel).context("Failed to set initial fuel")?;
    }
    
    Ok(store)
}

fn load_module(engine: &Engine, config: &SentinelConfig) -> Result<Component> {
    let path = &config.engine.guest_module_path;
    info!("Loading guest component from {}", path.display());
    
    if !path.exists() {
        anyhow::bail!("Guest component not found at {}. Build it first!", path.display());
    }
    
    Component::from_file(engine, path).context("Failed to parse WASM component")
}

fn setup_linker(engine: &Engine) -> Result<Linker<SentinelState>> {
    let mut linker = Linker::new(engine);
    // Add WASI imports
    wasmtime_wasi::add_to_linker_async(&mut linker).context("Failed to link WASI")?;
    // Add our custom imports
    SentinelGuest::add_to_linker(&mut linker, |state: &mut SentinelState| state)
        .context("Failed to link custom capabilities")?;
        
    Ok(linker)
}

pub async fn boot(config: SentinelConfig, context_json: String) -> Result<()> {
    info!("SENTINEL boot sequence starting");

    let engine = create_engine(&config)?;
    let limits = build_store_limits(&config);

    let capability_manager = Arc::new(CapabilityManager::new(config.clone()));
    let hitl = Arc::new(HitlBridge::new());
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
