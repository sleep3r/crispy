use crate::settings::{load_app_settings, load_llm_settings, save_llm_settings, update_app_setting, AppSettings, LlmSettings, LlmSettingsPublic};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub async fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    load_app_settings(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_app_setting(app: AppHandle, key: String, value: String) -> Result<(), String> {
    update_app_setting(&app, &key, value).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    // Update settings
    update_app_setting(&app, "autostart_enabled", enabled.to_string()).map_err(|e| e.to_string())?;
    
    // Apply autostart setting via plugin
    let autostart_manager = app.autolaunch();
    if enabled {
        autostart_manager.enable().map_err(|e| e.to_string())?;
    } else {
        autostart_manager.disable().map_err(|e| e.to_string())?;
    }
    
    Ok(())
}

/// Get LLM settings (endpoint and model, omit API key for security)
#[tauri::command]
pub async fn get_llm_settings(app: AppHandle) -> Result<LlmSettingsPublic, String> {
    let settings = load_llm_settings(&app).map_err(|e| e.to_string())?;
    Ok(LlmSettingsPublic {
        endpoint: settings.endpoint,
        model: settings.model,
    })
}

/// Set LLM settings (endpoint, API key, model)
#[tauri::command]
pub async fn set_llm_settings(
    app: AppHandle,
    endpoint: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let settings = LlmSettings {
        endpoint,
        api_key,
        model,
    };
    save_llm_settings(&app, &settings).map_err(|e| e.to_string())?;
    Ok(())
}
