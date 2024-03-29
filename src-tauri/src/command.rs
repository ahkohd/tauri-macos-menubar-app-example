use std::sync::Once;

use tauri_nspanel::ManagerExt;

use crate::fns::{
    setup_menubar_event_listeners, swizzle_to_menubar_panel, update_menubar_appearance,
};

const INIT: Once = Once::new();

#[tauri::command]
pub fn init(app_handle: tauri::AppHandle) {
    INIT.call_once(|| {
        swizzle_to_menubar_panel(&app_handle);
        update_menubar_appearance(&app_handle);
        setup_menubar_event_listeners(&app_handle);
    });
}

#[tauri::command]
pub fn show_menubar_panel(app_handle: tauri::AppHandle) {
    let panel = app_handle.get_panel("main").unwrap();
    panel.show();
}
