// Transcription: load model, run inference on file. Adapted from Handy (open license).

use crate::managers::model::{EngineType, ModelManager};
use anyhow::Result;
use log::{debug, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use transcribe_rs::{
    engines::{
        moonshine::{ModelVariant, MoonshineEngine, MoonshineModelParams},
        parakeet::{
            ParakeetEngine, ParakeetInferenceParams, ParakeetModelParams, TimestampGranularity,
        },
        whisper::{WhisperEngine, WhisperInferenceParams},
    },
    TranscriptionEngine,
};

enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetEngine),
    Moonshine(MoonshineEngine),
}

pub struct TranscriptionManager {
    engine: Mutex<Option<LoadedEngine>>,
    current_model_id: Mutex<Option<String>>,
    state: Mutex<HashMap<String, TranscriptionState>>,
    model_manager: std::sync::Arc<ModelManager>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionState {
    pub status: String,
    pub progress: f32,
    pub eta_seconds: Option<u64>,
    pub phase: Option<String>,
}

impl TranscriptionManager {
    pub fn new(model_manager: std::sync::Arc<ModelManager>) -> Self {
        Self {
            engine: Mutex::new(None),
            current_model_id: Mutex::new(None),
            state: Mutex::new(HashMap::new()),
            model_manager,
        }
    }

    pub fn get_current_model(&self) -> Option<String> {
        self.current_model_id.lock().unwrap().clone()
    }

    pub fn set_state(&self, recording_path: &str, state: TranscriptionState) {
        self.state
            .lock()
            .unwrap()
            .insert(recording_path.to_string(), state);
    }

    pub fn get_state(&self, recording_path: &str) -> Option<TranscriptionState> {
        self.state.lock().unwrap().get(recording_path).cloned()
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        if !model_info.is_downloaded {
            return Err(anyhow::anyhow!("Model not downloaded"));
        }
        let model_path = self.model_manager.get_model_path(model_id)?;

        let loaded = match model_info.engine_type {
            EngineType::Whisper => {
                let mut engine = WhisperEngine::new();
                engine.load_model(&model_path)
                    .map_err(|e| anyhow::anyhow!("Whisper load failed: {}", e))?;
                LoadedEngine::Whisper(engine)
            }
            EngineType::Parakeet => {
                let mut engine = ParakeetEngine::new();
                engine
                    .load_model_with_params(&model_path, ParakeetModelParams::int8())
                    .map_err(|e| anyhow::anyhow!("Parakeet load failed: {}", e))?;
                LoadedEngine::Parakeet(engine)
            }
            EngineType::Moonshine => {
                let mut engine = MoonshineEngine::new();
                engine
                    .load_model_with_params(
                        &model_path,
                        MoonshineModelParams::variant(ModelVariant::Base),
                    )
                    .map_err(|e| anyhow::anyhow!("Moonshine load failed: {}", e))?;
                LoadedEngine::Moonshine(engine)
            }
        };

        *self.engine.lock().unwrap() = Some(loaded);
        *self.current_model_id.lock().unwrap() = Some(model_id.to_string());
        debug!("Transcription model loaded: {}", model_id);
        Ok(())
    }

    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }
        let mut engine_guard = self.engine.lock().unwrap();
        let engine = engine_guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Model not loaded. Select and load a model first.")
        })?;

        let result = match engine {
            LoadedEngine::Whisper(e) => e
                .transcribe_samples(audio, Some(WhisperInferenceParams::default()))
                .map_err(|x| anyhow::anyhow!("Whisper: {}", x))?,
            LoadedEngine::Parakeet(e) => e
                .transcribe_samples(
                    audio,
                    Some(ParakeetInferenceParams {
                        timestamp_granularity: TimestampGranularity::Segment,
                        ..Default::default()
                    }),
                )
                .map_err(|x| anyhow::anyhow!("Parakeet: {}", x))?,
            LoadedEngine::Moonshine(e) => e
                .transcribe_samples(audio, None)
                .map_err(|x| anyhow::anyhow!("Moonshine: {}", x))?,
        };

        let text = result.text.trim().to_string();
        if text.is_empty() {
            info!("Transcription result is empty");
        } else {
            info!("Transcription length: {} chars", text.len());
        }
        Ok(text)
    }
}

/// Base directory for transcriptions: ~/Documents/Crispy/Transcriptions (next to Recordings and settings).
fn transcriptions_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = crate::paths::transcriptions_dir(app)
        .map_err(|e| anyhow::anyhow!(e))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Store transcription result by recording path. Uses a hash of path as filename.
pub fn transcription_result_path(_app: &AppHandle, recording_path: &str) -> Result<PathBuf> {
    let dir = transcriptions_dir(_app)?;
    let name = transcription_file_stem(recording_path);
    Ok(dir.join(format!("{}.txt", name)))
}

fn transcription_file_stem(recording_path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    recording_path.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Path to metadata file (model_id) for a transcription. Same stem as .txt but .meta.
pub fn transcription_metadata_path(_app: &AppHandle, recording_path: &str) -> Result<PathBuf> {
    let dir = transcriptions_dir(_app)?;
    let name = transcription_file_stem(recording_path);
    Ok(dir.join(format!("{}.meta", name)))
}

/// Path to chat history file for a transcription. Same stem as .txt but .chat.json.
pub fn transcription_chat_history_path(
    _app: &AppHandle,
    recording_path: &str,
) -> Result<PathBuf> {
    let dir = transcriptions_dir(_app)?;
    let name = transcription_file_stem(recording_path);
    Ok(dir.join(format!("{}.chat.json", name)))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TranscriptionMetadata {
    model_id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ChatHistoryMessage {
    pub role: String, // "user" | "assistant"
    pub content: String,
}

pub fn save_transcription_result(app: &AppHandle, recording_path: &str, text: &str) -> Result<()> {
    let path = transcription_result_path(app, recording_path)?;
    std::fs::write(&path, text)?;
    Ok(())
}

pub fn save_transcription_metadata(app: &AppHandle, recording_path: &str, model_id: &str) -> Result<()> {
    let path = transcription_metadata_path(app, recording_path)?;
    let meta = TranscriptionMetadata {
        model_id: model_id.to_string(),
    };
    let json = serde_json::to_string(&meta)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn load_transcription_result(app: &AppHandle, recording_path: &str) -> Result<Option<String>> {
    let path = transcription_result_path(app, recording_path)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(text))
}

pub fn load_transcription_metadata(app: &AppHandle, recording_path: &str) -> Result<Option<String>> {
    let path = transcription_metadata_path(app, recording_path)?;
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path)?;
    let meta: TranscriptionMetadata = serde_json::from_str(&json).map_err(|e| anyhow::anyhow!("metadata: {}", e))?;
    Ok(Some(meta.model_id))
}

pub fn save_transcription_chat_history(
    app: &AppHandle,
    recording_path: &str,
    messages: &[ChatHistoryMessage],
) -> Result<()> {
    let path = transcription_chat_history_path(app, recording_path)?;
    let json = serde_json::to_string_pretty(messages)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn load_transcription_chat_history(
    app: &AppHandle,
    recording_path: &str,
) -> Result<Vec<ChatHistoryMessage>> {
    let path = transcription_chat_history_path(app, recording_path)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let json = std::fs::read_to_string(&path)?;
    let messages: Vec<ChatHistoryMessage> =
        serde_json::from_str(&json).map_err(|e| anyhow::anyhow!("chat history: {}", e))?;
    Ok(messages)
}
