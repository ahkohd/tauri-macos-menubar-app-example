#![allow(clippy::unused_unit)]

use system_notification::WorkspaceListener;
use tauri::{LogicalPosition, Manager, Rect};
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

    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary()
            .into(),
    );

    panel
        .add_style_mask(StyleMask::empty().nonactivating_panel().into())
        .expect("failed to make menubar panel non-activating");

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

pub fn update_menubar_appearance(app_handle: &tauri::AppHandle) {
    let window = app_handle.get_webview_window("main").unwrap();

    popover::add_view(&window, None);
}

pub fn position_panel_at_menubar_icon(
    app_handle: &tauri::AppHandle,
    icon_rect: Rect,
    padding_top: f64,
) {
    let window = app_handle.get_webview_window("main").unwrap();
    let icon_physical_position = icon_rect.position.to_physical::<f64>(1.0);
    let icon_physical_size = icon_rect.size.to_physical::<f64>(1.0);
    let icon_center_x = icon_physical_position.x + icon_physical_size.width / 2.0;
    let icon_center_y = icon_physical_position.y + icon_physical_size.height / 2.0;
    let monitor = app_handle
        .monitor_from_point(icon_center_x, icon_center_y)
        .unwrap()
        .expect("menubar icon is not on a monitor");
    let scale_factor = monitor.scale_factor();
    let icon_position = icon_rect.position.to_logical::<f64>(scale_factor);
    let icon_size = icon_rect.size.to_logical::<f64>(scale_factor);
    let window_size = window.outer_size().unwrap().to_logical::<f64>(scale_factor);
    let work_area = monitor.work_area();
    let work_area_position = work_area.position.to_logical::<f64>(scale_factor);
    let work_area_size = work_area.size.to_logical::<f64>(scale_factor);

    let left = work_area_position.x;
    let right = left + work_area_size.width - window_size.width;
    let x = (icon_position.x + icon_size.width / 2.0 - window_size.width / 2.0).clamp(left, right);
    let y = work_area_position.y + padding_top;

    window.set_position(LogicalPosition::new(x, y)).unwrap();
}
