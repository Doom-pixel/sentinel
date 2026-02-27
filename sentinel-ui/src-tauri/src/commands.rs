//! Tauri commands for managing SENTINEL agents and state.
 
 use serde::{Deserialize, Serialize};
 use std::collections::HashMap;
 use tokio::sync::Mutex;
 use tauri::State;
 use bollard::Docker;
 use bollard::container::{Config, HostConfig, CreateContainerOptions, StartContainerOptions, LogOptions};
 use bollard::models::HostConfigLogConfig;
 use futures_util::StreamExt;
 
 #[derive(Default)]
 pub struct AgentState {
     pub active_agents: HashMap<String, String>, // ID -> ContainerID
     pub agent_logs: HashMap<String, Vec<LogEntry>>,
 }
 
 #[derive(Clone, Serialize, Deserialize, Debug)]
 pub struct LogEntry {
     pub level: String,
     pub target: String,
     pub message: String,
 }
 
 #[derive(Default)]
 pub struct HitlPendingSenders(pub Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>);
 
 #[tauri::command]
 pub async fn start_agent(
     state: State<'_, Mutex<AgentState>>,
     task: String,
     provider: String,
     model: String,
     api_key: String,
     target_dir: Option<String>,
     autonomy: String,
 ) -> Result<String, String> {
     let docker = Docker::connect_with_local_defaults().map_err(|e| e.to_string())?;
     let agent_id = format!("sentinel-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
 
     let mut env = vec![
         format!("SENTINEL_AGENT_ID={}", agent_id),
         format!("SENTINEL_TASK={}", task),
         format!("SENTINEL_PROVIDER={}", provider),
         format!("SENTINEL_MODEL={}", model),
         format!("SENTINEL_API_KEY={}", api_key),
         format!("SENTINEL_AUTONOMY={}", autonomy),
         "SENTINEL_CALLBACK_URL=http://host.docker.internal:9876".to_string(),
     ];
 
     let mut host_config = HostConfig {
         auto_remove: Some(true),
         extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
         ..Default::default()
     };
 
     if let Some(dir) = target_dir {
         if !dir.is_empty() {
             host_config.binds = Some(vec![format!("{}:/workspace", dir)]);
             env.push("SENTINEL_TARGET_DIR=/workspace".to_string());
         }
     }
 
     let config = Config {
         image: Some("sentinel-agent:latest".to_string()),
         env: Some(env),
         host_config: Some(host_config),
         ..Default::default()
     };
 
     docker.create_container(
         Some(CreateContainerOptions { name: &agent_id, platform: None }),
         config
     ).await.map_err(|e| e.to_string())?;
 
     docker.start_container(&agent_id, None::<StartContainerOptions<String>>)
         .await.map_err(|e| e.to_string())?;
 
     let mut s = state.lock().await;
     s.active_agents.insert(agent_id.clone(), agent_id.clone());
     s.agent_logs.insert(agent_id.clone(), Vec::new());
 
     // Spawn log follow task
     let state_clone = state.inner().clone();
     let agent_id_clone = agent_id.clone();
     let docker_clone = docker.clone();
 
     tokio::spawn(async move {
         let mut logs = docker_clone.logs(
             &agent_id_clone,
             Some(LogOptions {
                 follow: true,
                 stdout: true,
                 stderr: true,
                 ..Default::default()
             }),
         );
 
         while let Some(msg) = logs.next().await {
             if let Ok(m) = msg {
                 let text = String::from_utf8_lossy(&m.into_bytes()).to_string();
                 let mut s = state_clone.lock().await;
                 if let Some(agent_logs) = s.agent_logs.get_mut(&agent_id_clone) {
                     agent_logs.push(LogEntry {
                         level: "info".to_string(),
                         target: "container".to_string(),
                         message: text,
                     });
                 }
             }
         }
     });
 
     Ok(agent_id)
 }
 
 #[tauri::command]
 pub async fn get_novnc_port(agent_id: String) -> Result<u16, String> {
     // For now, noVNC is on 6080 inside the container, we should ideally map it
     // This is a placeholder since we currently use host networking or hardcoded ports
     Ok(6080)
 }
 
 #[tauri::command]
 pub async fn send_agent_message(
     state: State<'_, Mutex<AgentState>>,
     agent_id: String,
     message: String,
 ) -> Result<(), String> {
     let mut s = state.lock().await;
     if let Some(logs) = s.agent_logs.get_mut(&agent_id) {
         logs.push(LogEntry {
             level: "info".to_string(),
             target: "user".to_string(),
             message: format!("USER: {}", message),
         });
     }
     Ok(())
 }
 
 #[tauri::command]
 pub async fn get_active_tokens() -> Result<Vec<String>, String> {
     Ok(vec![])
 }
 
 #[tauri::command]
 pub async fn handle_hitl_approval(
     manifest_id: String,
     approved: bool,
     senders: State<'_, HitlPendingSenders>,
 ) -> Result<(), String> {
     let mut s = senders.0.lock().await;
     if let Some(tx) = s.remove(&manifest_id) {
         let _ = tx.send(approved);
     }
     Ok(())
 }
 
 #[tauri::command]
 pub async fn get_providers() -> Result<Vec<ProviderInfo>, String> {
     Ok(vec![
         ProviderInfo {
             id: "ollama".into(),
             name: "Ollama".into(),
             models: vec!["llama3.3:latest".into(), "qwen2.5:7b".into(), "deepseek-r1:8b".into()],
         },
         ProviderInfo {
             id: "openai".into(),
             name: "OpenAI".into(),
             models: vec!["gpt-4o".into(), "gpt-4o-mini".into(), "o3-mini".into()],
         },
         ProviderInfo {
             id: "anthropic".into(),
             name: "Anthropic".into(),
             models: vec!["claude-3-5-sonnet-20241022".into(), "claude-3-5-haiku-20241022".into()],
         },
         ProviderInfo {
             id: "google".into(),
             name: "Google Gemini".into(),
             models: vec!["gemini-1.5-pro".into(), "gemini-1.5-flash".into()],
         },
         ProviderInfo {
             id: "deepseek".into(),
             name: "Deepseek".into(),
             models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
         },
         ProviderInfo {
             id: "grok".into(),
             name: "xAI Grok".into(),
             models: vec!["grok-beta".into()],
         },
     ])
 }
 
 #[derive(Serialize)]
 pub struct ProviderInfo {
     pub id: String,
     pub name: String,
     pub models: Vec<String>,
 }
 
 #[tauri::command]
 pub async fn get_pending_manifests() -> Result<Vec<String>, String> {
     Ok(vec![])
 }
 
 #[tauri::command]
 pub async fn get_agent_logs(
     state: State<'_, Mutex<AgentState>>,
     agent_id: String,
 ) -> Result<Vec<LogEntry>, String> {
     let s = state.lock().await;
     if let Some(logs) = s.agent_logs.get(&agent_id) {
         Ok(logs.clone())
     } else {
         Err("Agent not found".to_string())
     }
 }
 
 #[tauri::command]
 pub async fn stop_agent(
     state: State<'_, Mutex<AgentState>>,
     agent_id: String,
 ) -> Result<(), String> {
     let docker = Docker::connect_with_local_defaults().map_err(|e| e.to_string())?;
     let _ = docker.stop_container(&agent_id, None).await;
     let mut s = state.lock().await;
     s.active_agents.remove(&agent_id);
     Ok(())
 }
