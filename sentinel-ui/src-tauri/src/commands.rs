//! Tauri IPC Commands for the SENTINEL Dashboard.

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
pub struct ModelConfig { pub provider: String, pub model: String, pub api_key: Option<String> }

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
        ProviderInfo { id: "anthropic".into(), name: "Anthropic (Claude)".into(), requires_key: true, default_model: "claude-sonnet-4-20250514".into() },
        ProviderInfo { id: "deepseek".into(), name: "Deepseek".into(), requires_key: true, default_model: "deepseek-chat".into() },
        ProviderInfo { id: "grok".into(), name: "xAI (Grok)".into(), requires_key: true, default_model: "grok-2".into() },
        ProviderInfo { id: "google".into(), name: "Google (Gemini)".into(), requires_key: true, default_model: "gemini-2.0-flash".into() },
    ]
}

#[tauri::command]
pub async fn start_agent(
    app: AppHandle, state: State<'_, Mutex<AgentState>>,
    module_path: String, model_config: ModelConfig,
    allow_read: Vec<String>, allow_write: Vec<String>,
) -> Result<AgentResult, String> {
    let mut agent = state.lock().await;
    if agent.is_running { return Ok(AgentResult { success: false, message: "Agent is already running".into() }); }

    let mut config = SentinelConfig::default();
    config.engine.guest_module_path = PathBuf::from(&module_path);
    config.filesystem.allowed_read_dirs = allow_read.iter().map(PathBuf::from).collect();
    config.filesystem.allowed_write_dirs = allow_write.iter().map(PathBuf::from).collect();

    let api_key = model_config.api_key.unwrap_or_default();
    config.llm.model = model_config.model;
    config.llm.provider = match model_config.provider.to_lowercase().as_str() {
        "ollama" => sentinel_host::llm::LlmProvider::Ollama { base_url: "http://localhost:11434".into() },
        "openai" => sentinel_host::llm::LlmProvider::OpenAi { api_key: api_key.clone(), org_id: None },
        "anthropic" => sentinel_host::llm::LlmProvider::Anthropic { api_key: api_key.clone() },
        "deepseek" => sentinel_host::llm::LlmProvider::Deepseek { api_key: api_key.clone(), base_url: None },
        "grok" => sentinel_host::llm::LlmProvider::Grok { api_key: api_key.clone() },
        "google" => sentinel_host::llm::LlmProvider::Google { api_key: api_key.clone() },
        other => sentinel_host::llm::LlmProvider::OpenAiCompatible { api_key: api_key.clone(), base_url: other.to_string() },
    };

    agent.is_running = true;
    let hitl = Arc::new(sentinel_host::hitl::HitlBridge::new());
    let cap_mgr = Arc::new(sentinel_host::capabilities::CapabilityManager::new(config.clone()));

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

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = app_handle.emit("sentinel://log", LogEntry { level: "info".into(), target: "dashboard".into(), message: format!("Booting SENTINEL with module: {}", module_path) });
        match sentinel_host::engine::boot(config).await {
            Ok(()) => { let _ = app_handle.emit("sentinel://log", LogEntry { level: "info".into(), target: "dashboard".into(), message: "Agent completed successfully".into() }); }
            Err(e) => { let _ = app_handle.emit("sentinel://log", LogEntry { level: "error".into(), target: "dashboard".into(), message: format!("Agent error: {}", e) }); }
        }
        let state = app_handle.state::<Mutex<AgentState>>();
        state.lock().await.is_running = false;
        let _ = app_handle.emit("sentinel://agent-stopped", ());
    });

    Ok(AgentResult { success: true, message: "Agent started".into() })
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
