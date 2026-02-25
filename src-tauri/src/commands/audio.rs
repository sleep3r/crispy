use crate::app_state::AppState;
use crate::audio;

#[tauri::command]
pub fn get_platform() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    return Ok("windows".to_string());
    #[cfg(target_os = "macos")]
    return Ok("macos".to_string());
    #[cfg(target_os = "linux")]
    return Ok("linux".to_string());
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return Ok("unknown".to_string());
}

#[tauri::command]
pub fn start_monitoring(
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
pub fn stop_monitoring(state: tauri::State<AppState>) -> Result<(), String> {
    audio::stop_monitoring(state.audio.clone())
}

#[tauri::command]
pub fn set_monitoring_volume(state: tauri::State<AppState>, volume: f32) -> Result<(), String> {
    audio::set_monitoring_volume(state.audio.clone(), volume)
}

#[tauri::command]
pub fn set_monitoring_model(
    state: tauri::State<AppState>,
    model_name: String,
) -> Result<(), String> {
    audio::set_monitoring_model(state.audio.clone(), model_name)
}
