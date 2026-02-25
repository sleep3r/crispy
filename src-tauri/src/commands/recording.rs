use std::collections::VecDeque;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
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

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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
    // Resolve bundle_id to actual PID-based app id if needed.
    // Settings now store bundle_id (e.g. "com.spotify.client") instead of
    // PID-based ids (e.g. "com.spotify.client_12345"). We resolve the bundle_id
    // to a currently running instance here.
    let resolved_app_id = if !app_id.is_empty() && app_id != "none" {
        let apps = recording::get_recordable_apps().unwrap_or_default();
        if let Some(running) = apps.iter().find(|a| a.bundle_id == app_id) {
            running.id.clone()
        } else {
            // Fallback: use as-is (might be an old PID-based id)
            app_id.to_string()
        }
    } else {
        app_id.to_string()
    };
    let app_id = &resolved_app_id;

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

    #[cfg(target_os = "windows")]
    if !app_id.is_empty() && app_id != "none" {
        // Reset stop flag
        recording
            .app_audio_stop
            .store(false, std::sync::atomic::Ordering::SeqCst);

        match recording::start_app_audio_capture(
            app_id,
            recording.app_buffer.clone(),
            recording.app_audio_stop.clone(),
        ) {
            Ok(handle) => {
                *recording.app_audio_worker.lock().unwrap() = Some(handle);
            }
            Err(e) => {
                eprintln!("Warning: Failed to start app audio capture: {}", e);
                eprintln!("Note: Process loopback requires Windows 10 2004+ (build 19041)");
                // Continue with mic-only recording
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

    #[cfg(target_os = "windows")]
    {
        let recording = state.recording.lock().unwrap();
        // Signal stop to capture thread
        recording
            .app_audio_stop
            .store(true, std::sync::atomic::Ordering::SeqCst);
        
        // Take worker handle and join
        let worker_opt = recording.app_audio_worker.lock().unwrap().take();
        drop(recording); // Release lock before joining
        
        if let Some(handle) = worker_opt {
            let _ = handle.join();
        }
        
        // Clear buffer
        let recording = state.recording.lock().unwrap();
        recording.app_buffer.lock().unwrap().clear();
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
        // Keep streams roughly aligned within ~50ms to reduce lip-sync drift.
        let max_desync_samples = (recording::SAMPLE_RATE / 20).max(frame_size); // 50 ms @ 48kHz
        let mut left_frame = vec![0.0f32; frame_size];
        let mut right_frame = vec![0.0f32; frame_size];
        let mut frames_encoded = 0;

        if std::env::var("CRISPY_AUDIO_DEBUG").is_ok() {
            println!("Recording worker started");
        }

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

            // Align buffer heads if one source gets significantly ahead.
            {
                let mut mic_buf = mic_buffer.lock().unwrap();
                let mut app_buf = app_buffer.lock().unwrap();
                let mic_len = mic_buf.len();
                let app_len = app_buf.len();

                if mic_len > app_len + max_desync_samples {
                    let trim = mic_len - app_len - max_desync_samples;
                    for _ in 0..trim {
                        let _ = mic_buf.pop_front();
                    }
                } else if app_len > mic_len + max_desync_samples {
                    let trim = app_len - mic_len - max_desync_samples;
                    for _ in 0..trim {
                        let _ = app_buf.pop_front();
                    }
                }
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
                    if std::env::var("CRISPY_AUDIO_DEBUG").is_ok() && frames_encoded % 100 == 0 {
                        println!("Wrote {} frames", frames_encoded);
                    }
                } else {
                    break;
                }
            }
        }

        if std::env::var("CRISPY_AUDIO_DEBUG").is_ok() {
            println!(
                "Recording worker stopped. Total frames encoded: {}",
                frames_encoded
            );
        }
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
    {
        let mut command = std::process::Command::new("explorer");
        command.arg(&recordings_dir);
        command
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

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
    {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", &url]);
        command
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    Ok(())
}

/// Parse WAV file header to extract duration.
/// Returns None if parsing fails (not a valid WAV).
/// Handles WAV files with extra chunks (LIST, INFO, etc.) by searching for "data" chunk.
fn get_wav_duration(path: &Path) -> Option<f64> {
    use std::io::{Read, Seek, SeekFrom};
    
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 12];
    
    // Read RIFF header (12 bytes)
    file.read_exact(&mut header).ok()?;
    
    // Check for "RIFF" and "WAVE" signatures
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return None;
    }
    
    let mut sample_rate = 0u32;
    let mut num_channels = 0u16;
    let mut bits_per_sample = 0u16;
    let mut data_size = 0u32;
    
    // Search for "fmt " and "data" chunks
    let mut chunks_found = vec![];
    loop {
        let mut chunk_header = [0u8; 8];
        if file.read_exact(&mut chunk_header).is_err() {
            break;
        }
        
        let chunk_id = &chunk_header[0..4];
        let chunk_id_str = String::from_utf8_lossy(chunk_id);
        let chunk_size = u32::from_le_bytes([
            chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7]
        ]);
        
        chunks_found.push(format!("{} ({})", chunk_id_str, chunk_size));
        
        if chunk_id == b"fmt " {
            // Read fmt chunk (should be at least 16 bytes for PCM)
            let mut fmt_data = vec![0u8; chunk_size as usize];
            file.read_exact(&mut fmt_data).ok()?;
            
            if fmt_data.len() >= 16 {
                num_channels = u16::from_le_bytes([fmt_data[2], fmt_data[3]]);
                sample_rate = u32::from_le_bytes([fmt_data[4], fmt_data[5], fmt_data[6], fmt_data[7]]);
                bits_per_sample = u16::from_le_bytes([fmt_data[14], fmt_data[15]]);
            }
        } else if chunk_id == b"data" {
            data_size = chunk_size;
            // Found data chunk, we have everything we need
            break;
        } else {
            // Skip unknown chunk
            file.seek(SeekFrom::Current(chunk_size as i64)).ok()?;
        }
    }
    
    if sample_rate == 0 || bits_per_sample == 0 || num_channels == 0 || data_size == 0 {
        eprintln!(
            "[WAV] Failed to parse {}: sr={}, bits={}, ch={}, data_size={}, chunks={:?}",
            path.display(), sample_rate, bits_per_sample, num_channels, data_size, chunks_found
        );
        return None;
    }
    
    // Calculate duration
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let num_samples = data_size / (bytes_per_sample * num_channels as u32);
    let duration_seconds = num_samples as f64 / sample_rate as f64;
    
    eprintln!(
        "[WAV] Parsed {}: {:.1}s (sr={}, ch={}, bits={}, chunks={:?})",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        duration_seconds, sample_rate, num_channels, bits_per_sample, chunks_found
    );
    
    Some(duration_seconds)
}

