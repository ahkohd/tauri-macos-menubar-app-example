use tauri::{AppHandle, Manager, PhysicalPosition, SystemTrayEvent};
use tauri_nspanel::ManagerExt;

pub fn handle(app_handle: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick {
            position: PhysicalPosition { x, y },
            ..
        } => {
            let panel = app_handle.get_panel("main").unwrap();

            if panel.is_visible() {
                panel.order_out(None);
                return;
            }

            const OFFSET_X: f64 = 50.0;
            const OFFSET_Y: f64 = 0.0;

            let window = app_handle.get_window("main").unwrap();

            let size = window.inner_size().unwrap();

            let half_width = size.width / 2;

            let x = x - (half_width as f64) + OFFSET_X;

            let y = y + OFFSET_Y;

            window.set_position(PhysicalPosition { x, y }).unwrap();

            panel.show();
        }
        _ => {}
    }
}
