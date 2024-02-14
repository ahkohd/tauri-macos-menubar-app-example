use tauri::{AppHandle, SystemTrayEvent};
use tauri_nspanel::ManagerExt;

use crate::fns::position_menubar_panel;

pub fn handle(app_handle: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { .. } => {
            let panel = app_handle.get_panel("main").unwrap();

            if panel.is_visible() {
                panel.order_out(None);
                return;
            }

            position_menubar_panel(&app_handle, 0.0);

            panel.show();
        }
        _ => {}
    }
}
