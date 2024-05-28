use tauri::{AppHandle, Manager, SystemTrayEvent};
use tauri_nspanel::ManagerExt;

use crate::fns::position_panel_at_menubar_icon;

pub fn handle(app_handle: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { position, size, .. } => {
            let panel = app_handle.get_panel("main").unwrap();

            if panel.is_visible() {
                panel.order_out(None);

                return;
            }

            let monitor_with_cursor = monitor::get_monitor_with_cursor().unwrap();

            let scale_factor = monitor_with_cursor.scale_factor();

            position_panel_at_menubar_icon(
                &app_handle,
                position.to_logical(scale_factor),
                size.to_logical(scale_factor),
                0.0,
            );

            let window = app_handle.get_window("main").unwrap();

            let window_monitor = window.current_monitor().unwrap().unwrap();

            let is_window_in_monitor_with_cursor =
                window_monitor.position().x as f64 == monitor_with_cursor.position().x;

            if is_window_in_monitor_with_cursor {
                panel.show();
            }
        }
        _ => {}
    }
}
