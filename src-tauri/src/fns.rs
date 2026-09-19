#![allow(clippy::unused_unit)]

use system_notification::WorkspaceListener;
use tauri::{AppHandle, Manager, PhysicalPosition};
use tauri_nspanel::{
    objc2_app_kit::NSRunningApplication, tauri_panel, CollectionBehavior, ManagerExt, PanelLevel,
    StyleMask, WebviewWindowExt,
};

tauri_panel! {
    panel!(MenubarPanel {})

    panel_event!(MenubarPanelEventHandler {
        window_did_resign_key(notification: &NSNotification) -> ()
    })
}

pub fn swizzle_to_menubar_panel(app_handle: &tauri::AppHandle) {
    let window = app_handle.get_webview_window("main").unwrap();

    let panel = window.to_panel::<MenubarPanel>().unwrap();

    let weak_panel = std::sync::Arc::downgrade(&panel);
    let event_handler = MenubarPanelEventHandler::new();

    event_handler.window_did_resign_key(move |_| {
        if let Some(panel) = weak_panel.upgrade() {
            panel.hide();
        }
    });

    panel.set_level(PanelLevel::Status.value());

    panel
        .add_style_mask(StyleMask::empty().nonactivating_panel().into())
        .expect("failed to make menubar panel non-activating");

    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary()
            .into(),
    );

    panel.set_event_handler(Some(event_handler.as_ref()));
}

pub fn setup_menubar_panel_listeners(app_handle: &tauri::AppHandle) {
    fn hide_menubar_panel(app_handle: tauri::AppHandle) {
        if NSRunningApplication::currentApplication().isActive() {
            return;
        }

        let panel = app_handle.get_webview_panel("main").unwrap();
        panel.hide();
    }

    app_handle.listen_workspace(
        "NSWorkspaceDidActivateApplicationNotification",
        hide_menubar_panel,
    );

    app_handle.listen_workspace(
        "NSWorkspaceActiveSpaceDidChangeNotification",
        hide_menubar_panel,
    );
}

pub fn update_menubar_appearance(app_handle: &AppHandle) {
    let panel = app_handle.get_webview_panel("main").unwrap();
    panel.set_corner_radius(13.0);
}

pub fn position_menubar_panel(app_handle: &tauri::AppHandle, padding_top: f64) {
    let window = app_handle.get_webview_window("main").unwrap();
    let cursor = app_handle.cursor_position().unwrap();
    let monitor = app_handle
        .monitor_from_point(cursor.x, cursor.y)
        .unwrap()
        .expect("cursor is not on a monitor");
    let work_area = monitor.work_area();
    let window_size = window.outer_size().unwrap();

    let left = f64::from(work_area.position.x);
    let right = left + f64::from(work_area.size.width) - f64::from(window_size.width);
    let x = (cursor.x - f64::from(window_size.width) / 2.0).clamp(left, right);
    let y = f64::from(work_area.position.y) + padding_top * monitor.scale_factor();

    window
        .set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
        .unwrap();
}
