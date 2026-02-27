//! # SENTINEL Dashboard — Tauri Commands
//!
//! IPC command handlers for the React frontend.
//! Uses Docker (via bollard) to manage agent containers.

use bollard::Docker;
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions, LogOutput, LogsOptions};
use bollard::models::HostConfig;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use uuid::Uuid;

// ── Shared State ────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct AgentState {
    pub active_agents: HashMap<String, AgentInfo>,
}

pub struct AgentInfo {
    pub container_id: String,
    pub target_directory: String,
    pub provider: String,
    pub model: String,
    pub novnc_port: u16,
}

#[derive(Default)]
pub struct HitlPendingSenders {
    pub pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
}

// ── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenInfo {
    pub id: String,
    pub scope: String,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
    pub requires_key: bool,
    pub default_model: String,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_agent(
    app: AppHandle,
    state: State<'_, Mutex<AgentState>>,
    target_directory: String,
    task_prompt: String,
    provider: String,
    model: String,
    api_key: String,
    // Resource limits from Settings
    max_memory_mb: Option<u64>,
    autonomy_level: Option<String>,
    network_timeout_secs: Option<u64>,
    // Notification webhooks from Settings
    discord_url: Option<String>,
    slack_url: Option<String>,
    telegram_url: Option<String>,
) -> Result<String, String> {
    let resolved_agent_id = Uuid::new_v4().to_string();

    // Connect to Docker
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| format!("Failed to connect to Docker: {}. Is Docker Desktop running?", e))?;

    // Verify Docker is available
    docker.ping().await
        .map_err(|e| format!("Docker is not responding: {}. Make sure Docker Desktop is running.", e))?;

    let _ = app.emit("sentinel://log", LogEntry {
        level: "info".into(),
        target: format!("{}::system", resolved_agent_id),
        message: "Docker connection established ✓".into(),
    });

    // Resolve API key — check env var if not provided directly
    let resolved_key = if api_key.is_empty() {
        std::env::var("SENTINEL_API_KEY").unwrap_or_default()
    } else {
        api_key
    };

    // Calculate memory limit for container (default: 512 MB)
    let memory_limit = max_memory_mb.unwrap_or(512) * 1024 * 1024;

    // Convert target directory to a Docker-mountable path
    let has_workspace = !target_directory.is_empty() && target_directory != ".";
    let host_path = target_directory.replace('\\', "/");

    // Determine autonomy level
    let autonomy = autonomy_level.unwrap_or_else(|| "read_report".to_string());
    let is_read_only = autonomy == "read_only";

    // Build environment variables for the agent
    let mut env_vars = vec![
        format!("SENTINEL_CALLBACK_URL=http://host.docker.internal:9876"),
        format!("SENTINEL_AGENT_ID={}", resolved_agent_id),
        format!("SENTINEL_PROVIDER={}", provider),
        format!("SENTINEL_MODEL={}", model),
        format!("SENTINEL_API_KEY={}", resolved_key),
        format!("SENTINEL_TARGET_DIR=/workspace"),
        format!("SENTINEL_TASK={}", task_prompt),
        format!("SENTINEL_AUTONOMY={}", autonomy),
    ];

    // Add optional config
    if let Some(v) = network_timeout_secs { env_vars.push(format!("SENTINEL_NETWORK_TIMEOUT={}", v)); }
    if let Some(ref v) = discord_url { if !v.is_empty() { env_vars.push(format!("SENTINEL_DISCORD_URL={}", v)); } }
    if let Some(ref v) = slack_url { if !v.is_empty() { env_vars.push(format!("SENTINEL_SLACK_URL={}", v)); } }
    if let Some(ref v) = telegram_url { if !v.is_empty() { env_vars.push(format!("SENTINEL_TELEGRAM_URL={}", v)); } }

    let env_strs: Vec<&str> = env_vars.iter().map(|s| s.as_str()).collect();

    // Create the container with noVNC port exposed
    let novnc_host_port = 6080 + (resolved_agent_id.as_bytes()[0] as u16 % 100);
    let mut port_bindings = std::collections::HashMap::new();
    port_bindings.insert(
        "6080/tcp".to_string(),
        Some(vec![bollard::models::PortBinding {
            host_ip: Some("127.0.0.1".to_string()),
            host_port: Some(novnc_host_port.to_string()),
        }]),
    );

    let mut binds = Vec::new();
    if has_workspace {
        binds.push(format!("{}:/workspace{}", host_path, if is_read_only { ":ro" } else { "" }));
    }

    let container_config = Config {
        image: Some("sentinel-agent:latest"),
        env: Some(env_strs),
        host_config: Some(HostConfig {
            binds: if binds.is_empty() { None } else { Some(binds) },
            memory: Some(memory_limit as i64),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            port_bindings: Some(port_bindings),
            ..Default::default()
        }),
        ..Default::default()
    };

    let container_name = format!("sentinel-{}", &resolved_agent_id[..8]);
    let container = docker.create_container(
        Some(CreateContainerOptions { name: &container_name, platform: None }),
        container_config,
    ).await.map_err(|e| format!("Failed to create container: {}", e))?;

    let container_id = container.id.clone();

    // Store agent info
    {
        let mut st = state.lock().await;
        st.active_agents.insert(resolved_agent_id.clone(), AgentInfo {
            container_id: container_id.clone(),
            target_directory: target_directory.clone(),
            provider: provider.clone(),
            model: model.clone(),
            novnc_port: novnc_host_port,
        });
    }

    let _ = app.emit("sentinel://log", LogEntry {
        level: "info".into(),
        target: format!("{}::system", resolved_agent_id),
        message: format!("Container {} created, starting agent...", &container_name),
    });

    // Start the container
    docker.start_container(&container_id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| format!("Failed to start container: {}", e))?;

    let _ = app.emit("sentinel://log", LogEntry {
        level: "info".into(),
        target: format!("{}::system", resolved_agent_id),
        message: format!("🐳 Agent {} running in Docker container", resolved_agent_id),
    });

    // Spawn a task to follow container logs and forward to the frontend
    let app_handle = app.clone();
    let id_for_logs = resolved_agent_id.clone();
    let docker_for_logs = Docker::connect_with_local_defaults().unwrap();
    let container_id_for_logs = container_id.clone();
    let target_dir_for_logs = target_directory.clone();

    tokio::spawn(async move {
        let mut log_stream = docker_for_logs.logs(
            &container_id_for_logs,
            Some(LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        );

        while let Some(log_result) = log_stream.next().await {
            match log_result {
                Ok(output) => {
                    let msg = match output {
                        LogOutput::StdOut { message } => String::from_utf8_lossy(&message).to_string(),
                        LogOutput::StdErr { message } => String::from_utf8_lossy(&message).to_string(),
                        _ => continue,
                    };
                    let msg = msg.trim().to_string();
                    if msg.is_empty() { continue; }

                    // Parse structured log lines: [LEVEL] target message
                    let (level, message) = if msg.starts_with('[') {
                        if let Some(end_bracket) = msg.find(']') {
                            let level = msg[1..end_bracket].to_lowercase();
                            let rest = msg[end_bracket+1..].trim().to_string();
                            (level, rest)
                        } else {
                            ("info".to_string(), msg)
                        }
                    } else {
                        ("info".to_string(), msg)
                    };

                    let _ = app_handle.emit("sentinel://log", LogEntry {
                        level,
                        target: format!("{}::agent", id_for_logs),
                        message,
                    });
                }
                Err(e) => {
                    let _ = app_handle.emit("sentinel://log", LogEntry {
                        level: "error".into(),
                        target: format!("{}::system", id_for_logs),
                        message: format!("Log stream error: {}", e),
                    });
                    break;
                }
            }
        }

        // Container has stopped — try to read and emit the report
        let report_path = format!("{}/SENTINEL_REPORT.md", target_dir_for_logs);
        if let Ok(report_content) = tokio::fs::read_to_string(&report_path).await {
            let _ = app_handle.emit("sentinel://log", LogEntry {
                level: "info".into(),
                target: format!("{}::agent", id_for_logs),
                message: "THOUGHT: Here is the audit report:".into(),
            });
            // Send report in manageable chunks so it renders well in chat
            for section in report_content.split("\n## ") {
                let text = if section.starts_with('#') {
                    section.to_string()
                } else {
                    format!("## {}", section)
                };
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let _ = app_handle.emit("sentinel://log", LogEntry {
                        level: "info".into(),
                        target: format!("{}::agent", id_for_logs),
                        message: format!("THOUGHT: {}", trimmed),
                    });
                }
            }
        }

        let _ = app_handle.emit("sentinel://log", LogEntry {
            level: "info".into(),
            target: format!("{}::system", id_for_logs),
            message: "Agent container stopped".into(),
        });
        let _ = app_handle.emit("sentinel://agent-stopped", id_for_logs);
    });

    Ok(resolved_agent_id)
}

