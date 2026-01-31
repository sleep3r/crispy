// Transcription model commands. Adapted from Handy (open license).

use crate::managers::model::{ModelInfo, ModelManager};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, serde::Serialize)]
struct ModelStateEvent {
    event_type: String,
    model_id: Option<String>,
    model_name: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Default)]
pub struct SelectedModelState(pub Arc<std::sync::Mutex<String>>);

#[tauri::command]
pub async fn get_available_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(model_manager.get_available_models())
}

#[tauri::command]
pub async fn get_model_info(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<Option<ModelInfo>, String> {
    Ok(model_manager.get_model_info(&model_id))
}

#[tauri::command]
pub async fn download_model(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .download_model(&model_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_model(
    model_manager: State<'_, Arc<ModelManager>>,
    selected_state: State<'_, SelectedModelState>,
    app_handle: AppHandle,
    model_id: String,
) -> Result<(), String> {
    model_manager.delete_model(&model_id).map_err(|e| e.to_string())?;
    let mut sel = selected_state.0.lock().unwrap();
    if *sel == model_id {
        *sel = String::new();
        let _ = app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "unloaded".to_string(),
                model_id: None,
                model_name: None,
                error: None,
            },
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn set_active_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    selected_state: State<'_, SelectedModelState>,
    model_id: String,
) -> Result<(), String> {
    if model_id == "none" {
        let mut sel = selected_state.0.lock().unwrap();
        *sel = String::new();
        let _ = app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "unloaded".to_string(),
                model_id: None,
                model_name: None,
                error: None,
            },
        );
        return Ok(());
    }

    let info = model_manager
        .get_model_info(&model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;
    if !info.is_downloaded {
        return Err(format!("Model not downloaded: {}", model_id));
    }
    let _ = app_handle.emit(
        "model-state-changed",
        ModelStateEvent {
            event_type: "loading_started".to_string(),
            model_id: Some(info.id.clone()),
            model_name: Some(info.name.clone()),
            error: None,
        },
    );

    let mut sel = selected_state.0.lock().unwrap();
    *sel = info.id.clone();

    let _ = app_handle.emit(
        "model-state-changed",
        ModelStateEvent {
            event_type: "loading_completed".to_string(),
            model_id: Some(info.id),
            model_name: Some(info.name),
            error: None,
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn get_current_model(
    selected_state: State<'_, SelectedModelState>,
) -> Result<String, String> {
    let current = selected_state.0.lock().unwrap().clone();
    Ok(if current.is_empty() {
        "none".to_string()
    } else {
        current
    })
}

#[tauri::command]
pub async fn cancel_download(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .cancel_download(&model_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recommended_first_model() -> Result<String, String> {
    Ok("parakeet-tdt-0.6b-v3".to_string())
}
