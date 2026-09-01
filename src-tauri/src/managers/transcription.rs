// Transcription: load model, run inference on file. Adapted from Handy (open license).

use crate::managers::model::{EngineType, ModelManager};
use anyhow::Result;
use log::{debug, info};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use transcribe_rs::{
    onnx::{
        canary::CanaryModel, cohere::CohereModel, gigaam::GigaAMModel,
        moonshine::{MoonshineModel, MoonshineVariant, StreamingModel},
        parakeet::ParakeetModel, sense_voice::SenseVoiceModel, Quantization,
    },
    whisper_cpp::WhisperEngine,
    SpeechModel, TranscribeOptions,
};

/// All engines expose the unified `SpeechModel` trait in transcribe-rs 0.3, so we
/// keep a single boxed trait object instead of a per-engine enum.
type LoadedEngine = Box<dyn SpeechModel>;

/// Zero-pad `audio` up to a length the loaded engine's front-end will accept.
///
/// Both Moonshine engines in transcribe-rs 0.3.11 reject certain input lengths
/// outright instead of padding internally:
///
/// * `moonshine_streaming` feeds `samples.chunks(1280)` and its first convolution
///   aborts on a final chunk under 80 samples, so any recording whose length mod
///   1280 falls in `1..=79` fails with "Invalid input shape".
/// * `moonshine` (batch) has the same shape at a 1600-sample floor, i.e. clips
///   shorter than 100 ms.
///
/// Both are upstream bugs. Trailing silence is inaudible to the model and leaves
/// output on already-valid input unchanged, so padding is the safe fix. Without it
/// a fully transcribed recording is discarded by the `?` in the caller.
fn pad_audio_for_engine(engine_id: &str, audio: &mut Vec<f32>) {
    let target = match engine_id {
        "moonshine_streaming" => {
            const CHUNK: usize = 1280;
            audio.len().div_ceil(CHUNK) * CHUNK
        }
        "moonshine" => 1600,
        _ => return,
    };
    if audio.len() < target {
        audio.resize(target, 0.0);
    }
}

