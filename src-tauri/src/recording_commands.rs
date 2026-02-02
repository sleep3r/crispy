use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::Engine;
use tauri::AppHandle;

use crate::app_state::AppState;
use crate::recording;

static RECORDING_ACTIVE: AtomicBool = AtomicBool::new(false);

fn recordings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::paths::recordings_dir(app)?;
    crate::paths::ensure_dir(&dir)?;
    Ok(dir)
}

pub fn do_start_recording(
    app: &AppHandle,
    state: &AppState,
    app_id: &str,
) -> Result<(), String> {
    let mut recording = state.recording.lock().unwrap();

    if recording.writer.lock().unwrap().is_some() {
        return Err("Recording already in progress".to_string());
    }

    let output_dir = recordings_dir(app)?;

    let now = chrono::Local::now();
    let filename = format!("recording_{}.wav", now.format("%Y%m%d_%H%M%S"));
    let output_path = output_dir.join(filename);

    let writer = recording::WavWriter::new(output_path)
        .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

    *recording.writer.lock().unwrap() = Some(writer);
    recording.mic_buffer.lock().unwrap().clear();
    recording.app_buffer.lock().unwrap().clear();

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    if !app_id.is_empty() && app_id != "none" {
        match recording::start_app_audio_capture(app_id, recording.app_buffer.clone()) {
            Ok(stream) => {
                *recording.app_audio_stream.lock().unwrap() = Some(stream);
            }
            Err(e) => {
                eprintln!("Warning: Failed to start app audio capture: {}", e);
            }
        }
    }

    let handle = start_recording_worker(
        recording.mic_buffer.clone(),
        recording.app_buffer.clone(),
        recording.writer.clone(),
    );
    recording.worker = Some(handle);
    Ok(())
}

pub fn do_stop_recording(state: &AppState) -> Result<String, String> {
    RECORDING_ACTIVE.store(false, Ordering::SeqCst);

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let recording = state.recording.lock().unwrap();
        let stream_opt = recording.app_audio_stream.lock().unwrap().take();
        recording.app_buffer.lock().unwrap().clear();
        drop(recording);
        if let Some(stream) = stream_opt {
            let _ = stream.stop_capture();
        }
    }

    let worker_handle = {
        let mut recording = state.recording.lock().unwrap();
        recording.worker.take()
    };

    if let Some(handle) = worker_handle {
        let _ = handle.join();
    }

    let recording = state.recording.lock().unwrap();
    let writer_option = recording.writer.clone();
    let mic_buffer = recording.mic_buffer.clone();
    let app_buffer = recording.app_buffer.clone();
    drop(recording);

    if let Some(writer) = writer_option.lock().unwrap().take() {
        let output_path = writer.finalize()?;
        mic_buffer.lock().unwrap().clear();
        app_buffer.lock().unwrap().clear();
        return Ok(output_path.to_string_lossy().to_string());
    }

    Err("No recording in progress".to_string())
}

fn start_recording_worker(
    mic_buffer: Arc<Mutex<VecDeque<f32>>>,
    app_buffer: Arc<Mutex<VecDeque<f32>>>,
    writer: Arc<Mutex<Option<recording::WavWriter>>>,
) -> std::thread::JoinHandle<()> {
    RECORDING_ACTIVE.store(true, Ordering::SeqCst);

    thread::spawn(move || {
        let frame_size = 1152;
        let mut left_frame = vec![0.0f32; frame_size];
        let mut right_frame = vec![0.0f32; frame_size];
        let mut frames_encoded = 0;

        println!("Recording worker started");

        while RECORDING_ACTIVE.load(Ordering::SeqCst) {
            {
                if writer.lock().unwrap().is_none() {
                    println!("Writer is None, stopping worker");
                    break;
                }
            }

            let mic_available = mic_buffer.lock().unwrap().len();
            if mic_available < frame_size {
                thread::sleep(Duration::from_millis(10));
                continue;
            }

            {
                let mut mic_buf = mic_buffer.lock().unwrap();
                for i in 0..frame_size {
                    left_frame[i] = mic_buf.pop_front().unwrap_or(0.0);
                }
            }

            let app_available = app_buffer.lock().unwrap().len();
            if app_available >= frame_size {
                let mut app_buf = app_buffer.lock().unwrap();
                for i in 0..frame_size {
                    right_frame[i] = app_buf.pop_front().unwrap_or(0.0);
                }
            } else {
                for i in 0..frame_size {
                    right_frame[i] = 0.0;
                }
            }

            for i in 0..frame_size {
                let mixed = left_frame[i] + right_frame[i];
                left_frame[i] = mixed;
                right_frame[i] = mixed;
            }

            {
                let mut guard = writer.lock().unwrap();
                if let Some(w) = guard.as_mut() {
                    if let Err(e) = w.write_samples(&left_frame, &right_frame) {
                        eprintln!("Recording write error: {}", e);
                        break;
                    }
                    frames_encoded += 1;
                    if frames_encoded % 100 == 0 {
                        println!("Wrote {} frames", frames_encoded);
                    }
                } else {
                    break;
                }
            }
        }

        println!(
            "Recording worker stopped. Total frames encoded: {}",
            frames_encoded
        );
        RECORDING_ACTIVE.store(false, Ordering::SeqCst);
    })
}