#[derive(serde::Serialize)]
pub struct RecordingFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub created: u64,
    pub duration_seconds: Option<f64>,  // Duration from WAV header
}

#[tauri::command]
pub fn get_recordings(
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<Vec<RecordingFile>, String> {
    let recordings_dir = recordings_dir(&app)?;

    if !recordings_dir.exists() {
        return Ok(Vec::new());
    }

    // Hide currently active recording file from history until it's finalized.
    let active_recording_path: Option<String> = {
        let recording = state.recording.lock().unwrap();
        let writer_guard = recording.writer.lock().unwrap();
        writer_guard
            .as_ref()
            .map(|w| w.output_path().to_string_lossy().to_string())
    };

    let mut recordings = Vec::new();
    let entries = std::fs::read_dir(&recordings_dir)
        .map_err(|e| format!("Failed to read recordings directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("wav") {
            let path_str = path.to_string_lossy().to_string();
            if active_recording_path.as_ref().is_some_and(|active| active == &path_str) {
                continue;
            }

            let metadata = std::fs::metadata(&path)
                .map_err(|e| format!("Failed to get file metadata: {}", e))?;

            let created = metadata
                .created()
                .or_else(|_| metadata.modified())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);

            // Parse WAV header to get duration (fast, only reads 44 bytes)
            let duration_seconds = get_wav_duration(&path);
            
            recordings.push(RecordingFile {
                name: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                path: path.to_string_lossy().to_string(),
                size: metadata.len(),
                created,
                duration_seconds,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal valid WAV file in memory and write it to a temp file.
    fn write_test_wav(
        path: &Path,
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
        num_samples_per_channel: u32,
    ) {
        let bytes_per_sample = (bits_per_sample / 8) as u32;
        let data_size = num_samples_per_channel * channels as u32 * bytes_per_sample;
        let fmt_chunk_size: u32 = 16;
        // RIFF header (12) + fmt chunk (8 + 16) + data chunk header (8) + data
        let file_size = 4 + (8 + fmt_chunk_size) + 8 + data_size;

        let mut buf = Vec::new();
        // RIFF header
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        // fmt chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&fmt_chunk_size.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate * channels as u32 * bytes_per_sample;
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align = channels * (bits_per_sample / 8);
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        // data chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        // Write silence (zeros)
        buf.extend(vec![0u8; data_size as usize]);

        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(&buf).unwrap();
    }

    #[test]
    fn wav_duration_48khz_stereo_16bit() {
        let dir = std::env::temp_dir().join("crispy_test_wav_48k");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_48k.wav");

        // 48000 samples/channel = exactly 1 second at 48kHz
        write_test_wav(&path, 48000, 2, 16, 48000);
        let duration = get_wav_duration(&path).unwrap();
        assert!((duration - 1.0).abs() < 0.001, "Expected ~1.0s, got {}", duration);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_duration_44100hz_mono_16bit() {
        let dir = std::env::temp_dir().join("crispy_test_wav_44k");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_44k.wav");

        // 44100 samples = exactly 1 second at 44.1kHz
        write_test_wav(&path, 44100, 1, 16, 44100);
        let duration = get_wav_duration(&path).unwrap();
        assert!((duration - 1.0).abs() < 0.001, "Expected ~1.0s, got {}", duration);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_duration_5_seconds() {
        let dir = std::env::temp_dir().join("crispy_test_wav_5s");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_5s.wav");

        write_test_wav(&path, 48000, 2, 16, 48000 * 5);
        let duration = get_wav_duration(&path).unwrap();
        assert!((duration - 5.0).abs() < 0.001, "Expected ~5.0s, got {}", duration);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_duration_returns_none_for_non_wav_file() {
        let dir = std::env::temp_dir().join("crispy_test_wav_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not_a_wav.wav");
        std::fs::write(&path, b"this is not a wav file").unwrap();

        assert!(get_wav_duration(&path).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_duration_returns_none_for_missing_file() {
        let path = std::env::temp_dir().join("crispy_nonexistent.wav");
        assert!(get_wav_duration(&path).is_none());
    }

    #[test]
    fn wav_duration_with_extra_chunks() {
        let dir = std::env::temp_dir().join("crispy_test_wav_chunks");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_extra_chunks.wav");

        // Build WAV with a LIST chunk before the data chunk
        let sample_rate: u32 = 48000;
        let channels: u16 = 2;
        let bits_per_sample: u16 = 16;
        let num_samples: u32 = 48000; // 1 second
        let bytes_per_sample = (bits_per_sample / 8) as u32;
        let data_size = num_samples * channels as u32 * bytes_per_sample;

        let list_content = b"INFOIART\x05\x00\x00\x00Test\x00"; // fake LIST chunk
        let list_chunk_size = list_content.len() as u32;

        let fmt_chunk_size: u32 = 16;
        let file_size = 4 + (8 + fmt_chunk_size) + (8 + list_chunk_size) + 8 + data_size;

        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        // fmt chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&fmt_chunk_size.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate * channels as u32 * bytes_per_sample;
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align = channels * (bits_per_sample / 8);
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        // LIST chunk (extra chunk before data)
        buf.extend_from_slice(b"LIST");
        buf.extend_from_slice(&list_chunk_size.to_le_bytes());
        buf.extend_from_slice(list_content);
        // data chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend(vec![0u8; data_size as usize]);

        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&buf).unwrap();

        let duration = get_wav_duration(&path).unwrap();
        assert!((duration - 1.0).abs() < 0.001, "Expected ~1.0s, got {}", duration);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_duration_returns_none_for_truncated_header() {
        let dir = std::env::temp_dir().join("crispy_test_wav_trunc");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("truncated.wav");
        // Only write RIFF header, no fmt or data chunks
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        std::fs::write(&path, &buf).unwrap();

        assert!(get_wav_duration(&path).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }
}
