use tauri::Manager;
use tauri_nspanel::{
    cocoa::{
        appkit::{NSMainMenuWindowLevel, NSView, NSWindow},
        base::id,
    },
    objc::{msg_send, sel, sel_impl},
    panel_delegate, ManagerExt, WindowExt,
};

#[allow(non_upper_case_globals)]
const NSWindowStyleMaskNonActivatingPanel: i32 = 1 << 7;

pub fn swizzle_to_menubar_panel(app_handle: &tauri::AppHandle) {
    let window = app_handle.get_window("main").unwrap();

    let panel_delegate = panel_delegate!(SpotlightPanelDelegate {
        indow_did_become_key,
        window_did_resign_key
    });

    let handle = window.app_handle();

    panel_delegate.set_listener(Box::new(move |delegate_name: String| {
        match delegate_name.as_str() {
            "window_did_become_key" => {
                handle.trigger_global("menubar_panel_did_become_key", None);
            }
            "window_did_resign_key" => {
                handle.trigger_global("menubar_panel_did_resign_key", None);
            }
            _ => (),
        }
    }));

    let panel = window.to_panel().unwrap();

    panel.set_level(NSMainMenuWindowLevel + 1);

    panel.set_style_mask(NSWindowStyleMaskNonActivatingPanel);

    panel.set_delegate(panel_delegate);
}

pub fn setup_menubar_event_listeners(app_handle: &tauri::AppHandle) {
    let handle = app_handle.app_handle();

    app_handle.listen_global("menubar_panel_did_resign_key", move |_| {
        let panel = handle.get_panel("main").unwrap();

        panel.order_out(None);
    });
}

pub fn update_menubar_appearance(app_handle: &tauri::AppHandle) {
    let window = app_handle.get_window("main").unwrap();

    set_corner_radius(&window, 13.0);
}

pub fn set_corner_radius(window: &tauri::Window, radius: f64) {
    let win: id = window.ns_window().unwrap() as _;

    unsafe {
        let view: id = win.contentView();

        view.wantsLayer();

        let layer: id = view.layer();

        let _: () = msg_send![layer, setCornerRadius: radius];
    }
}
