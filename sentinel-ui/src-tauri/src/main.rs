#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use sentinel_ui_lib::commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::start_agent,
            commands::get_active_tokens,
            commands::handle_hitl_approval,
            commands::get_providers,
            commands::get_pending_manifests,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run SENTINEL Dashboard");
}
