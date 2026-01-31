#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

mod commands;
mod managers;
mod recording;
mod llm_settings;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use base64::Engine;
use tauri::image::Image;
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_positioner::{Position, WindowExt};
use recording::{RecordingState, RecordableApp};

#[derive(serde::Serialize)]
struct AudioDevice {
    id: String,
    name: String,
}

struct AppState {
    audio: Arc<Mutex<AudioMonitorState>>,
    recording: Arc<Mutex<RecordingState>>,
}

struct AudioMonitorState {
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
    shared: Option<Arc<Mutex<SharedAudio>>>,
}

#[derive(Clone, Copy)]
enum ModelKind {
    Dummy,
    Noisy,
}

impl ModelKind {
    fn from_name(name: &str) -> Self {
        match name {
            "noisy" => ModelKind::Noisy,
            _ => ModelKind::Dummy,
        }
    }
}

struct SharedAudio {
    buffer: VecDeque<f32>,
    max_len: usize,
    resample_pos: f64,
    input_rate: f32,
    output_rate: f32,
    model: ModelKind,
    volume: f32,
    rng_state: u32,
}

impl SharedAudio {
    fn new(input_rate: f32, output_rate: f32, model: ModelKind, volume: f32) -> Self {
        let max_len = input_rate as usize; // ~1s of audio
        Self {
            buffer: VecDeque::with_capacity(max_len),
            max_len,
            resample_pos: 0.0,
            input_rate,
            output_rate,
            model,
            volume,
            rng_state: 0x1234_abcd,
        }
    }

    fn push_sample(&mut self, sample: f32) {
        if self.buffer.len() >= self.max_len {
            self.buffer.pop_front();
        }
        self.buffer.push_back(sample);
    }

    fn next_sample(&mut self) -> f32 {
        if self.buffer.len() < 2 {
            return 0.0;
        }

        let step = self.input_rate as f64 / self.output_rate as f64;
        while self.resample_pos >= 1.0 {
            self.buffer.pop_front();
            self.resample_pos -= 1.0;
            if self.buffer.len() < 2 {
                return 0.0;
            }
        }

        let s0 = *self.buffer.get(0).unwrap_or(&0.0);
        let s1 = *self.buffer.get(1).unwrap_or(&0.0);
        let frac = self.resample_pos as f32;
        let mut sample = s0 + (s1 - s0) * frac;

        if let ModelKind::Noisy = self.model {
            // Simple deterministic noise (LCG)
            self.rng_state = self
                .rng_state
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let noise = (self.rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            sample += noise * 0.05;
        }

        self.resample_pos += step;
        sample * self.volume
    }
}

#[tauri::command]
#[allow(deprecated)]
fn get_input_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => {
            let mut result = Vec::new();
            for device in devices {
                if let Ok(name) = device.name() {
                    result.push(AudioDevice {
                        id: name.clone(),
                        name,
                    });
                }
            }
            result.sort_by(|a, b| a.name.cmp(&b.name));
            result.dedup_by(|a, b| a.name == b.name);
            Ok(result)
        }
        Err(e) => Err(format!("Failed to get input devices: {}", e)),
    }
}

#[tauri::command]
#[allow(deprecated)]
fn get_output_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();
    match host.output_devices() {
        Ok(devices) => {
            let mut result = Vec::new();
            for device in devices {
                if let Ok(name) = device.name() {
                    result.push(AudioDevice {
                        id: name.clone(),
                        name,
                    });
                }
            }
            result.sort_by(|a, b| a.name.cmp(&b.name));
            result.dedup_by(|a, b| a.name == b.name);
            Ok(result)
        }
        Err(e) => Err(format!("Failed to get output devices: {}", e)),
    }
}

#[derive(serde::Serialize)]
struct DefaultDevices {
    default_input: Option<String>,
    blackhole_output: Option<String>,
}

