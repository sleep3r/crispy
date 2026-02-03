#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

mod app_state;
mod audio;
mod commands;
mod llm_settings;
mod managers;
mod paths;
mod recording;
mod recording_commands;
mod window;

#[cfg(target_os = "macos")]
mod system_input_volume;

#[cfg(target_os = "windows")]
mod windows_audio;

#[cfg(target_os = "windows")]
extern crate windows_core;

use std::sync::{Arc, Mutex};
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;

use app_state::AppState;
use audio::AudioMonitorState;
use recording::RecordingState;
use tauri::tray::{TrayIconEvent};
use tauri::Manager;
use tauri_plugin_positioner;

#[tauri::command]
fn get_platform() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    return Ok("windows".to_string());
    #[cfg(target_os = "macos")]
    return Ok("macos".to_string());
    #[cfg(target_os = "linux")]
    return Ok("linux".to_string());
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return Ok("unknown".to_string());
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
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

            if let Ok(app_settings) = llm_settings::load_app_settings(app.handle()) {
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
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_platform,
            audio::get_input_devices,
            audio::get_output_devices,
            audio::get_default_devices,
            start_monitoring,
            stop_monitoring,
            set_monitoring_volume,
            set_monitoring_model,
            audio::get_system_input_volume,
            audio::set_system_input_volume,
            audio::get_blackhole_status,
            recording_commands::get_recordable_apps,
            recording_commands::start_recording,
            recording_commands::stop_recording,
            recording_commands::is_recording,
            recording_commands::get_recordings_dir_path,
            recording_commands::open_recordings_dir,
            recording_commands::open_url,
            window::show_main_window_cmd,
            window::quit_app,
            recording_commands::get_recordings,
            recording_commands::rename_recording,
            recording_commands::delete_recording,
            recording_commands::read_recording_file,
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
            commands::transcription::get_llm_settings,
            commands::transcription::set_llm_settings,
            commands::transcription::stream_transcription_chat,
            commands::transcription::get_transcription_chat_history,
            commands::transcription::set_transcription_chat_history,
            commands::settings::get_app_settings,
            commands::settings::set_app_setting,
            commands::convert::convert_to_wav,
            commands::convert::check_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn start_monitoring(
    state: tauri::State<AppState>,
    app_handle: tauri::AppHandle,
    device_name: String,
    output_device_name: String,
    model_name: String,
    volume: f32,
) -> Result<(), String> {
    let recording_mic_buffer = state.recording.lock().unwrap().mic_buffer.clone();
    audio::start_monitoring(
        state.audio.clone(),
        recording_mic_buffer,
        app_handle,
        device_name,
        output_device_name,
        model_name,
        volume,
    )
}

#[tauri::command]
fn stop_monitoring(state: tauri::State<AppState>) -> Result<(), String> {
    audio::stop_monitoring(state.audio.clone())
}

#[tauri::command]
fn set_monitoring_volume(state: tauri::State<AppState>, volume: f32) -> Result<(), String> {
    audio::set_monitoring_volume(state.audio.clone(), volume)
}

#[tauri::command]
fn set_monitoring_model(
    state: tauri::State<AppState>,
    model_name: String,
) -> Result<(), String> {
    audio::set_monitoring_model(state.audio.clone(), model_name)
}
