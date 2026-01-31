use crate::llm_settings::{load_app_settings, update_app_setting, AppSettings};
use tauri::AppHandle;

#[tauri::command]
pub async fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    load_app_settings(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_app_setting(app: AppHandle, key: String, value: String) -> Result<(), String> {
    update_app_setting(&app, &key, value).map_err(|e| e.to_string())
}
