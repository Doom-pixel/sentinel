//! Tauri IPC Commands for the SENTINEL Dashboard.
//!
//! Bridges the React frontend to the sentinel-host engine via IPC commands.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use sentinel_host::config::SentinelConfig;
use sentinel_host::hitl::ManifestInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

pub struct AgentState {
    pub hitl: Option<Arc<sentinel_host::hitl::HitlBridge>>,
    pub capability_manager: Option<Arc<sentinel_host::capabilities::CapabilityManager>>,
    pub is_running: bool,
}

impl Default for AgentState {
    fn default() -> Self { Self { hitl: None, capability_manager: None, is_running: false } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo { pub id: String, pub name: String, pub requires_key: bool, pub default_model: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo { pub id: String, pub scope: String, pub is_valid: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult { pub success: bool, pub message: String }

#[tauri::command]
pub async fn get_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo { id: "ollama".into(), name: "Ollama (Local)".into(), requires_key: false, default_model: "llama3.1:8b".into() },
        ProviderInfo { id: "openai".into(), name: "OpenAI".into(), requires_key: true, default_model: "gpt-4o".into() },
        ProviderInfo { id: "anthropic".into(), name: "Anthropic".into(), requires_key: true, default_model: "claude-sonnet-4-20250514".into() },
        ProviderInfo { id: "deepseek".into(), name: "Deepseek".into(), requires_key: true, default_model: "deepseek-chat".into() },
        ProviderInfo { id: "grok".into(), name: "xAI (Grok)".into(), requires_key: true, default_model: "grok-2".into() },
        ProviderInfo { id: "google".into(), name: "Google (Gemini)".into(), requires_key: true, default_model: "gemini-2.0-flash".into() },
    ]
}

/// Boot the SENTINEL agent with the given target directory and task prompt.
#[tauri::command]
pub async fn start_agent(
    app: AppHandle,
    state: State<'_, Mutex<AgentState>>,
    provider: String,
    model: String,
    api_key: Option<String>,
    target_directory: String,
    task_prompt: String,
) -> Result<(), String> {
    let mut agent = state.lock().await;
    if agent.is_running {
        return Err("Agent is already running".into());
    }

    // Guaranteed execution: Embed the audited release guest module directly into the Tauri binary.
    // This removes the need for brittle filesystem crawling and workspace dependency at runtime.
    const GUEST_WASM: &[u8] = include_bytes!("../../../target/wasm32-wasip1/release/sentinel_guest.wasm");
    
    // Write it out to the OS temporary directory so Wasmtime can memory-map or compile it.
    let wasm_path = std::env::temp_dir().join("sentinel_guest.wasm");
    if let Err(e) = std::fs::write(&wasm_path, GUEST_WASM) {
        return Err(format!("Failed to write embedded guest module to temp storage: {}", e));
    }

    // Build config — target_directory is both the read and write scope
    let mut config = SentinelConfig::default();
    config.engine.guest_module_path = wasm_path;
    config.filesystem.allowed_read_dirs = vec![PathBuf::from(&target_directory)];
    config.filesystem.allowed_write_dirs = vec![PathBuf::from(&target_directory)];

    // Use provided API key, falling back to env vars
    let resolved_key = api_key.filter(|k| !k.is_empty()).unwrap_or_else(|| {
        match provider.as_str() {
            "openai" => std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            "deepseek" => std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
            "grok" => std::env::var("XAI_API_KEY").unwrap_or_default(),
            "google" => std::env::var("GOOGLE_API_KEY").unwrap_or_default(),
            _ => String::new(),
        }
    });

    config.llm.model = model.clone();
    config.llm.provider = match provider.as_str() {
        "ollama" => sentinel_host::llm::LlmProvider::Ollama { base_url: "http://localhost:11434".into() },
        "openai" => sentinel_host::llm::LlmProvider::OpenAi { api_key: resolved_key.clone(), org_id: None },
        "anthropic" => sentinel_host::llm::LlmProvider::Anthropic { api_key: resolved_key.clone() },
        "deepseek" => sentinel_host::llm::LlmProvider::Deepseek { api_key: resolved_key.clone(), base_url: None },
        "grok" => sentinel_host::llm::LlmProvider::Grok { api_key: resolved_key.clone() },
        "google" => sentinel_host::llm::LlmProvider::Google { api_key: resolved_key.clone() },
        other => sentinel_host::llm::LlmProvider::OpenAiCompatible { api_key: resolved_key.clone(), base_url: other.to_string() },
    };

    agent.is_running = true;

    let hitl = Arc::new(sentinel_host::hitl::HitlBridge::new());
    let cap_mgr = Arc::new(sentinel_host::capabilities::CapabilityManager::new(config.clone()));

    // Wire HITL to emit events to the frontend
    let app_handle = app.clone();
    hitl.set_approval_callback(Box::new(move |info: ManifestInfo| {
        let _ = app_handle.emit("sentinel://hitl-request", &info);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let app_handle2 = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let state = app_handle2.state::<Mutex<HitlPendingSenders>>();
            state.lock().await.pending.insert(info.id, tx);
        });
        rx
    })).await;

