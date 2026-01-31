use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_positioner::{Position, WindowExt};

#[cfg(target_os = "macos")]
fn set_tray_window_level(window: &tauri::WebviewWindow) {
    if let Ok(raw_ptr) = window.ns_window() {
        if raw_ptr.is_null() {
            return;
        }
        let ns_window: *mut objc2_app_kit::NSWindow = raw_ptr.cast();
        unsafe {
            const CG_SHIELDING_WINDOW_LEVEL: isize = 2147483630;
            (*ns_window).setLevel(CG_SHIELDING_WINDOW_LEVEL);
            (*ns_window).makeKeyAndOrderFront(None);
        }
    }
}

pub fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
}

#[tauri::command]
pub fn show_main_window_cmd(app: tauri::AppHandle) {
    show_main_window(&app);
}

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

pub fn show_or_toggle_tray_popup(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("tray-popup") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.set_always_on_top(true);
            #[cfg(target_os = "macos")]
            {
                set_tray_window_level(&window);
                let _ = window.set_visible_on_all_workspaces(true);
            }
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.move_window(Position::TrayBottomCenter);
        }
        return;
    }

    let url = WebviewUrl::App("index.html".into());
    let _ = WebviewWindowBuilder::new(app, "tray-popup", url)
        .title("Crispy")
        .inner_size(260.0, 280.0)
        .decorations(false)
        .resizable(false)
        .build();

    if let Some(window) = app.get_webview_window("tray-popup") {
        let _ = window.set_always_on_top(true);
        #[cfg(target_os = "macos")]
        {
            set_tray_window_level(&window);
            let _ = window.set_visible_on_all_workspaces(true);
        }
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.move_window(Position::TrayBottomCenter);
    }
}
