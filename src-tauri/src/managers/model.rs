// Transcription models: list, download, init. Adapted from Handy (open license).

use anyhow::Result;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tar::Archive;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineType {
    Whisper,
    Parakeet,
    Moonshine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: Option<String>,
    pub size_mb: u64,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
    pub is_directory: bool,
    pub engine_type: EngineType,
    pub accuracy_score: f32,
    pub speed_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

pub struct ModelManager {
    app_handle: AppHandle,
    models_dir: PathBuf,
    available_models: Mutex<HashMap<String, ModelInfo>>,
}

impl ModelManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        let models_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?
            .join("models");

        if !models_dir.exists() {
            fs::create_dir_all(&models_dir)?;
        }

        let mut available_models = HashMap::new();

        available_models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: "Fast and fairly accurate.".to_string(),
                filename: "ggml-small.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-small.bin".to_string()),
                size_mb: 487,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.60,
                speed_score: 0.85,
            },
        );

        available_models.insert(
            "medium".to_string(),
            ModelInfo {
                id: "medium".to_string(),
                name: "Whisper Medium".to_string(),
                description: "Good accuracy, medium speed".to_string(),
                filename: "whisper-medium-q4_1.bin".to_string(),
                url: Some("https://blob.handy.computer/whisper-medium-q4_1.bin".to_string()),
                size_mb: 492,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.75,
                speed_score: 0.60,
            },
        );

        available_models.insert(
            "turbo".to_string(),
            ModelInfo {
                id: "turbo".to_string(),
                name: "Whisper Turbo".to_string(),
                description: "Balanced accuracy and speed.".to_string(),
                filename: "ggml-large-v3-turbo.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-large-v3-turbo.bin".to_string()),
                size_mb: 1600,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.80,
                speed_score: 0.40,
            },
        );

        available_models.insert(
            "large".to_string(),
            ModelInfo {
                id: "large".to_string(),
                name: "Whisper Large".to_string(),
                description: "Good accuracy, but slow.".to_string(),
                filename: "ggml-large-v3-q5_0.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-large-v3-q5_0.bin".to_string()),
                size_mb: 1100,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.85,
                speed_score: 0.30,
            },
        );

        available_models.insert(
            "parakeet-tdt-0.6b-v2".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v2".to_string(),
                name: "Parakeet V2".to_string(),
                description: "English only. The best model for English speakers.".to_string(),
                filename: "parakeet-tdt-0.6b-v2-int8".to_string(),
                url: Some("https://blob.handy.computer/parakeet-v2-int8.tar.gz".to_string()),
                size_mb: 473,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.85,
                speed_score: 0.85,
            },
        );

        available_models.insert(
            "parakeet-tdt-0.6b-v3".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v3".to_string(),
                name: "Parakeet V3".to_string(),
                description: "Fast and accurate".to_string(),
                filename: "parakeet-tdt-0.6b-v3-int8".to_string(),
                url: Some("https://blob.handy.computer/parakeet-v3-int8.tar.gz".to_string()),
                size_mb: 478,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.80,
                speed_score: 0.85,
            },
        );

        available_models.insert(
            "moonshine-base".to_string(),
            ModelInfo {
                id: "moonshine-base".to_string(),
                name: "Moonshine Base".to_string(),
                description: "Very fast, English only. Handles accents well.".to_string(),
                filename: "moonshine-base".to_string(),
                url: Some("https://blob.handy.computer/moonshine-base.tar.gz".to_string()),
                size_mb: 58,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Moonshine,
                accuracy_score: 0.70,
                speed_score: 0.90,
            },
        );

        let manager = Self {
            app_handle: app_handle.clone(),
            models_dir,
            available_models: Mutex::new(available_models),
        };

        manager.migrate_bundled_models()?;
        manager.update_download_status()?;

        Ok(manager)
    }

    pub fn get_available_models(&self) -> Vec<ModelInfo> {
        let models = self.available_models.lock().unwrap();
        models.values().cloned().collect()
    }

    pub fn get_model_info(&self, model_id: &str) -> Option<ModelInfo> {
        let models = self.available_models.lock().unwrap();
        models.get(model_id).cloned()
    }

    fn migrate_bundled_models(&self) -> Result<()> {
        let bundled_models = ["ggml-small.bin"];
        for filename in &bundled_models {
            let bundled_path = self.app_handle.path().resolve(
                &format!("resources/models/{}", filename),
                tauri::path::BaseDirectory::Resource,
            );
            if let Ok(bundled_path) = bundled_path {
                if bundled_path.exists() {
                    let user_path = self.models_dir.join(filename);
                    if !user_path.exists() {
                        fs::copy(&bundled_path, &user_path)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn update_download_status(&self) -> Result<()> {
        let mut models = self.available_models.lock().unwrap();
        for model in models.values_mut() {
            if model.is_directory {
                let model_path = self.models_dir.join(&model.filename);
                let partial_path = self
                    .models_dir
                    .join(format!("{}.partial", &model.filename));
                let extracting_path = self
                    .models_dir
                    .join(format!("{}.extracting", &model.filename));
                if extracting_path.exists() {
                    let _ = fs::remove_dir_all(&extracting_path);
                }
                model.is_downloaded = model_path.exists() && model_path.is_dir();
                model.is_downloading = false;
                model.partial_size = partial_path
                    .exists()
                    .then(|| partial_path.metadata().map(|m| m.len()).unwrap_or(0))
                    .unwrap_or(0);
            } else {
                let model_path = self.models_dir.join(&model.filename);
                let partial_path = self
                    .models_dir
                    .join(format!("{}.partial", &model.filename));
                model.is_downloaded = model_path.exists();
                model.is_downloading = false;
                model.partial_size = partial_path
                    .exists()
                    .then(|| partial_path.metadata().map(|m| m.len()).unwrap_or(0))
                    .unwrap_or(0);
            }
        }
        Ok(())
    }

    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };
        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        let url = model_info
            .url
            .ok_or_else(|| anyhow::anyhow!("No download URL for model"))?;
        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));

        if model_path.exists() {
            if partial_path.exists() {
                let _ = fs::remove_file(&partial_path);
            }
            self.update_download_status()?;
            return Ok(());
        }

        let mut resume_from = if partial_path.exists() {
            partial_path.metadata()?.len()
        } else {
            0
        };

        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = true;
            }
        }

        let client = reqwest::Client::new();
        let mut request = client.get(&url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }
        let mut response = request.send().await?;

        if resume_from > 0 && response.status() == reqwest::StatusCode::OK {
            drop(response);
            let _ = fs::remove_file(&partial_path);
            resume_from = 0;
            response = client.get(&url).send().await?;
        }

        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = false;
            }
            return Err(anyhow::anyhow!(
                "Failed to download model: HTTP {}",
                response.status()
            ));
        }

        let total_size = if resume_from > 0 {
            resume_from + response.content_length().unwrap_or(0)
        } else {
            response.content_length().unwrap_or(0)
        };

        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();
        let mut file = if resume_from > 0 {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)?
        } else {
            std::fs::File::create(&partial_path)?
        };

        let _ = self.app_handle.emit(
            "model-download-progress",
            &DownloadProgress {
                model_id: model_id.to_string(),
                downloaded,
                total: total_size,
                percentage: if total_size > 0 {
                    (downloaded as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                },
            },
        );

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                let mut models = self.available_models.lock().unwrap();
                if let Some(model) = models.get_mut(model_id) {
                    model.is_downloading = false;
                }
                e
            })?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            let percentage = if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                0.0
            };
            let _ = self.app_handle.emit(
                "model-download-progress",
                &DownloadProgress {
                    model_id: model_id.to_string(),
                    downloaded,
                    total: total_size,
                    percentage,
                },
            );
        }

        file.flush()?;
        drop(file);

        if total_size > 0 {
            let actual_size = partial_path.metadata()?.len();
            if actual_size != total_size {
                let _ = fs::remove_file(&partial_path);
                let mut models = self.available_models.lock().unwrap();
                if let Some(model) = models.get_mut(model_id) {
                    model.is_downloading = false;
                }
                return Err(anyhow::anyhow!(
                    "Download incomplete: expected {} bytes, got {} bytes",
                    total_size,
                    actual_size
                ));
            }
        }

        if model_info.is_directory {
            let _ = self.app_handle.emit("model-extraction-started", model_id);
            let temp_extract_dir = self
                .models_dir
                .join(format!("{}.extracting", &model_info.filename));
            let final_model_dir = self.models_dir.join(&model_info.filename);
            if temp_extract_dir.exists() {
                let _ = fs::remove_dir_all(&temp_extract_dir);
            }
            fs::create_dir_all(&temp_extract_dir)?;
            let tar_gz = File::open(&partial_path)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);
            archive.unpack(&temp_extract_dir).map_err(|e| {
                let _ = fs::remove_dir_all(&temp_extract_dir);
                anyhow::anyhow!("Failed to extract archive: {}", e)
            })?;
            let extracted_dirs: Vec<_> = fs::read_dir(&temp_extract_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .collect();
            if extracted_dirs.len() == 1 {
                let source_dir = extracted_dirs[0].path();
                if final_model_dir.exists() {
                    fs::remove_dir_all(&final_model_dir)?;
                }
                fs::rename(&source_dir, &final_model_dir)?;
                let _ = fs::remove_dir_all(&temp_extract_dir);
            } else {
                if final_model_dir.exists() {
                    fs::remove_dir_all(&final_model_dir)?;
                }
                fs::rename(&temp_extract_dir, &final_model_dir)?;
            }
            let _ = self.app_handle.emit("model-extraction-completed", model_id);
            let _ = fs::remove_file(&partial_path);
        } else {
            fs::rename(&partial_path, &model_path)?;
        }

        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = false;
                model.is_downloaded = true;
                model.partial_size = 0;
            }
        }
        let _ = self.app_handle.emit("model-download-complete", model_id);
        Ok(())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        let model_info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));
        let mut deleted = false;
        if model_info.is_directory {
            if model_path.exists() && model_path.is_dir() {
                fs::remove_dir_all(&model_path)?;
                deleted = true;
            }
        } else if model_path.exists() {
            fs::remove_file(&model_path)?;
            deleted = true;
        }
        if partial_path.exists() {
            fs::remove_file(&partial_path)?;
            deleted = true;
        }
        if !deleted {
            return Err(anyhow::anyhow!("No model files found to delete"));
        }
        self.update_download_status()?;
        Ok(())
    }

    pub fn get_model_path(&self, model_id: &str) -> Result<PathBuf> {
        let model_info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        if !model_info.is_downloaded {
            return Err(anyhow::anyhow!("Model not available: {}", model_id));
        }
        if model_info.is_downloading {
            return Err(anyhow::anyhow!("Model is currently downloading: {}", model_id));
        }
        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));
        if model_info.is_directory {
            if model_path.exists() && model_path.is_dir() && !partial_path.exists() {
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model directory not found: {}",
                    model_id
                ))
            }
        } else if model_path.exists() && !partial_path.exists() {
            Ok(model_path)
        } else {
            Err(anyhow::anyhow!("Complete model file not found: {}", model_id))
        }
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        let _ = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        let mut models = self.available_models.lock().unwrap();
        if let Some(model) = models.get_mut(model_id) {
            model.is_downloading = false;
        }
        self.update_download_status()?;
        Ok(())
    }
}