#[tauri::command]
pub async fn get_novnc_port(
    state: State<'_, Mutex<AgentState>>,
    agent_id: String,
) -> Result<u16, String> {
    let st = state.lock().await;
    match st.active_agents.get(&agent_id) {
        Some(info) => Ok(info.novnc_port),
        None => Err("Agent not found".to_string()),
    }
}

#[tauri::command]
pub async fn get_active_tokens(_state: State<'_, Mutex<AgentState>>) -> Result<Vec<TokenInfo>, String> {
    // In Docker mode, we don't have granular capability tokens.
    // Return container-level permissions.
    Ok(vec![
        TokenInfo { id: "docker-fs".into(), scope: "filesystem (mounted volume)".into(), is_valid: true },
        TokenInfo { id: "docker-net".into(), scope: "network (LLM API access)".into(), is_valid: true },
    ])
}

#[tauri::command]
pub async fn handle_hitl_approval(
    _state: State<'_, HitlPendingSenders>,
    _manifest_id: String,
    _approved: bool,
) -> Result<(), String> {
    // HITL is not yet implemented in Docker mode
    Ok(())
}

#[tauri::command]
pub async fn get_providers() -> Result<Vec<ProviderInfo>, String> {
    Ok(vec![
        ProviderInfo { id: "ollama".into(), name: "Ollama (Local)".into(), models: vec!["llama3.3:latest".into(), "llama3.1:8b".into(), "qwen2.5:7b".into(), "mistral:7b".into(), "deepseek-r1:8b".into()], requires_key: false, default_model: "llama3.3:latest".into() },
        ProviderInfo { id: "openai".into(), name: "OpenAI".into(), models: vec!["gpt-5.2".into(), "gpt-4.1".into(), "gpt-4.1-mini".into(), "gpt-4.1-nano".into(), "o3-mini".into()], requires_key: true, default_model: "gpt-5.2".into() },
        ProviderInfo { id: "anthropic".into(), name: "Anthropic".into(), models: vec!["claude-opus-4.6".into(), "claude-sonnet-4.6".into(), "claude-sonnet-4-20250514".into(), "claude-3.5-haiku-20241022".into()], requires_key: true, default_model: "claude-sonnet-4.6".into() },
        ProviderInfo { id: "deepseek".into(), name: "Deepseek".into(), models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()], requires_key: true, default_model: "deepseek-chat".into() },
        ProviderInfo { id: "grok".into(), name: "xAI (Grok)".into(), models: vec!["grok-4.20".into(), "grok-3".into(), "grok-3-mini".into()], requires_key: true, default_model: "grok-4.20".into() },
        ProviderInfo { id: "google".into(), name: "Google (Gemini)".into(), models: vec!["gemini-3.1".into(), "gemini-3-flash".into(), "gemini-2.5-pro".into(), "gemini-2.5-flash".into()], requires_key: true, default_model: "gemini-3-flash".into() },
    ])
}

#[tauri::command]
pub async fn send_agent_message(
    app: AppHandle,
    _state: State<'_, Mutex<AgentState>>,
    agent_id: String,
    message: String,
) -> Result<(), String> {
    // Emit the user message as a log entry so the agent can see it
    let _ = app.emit("sentinel://log", LogEntry {
        level: "info".into(),
        target: format!("{}::user", agent_id),
        message: format!("USER: {}", message),
    });
    // TODO: forward message to running container via callback server
    Ok(())
}

#[tauri::command]
pub async fn get_pending_manifests() -> Result<Vec<serde_json::Value>, String> {
    Ok(vec![])
}
