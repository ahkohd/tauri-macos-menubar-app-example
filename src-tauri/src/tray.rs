use tauri::{
    image::Image,
    tray::{MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tauri_nspanel::ManagerExt;

use crate::fns::position_panel_at_menubar_icon;

pub fn create(app_handle: &AppHandle) -> tauri::Result<TrayIcon> {
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    TrayIconBuilder::with_id("tray")
        .icon(icon)
        .icon_as_template(true)
        .on_tray_icon_event(|tray, event| {
            let app_handle = tray.app_handle();

            if let TrayIconEvent::Click {
                button_state, rect, ..
            } = event
            {
                if button_state == MouseButtonState::Up {
                    let panel = app_handle.get_webview_panel("main").unwrap();

                    if panel.is_visible() {
                        panel.order_out(None);

                        return;
                    }

                    let monitor_with_cursor = monitor::get_monitor_with_cursor().unwrap();

                    let scale_factor = monitor_with_cursor.scale_factor();

                    position_panel_at_menubar_icon(
                        app_handle,
                        rect.position.to_logical(scale_factor),
                        rect.size.to_logical(scale_factor),
                        0.0,
                    );

                    let window = app_handle.get_webview_window("main").unwrap();

                    let window_monitor = window.current_monitor().unwrap().unwrap();

                    let is_window_in_monitor_with_cursor =
                        window_monitor.position().x as f64 == monitor_with_cursor.position().x;

                    if is_window_in_monitor_with_cursor {
                        panel.show();
                    }
                }
            }
        })
        .build(app_handle)
}
