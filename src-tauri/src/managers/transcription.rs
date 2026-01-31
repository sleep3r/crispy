// Transcription: load model, run inference on file. Adapted from Handy (open license).

use crate::managers::model::{EngineType, ModelManager};
use anyhow::Result;
use hound::WavReader;
use log::{debug, info};
use rubato::{FftFixedIn, Resampler};
use std::path::Path;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Manager;
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

const WHISPER_SAMPLE_RATE: usize = 16000;
const RESAMPLER_CHUNK: usize = 1024;

/// Read WAV file and return mono f32 samples at 16 kHz for transcription.
pub fn wav_to_16k_mono_f32(wav_path: &Path) -> Result<Vec<f32>> {
    let mut reader = WavReader::open(wav_path)?;
    let spec = reader.spec();
    let sample_rate_in = spec.sample_rate as usize;
    let channels = spec.channels as usize;

    let mut mono_48k: Vec<f32> = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = 32768.0f32;
            for s in reader.samples::<i16>() {
                let s = s?;
                mono_48k.push((s as f32) / max_val);
            }
        }
        hound::SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                mono_48k.push(s?);
            }
        }
    }

    // Stereo -> mono: take left channel (every first sample per frame)
    if channels == 2 {
        mono_48k = mono_48k.iter().step_by(2).copied().collect();
    }

    if sample_rate_in == WHISPER_SAMPLE_RATE {
        return Ok(mono_48k);
    }

    // Resample to 16 kHz
    let mut resampler = FftFixedIn::<f32>::new(
        sample_rate_in,
        WHISPER_SAMPLE_RATE,
        RESAMPLER_CHUNK,
        1,
        1,
    )?;
    let mut out = Vec::with_capacity(mono_48k.len() * WHISPER_SAMPLE_RATE / sample_rate_in);
    let mut pos = 0;
    while pos + RESAMPLER_CHUNK <= mono_48k.len() {
        let chunk = &mono_48k[pos..pos + RESAMPLER_CHUNK];
        let out_chunk = resampler.process(&[chunk], None)?;
        out.extend_from_slice(&out_chunk[0]);
        pos += RESAMPLER_CHUNK;
    }
    if pos < mono_48k.len() {
        let mut pad = mono_48k[pos..].to_vec();
        pad.resize(RESAMPLER_CHUNK, 0.0);
        let out_chunk = resampler.process(&[&pad], None)?;
        out.extend_from_slice(&out_chunk[0]);
    }
    Ok(out)
}

enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetEngine),
    Moonshine(MoonshineEngine),
}

pub struct TranscriptionManager {
    engine: Mutex<Option<LoadedEngine>>,
    current_model_id: Mutex<Option<String>>,
    model_manager: std::sync::Arc<ModelManager>,
}

impl TranscriptionManager {
    pub fn new(model_manager: std::sync::Arc<ModelManager>) -> Self {
        Self {
            engine: Mutex::new(None),
            current_model_id: Mutex::new(None),
            model_manager,
        }
    }

    pub fn is_model_loaded(&self) -> bool {
        self.engine.lock().unwrap().is_some()
    }

    pub fn get_current_model(&self) -> Option<String> {
        self.current_model_id.lock().unwrap().clone()
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

/// Store transcription result by recording path. Uses a hash of path as filename.
pub fn transcription_result_path(app: &AppHandle, recording_path: &str) -> Result<std::path::PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("app data dir: {}", e))?
        .join("transcriptions");
    std::fs::create_dir_all(&dir)?;
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
pub fn transcription_metadata_path(app: &AppHandle, recording_path: &str) -> Result<std::path::PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("app data dir: {}", e))?
        .join("transcriptions");
    std::fs::create_dir_all(&dir)?;
    let name = transcription_file_stem(recording_path);
    Ok(dir.join(format!("{}.meta", name)))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TranscriptionMetadata {
    model_id: String,
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
