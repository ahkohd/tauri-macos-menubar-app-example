// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod command;
mod fns;
mod tray;

use tauri::SystemTray;
use tauri_nspanel;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            command::init,
            command::show_menubar_panel
        ])
        .plugin(tauri_nspanel::init())
        .system_tray(SystemTray::new())
        .on_system_tray_event(tray::handle)
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