// --- Tauri commands ---

#[tauri::command]
pub fn get_recordable_apps() -> Result<Vec<recording::RecordableApp>, String> {
    recording::get_recordable_apps()
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    state: tauri::State<AppState>,
    app_id: String,
) -> Result<(), String> {
    do_start_recording(&app, state.inner(), &app_id)
}

#[tauri::command]
pub fn stop_recording(state: tauri::State<AppState>) -> Result<String, String> {
    do_stop_recording(state.inner())
}

#[tauri::command]
pub fn is_recording(state: tauri::State<AppState>) -> Result<bool, String> {
    let recording = state.recording.lock().unwrap();
    let is_active = recording.writer.lock().unwrap().is_some();
    Ok(is_active)
}

#[tauri::command]
pub fn get_recordings_dir_path(app: AppHandle) -> Result<String, String> {
    Ok(recordings_dir(&app)?.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_recordings_dir(app: AppHandle) -> Result<(), String> {
    let recordings_dir = recordings_dir(&app)?;

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&recordings_dir)
        .spawn()
        .map_err(|e| format!("Failed to open directory: {}", e))?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&recordings_dir)
        .spawn()
        .map_err(|e| format!("Failed to open directory: {}", e))?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&recordings_dir)
        .spawn()
        .map_err(|e| format!("Failed to open directory: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/C", "start", &url])
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;

    Ok(())
}

#[derive(serde::Serialize)]
pub struct RecordingFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub created: u64,
}

#[tauri::command]
pub fn get_recordings(app: AppHandle) -> Result<Vec<RecordingFile>, String> {
    let recordings_dir = recordings_dir(&app)?;

    if !recordings_dir.exists() {
        return Ok(Vec::new());
    }

    let mut recordings = Vec::new();
    let entries = std::fs::read_dir(&recordings_dir)
        .map_err(|e| format!("Failed to read recordings directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("wav") {
            let metadata = std::fs::metadata(&path)
                .map_err(|e| format!("Failed to get file metadata: {}", e))?;

            let created = metadata
                .created()
                .or_else(|_| metadata.modified())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);

            recordings.push(RecordingFile {
                name: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                path: path.to_string_lossy().to_string(),
                size: metadata.len(),
                created,
            });
        }
    }

    recordings.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(recordings)
}

#[tauri::command]
pub fn rename_recording(app: tauri::AppHandle, path: String, new_name: String) -> Result<(), String> {
    let old_path_str = path.clone();
    let path = Path::new(&path);
    if !path.exists() {
        return Err("Recording not found".to_string());
    }
    let parent = path.parent().ok_or("Invalid path")?;
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if new_name.contains(std::path::MAIN_SEPARATOR)
        || new_name.contains('/')
        || new_name.contains('\\')
    {
        return Err("Name cannot contain path separators".to_string());
    }
    let base = Path::new(new_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(new_name);
    let new_path = parent.join(format!("{}.wav", base));
    if new_path == path {
        return Ok(());
    }
    if new_path.exists() {
        return Err("A file with this name already exists".to_string());
    }
    std::fs::rename(&path, &new_path).map_err(|e| format!("Failed to rename: {}", e))?;

    let new_path_str = new_path.to_string_lossy();
    if let (Ok(old_txt), Ok(new_txt)) = (
        crate::managers::transcription::transcription_result_path(&app, &old_path_str),
        crate::managers::transcription::transcription_result_path(&app, &new_path_str),
    ) {
        if old_txt.exists() && old_txt != new_txt {
            let _ = std::fs::rename(&old_txt, &new_txt);
        }
    }
    if let (Ok(old_meta), Ok(new_meta)) = (
        crate::managers::transcription::transcription_metadata_path(&app, &old_path_str),
        crate::managers::transcription::transcription_metadata_path(&app, &new_path_str),
    ) {
        if old_meta.exists() && old_meta != new_meta {
            let _ = std::fs::rename(&old_meta, &new_meta);
        }
    }
    if let (Ok(old_chat), Ok(new_chat)) = (
        crate::managers::transcription::transcription_chat_history_path(&app, &old_path_str),
        crate::managers::transcription::transcription_chat_history_path(&app, &new_path_str),
    ) {
        if old_chat.exists() && old_chat != new_chat {
            let _ = std::fs::rename(&old_chat, &new_chat);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn delete_recording(path: String) -> Result<(), String> {
    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete recording: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn read_recording_file(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read recording: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
