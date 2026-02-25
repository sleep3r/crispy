#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

mod app_state;
mod audio;
mod commands;
mod settings;
mod managers;
mod paths;
mod recording;
mod window;

#[cfg(target_os = "macos")]
mod system_input_volume;

#[cfg(target_os = "windows")]
mod windows_audio;

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;
use std::time::SystemTime;

use app_state::AppState;
use audio::AudioMonitorState;
use recording::RecordingState;
use tauri::tray::{TrayIconEvent};
use tauri::Manager;
use tauri_plugin_positioner;
use tauri_plugin_autostart::ManagerExt;

/// Timestamp (epoch millis) when the tray popup was last shown.
/// Used to ignore spurious blur events during the Finder activation workaround.
static TRAY_POPUP_SHOWN_AT: AtomicU64 = AtomicU64::new(0);

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}


/// Parse HTTP Range header like "bytes=0-1023" into (start, end) inclusive.
fn parse_range(header: &str, file_size: u64) -> Option<(u64, u64)> {
    let header = header.trim();
    if !header.starts_with("bytes=") {
        return None;
    }
    let range_spec = &header[6..];
    let mut parts = range_spec.splitn(2, '-');
    let start_str = parts.next()?.trim();
    let end_str = parts.next()?.trim();

    if file_size == 0 {
        return None;
    }

    if start_str.is_empty() {
        // Suffix range: bytes=-500
        let suffix: u64 = end_str.parse().ok()?;
        let start = file_size.saturating_sub(suffix);
        Some((start, file_size - 1))
    } else {
        let start: u64 = start_str.parse().ok()?;
        if start >= file_size {
            return None;
        }
        let end = if end_str.is_empty() {
            file_size - 1
        } else {
            end_str.parse().ok()?
        };
        Some((start, end.min(file_size - 1)))
    }
}

