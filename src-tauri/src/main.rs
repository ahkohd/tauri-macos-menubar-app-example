// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod command;
mod fns;
mod models;
mod state;
mod tray;
mod watcher;

use std::sync::Arc;

use state::AppState;
use tauri::Manager;

fn main() {
    let app_state = Arc::new(AppState::new());

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            // Init commands
            command::init,
            command::show_menubar_panel,
            // Project commands
            command::create_project,
            command::get_projects,
            command::get_project,
            command::update_project,
            command::delete_project,
            // Watcher commands
            command::start_watching,
            command::stop_watching,
            command::is_watching,
            // Log commands
            command::get_logs,
            command::clear_logs,
            // Supabase CLI commands
            command::deploy_edge_function,
            command::run_migration,
            command::link_supabase_project,
            command::init_supabase_project,
        ])
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let app_handle = app.app_handle();

            tray::create(app_handle)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
