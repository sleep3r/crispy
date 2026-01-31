// Commands for transcription: start, get result, open result window.

use crate::commands::models::SelectedModelState;
use crate::managers::transcription::{
    load_transcription_result, load_transcription_metadata, save_transcription_result,
    save_transcription_metadata, wav_to_16k_mono_f32, TranscriptionManager,
};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
pub struct TranscriptionStatusEvent {
    pub recording_path: String,
    pub status: String, // "started" | "completed" | "error"
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionOpenEvent {
    pub recording_path: String,
}

#[tauri::command]
pub async fn start_transcription(
    app: AppHandle,
    recording_path: String,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    selected_model_state: State<'_, SelectedModelState>,
) -> Result<(), String> {
    let _ = app.emit(
        "transcription-status",
        TranscriptionStatusEvent {
            recording_path: recording_path.clone(),
            status: "started".to_string(),
            error: None,
        },
    );

    let app_clone = app.clone();
    let path_clone = recording_path.clone();
    let tm = Arc::clone(transcription_manager.inner());
    let sel = selected_model_state.0.clone();

    std::thread::spawn(move || {
        let result = run_transcription(&app_clone, &path_clone, &tm, &sel);
        let (status, err) = match result {
            Ok(()) => ("completed".to_string(), None),
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };
        let _ = app_clone.emit(
            "transcription-status",
            TranscriptionStatusEvent {
                recording_path: path_clone,
                status,
                error: err,
            },
        );
    });

    Ok(())
}

fn run_transcription(
    app: &AppHandle,
    recording_path: &str,
    tm: &TranscriptionManager,
    selected_model: &Arc<std::sync::Mutex<String>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let model_id = {
        let sel = selected_model.lock().map_err(|e| e.to_string())?;
        sel.clone()
    };
    if model_id.is_empty() || model_id == "none" {
        return Err("No transcription model selected. Choose a model in Settings.".into());
    }

    let audio = wav_to_16k_mono_f32(Path::new(recording_path))?;
    if audio.is_empty() {
        save_transcription_result(app, recording_path, "")?;
        save_transcription_metadata(app, recording_path, &model_id)?;
        return Ok(());
    }

    let current = tm.get_current_model();
    if current.as_deref() != Some(model_id.as_str()) {
        tm.load_model(&model_id)?;
    }
    let text = tm.transcribe(audio)?;
    save_transcription_result(app, recording_path, &text)?;
    save_transcription_metadata(app, recording_path, &model_id)?;
    Ok(())
}

#[tauri::command]
pub async fn get_transcription_result(
    app: AppHandle,
    recording_path: String,
) -> Result<Option<String>, String> {
    load_transcription_result(&app, &recording_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_transcription_model(
    app: AppHandle,
    recording_path: String,
) -> Result<Option<String>, String> {
    load_transcription_metadata(&app, &recording_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_transcription_window(app: AppHandle, recording_path: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("transcription-result") {
        let _ = window.emit(
            "transcription-open",
            TranscriptionOpenEvent {
                recording_path: recording_path.clone(),
            },
        );
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let encoded = urlencoding::encode(&recording_path);
    let url = WebviewUrl::App(format!("index.html?recording_path={}", encoded).into());
    WebviewWindowBuilder::new(&app, "transcription-result", url)
        .title("Transcription Result")
        .inner_size(500.0, 400.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn has_transcription_result(
    app: AppHandle,
    recording_path: String,
) -> Result<bool, String> {
    let path = crate::managers::transcription::transcription_result_path(&app, &recording_path)
        .map_err(|e| e.to_string())?;
    Ok(path.exists())
}

/// Placeholder: answers a question about the transcription. Always returns the same message for now.
#[tauri::command]
pub async fn ask_transcription_question(
    _recording_path: String,
    _question: String,
) -> Result<String, String> {
    Ok("Your question has been received. Question handling will be added later.".to_string())
}