pub struct TranscriptionManager {
    engine: Mutex<Option<LoadedEngine>>,
    current_model_id: Mutex<Option<String>>,
    state: Mutex<HashMap<String, TranscriptionState>>,
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
    model_manager: Arc<ModelManager>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionState {
    pub status: String,
    pub progress: f32,
    pub eta_seconds: Option<u64>,
    pub phase: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionStatusEvent {
    pub recording_path: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionPhaseEvent {
    pub recording_path: String,
    pub phase: String,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionProgressEvent {
    pub recording_path: String,
    pub progress: f32,
    pub eta_seconds: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionOpenEvent {
    pub recording_path: String,
}

impl TranscriptionManager {
    pub fn new(model_manager: Arc<ModelManager>) -> Self {
        Self {
            engine: Mutex::new(None),
            current_model_id: Mutex::new(None),
            state: Mutex::new(HashMap::new()),
            cancel_flags: Mutex::new(HashMap::new()),
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

    pub fn create_cancel_flag(&self, recording_path: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.cancel_flags
            .lock()
            .unwrap()
            .insert(recording_path.to_string(), flag.clone());
        flag
    }

    pub fn cancel(&self, recording_path: &str) -> bool {
        if let Some(flag) = self.cancel_flags.lock().unwrap().get(recording_path) {
            flag.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn remove_cancel_flag(&self, recording_path: &str) {
        self.cancel_flags.lock().unwrap().remove(recording_path);
    }

    pub fn get_all_states(&self) -> HashMap<String, TranscriptionState> {
        self.state.lock().unwrap().clone()
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

        // ONNX dir models are quantized variants (model.int8.onnx) except the few
        // shipped as FP32; pick the quantization from the filename convention.
        let quant = if model_info.filename.contains("int8") {
            Quantization::Int8
        } else {
            Quantization::FP32
        };

        // Drop the outgoing engine before building the new one; otherwise a
        // large -> breeze-asr switch transiently holds both (~2.1 GB resident).
        {
            *self.engine.lock().unwrap() = None;
            *self.current_model_id.lock().unwrap() = None;
        }

        let loaded: LoadedEngine = match model_info.engine_type {
            EngineType::Whisper => Box::new(
                WhisperEngine::load(&model_path)
                    .map_err(|e| anyhow::anyhow!("Whisper load failed: {}", e))?,
            ),
            EngineType::Parakeet => Box::new(
                ParakeetModel::load(&model_path, &quant)
                    .map_err(|e| anyhow::anyhow!("Parakeet load failed: {}", e))?,
            ),
            EngineType::Moonshine => Box::new(
                MoonshineModel::load(&model_path, MoonshineVariant::Base, &quant)
                    .map_err(|e| anyhow::anyhow!("Moonshine load failed: {}", e))?,
            ),
            // 0 threads = let ORT pick. The bundles ship plainly-named `.ort`
            // files, so the quant selector must stay at FP32.
            EngineType::MoonshineStreaming => Box::new(
                StreamingModel::load(&model_path, 0, &Quantization::FP32)
                    .map_err(|e| anyhow::anyhow!("Moonshine streaming load failed: {}", e))?,
            ),
            EngineType::GigaAM => Box::new(
                GigaAMModel::load(&model_path, &quant)
                    .map_err(|e| anyhow::anyhow!("GigaAM load failed: {}", e))?,
            ),
            EngineType::SenseVoice => Box::new(
                SenseVoiceModel::load(&model_path, &quant)
                    .map_err(|e| anyhow::anyhow!("SenseVoice load failed: {}", e))?,
            ),
            EngineType::Canary => Box::new(
                CanaryModel::load(&model_path, &quant)
                    .map_err(|e| anyhow::anyhow!("Canary load failed: {}", e))?,
            ),
            EngineType::Cohere => Box::new(
                CohereModel::load(&model_path, &quant)
                    .map_err(|e| anyhow::anyhow!("Cohere load failed: {}", e))?,
            ),
        };

        *self.engine.lock().unwrap() = Some(loaded);
        *self.current_model_id.lock().unwrap() = Some(model_id.to_string());
        debug!("Transcription model loaded: {}", model_id);
        Ok(())
    }

    pub fn transcribe(&self, mut audio: Vec<f32>) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }
        let mut engine_guard = self.engine.lock().unwrap();
        let engine = engine_guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Model not loaded. Select and load a model first.")
        })?;

        pad_audio_for_engine(engine.capabilities().engine_id, &mut audio);

        let result = engine
            .transcribe(&audio, &TranscribeOptions::default())
            .map_err(|x| anyhow::anyhow!("Transcription failed: {}", x))?;

        let text = result.text.trim().to_string();
        if text.is_empty() {
            info!("Transcription result is empty");
        } else {
            info!("Transcription length: {} chars", text.len());
        }
        Ok(text)
    }

    /// Transcribe audio and return word-level segments with timestamps.
    /// Returns Vec<(start_seconds, end_seconds, word_text)>.
    /// For Parakeet: uses Word-level timestamps for precise per-word timing.
    /// For Whisper/Moonshine: returns single segment per chunk (fallback).
    pub fn transcribe_with_timestamps(
        &self,
        mut audio: Vec<f32>,
        chunk_offset_seconds: f64,
    ) -> Result<Vec<(f64, f64, String)>> {
        if audio.is_empty() {
            return Ok(Vec::new());
        }
        let mut engine_guard = self.engine.lock().unwrap();
        let engine = engine_guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Model not loaded. Select and load a model first.")
        })?;

        // Snapshot the real duration before padding, or the fallback segment below
        // stretches by up to 80 ms and drags diarization's word alignment with it.
        let chunk_duration = audio.len() as f64 / 16000.0;
        pad_audio_for_engine(engine.capabilities().engine_id, &mut audio);

        let result = engine
            .transcribe(&audio, &TranscribeOptions::default())
            .map_err(|x| anyhow::anyhow!("Transcription failed: {}", x))?;

        let text = result.text.trim().to_string();
        if text.is_empty() {
            return Ok(Vec::new());
        }

        // If we have segments (word timestamps), use them
        if let Some(segments) = result.segments {
            if !segments.is_empty() {
                let word_segments: Vec<(f64, f64, String)> = segments
                    .into_iter()
                    .filter(|s| !s.text.trim().is_empty())
                    .map(|s| {
                        (
                            chunk_offset_seconds + s.start as f64,
                            chunk_offset_seconds + s.end as f64,
                            s.text,
                        )
                    })
                    .collect();
                info!("Transcription with {} word segments", word_segments.len());
                return Ok(word_segments);
            }
        }

        // Fallback: return whole text as single segment
        info!("Transcription fallback: single segment, {} chars", text.len());
        Ok(vec![(
            chunk_offset_seconds,
            chunk_offset_seconds + chunk_duration,
            text,
        )])
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

#[cfg(test)]
mod tests {
    use super::pad_audio_for_engine;

    #[test]
    fn streaming_pads_to_a_whole_number_of_chunks() {
        // transcribe-rs feeds this engine `samples.chunks(1280)` and its first
        // convolution rejects a final chunk under 80 samples, so every length
        // must come out a clean multiple of 1280.
        for len in [1usize, 2, 79, 80, 1281, 1359, 1360, 16000, 47999] {
            let mut audio = vec![0.5f32; len];
            pad_audio_for_engine("moonshine_streaming", &mut audio);
            assert_eq!(audio.len() % 1280, 0, "len {len} left a partial chunk");
            assert!(audio.len() >= len);
            assert!(audio.len() - len < 1280, "len {len} over-padded");
        }
    }

    #[test]
    fn streaming_leaves_aligned_audio_untouched() {
        for len in [1280usize, 2560, 1280 * 63] {
            let mut audio = vec![0.5f32; len];
            pad_audio_for_engine("moonshine_streaming", &mut audio);
            assert_eq!(audio.len(), len);
        }
    }

    #[test]
    fn batch_moonshine_pads_up_to_the_floor() {
        let mut audio = vec![0.5f32; 800];
        pad_audio_for_engine("moonshine", &mut audio);
        assert_eq!(audio.len(), 1600);

        let mut audio = vec![0.5f32; 4000];
        pad_audio_for_engine("moonshine", &mut audio);
        assert_eq!(audio.len(), 4000);
    }

    #[test]
    fn padding_is_silent_and_preserves_the_original_samples() {
        let mut audio = vec![0.5f32; 100];
        pad_audio_for_engine("moonshine_streaming", &mut audio);
        assert!(audio[..100].iter().all(|s| *s == 0.5));
        assert!(audio[100..].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn other_engines_are_never_padded() {
        for engine in ["whisper_cpp", "parakeet", "gigaam", "sense_voice", "canary"] {
            let mut audio = vec![0.5f32; 777];
            pad_audio_for_engine(engine, &mut audio);
            assert_eq!(audio.len(), 777, "{engine} was padded");
        }
    }
}
