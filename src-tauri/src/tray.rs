use tauri::{AppHandle, SystemTrayEvent};
use tauri_nspanel::ManagerExt;

use crate::fns::position_panel_under_menubar_icon;

pub fn handle(app_handle: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { position, size, .. } => {
            let panel = app_handle.get_panel("main").unwrap();

            if panel.is_visible() {
                panel.order_out(None);
                return;
            }

            let scale_factor = monitor::get_monitor_with_cursor().unwrap().scale_factor();

            position_panel_under_menubar_icon(
                &app_handle,
                position.to_logical(scale_factor),
                size.to_logical(scale_factor),
                0.0,
            );

            panel.show();
        }
        _ => {}
    }
}
