//! SENTINEL Dashboard — Tauri Backend
//!
//! Bridges the React frontend to Docker-based agent containers via IPC commands.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use sentinel_ui_lib::commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(tokio::sync::Mutex::new(commands::AgentState::default()))
        .manage(commands::HitlPendingSenders::default())
        .invoke_handler(tauri::generate_handler![
            commands::start_agent,
            commands::get_novnc_port,
            commands::send_agent_message,
            commands::get_active_tokens,
            commands::handle_hitl_approval,
            commands::get_providers,
            commands::get_pending_manifests,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run SENTINEL Dashboard");
}
