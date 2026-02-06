use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_positioner::{Position, WindowExt};

/// Configure tray popup to appear over fullscreen Spaces (macOS).
/// Must be called from the main thread (tray click handler uses run_on_main_thread).
#[cfg(target_os = "macos")]
fn configure_popup_for_fullscreen(window: &tauri::WebviewWindow) {
    let Ok(raw) = window.ns_window() else { return };
    if raw.is_null() {
        return;
    }
    let ns_window: *mut objc2_app_kit::NSWindow = raw.cast();
    unsafe {
        use objc2_app_kit::NSWindowCollectionBehavior as Beh;
        // Allow window in fullscreen Spaces
        // Note: CanJoinAllSpaces is mutually exclusive with MoveToActiveSpace
        let behavior = Beh::CanJoinAllSpaces | Beh::FullScreenAuxiliary | Beh::Transient;
        (*ns_window).setCollectionBehavior(behavior);
        
        // Popup-like level
        (*ns_window).setLevel(objc2_app_kit::NSPopUpMenuWindowLevel);
        
        // Show without forcing key (reduces space switching / stealing focus)
        (*ns_window).orderFrontRegardless();
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

fn show_tray_popup_window(window: &tauri::WebviewWindow) {
    let _ = window.set_always_on_top(true);
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_visible_on_all_workspaces(true);
        configure_popup_for_fullscreen(window);
    }

    // Record the moment we show the popup so blur events can be ignored during grace period
    crate::TRAY_POPUP_SHOWN_AT.store(crate::epoch_millis(), std::sync::atomic::Ordering::SeqCst);

    let _ = window.show();

    // macOS: don't force focus immediately (reduces switching out of fullscreen)
    // Windows/Linux: force focus so click-outside triggers blur and hides the tray.
    #[cfg(not(target_os = "macos"))]
    let _ = window.set_focus();
    #[cfg(target_os = "macos")]
    {
        let _ = window.move_window(Position::TrayBottomCenter);
        // After Finder activation workaround, restore focus to the tray window
        let window_clone = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = window_clone.set_focus();
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window.move_window(Position::TrayCenter);
}

pub fn show_or_toggle_tray_popup(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("tray-popup") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            show_tray_popup_window(&window);
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
        show_tray_popup_window(&window);
    }
}