fn main() {
    // Reduce noisy native logs (if supported by dependencies).
    std::env::set_var("CPUINFO_LOG_LEVEL", "fatal");
    std::env::set_var("GGML_LOG_LEVEL", "error");

    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("stream", |_ctx, request, responder| {
            // Use Tauri's async runtime instead of unbounded thread spawning
            tauri::async_runtime::spawn(async move {
                use std::io::{Read, Seek, SeekFrom};

                let uri = request.uri().to_string();
                // Expected: stream://localhost/<encoded-path>
                let path = uri
                    .strip_prefix("stream://localhost/")
                    .unwrap_or("");
                let path = urlencoding::decode(path).unwrap_or_default().to_string();

                let file = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("[stream://] Failed to open file {}: {}", path, e);
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(404)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    }
                };

                let file_size = match file.metadata() {
                    Ok(m) => m.len(),
                    Err(e) => {
                        eprintln!("[stream://] Failed to get file metadata: {}", e);
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(500)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    }
                };

                // Empty files should return 200 with empty body
                if file_size == 0 {
                    responder.respond(
                        tauri::http::Response::builder()
                            .status(200)
                            .header("Content-Type", "audio/wav")
                            .header("Content-Length", "0")
                            .header("Accept-Ranges", "bytes")
                            .body(Vec::new())
                            .unwrap(),
                    );
                    return;
                }

                // Parse Range header
                let range_header = request
                    .headers()
                    .get("range")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");

                if let Some(range) = parse_range(range_header, file_size) {
                    // Range request - read only the requested bytes
                    let mut f = file;
                    let length = range.1 - range.0 + 1;

                    // Seek to start position
                    if let Err(e) = f.seek(SeekFrom::Start(range.0)) {
                        eprintln!("[stream://] Seek failed: {}", e);
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(500)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    }

                    // Read exactly the requested range
                    let mut buf = vec![0u8; length as usize];
                    if let Err(e) = f.read_exact(&mut buf) {
                        eprintln!("[stream://] Read failed: {}", e);
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(500)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    }

                    responder.respond(
                        tauri::http::Response::builder()
                            .status(206)
                            .header("Content-Type", "audio/wav")
                            .header("Content-Length", length.to_string())
                            .header(
                                "Content-Range",
                                format!("bytes {}-{}/{}", range.0, range.1, file_size),
                            )
                            .header("Accept-Ranges", "bytes")
                            .body(buf)
                            .unwrap(),
                    );
                } else {
                    // No range - stream the entire file
                    // Note: Still reading into memory for simplicity with Tauri's Response API
                    // For truly large files, consider implementing chunked streaming
                    let mut f = file;
                    let mut buf = Vec::with_capacity(file_size as usize);
                    if let Err(e) = f.read_to_end(&mut buf) {
                        eprintln!("[stream://] Full read failed: {}", e);
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(500)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    }

                    responder.respond(
                        tauri::http::Response::builder()
                            .status(200)
                            .header("Content-Type", "audio/wav")
                            .header("Content-Length", file_size.to_string())
                            .header("Accept-Ranges", "bytes")
                            .body(buf)
                            .unwrap(),
                    );
                }
            });
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(AppState {
            audio: Arc::new(Mutex::new(AudioMonitorState::new())),
            recording: Arc::new(Mutex::new(RecordingState::new())),
        })
        .manage(commands::models::SelectedModelState(Arc::new(Mutex::new(
            String::new(),
        ))))
        .setup(|app| {
            let model_manager = Arc::new(
                managers::model::ModelManager::new(app.handle()).map_err(|e| e.to_string())?,
            );
            app.manage(model_manager.clone());
            let transcription_manager = Arc::new(
                managers::transcription::TranscriptionManager::new(model_manager),
            );
            app.manage(transcription_manager);

            if let Ok(app_settings) = settings::load_app_settings(app.handle()) {
                if !app_settings.selected_transcription_model.is_empty()
                    && app_settings.selected_transcription_model != "none"
                {
                    if let Some(selected) = app
                        .try_state::<commands::models::SelectedModelState>()
                        .map(|s| s.0.clone())
                    {
                        if let Ok(mut guard) = selected.lock() {
                            *guard = app_settings.selected_transcription_model;
                        }
                    }
                }
                
                // Apply autostart setting
                let autostart_manager = app.handle().autolaunch();
                if app_settings.autostart_enabled == "true" {
                    let _ = autostart_manager.enable();
                } else {
                    let _ = autostart_manager.disable();
                }
            }

            // On macOS we want a template icon so it adapts to light/dark menu bar.
            // On other platforms we use a solid black icon so it's always visible.
            #[cfg(target_os = "macos")]
            let (icon, icon_as_template) = {
                let base_icon = app
                    .path()
                    .resolve("resources/tray.png", tauri::path::BaseDirectory::Resource)
                    .ok()
                    .and_then(|p| tauri::image::Image::from_path(p).ok())
                    .or_else(|| app.default_window_icon().cloned())
                    .expect("tray icon: provide resources/tray.png or a default icon");
                (base_icon, true)
            };

            #[cfg(not(target_os = "macos"))]
            let (icon, icon_as_template) = {
                let black_icon = app
                    .path()
                    .resolve("resources/tray-black.png", tauri::path::BaseDirectory::Resource)
                    .ok()
                    .and_then(|p| tauri::image::Image::from_path(p).ok());

                // Fallback to original icon if black one is missing.
                let base_icon = black_icon
                    .or_else(|| {
                        app.path()
                            .resolve("resources/tray.png", tauri::path::BaseDirectory::Resource)
                            .ok()
                            .and_then(|p| tauri::image::Image::from_path(p).ok())
                    })
                    .or_else(|| app.default_window_icon().cloned())
                    .expect("tray icon: provide resources/tray-black.png/tray.png or a default icon");

                (base_icon, false)
            };

            let tray = tauri::tray::TrayIconBuilder::new()
                .icon(icon)
                .menu_on_left_click(false)
                .icon_as_template(icon_as_template)
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click {
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle().clone();
                        thread::spawn(move || {
                            #[cfg(target_os = "macos")]
                            {
                                // Activate Finder without opening a new window
                                let _ = std::process::Command::new("osascript")
                                    .args([
                                        "-e",
                                        "tell application \"Finder\" to activate",
                                    ])
                                    .status();
                                thread::sleep(Duration::from_millis(100));
                            }
                            let app_for_closure = app.clone();
                            // AppKit window ops must run on main thread (avoids "foreign exception" crash)
                            let _ = app.run_on_main_thread(move || {
                                window::show_or_toggle_tray_popup(&app_for_closure);
                            });
                        });
                    }
                })
                .build(app)
                .map_err(|e| e.to_string())?;

            app.manage(tray);
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if window.label() == "main" {
                        api.prevent_close();
                        let _ = window.hide();
                        #[cfg(target_os = "macos")]
                        {
                            let _ = window.app_handle().set_activation_policy(
                                tauri::ActivationPolicy::Accessory,
                            );
                        }
                    }
                }
                tauri::WindowEvent::Focused(false) => {
                    if window.label() == "tray-popup" {
                        // Grace period: ignore blur events within 600ms of showing the popup.
                        // The Finder activation workaround causes spurious focus loss
                        // that would otherwise immediately hide the popup.
                        let shown_at = TRAY_POPUP_SHOWN_AT.load(AtomicOrdering::SeqCst);
                        let now = epoch_millis();
                        if now.saturating_sub(shown_at) < 600 {
                            // Too soon after showing — ignore this blur
                            return;
                        }
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::audio::get_platform,
            audio::get_input_devices,
            audio::get_output_devices,
            audio::get_default_devices,
            commands::audio::start_monitoring,
            commands::audio::stop_monitoring,
            commands::audio::set_monitoring_volume,
            commands::audio::set_monitoring_model,
            audio::get_system_input_volume,
            audio::set_system_input_volume,
            audio::get_blackhole_status,
            commands::recording::get_recordable_apps,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::is_recording,
            commands::recording::get_recordings_dir_path,
            commands::recording::open_recordings_dir,
            commands::recording::open_url,
            window::show_main_window_cmd,
            window::quit_app,
            commands::recording::get_recordings,
            commands::recording::rename_recording,
            commands::recording::delete_recording,
            commands::recording::read_recording_file,
            commands::models::get_available_models,
            commands::ns_models::get_available_ns_models,
            commands::models::get_model_info,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::set_active_model,
            commands::models::get_current_model,
            commands::models::cancel_download,
            commands::models::get_recommended_first_model,
            commands::transcription::start_transcription,
            commands::transcription::get_transcription_result,
            commands::transcription::get_transcription_model,
            commands::transcription::get_transcription_state,
            commands::transcription::open_transcription_window,
            commands::transcription::has_transcription_result,
            commands::settings::get_llm_settings,
            commands::settings::set_llm_settings,
            commands::transcription::stream_transcription_chat,
            commands::transcription::get_transcription_chat_history,
            commands::transcription::set_transcription_chat_history,
            commands::transcription::cancel_transcription,
            commands::transcription::get_all_transcription_states,
            commands::settings::get_app_settings,
            commands::settings::set_app_setting,
            commands::settings::set_autostart,
            commands::convert::convert_to_wav,
            commands::convert::check_ffmpeg,
            commands::permissions::check_permissions,
            commands::permissions::request_permission,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_standard() {
        assert_eq!(parse_range("bytes=0-1023", 10000), Some((0, 1023)));
    }

    #[test]
    fn parse_range_from_start_to_end() {
        assert_eq!(parse_range("bytes=0-9999", 10000), Some((0, 9999)));
    }

    #[test]
    fn parse_range_open_ended() {
        // No end specified → goes to file_size - 1
        assert_eq!(parse_range("bytes=500-", 10000), Some((500, 9999)));
    }

    #[test]
    fn parse_range_suffix() {
        // Last 500 bytes: bytes=-500
        assert_eq!(parse_range("bytes=-500", 10000), Some((9500, 9999)));
    }

    #[test]
    fn parse_range_suffix_larger_than_file() {
        // Suffix bigger than file → starts at 0
        assert_eq!(parse_range("bytes=-99999", 100), Some((0, 99)));
    }

    #[test]
    fn parse_range_clamps_end_to_file_size() {
        assert_eq!(parse_range("bytes=0-99999", 100), Some((0, 99)));
    }

    #[test]
    fn parse_range_empty_file() {
        assert_eq!(parse_range("bytes=0-10", 0), None);
    }

    #[test]
    fn parse_range_start_beyond_file() {
        assert_eq!(parse_range("bytes=10000-20000", 100), None);
    }

    #[test]
    fn parse_range_invalid_prefix() {
        assert_eq!(parse_range("chars=0-100", 10000), None);
    }

    #[test]
    fn parse_range_whitespace_trimmed() {
        assert_eq!(parse_range("  bytes=0-1023  ", 10000), Some((0, 1023)));
    }
}
