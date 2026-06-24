// LLM settings storage and retrieval

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Serializes the read-modify-write cycle for the settings file so concurrent
/// `set_app_setting` / `set_llm_settings` commands can't lose each other's writes.
static SETTINGS_LOCK: Mutex<()> = Mutex::new(());

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
    // Upper bound for NME-SC's automatic speaker-count estimation (not a hard target).
    "6".to_string()
}

fn default_diarization_threshold() -> String {
    "0.50".to_string()
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
            diarization_max_speakers: "6".to_string(),
            diarization_threshold: "0.50".to_string(),
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
    // An existing settings file could not be parsed. Preserve it as a .corrupt
    // backup instead of silently returning defaults and overwriting it on the next
    // save (which would permanently destroy e.g. the stored LLM api_key).
    let backup = path.with_extension("json.corrupt");
    let _ = std::fs::rename(&path, &backup);
    Ok(SettingsFile::default())
}

fn save_settings_file(app: &AppHandle, settings: &SettingsFile) -> Result<()> {
    let path = settings_file_path(app)?;
    let json = serde_json::to_string_pretty(settings)?;
    // Atomic write: write to a sibling temp file and rename over the target so a
    // crash / power loss mid-write can't leave a truncated, unparseable file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn load_llm_settings(app: &AppHandle) -> Result<LlmSettings> {
    Ok(load_settings_file(app)?.llm)
}

pub fn save_llm_settings(app: &AppHandle, settings: &LlmSettings) -> Result<()> {
    let _guard = SETTINGS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = load_settings_file(app)?;
    file.llm = settings.clone();
    save_settings_file(app, &file)
}

pub fn load_app_settings(app: &AppHandle) -> Result<AppSettings> {
    Ok(load_settings_file(app)?.app)
}

pub fn update_app_setting(app: &AppHandle, key: &str, value: String) -> Result<()> {
    // Hold the lock across the whole load-modify-save so concurrent updates to
    // different keys don't clobber each other (lost update). Operate on the full
    // settings file directly (not via a separate locking save helper) to avoid a
    // non-reentrant double-lock.
    let _guard = SETTINGS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = load_settings_file(app)?;
    let settings = &mut file.app;
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
    save_settings_file(app, &file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_settings_default_values() {
        let settings = LlmSettings::default();
        assert_eq!(settings.endpoint, "https://api.openai.com/v1");
        assert!(settings.api_key.is_empty());
        assert_eq!(settings.model, "gpt-4");
    }

    #[test]
    fn app_settings_default_values() {
        let settings = AppSettings::default();
        assert!(settings.selected_microphone.is_empty());
        assert!(settings.selected_output_device.is_empty());
        assert_eq!(settings.microphone_volume, "100");
        assert_eq!(settings.selected_model, "dummy");
        assert_eq!(settings.selected_transcription_model, "none");
        assert_eq!(settings.selected_recording_app, "none");
        assert_eq!(settings.autostart_enabled, "false");
        assert_eq!(settings.diarization_enabled, "false");
        assert_eq!(settings.diarization_max_speakers, "6");
        assert_eq!(settings.diarization_threshold, "0.50");
        assert_eq!(settings.diarization_merge_gap, "2.5");
    }

    #[test]
    fn settings_file_default_values() {
        let settings = SettingsFile::default();
        assert_eq!(settings.llm.endpoint, "https://api.openai.com/v1");
        assert_eq!(settings.app.selected_model, "dummy");
    }

    #[test]
    fn llm_settings_serialization_roundtrip() {
        let settings = LlmSettings {
            endpoint: "https://custom.api.com".to_string(),
            api_key: "sk-test-key".to_string(),
            model: "gpt-4o".to_string(),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: LlmSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.endpoint, settings.endpoint);
        assert_eq!(deserialized.api_key, settings.api_key);
        assert_eq!(deserialized.model, settings.model);
    }

    #[test]
    fn app_settings_serialization_roundtrip() {
        let mut settings = AppSettings::default();
        settings.selected_microphone = "mic-1".to_string();
        settings.microphone_volume = "75".to_string();
        settings.diarization_enabled = "true".to_string();

        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.selected_microphone, "mic-1");
        assert_eq!(deserialized.microphone_volume, "75");
        assert_eq!(deserialized.diarization_enabled, "true");
    }

    #[test]
    fn settings_file_full_roundtrip() {
        let settings = SettingsFile {
            llm: LlmSettings {
                endpoint: "https://api.example.com".to_string(),
                api_key: "key123".to_string(),
                model: "claude".to_string(),
            },
            app: AppSettings {
                selected_microphone: "mic-2".to_string(),
                diarization_max_speakers: "5".to_string(),
                ..AppSettings::default()
            },
        };

        let json = serde_json::to_string_pretty(&settings).unwrap();
        let deserialized: SettingsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.llm.model, "claude");
        assert_eq!(deserialized.app.selected_microphone, "mic-2");
        assert_eq!(deserialized.app.diarization_max_speakers, "5");
        // Verify defaults are preserved for unset fields
        assert_eq!(deserialized.app.microphone_volume, "100");
    }

    #[test]
    fn app_settings_deserializes_with_missing_diarization_fields() {
        // Simulates loading a settings file from before diarization was added
        let json = r#"{
            "selected_microphone": "test-mic",
            "selected_output_device": "",
            "microphone_volume": "80",
            "selected_model": "rnnoise",
            "selected_transcription_model": "small",
            "selected_recording_app": "none"
        }"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.selected_microphone, "test-mic");
        assert_eq!(settings.microphone_volume, "80");
        // Missing fields should get defaults
        assert_eq!(settings.autostart_enabled, "false");
        assert_eq!(settings.diarization_enabled, "false");
        assert_eq!(settings.diarization_max_speakers, "6");
        assert_eq!(settings.diarization_threshold, "0.50");
        assert_eq!(settings.diarization_merge_gap, "2.5");
    }

    #[test]
    fn llm_settings_public_omits_api_key() {
        let public_settings = LlmSettingsPublic {
            endpoint: "https://api.openai.com/v1".to_string(),
            model: "gpt-4".to_string(),
        };
        let json = serde_json::to_string(&public_settings).unwrap();
        assert!(!json.contains("api_key"));
        assert!(json.contains("endpoint"));
        assert!(json.contains("model"));
    }

    #[test]
    fn settings_file_deserializes_from_llm_only_json() {
        let json = r#"{
            "endpoint": "https://api.openai.com/v1",
            "api_key": "sk-test",
            "model": "gpt-4"
        }"#;
        // This is the legacy format where only LLM settings were saved
        let llm: LlmSettings = serde_json::from_str(json).unwrap();
        assert_eq!(llm.endpoint, "https://api.openai.com/v1");
        assert_eq!(llm.api_key, "sk-test");
    }
}