#[tauri::command]
#[allow(deprecated)]
fn get_default_devices() -> Result<DefaultDevices, String> {
    let host = cpal::default_host();
    
    // Get default input device
    let default_input = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    
    // Find BlackHole in output devices
    let mut blackhole_output: Option<String> = None;
    if let Ok(devices) = host.output_devices() {
        for device in devices {
            if let Ok(name) = device.name() {
                let name_lower = name.to_lowercase();
                if name_lower.contains("blackhole") || name_lower.contains("black hole") {
                    blackhole_output = Some(name);
                    break;
                }
            }
        }
    }
    
    Ok(DefaultDevices {
        default_input,
        blackhole_output,
    })
}

#[tauri::command]
#[allow(deprecated)]
fn start_monitoring(
    state: tauri::State<AppState>,
    app_handle: tauri::AppHandle,
    device_name: String,
    output_device_name: String,
    model_name: String,
    volume: f32,
) -> Result<(), String> {
    if device_name.trim().is_empty() {
        return Err("No input device selected".to_string());
    }

    let recording_mic_buffer = state.recording.lock().unwrap().mic_buffer.clone();

    // Stop any existing stream first
    {
        let mut audio = state.audio.lock().unwrap();
        audio.input_stream = None;
        audio.output_stream = None;
        audio.shared = None;
    }

    let host = cpal::default_host();

    // Find the device
    let device = if device_name == "Default" {
        host.default_input_device()
    } else {
        host.input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
    }
    .ok_or("Failed to find input device")?;

    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let input_channels = config.channels() as usize;
    let input_sample_format = config.sample_format();
    let input_config: cpal::StreamConfig = config.clone().into();
    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let output_device = if output_device_name.trim().is_empty() {
        None
    } else if output_device_name == "Default" {
        host.default_output_device()
    } else {
        host.output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|n| n == output_device_name).unwrap_or(false))
    };

    let (output_config, output_channels, output_sample_format, output_stream_config) =
        if let Some(ref output_device) = output_device {
            let output_config = output_device
                .default_output_config()
                .map_err(|e| e.to_string())?;
            let output_channels = output_config.channels() as usize;
            let output_sample_format = output_config.sample_format();
            let output_stream_config: cpal::StreamConfig = output_config.clone().into();
            (Some(output_config), Some(output_channels), Some(output_sample_format), Some(output_stream_config))
        } else {
            (None, None, None, None)
        };

    let shared: Option<Arc<Mutex<SharedAudio>>> = if let Some(ref output_config) = output_config {
        Some(Arc::new(Mutex::new(SharedAudio::new(
            config.sample_rate() as f32,
            output_config.sample_rate() as f32,
            ModelKind::from_name(&model_name),
            volume.clamp(0.0, 1.0),
        ))))
    } else {
        None
    };

    let last_emit = Arc::new(Mutex::new(Instant::now()));

    let input_stream = match input_sample_format {
        cpal::SampleFormat::F32 => {
            let last_emit = last_emit.clone();
            let app_handle = app_handle.clone();
            let shared = shared.clone();
            let rec_buffer = recording_mic_buffer.clone();
            device.build_input_stream(
                &input_config,
                move |data: &[f32], _: &_| {
                    let mut sum = 0.0;
                    let mut frames = 0.0;

                    for frame in data.chunks(input_channels) {
                        let mut acc = 0.0;
                        for &sample in frame {
                            acc += sample;
                        }
                        let mono = acc / input_channels as f32;
                        
                        // Apply model processing for recording
                        let processed = if let Some(shared) = shared.as_ref() {
                            let mut s = shared.lock().unwrap();
                            s.push_sample(mono);
                            // Get processed version for recording
                            let mut temp_sample = mono * s.volume;
                            if let ModelKind::Noisy = s.model {
                                s.rng_state = s.rng_state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                                let noise = (s.rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                                temp_sample += noise * 0.05;
                            }
                            temp_sample
                        } else {
                            mono
                        };
                        
                        // Tee to recording buffer
                        let mut rec_buf = rec_buffer.lock().unwrap();
                        if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                            rec_buf.pop_front();
                        }
                        rec_buf.push_back(processed);
                        
                        sum += mono * mono;
                        frames += 1.0;
                    }

                    if frames > 0.0 {
                        let rms = (sum / frames).sqrt();
                        let mut last = last_emit.lock().unwrap();
                        if last.elapsed() >= Duration::from_millis(16) {
                            *last = Instant::now();
                            let _ = app_handle.emit("microphone-level", rms);
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let last_emit = last_emit.clone();
            let app_handle = app_handle.clone();
            let shared = shared.clone();
            let rec_buffer = recording_mic_buffer.clone();
            device.build_input_stream(
                &input_config,
                move |data: &[i16], _: &_| {
                    let mut sum = 0.0;
                    let mut frames = 0.0;

                    for frame in data.chunks(input_channels) {
                        let mut acc = 0.0;
                        for &sample in frame {
                            acc += sample as f32 / 32768.0;
                        }
                        let mono = acc / input_channels as f32;
                        
                        let processed = if let Some(shared) = shared.as_ref() {
                            let mut s = shared.lock().unwrap();
                            s.push_sample(mono);
                            let mut temp_sample = mono * s.volume;
                            if let ModelKind::Noisy = s.model {
                                s.rng_state = s.rng_state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                                let noise = (s.rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                                temp_sample += noise * 0.05;
                            }
                            temp_sample
                        } else {
                            mono
                        };
                        
                        let mut rec_buf = rec_buffer.lock().unwrap();
                        if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                            rec_buf.pop_front();
                        }
                        rec_buf.push_back(processed);
                        
                        sum += mono * mono;
                        frames += 1.0;
                    }

                    if frames > 0.0 {
                        let rms = (sum / frames).sqrt();
                        let mut last = last_emit.lock().unwrap();
                        if last.elapsed() >= Duration::from_millis(16) {
                            *last = Instant::now();
                            let _ = app_handle.emit("microphone-level", rms);
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let last_emit = last_emit.clone();
            let app_handle = app_handle.clone();
            let shared = shared.clone();
            let rec_buffer = recording_mic_buffer.clone();
            device.build_input_stream(
                &input_config,
                move |data: &[u16], _: &_| {
                    let mut sum = 0.0;
                    let mut frames = 0.0;

                    for frame in data.chunks(input_channels) {
                        let mut acc = 0.0;
                        for &sample in frame {
                            acc += (sample as f32 - 32768.0) / 32768.0;
                        }
                        let mono = acc / input_channels as f32;
                        
                        let processed = if let Some(shared) = shared.as_ref() {
                            let mut s = shared.lock().unwrap();
                            s.push_sample(mono);
                            let mut temp_sample = mono * s.volume;
                            if let ModelKind::Noisy = s.model {
                                s.rng_state = s.rng_state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                                let noise = (s.rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                                temp_sample += noise * 0.05;
                            }
                            temp_sample
                        } else {
                            mono
                        };
                        
                        let mut rec_buf = rec_buffer.lock().unwrap();
                        if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                            rec_buf.pop_front();
                        }
                        rec_buf.push_back(processed);
                        
                        sum += mono * mono;
                        frames += 1.0;
                    }

                    if frames > 0.0 {
                        let rms = (sum / frames).sqrt();
                        let mut last = last_emit.lock().unwrap();
                        if last.elapsed() >= Duration::from_millis(16) {
                            *last = Instant::now();
                            let _ = app_handle.emit("microphone-level", rms);
                        }
                    }
                },
                err_fn,
                None,
            )
        }
        _ => return Err(format!("Unsupported sample format: {}", input_sample_format)),
    }
    .map_err(|e| e.to_string())?;

    let output_stream = if let (Some(output_device), Some(output_stream_config), Some(output_channels), Some(output_sample_format), Some(shared)) =
        (output_device, output_stream_config, output_channels, output_sample_format, shared.clone())
    {
        let output_stream = match output_sample_format {
            cpal::SampleFormat::F32 => {
                let shared = shared.clone();
                output_device.build_output_stream(
                    &output_stream_config,
                    move |data: &mut [f32], _: &_| {
                        let mut shared = shared.lock().unwrap();
                        for frame in data.chunks_mut(output_channels) {
                            let sample = shared.next_sample();
                            for out in frame.iter_mut() {
                                *out = sample;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let shared = shared.clone();
                output_device.build_output_stream(
                    &output_stream_config,
                    move |data: &mut [i16], _: &_| {
                        let mut shared = shared.lock().unwrap();
                        for frame in data.chunks_mut(output_channels) {
                            let sample = shared.next_sample();
                            let clamped = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                            for out in frame.iter_mut() {
                                *out = clamped;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let shared = shared.clone();
                output_device.build_output_stream(
                    &output_stream_config,
                    move |data: &mut [u16], _: &_| {
                        let mut shared = shared.lock().unwrap();
                        for frame in data.chunks_mut(output_channels) {
                            let sample = shared.next_sample();
                            let clamped = (sample.clamp(-1.0, 1.0) * 0.5 + 0.5) * 65535.0;
                            let out_sample = clamped as u16;
                            for out in frame.iter_mut() {
                                *out = out_sample;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            _ => return Err(format!("Unsupported sample format: {}", output_sample_format)),
        }
        .map_err(|e| e.to_string())?;
        Some(output_stream)
    } else {
        None
    };

    input_stream.play().map_err(|e| e.to_string())?;
    if let Some(ref output_stream) = output_stream {
        output_stream.play().map_err(|e| e.to_string())?;
    }

    let mut audio = state.audio.lock().unwrap();
    audio.input_stream = Some(input_stream);
    audio.output_stream = output_stream;
    audio.shared = shared;

    Ok(())
}

#[tauri::command]
fn stop_monitoring(state: tauri::State<AppState>) -> Result<(), String> {
    let mut audio = state.audio.lock().unwrap();
    audio.input_stream = None;
    audio.output_stream = None;
    audio.shared = None;
    Ok(())
}

#[tauri::command]
fn set_monitoring_volume(
    state: tauri::State<AppState>,
    volume: f32,
) -> Result<(), String> {
    let audio = state.audio.lock().unwrap();
    if let Some(shared) = audio.shared.as_ref() {
        let mut shared = shared.lock().unwrap();
        shared.volume = volume.clamp(0.0, 1.0);
    }
    Ok(())
}

#[tauri::command]
fn set_monitoring_model(
    state: tauri::State<AppState>,
    model_name: String,
) -> Result<(), String> {
    let audio = state.audio.lock().unwrap();
    if let Some(shared) = audio.shared.as_ref() {
        let mut shared = shared.lock().unwrap();
        shared.model = ModelKind::from_name(&model_name);
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct BlackHoleStatus {
    installed: bool,
    paths: Vec<String>,
}

#[tauri::command]
fn get_blackhole_status() -> Result<BlackHoleStatus, String> {
    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver",
            "/Library/Audio/Plug-Ins/HAL/BlackHole16ch.driver",
            "/Library/Audio/Plug-Ins/HAL/BlackHole64ch.driver",
            "/Library/Audio/Plug-Ins/HAL/BlackHole 2ch.driver",
            "/Library/Audio/Plug-Ins/HAL/BlackHole 16ch.driver",
            "/Library/Audio/Plug-Ins/HAL/BlackHole 64ch.driver",
        ];

        let mut found = Vec::new();
        for path in candidates {
            if std::path::Path::new(path).exists() {
                found.push(path.to_string());
            }
        }

        return Ok(BlackHoleStatus {
            installed: !found.is_empty(),
            paths: found,
        });
    }

    #[cfg(not(target_os = "macos"))]
    Ok(BlackHoleStatus {
        installed: true,
        paths: Vec::new(),
    })
}

// Recording commands
#[tauri::command]
fn get_recordable_apps() -> Result<Vec<RecordableApp>, String> {
    recording::get_recordable_apps()
}

fn do_start_recording(state: &AppState) -> Result<(), String> {
    let mut recording = state.recording.lock().unwrap();

    if recording.writer.lock().unwrap().is_some() {
        return Err("Recording already in progress".to_string());
    }

    let home = std::env::var("HOME").map_err(|_| "Cannot find home directory".to_string())?;
    let output_dir = std::path::PathBuf::from(home)
        .join("Documents")
        .join("Crispy")
        .join("Recordings");

    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    let now = chrono::Local::now();
    let filename = format!("recording_{}.wav", now.format("%Y%m%d_%H%M%S"));
    let output_path = output_dir.join(filename);

    let writer = recording::WavWriter::new(output_path)
        .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

    *recording.writer.lock().unwrap() = Some(writer);
    recording.mic_buffer.lock().unwrap().clear();
    recording.app_buffer.lock().unwrap().clear();

    let handle = start_recording_worker(
        recording.mic_buffer.clone(),
        recording.app_buffer.clone(),
        recording.writer.clone(),
    );
    recording.worker = Some(handle);
    Ok(())
}

fn do_stop_recording(state: &AppState) -> Result<String, String> {
    RECORDING_ACTIVE.store(false, Ordering::SeqCst);

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

#[tauri::command]
fn start_recording(
    state: tauri::State<AppState>,
    _app_id: String,
) -> Result<(), String> {
    do_start_recording(state.inner())
}

#[tauri::command]
fn stop_recording(state: tauri::State<AppState>) -> Result<String, String> {
    do_stop_recording(state.inner())
}

#[tauri::command]
fn is_recording(state: tauri::State<AppState>) -> Result<bool, String> {
    let recording = state.recording.lock().unwrap();
    let is_active = recording.writer.lock().unwrap().is_some();
    Ok(is_active)
}

#[tauri::command]
fn get_recordings_dir_path() -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|_| "Cannot find home directory".to_string())?;
    let recordings_dir = std::path::PathBuf::from(home)
        .join("Documents")
        .join("Crispy")
        .join("Recordings");
    
    Ok(recordings_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn open_recordings_dir() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "Cannot find home directory".to_string())?;
    let recordings_dir = std::path::PathBuf::from(home)
        .join("Documents")
        .join("Crispy")
        .join("Recordings");
    
    // Create directory if it doesn't exist
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;
    
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }
    
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", &url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    
    Ok(())
}

#[derive(serde::Serialize)]
struct RecordingFile {
    name: String,
    path: String,
    size: u64,
    created: u64, // Unix timestamp in seconds
}

#[tauri::command]
fn get_recordings() -> Result<Vec<RecordingFile>, String> {
    let home = std::env::var("HOME").map_err(|_| "Cannot find home directory".to_string())?;
    let recordings_dir = std::path::PathBuf::from(home)
        .join("Documents")
        .join("Crispy")
        .join("Recordings");
    
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
            
            let created = metadata.created()
                .or_else(|_| metadata.modified())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);
            
            recordings.push(RecordingFile {
                name: path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                path: path.to_string_lossy().to_string(),
                size: metadata.len(),
                created,
            });
        }
    }
    
    // Sort by creation time, newest first
    recordings.sort_by(|a, b| b.created.cmp(&a.created));
    
    Ok(recordings)
}

#[tauri::command]
fn rename_recording(app: tauri::AppHandle, path: String, new_name: String) -> Result<(), String> {
    let old_path_str = path.clone();
    let path = std::path::Path::new(&path);
    if !path.exists() {
        return Err("Recording not found".to_string());
    }
    let parent = path
        .parent()
        .ok_or("Invalid path")?;
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if new_name.contains(std::path::MAIN_SEPARATOR) || new_name.contains('/') || new_name.contains('\\') {
        return Err("Name cannot contain path separators".to_string());
    }
    let base = std::path::Path::new(new_name).file_stem().and_then(|s| s.to_str()).unwrap_or(new_name);
    let new_path = parent.join(format!("{}.wav", base));
    if new_path == path {
        return Ok(());
    }
    if new_path.exists() {
        return Err("A file with this name already exists".to_string());
    }
    std::fs::rename(&path, &new_path).map_err(|e| format!("Failed to rename: {}", e))?;

    // Move transcription result and metadata to the new path so they stay associated with the recording
    let new_path_str = new_path.to_string_lossy();
    if let (Ok(old_txt), Ok(new_txt)) = (
        managers::transcription::transcription_result_path(&app, &old_path_str),
        managers::transcription::transcription_result_path(&app, &new_path_str),
    ) {
        if old_txt.exists() && old_txt != new_txt {
            let _ = std::fs::rename(&old_txt, &new_txt);
        }
    }
    if let (Ok(old_meta), Ok(new_meta)) = (
        managers::transcription::transcription_metadata_path(&app, &old_path_str),
        managers::transcription::transcription_metadata_path(&app, &new_path_str),
    ) {
        if old_meta.exists() && old_meta != new_meta {
            let _ = std::fs::rename(&old_meta, &new_meta);
        }
    }
    if let (Ok(old_chat), Ok(new_chat)) = (
        managers::transcription::transcription_chat_history_path(&app, &old_path_str),
        managers::transcription::transcription_chat_history_path(&app, &new_path_str),
    ) {
        if old_chat.exists() && old_chat != new_chat {
            let _ = std::fs::rename(&old_chat, &new_chat);
        }
    }

    Ok(())
}

#[tauri::command]
fn delete_recording(path: String) -> Result<(), String> {
    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete recording: {}", e))?;
    Ok(())
}

#[tauri::command]
fn read_recording_file(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read recording: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};

static RECORDING_ACTIVE: AtomicBool = AtomicBool::new(false);

fn start_recording_worker(
    mic_buffer: Arc<Mutex<VecDeque<f32>>>,
    app_buffer: Arc<Mutex<VecDeque<f32>>>,
    writer: Arc<Mutex<Option<recording::WavWriter>>>,
) -> std::thread::JoinHandle<()> {
    RECORDING_ACTIVE.store(true, Ordering::SeqCst);
    
    thread::spawn(move || {
        let frame_size = 1152; // MP3 frame size
        let mut left_frame = vec![0.0f32; frame_size];
        let mut right_frame = vec![0.0f32; frame_size];
        let mut frames_encoded = 0;

        println!("Recording worker started");

        while RECORDING_ACTIVE.load(Ordering::SeqCst) {
            // Check if writer still exists
            {
                let has_writer = writer.lock().unwrap().is_none();
                if has_writer {
                    println!("Writer is None, stopping worker");
                    break;
                }
            }

            // --- Pull mic frame ---
            let mic_available = {
                let mic_buf = mic_buffer.lock().unwrap();
                mic_buf.len()
            };

            if mic_available < frame_size {
                thread::sleep(Duration::from_millis(10));
                continue;
            }

            // Lock and pull mic samples
            {
                let mut mic_buf = mic_buffer.lock().unwrap();
                for i in 0..frame_size {
                    left_frame[i] = mic_buf.pop_front().unwrap_or(0.0);
                }
            } // mic_buf lock dropped here

            // --- Pull app frame (or silence) ---
            {
                let mut app_buf = app_buffer.lock().unwrap();
                for i in 0..frame_size {
                    right_frame[i] = app_buf.pop_front().unwrap_or(0.0);
                }
            } // app_buf lock dropped here

            // --- Write to WAV ---
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
            } // writer lock dropped here
        }

        println!("Recording worker stopped. Total frames encoded: {}", frames_encoded);
        RECORDING_ACTIVE.store(false, Ordering::SeqCst);
    })
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
}

#[tauri::command]
fn show_main_window_cmd(app: tauri::AppHandle) {
    show_main_window(&app);
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

fn show_or_toggle_tray_popup(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("tray-popup") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.move_window(Position::TrayBottomCenter);
        }
        return;
    }
    let url = WebviewUrl::App("index.html".into());
    let _ = WebviewWindowBuilder::new(app, "tray-popup", url)
        .title("Crispy")
        .inner_size(260.0, 220.0)
        .decorations(false)
        .resizable(false)
        .build();
    if let Some(window) = app.get_webview_window("tray-popup") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.move_window(Position::TrayBottomCenter);
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .manage(AppState {
            audio: Arc::new(Mutex::new(AudioMonitorState {
                input_stream: None,
                output_stream: None,
                shared: None,
            })),
            recording: Arc::new(Mutex::new(RecordingState::new())),
        })
        .manage(commands::models::SelectedModelState(Arc::new(Mutex::new(
            String::new(),
        ))))
        .setup(|app| {
            let model_manager = Arc::new(
                managers::model::ModelManager::new(app.handle())
                    .map_err(|e| e.to_string())?,
            );
            app.manage(model_manager.clone());
            let transcription_manager = Arc::new(managers::transcription::TranscriptionManager::new(
                model_manager,
            ));
            app.manage(transcription_manager);
            if let Ok(app_settings) = llm_settings::load_app_settings(app.handle()) {
                if !app_settings.selected_transcription_model.is_empty()
                    && app_settings.selected_transcription_model != "none"
                {
                    if let Some(selected) = app
                        .try_state::<commands::models::SelectedModelState>()
                        .map(|s| s.0.clone())
                    {
                        if let Ok(mut guard) = selected.lock() {
                            *guard = app_settings.selected_transcription_model;
                        }
                    }
                }
            }
            let icon = app
                .path()
                .resolve("resources/tray.png", tauri::path::BaseDirectory::Resource)
                .ok()
                .and_then(|p| Image::from_path(p).ok())
                .or_else(|| app.default_window_icon().cloned());
            let icon = icon.expect("tray icon: run scripts/tray_icon.py or provide default icon");
            let tray = TrayIconBuilder::new()
                .icon(icon)
                .menu_on_left_click(false)
                .icon_as_template(true)
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click {
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_or_toggle_tray_popup(tray.app_handle());
                    }
                })
                .build(app)
                .map_err(|e| e.to_string())?;

            app.manage(tray);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_input_devices,
            get_output_devices,
            get_default_devices,
            start_monitoring,
            stop_monitoring,
            set_monitoring_volume,
            set_monitoring_model,
            get_blackhole_status,
            get_recordable_apps,
            start_recording,
            stop_recording,
            is_recording,
            get_recordings_dir_path,
            open_recordings_dir,
            open_url,
            show_main_window_cmd,
            quit_app,
            get_recordings,
            rename_recording,
            delete_recording,
            read_recording_file,
            commands::models::get_available_models,
            commands::models::get_model_info,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::set_active_model,
            commands::models::get_current_model,
            commands::models::cancel_download,
            commands::models::get_recommended_first_model,
            commands::transcription::start_transcription,
            commands::transcription::get_transcription_result,
            commands::transcription::get_transcription_model,
            commands::transcription::open_transcription_window,
            commands::transcription::has_transcription_result,
            commands::transcription::get_llm_settings,
            commands::transcription::set_llm_settings,
            commands::transcription::stream_transcription_chat,
            commands::transcription::get_transcription_chat_history,
            commands::transcription::set_transcription_chat_history,
            commands::settings::get_app_settings,
            commands::settings::set_app_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
