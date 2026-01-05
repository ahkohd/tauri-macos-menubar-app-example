// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod command;
mod diff;
mod fns;
mod generator;
mod introspection;
mod models;
mod parsing;
mod schema;
mod state;
mod supabase_api;
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
            // Access token commands
            command::set_access_token,
            command::has_access_token,
            command::clear_access_token,
            command::validate_access_token,
            // Remote project commands
            command::list_remote_projects,
            command::list_organizations,
            command::pull_project,
            command::push_project,
            // Project commands
            command::create_project,
            command::get_projects,
            command::get_project,
            command::update_project,
            command::delete_project,
            command::link_supabase_project,
            // Watcher commands
            command::start_watching,
            command::stop_watching,
            command::is_watching,
            // Log commands
            command::get_logs,
            command::clear_logs,
            // Supabase API commands
            command::run_query,
            command::deploy_edge_function,
            command::get_remote_schema,
            // Supabase Logs API commands
            command::query_supabase_logs,
            command::get_edge_function_logs,
            command::get_postgres_logs,
            command::get_auth_logs,
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
