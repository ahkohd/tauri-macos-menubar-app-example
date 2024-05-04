use std::ffi::CString;

use tauri::{LogicalPosition, LogicalSize, Manager};
use tauri_nspanel::{
    block::ConcreteBlock,
    cocoa::{
        appkit::NSMainMenuWindowLevel,
        base::{id, nil},
        foundation::NSRect,
    },
    objc::{class, msg_send, runtime::NO, sel, sel_impl},
    panel_delegate, ManagerExt, WindowExt,
};

use popover;

#[allow(non_upper_case_globals)]
const NSWindowStyleMaskNonActivatingPanel: i32 = 1 << 7;

pub fn swizzle_to_menubar_panel(app_handle: &tauri::AppHandle) {
    let window = app_handle.get_window("main").unwrap();

    let panel_delegate = panel_delegate!(SpotlightPanelDelegate {
        window_did_resign_key
    });

    let handle = window.app_handle();

    panel_delegate.set_listener(Box::new(move |delegate_name: String| {
        match delegate_name.as_str() {
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

pub fn setup_menubar_panel_listeners(app_handle: &tauri::AppHandle) {
    fn hide_menubar_panel(app_handle: &tauri::AppHandle) {
        if check_menubar_frontmost() {
            return;
        }

        let panel = app_handle.get_panel("main").unwrap();

        panel.order_out(None);
    }

    let handle = app_handle.app_handle();

    app_handle.listen_global("menubar_panel_did_resign_key", move |_| {
        hide_menubar_panel(&handle);
    });

    let handle = app_handle.app_handle();

    let callback = Box::new(move || {
        hide_menubar_panel(&handle);
    });

    register_workspace_listener(
        "NSWorkspaceDidActivateApplicationNotification".into(),
        callback.clone(),
    );

    register_workspace_listener(
        "NSWorkspaceActiveSpaceDidChangeNotification".into(),
        callback,
    );
}

pub fn update_menubar_appearance(app_handle: &tauri::AppHandle) {
    let window = app_handle.get_window("main").unwrap();

    popover::add_view(&window, None);
}

pub fn position_panel_under_menubar_icon(
    app_handle: &tauri::AppHandle,
    menubar_icon_position: LogicalPosition<f64>,
    menubar_icon_size: LogicalSize<f64>,
    padding_top: f64,
) {
    let window = app_handle.get_window("main").unwrap();

    let monitor = monitor::get_monitor_with_cursor().unwrap();

    let scale_factor = monitor.scale_factor();

    let visible_area = monitor.visible_area();

    let monitor_pos = visible_area.position().to_logical::<f64>(scale_factor);

    let monitor_size = visible_area.size().to_logical::<f64>(scale_factor);

    let handle: id = window.ns_window().unwrap() as _;

    let mut win_frame: NSRect = unsafe { msg_send![handle, frame] };

    win_frame.origin.y = (monitor_pos.y + monitor_size.height) - win_frame.size.height;

    win_frame.origin.y -= padding_top;

    win_frame.origin.x = {
        let top_right = menubar_icon_position.x + (win_frame.size.width / 2.0);

        let is_offscreen = top_right > monitor_pos.x + monitor_size.width;

        if !is_offscreen {
            menubar_icon_position.x - (win_frame.size.width / 2.0)
        } else {
            let diff = top_right - (monitor_pos.x + monitor_size.width);

            menubar_icon_position.x - (win_frame.size.width / 2.0) - diff
        }
    } + menubar_icon_size.width / 2.0;

    let _: () = unsafe { msg_send![handle, setFrame: win_frame display: NO] };
}

fn register_workspace_listener(name: String, callback: Box<dyn Fn()>) {
    let workspace: id = unsafe { msg_send![class!(NSWorkspace), sharedWorkspace] };

    let notification_center: id = unsafe { msg_send![workspace, notificationCenter] };

    let block = ConcreteBlock::new(move |_notif: id| {
        callback();
    });

    let block = block.copy();

    let name: id =
        unsafe { msg_send![class!(NSString), stringWithCString: CString::new(name).unwrap()] };

    unsafe {
        let _: () = msg_send![
            notification_center,
            addObserverForName: name object: nil queue: nil usingBlock: block
        ];
    }
}

fn app_pid() -> i32 {
    let process_info: id = unsafe { msg_send![class!(NSProcessInfo), processInfo] };

    let pid: i32 = unsafe { msg_send![process_info, processIdentifier] };

    pid
}

fn get_frontmost_app_pid() -> i32 {
    let workspace: id = unsafe { msg_send![class!(NSWorkspace), sharedWorkspace] };

    let frontmost_application: id = unsafe { msg_send![workspace, frontmostApplication] };

    let pid: i32 = unsafe { msg_send![frontmost_application, processIdentifier] };

    pid
}

pub fn check_menubar_frontmost() -> bool {
    get_frontmost_app_pid() == app_pid()
}
