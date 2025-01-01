use std::sync::Once;

use tauri_nspanel::ManagerExt;

use crate::fns::{
    setup_menubar_panel_listeners, swizzle_to_menubar_panel, update_menubar_appearance,
};

static INIT: Once = Once::new();

#[tauri::command]
pub fn init(app_handle: tauri::AppHandle) {
    INIT.call_once(|| {
        swizzle_to_menubar_panel(&app_handle);

        update_menubar_appearance(&app_handle);

        setup_menubar_panel_listeners(&app_handle);
    });
}

#[tauri::command]
pub fn show_menubar_panel(app_handle: tauri::AppHandle) {
    let panel = app_handle.get_webview_panel("main").unwrap();

    panel.show();
}

#[tauri::command]
pub fn change_tray_title(app_handle: tauri::AppHandle, title: String) {
    let tray_handle = app_handle.tray_by_id("tray").unwrap();

    if let Err(e) = tray_handle.set_title(Some(&title)) {
        eprintln!("failed to set title: {}", e);
    } else {
        println!("title set successfully to '{}'", title);
    }
}
