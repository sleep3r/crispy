#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

mod app_state;
mod audio;
mod commands;
mod llm_settings;
mod managers;
mod recording;
mod recording_commands;
mod window;

#[cfg(target_os = "macos")]
mod system_input_volume;

use std::sync::{Arc, Mutex};

use app_state::AppState;
use audio::AudioMonitorState;
use recording::RecordingState;
use tauri::tray::{TrayIconEvent};
use tauri::Manager;
use tauri_plugin_positioner;

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

            let icon = app
                .path()
                .resolve("resources/tray.png", tauri::path::BaseDirectory::Resource)
                .ok()
                .and_then(|p| tauri::image::Image::from_path(p).ok())
                .or_else(|| app.default_window_icon().cloned());
            let icon = icon.expect("tray icon: run scripts/tray_icon.py or provide default icon");
            let tray = tauri::tray::TrayIconBuilder::new()
                .icon(icon)
                .menu_on_left_click(false)
                .icon_as_template(true)
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click {
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        window::show_or_toggle_tray_popup(tray.app_handle());
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
            commands::transcription::open_transcription_window,
            commands::transcription::has_transcription_result,
            commands::transcription::get_llm_settings,
            commands::transcription::set_llm_settings,
            commands::transcription::stream_transcription_chat,
            commands::transcription::get_transcription_chat_history,
            commands::transcription::set_transcription_chat_history,
            commands::settings::get_app_settings,
            commands::settings::set_app_setting,
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
