use tauri::Manager;
use tauri_nspanel::{
    cocoa::{
        appkit::{NSMainMenuWindowLevel, NSView, NSWindow},
        base::id,
        foundation::{NSPoint, NSRect},
    },
    objc::{class, msg_send, runtime::NO, sel, sel_impl},
    panel_delegate, ManagerExt, WindowExt,
};

#[allow(non_upper_case_globals)]
const NSWindowStyleMaskNonActivatingPanel: i32 = 1 << 7;

pub fn swizzle_to_menubar_panel(app_handle: &tauri::AppHandle) {
    let window = app_handle.get_window("main").unwrap();

    let panel_delegate = panel_delegate!(SpotlightPanelDelegate {
        window_did_become_key,
        window_did_resign_key,
        window_did_update
    });

    let handle = window.app_handle();

    panel_delegate.set_listener(Box::new(move |delegate_name: String| {
        match delegate_name.as_str() {
            "window_did_become_key" => {
                println!(" i became the key");
            }
            "window_did_resign_key" => {
                handle.trigger_global("menubar_panel_did_resign_key", None);
            }
            "window_did_update" => {
                handle.trigger_global("menubar_panel_did_update", None);
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
        println!("1. did resign key");

        println!("2. menubar is frontmost: {}", check_menubar_frontmost());

        let panel = handle.get_panel("main").unwrap();

        if check_menubar_frontmost() {
            return;
        }

        panel.order_out(None);
    });

    let handle = app_handle.app_handle();

    app_handle.listen_global("menubar_panel_did_update", move |_| {
        // if check_cursor_in_menubar_panel(&handle) {
        //     return;
        // }
        //
        // if !check_menubar_is_frontmost() {
        //     let panel = handle.get_panel("main").unwrap();
        //
        //     panel.order_out(None);
        // }
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

pub fn position_menubar_panel(app_handle: &tauri::AppHandle, padding_top: f64) {
    let window = app_handle.get_window("main").unwrap();

    let monitor = monitor::get_monitor_with_cursor().unwrap();

    let scale_factor = monitor.scale_factor();

    let visible_area = monitor.visible_area();

    let monitor_pos = visible_area.position().to_logical::<f64>(scale_factor);

    let monitor_size = visible_area.size().to_logical::<f64>(scale_factor);

    let mouse_location: NSPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };

    let handle: id = window.ns_window().unwrap() as _;

    let mut win_frame: NSRect = unsafe { msg_send![handle, frame] };

    win_frame.origin.y = (monitor_pos.y + monitor_size.height) - win_frame.size.height;

    win_frame.origin.y -= padding_top;

    win_frame.origin.x = {
        let top_right = mouse_location.x + (win_frame.size.width / 2.0);

        let is_offscreen = top_right > monitor_pos.x + monitor_size.width;

        if !is_offscreen {
            mouse_location.x - (win_frame.size.width / 2.0)
        } else {
            let diff = top_right - (monitor_pos.x + monitor_size.width);

            mouse_location.x - (win_frame.size.width / 2.0) - diff
        }
    };

    let _: () = unsafe { msg_send![handle, setFrame: win_frame display: NO] };
}

fn get_frontmost_app_pid() -> i32 {
    let workspace: id = unsafe { msg_send![class!(NSWorkspace), sharedWorkspace] };

    let frontmost_application: id = unsafe { msg_send![workspace, frontmostApplication] };

    let pid: i32 = unsafe { msg_send![frontmost_application, processIdentifier] };

    pid
}

fn app_pid() -> i32 {
    let process_info: id = unsafe { msg_send![class!(NSProcessInfo), processInfo] };

    let pid: i32 = unsafe { msg_send![process_info, processIdentifier] };

    pid
}

pub fn check_menubar_frontmost() -> bool {
    get_frontmost_app_pid() == app_pid()
}
