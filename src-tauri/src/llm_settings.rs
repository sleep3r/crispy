// LLM settings storage and retrieval

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub selected_microphone: String,
    pub selected_output_device: String,
    pub microphone_volume: String,
    pub selected_model: String,
    pub selected_transcription_model: String,
    pub selected_recording_app: String,
    #[serde(default = "default_false_string")]
    pub autostart_enabled: String,
    #[serde(default = "default_false_string")]
    pub diarization_enabled: String,
    #[serde(default = "default_diarization_max_speakers")]
    pub diarization_max_speakers: String,
    #[serde(default = "default_diarization_threshold")]
    pub diarization_threshold: String,
    #[serde(default = "default_diarization_merge_gap")]
    pub diarization_merge_gap: String,
}

fn default_false_string() -> String {
    "false".to_string()
}

fn default_diarization_max_speakers() -> String {
    "3".to_string()
}

fn default_diarization_threshold() -> String {
    "0.30".to_string()
}

fn default_diarization_merge_gap() -> String {
    "2.5".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            selected_microphone: String::new(),
            selected_output_device: String::new(),
            microphone_volume: "100".to_string(),
            selected_model: "dummy".to_string(),
            selected_transcription_model: "none".to_string(),
            selected_recording_app: "none".to_string(),
            autostart_enabled: "false".to_string(),
            diarization_enabled: "false".to_string(),
            diarization_max_speakers: "3".to_string(),
            diarization_threshold: "0.30".to_string(),
            diarization_merge_gap: "2.5".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettingsPublic {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsFile {
    pub llm: LlmSettings,
    pub app: AppSettings,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            llm: LlmSettings::default(),
            app: AppSettings::default(),
        }
    }
}

fn settings_file_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = crate::paths::crispy_documents_root(app).map_err(|e| anyhow::anyhow!(e))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("settings.json"))
}

fn legacy_settings_file_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("app data dir: {}", e))?;
    Ok(dir.join("settings.json"))
}

fn load_settings_file(app: &AppHandle) -> Result<SettingsFile> {
    let path = settings_file_path(app)?;
    if !path.exists() {
        // Try legacy location for automatic migration
        if let Ok(legacy_path) = legacy_settings_file_path(app) {
            if legacy_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&legacy_path) {
                    if let Ok(settings) = serde_json::from_str::<SettingsFile>(&contents) {
                        let _ = save_settings_file(app, &settings);
                        return Ok(settings);
                    }
                    if let Ok(llm_only) = serde_json::from_str::<LlmSettings>(&contents) {
                        let settings = SettingsFile {
                            llm: llm_only,
                            app: AppSettings::default(),
                        };
                        let _ = save_settings_file(app, &settings);
                        return Ok(settings);
                    }
                    if let Ok(app_only) = serde_json::from_str::<AppSettings>(&contents) {
                        let settings = SettingsFile {
                            llm: LlmSettings::default(),
                            app: app_only,
                        };
                        let _ = save_settings_file(app, &settings);
                        return Ok(settings);
                    }
                }
            }
        }
        return Ok(SettingsFile::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    if let Ok(settings) = serde_json::from_str::<SettingsFile>(&contents) {
        return Ok(settings);
    }
    if let Ok(llm_only) = serde_json::from_str::<LlmSettings>(&contents) {
        return Ok(SettingsFile {
            llm: llm_only,
            app: AppSettings::default(),
        });
    }
    if let Ok(app_only) = serde_json::from_str::<AppSettings>(&contents) {
        return Ok(SettingsFile {
            llm: LlmSettings::default(),
            app: app_only,
        });
    }
    Ok(SettingsFile::default())
}

fn save_settings_file(app: &AppHandle, settings: &SettingsFile) -> Result<()> {
    let path = settings_file_path(app)?;
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn load_llm_settings(app: &AppHandle) -> Result<LlmSettings> {
    Ok(load_settings_file(app)?.llm)
}

pub fn save_llm_settings(app: &AppHandle, settings: &LlmSettings) -> Result<()> {
    let mut file = load_settings_file(app)?;
    file.llm = settings.clone();
    save_settings_file(app, &file)
}

pub fn load_app_settings(app: &AppHandle) -> Result<AppSettings> {
    Ok(load_settings_file(app)?.app)
}

pub fn save_app_settings(app: &AppHandle, settings: &AppSettings) -> Result<()> {
    let mut file = load_settings_file(app)?;
    file.app = settings.clone();
    save_settings_file(app, &file)
}

pub fn update_app_setting(app: &AppHandle, key: &str, value: String) -> Result<()> {
    let mut settings = load_app_settings(app)?;
    match key {
        "selected_microphone" => settings.selected_microphone = value,
        "selected_output_device" => settings.selected_output_device = value,
        "microphone_volume" => settings.microphone_volume = value,
        "selected_model" => settings.selected_model = value,
        "selected_transcription_model" => settings.selected_transcription_model = value,
        "selected_recording_app" => settings.selected_recording_app = value,
        "autostart_enabled" => settings.autostart_enabled = value,
        "diarization_enabled" => settings.diarization_enabled = value,
        "diarization_max_speakers" => settings.diarization_max_speakers = value,
        "diarization_threshold" => settings.diarization_threshold = value,
        "diarization_merge_gap" => settings.diarization_merge_gap = value,
        _ => return Err(anyhow::anyhow!("Unknown setting key: {}", key)),
    }
    save_app_settings(app, &settings)
}