    agent.hitl = Some(hitl.clone());
    agent.capability_manager = Some(cap_mgr.clone());
    drop(agent);

    // Build context JSON for the guest
    let context_json = serde_json::json!({
        "target_directory": target_directory,
        "task_prompt": task_prompt,
    }).to_string();

    let app_handle = app.clone();
    let _ = app_handle.emit("sentinel://log", LogEntry {
        level: "info".into(), target: "sentinel".into(),
        message: format!("Booting agent — target: {}, model: {}", target_directory, model),
    });

    let result = sentinel_host::engine::boot(config, context_json).await;

    match result {
        Ok(()) => {
            let _ = app_handle.emit("sentinel://log", LogEntry {
                level: "info".into(), target: "sentinel".into(),
                message: "Agent completed successfully".into(),
            });
        }
        Err(ref e) => {
            let _ = app_handle.emit("sentinel://log", LogEntry {
                level: "error".into(), target: "sentinel".into(),
                message: format!("Agent error: {}", e),
            });
        }
    }

    let state_lock = app_handle.state::<Mutex<AgentState>>();
    state_lock.lock().await.is_running = false;
    let _ = app_handle.emit("sentinel://agent-stopped", ());

    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_active_tokens(state: State<'_, Mutex<AgentState>>) -> Result<Vec<TokenInfo>, String> {
    let agent = state.lock().await;
    if let Some(ref cap_mgr) = agent.capability_manager {
        let tokens = cap_mgr.list_tokens().await;
        Ok(tokens.into_iter().map(|t| TokenInfo { id: t.id.clone(), scope: format!("{:?}", t.scope), is_valid: t.is_valid() }).collect())
    } else { Ok(vec![]) }
}

#[tauri::command]
pub async fn get_pending_manifests(state: State<'_, Mutex<AgentState>>) -> Result<Vec<ManifestInfo>, String> {
    let agent = state.lock().await;
    if let Some(ref hitl) = agent.hitl { Ok(hitl.get_pending_manifests().await) } else { Ok(vec![]) }
}

#[tauri::command]
pub async fn handle_hitl_approval(app: AppHandle, manifest_id: String, approved: bool) -> Result<AgentResult, String> {
    let state = app.state::<Mutex<HitlPendingSenders>>();
    let mut senders = state.lock().await;
    if let Some(tx) = senders.pending.remove(&manifest_id) {
        let _ = tx.send(approved);
        Ok(AgentResult { success: true, message: format!("Manifest {} {}", manifest_id, if approved { "approved" } else { "rejected" }) })
    } else {
        Ok(AgentResult { success: false, message: format!("No pending manifest: {}", manifest_id) })
    }
}

pub struct HitlPendingSenders { pub pending: std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>> }
impl Default for HitlPendingSenders { fn default() -> Self { Self { pending: std::collections::HashMap::new() } } }

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry { pub level: String, pub target: String, pub message: String }
