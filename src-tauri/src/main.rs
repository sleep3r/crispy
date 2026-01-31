#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

mod commands;
mod managers;
mod recording;
mod llm_settings;

#[cfg(target_os = "macos")]
mod system_input_volume;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use base64::Engine;
use nnnoiseless::{DenoiseState, FRAME_SIZE as RNNOISE_FRAME_SIZE};
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
    shared: Option<Arc<Mutex<NsState>>>,
    last_input_rate: Option<f32>,
    last_output_rate: Option<f32>,
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

    /// Pushes one input sample; returns processed sample(s) for recording when applicable.
    fn push_sample(&mut self, sample: f32) -> Option<Vec<f32>> {
        if self.buffer.len() >= self.max_len {
            self.buffer.pop_front();
        }
        self.buffer.push_back(sample);

        let mut processed = sample * self.volume;
        if let ModelKind::Noisy = self.model {
            self.rng_state = self
                .rng_state
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let noise = (self.rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            processed += noise * 0.05;
        }
        Some(vec![processed])
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

/// RNNoise-based processor: frame-based (480 samples at 48 kHz). Expects 48 kHz input.
struct RnnNoiseProcessor {
    denoise: Box<DenoiseState<'static>>,
    input_buf: VecDeque<f32>,
    output_buf: VecDeque<f32>,
    resample_pos: f64,
    input_rate: f32,
    output_rate: f32,
    volume: f32,
    first_frame: bool,
    max_output_len: usize,
}

impl RnnNoiseProcessor {
    fn new(input_rate: f32, output_rate: f32, volume: f32) -> Self {
        let max_output_len = input_rate as usize;
        Self {
            denoise: DenoiseState::new(),
            input_buf: VecDeque::with_capacity(RNNOISE_FRAME_SIZE * 2),
            output_buf: VecDeque::with_capacity(max_output_len),
            resample_pos: 0.0,
            input_rate,
            output_rate,
            volume: volume.clamp(0.0, 1.0),
            first_frame: true,
            max_output_len,
        }
    }

    /// Pushes one sample ([-1, 1]); when a full frame is ready, returns 480 processed samples for recording.
    fn push_sample(&mut self, sample: f32) -> Option<Vec<f32>> {
        if self.input_buf.len() >= self.max_output_len {
            self.input_buf.pop_front();
        }
        self.input_buf.push_back(sample);

        if self.input_buf.len() < RNNOISE_FRAME_SIZE {
            return None;
        }

        let mut input_frame = [0.0f32; 480];
        for (i, s) in self.input_buf.drain(..RNNOISE_FRAME_SIZE).enumerate() {
            if i < RNNOISE_FRAME_SIZE {
                input_frame[i] = s * 32768.0;
            }
        }
        let mut output_frame = [0.0f32; 480];
        self.denoise.process_frame(&mut output_frame[..], &input_frame[..]);

        let out_samples: Vec<f32> = output_frame
            .iter()
            .map(|&s| (s / 32768.0).clamp(-1.0, 1.0) * self.volume)
            .collect();

        if self.first_frame {
            self.first_frame = false;
            return None;
        }

        for &out in &out_samples {
            if self.output_buf.len() >= self.max_output_len {
                self.output_buf.pop_front();
            }
            self.output_buf.push_back(out);
        }
        Some(out_samples)
    }

    fn next_sample(&mut self) -> f32 {
        if self.output_buf.len() < 2 {
            return 0.0;
        }
        let step = self.input_rate as f64 / self.output_rate as f64;
        while self.resample_pos >= 1.0 {
            self.output_buf.pop_front();
            self.resample_pos -= 1.0;
            if self.output_buf.len() < 2 {
                return 0.0;
            }
        }
        let s0 = *self.output_buf.get(0).unwrap_or(&0.0);
        let s1 = *self.output_buf.get(1).unwrap_or(&0.0);
        let frac = self.resample_pos as f32;
        self.resample_pos += step;
        s0 + (s1 - s0) * frac
    }
}

enum NsState {
    Legacy(SharedAudio),
    RnnNoise(RnnNoiseProcessor),
}

impl NsState {
    fn push_sample(&mut self, sample: f32) -> Option<Vec<f32>> {
        match self {
            NsState::Legacy(s) => s.push_sample(sample),
            NsState::RnnNoise(s) => s.push_sample(sample),
        }
    }

    fn next_sample(&mut self) -> f32 {
        match self {
            NsState::Legacy(s) => s.next_sample(),
            NsState::RnnNoise(s) => s.next_sample(),
        }
    }

    fn set_volume(&mut self, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        match self {
            NsState::Legacy(s) => s.volume = v,
            NsState::RnnNoise(s) => s.volume = v,
        }
    }

    fn volume(&self) -> f32 {
        match self {
            NsState::Legacy(s) => s.volume,
            NsState::RnnNoise(s) => s.volume,
        }
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

    let shared: Option<Arc<Mutex<NsState>>> = if let Some(ref output_config) = output_config {
        let input_rate = config.sample_rate() as f32;
        let output_rate = output_config.sample_rate() as f32;
        let vol = volume.clamp(0.0, 1.0);
        let ns = if model_name == "rnnnoise" && (input_rate - 48000.0).abs() < 1.0 {
            NsState::RnnNoise(RnnNoiseProcessor::new(input_rate, output_rate, vol))
        } else {
            NsState::Legacy(SharedAudio::new(
                input_rate,
                output_rate,
                ModelKind::from_name(&model_name),
                vol,
            ))
        };
        Some(Arc::new(Mutex::new(ns)))
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
                        
                        // Apply model and tee to recording buffer
                        if let Some(shared) = shared.as_ref() {
                            let mut s = shared.lock().unwrap();
                            if let Some(samples) = s.push_sample(mono) {
                                let mut rec_buf = rec_buffer.lock().unwrap();
                                for sample in samples {
                                    if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                                        rec_buf.pop_front();
                                    }
                                    rec_buf.push_back(sample);
                                }
                            }
                        } else {
                            let mut rec_buf = rec_buffer.lock().unwrap();
                            if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                                rec_buf.pop_front();
                            }
                            rec_buf.push_back(mono);
                        }
                        
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
                        
                        if let Some(shared) = shared.as_ref() {
                            let mut s = shared.lock().unwrap();
                            if let Some(samples) = s.push_sample(mono) {
                                let mut rec_buf = rec_buffer.lock().unwrap();
                                for sample in samples {
                                    if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                                        rec_buf.pop_front();
                                    }
                                    rec_buf.push_back(sample);
                                }
                            }
                        } else {
                            let mut rec_buf = rec_buffer.lock().unwrap();
                            if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                                rec_buf.pop_front();
                            }
                            rec_buf.push_back(mono);
                        }
                        
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
                        
                        if let Some(shared) = shared.as_ref() {
                            let mut s = shared.lock().unwrap();
                            if let Some(samples) = s.push_sample(mono) {
                                let mut rec_buf = rec_buffer.lock().unwrap();
                                for sample in samples {
                                    if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                                        rec_buf.pop_front();
                                    }
                                    rec_buf.push_back(sample);
                                }
                            }
                        } else {
                            let mut rec_buf = rec_buffer.lock().unwrap();
                            if rec_buf.len() >= recording::SAMPLE_RATE * 10 {
                                rec_buf.pop_front();
                            }
                            rec_buf.push_back(mono);
                        }
                        
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
    audio.shared = shared.clone();
    audio.last_input_rate = Some(config.sample_rate() as f32);
    audio.last_output_rate = output_config
        .as_ref()
        .map(|c| c.sample_rate() as f32);

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
        shared.set_volume(volume);
    }
    Ok(())
}

#[tauri::command]
fn set_monitoring_model(
    state: tauri::State<AppState>,
    model_name: String,
) -> Result<(), String> {
    let audio = state.audio.lock().unwrap();
    let shared = audio.shared.as_ref().ok_or("Monitoring not started")?;
    let (vol, input_rate, output_rate) = {
        let guard = shared.lock().unwrap();
        let v = guard.volume();
        let ir = audio.last_input_rate.unwrap_or(48000.0);
        let or = audio.last_output_rate.unwrap_or(48000.0);
        (v, ir, or)
    };
    let mut guard = shared.lock().unwrap();
    *guard = if model_name == "rnnnoise" && (input_rate - 48000.0).abs() < 1.0 {
        NsState::RnnNoise(RnnNoiseProcessor::new(input_rate, output_rate, vol))
    } else {
        NsState::Legacy(SharedAudio::new(
            input_rate,
            output_rate,
            ModelKind::from_name(&model_name),
            vol,
        ))
    };
    Ok(())
}

/// Get system default input device volume (0..100). macOS only; same as System Settings → Sound → Input.
#[tauri::command]
fn get_system_input_volume() -> Result<u8, String> {
    #[cfg(target_os = "macos")]
    {
        let v = system_input_volume::get_system_input_volume()?;
        Ok((v * 100.0).round() as u8)
    }
    #[cfg(not(target_os = "macos"))]
    Err("System input volume is only supported on macOS.".to_string())
}

/// Set system default input device volume (0..100). macOS only.
#[tauri::command]
fn set_system_input_volume(volume: u8) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let v = (volume.min(100) as f32) / 100.0;
        system_input_volume::set_system_input_volume(v)
    }
    #[cfg(not(target_os = "macos"))]
    let _ = volume;
    #[cfg(not(target_os = "macos"))]
    Err("System input volume is only supported on macOS.".to_string())
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

fn do_start_recording(state: &AppState, app_id: &str) -> Result<(), String> {
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

    // Start app audio capture if app is selected (not "none")
    #[cfg(target_os = "macos")]
    if !app_id.is_empty() && app_id != "none" {
        match recording::start_app_audio_capture(app_id, recording.app_buffer.clone()) {
            Ok(stream) => {
                *recording.app_audio_stream.lock().unwrap() = Some(stream);
            }
            Err(e) => {
                eprintln!("Warning: Failed to start app audio capture: {}", e);
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

fn do_stop_recording(state: &AppState) -> Result<String, String> {
    RECORDING_ACTIVE.store(false, Ordering::SeqCst);

    // Stop app audio capture if running
    #[cfg(target_os = "macos")]
    {
        let recording = state.recording.lock().unwrap();
        let stream_opt = recording.app_audio_stream.lock().unwrap().take();
        // Clear app buffer to avoid trailing audio after stop
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

#[tauri::command]
fn start_recording(
    state: tauri::State<AppState>,
    app_id: String,
) -> Result<(), String> {
    do_start_recording(state.inner(), &app_id)
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
            let app_available = {
                let app_buf = app_buffer.lock().unwrap();
                app_buf.len()
            };
            if app_available >= frame_size {
                let mut app_buf = app_buffer.lock().unwrap();
                for i in 0..frame_size {
                    right_frame[i] = app_buf.pop_front().unwrap_or(0.0);
                }
            } else {
                // No app audio; use silence
                for i in 0..frame_size {
                    right_frame[i] = 0.0;
                }
            } // app_buf lock dropped here

            // --- Mix into dual-mono (L/R = mic + app) ---
            for i in 0..frame_size {
                let mixed = left_frame[i] + right_frame[i];
                left_frame[i] = mixed;
                right_frame[i] = mixed;
            }

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
    #[cfg(target_os = "macos")]
    fn set_tray_window_level(window: &tauri::WebviewWindow) {
        if let Ok(raw_ptr) = window.ns_window() {
            if raw_ptr.is_null() {
                return;
            }
            let ns_window: *mut objc2_app_kit::NSWindow = raw_ptr.cast();
            unsafe {
                // Use CGShieldingWindowLevel (2147483630) - highest level that works with fullscreen
                // This is higher than screensaver (1000) and should appear over fullscreen apps
                const CG_SHIELDING_WINDOW_LEVEL: isize = 2147483630;
                (*ns_window).setLevel(CG_SHIELDING_WINDOW_LEVEL);
                (*ns_window).makeKeyAndOrderFront(None);
            }
        }
    }

    if let Some(window) = app.get_webview_window("tray-popup") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.set_always_on_top(true);
            #[cfg(target_os = "macos")]
            {
                set_tray_window_level(&window);
                let _ = window.set_visible_on_all_workspaces(true);
            }
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.move_window(Position::TrayBottomCenter);
        }
        return;
    }
    let url = WebviewUrl::App("index.html".into());
    let _ = WebviewWindowBuilder::new(app, "tray-popup", url)
        .title("Crispy")
        .inner_size(260.0, 280.0)
        .decorations(false)
        .resizable(false)
        .build();
    if let Some(window) = app.get_webview_window("tray-popup") {
        let _ = window.set_always_on_top(true);
        #[cfg(target_os = "macos")]
        {
            set_tray_window_level(&window);
            let _ = window.set_visible_on_all_workspaces(true);
        }
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
                last_input_rate: None,
                last_output_rate: None,
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
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Main window: hide to tray instead of closing (app keeps running)
                    if window.label() == "main" {
                        api.prevent_close();
                        let _ = window.hide();
                        #[cfg(target_os = "macos")]
                        {
                            let _ = window.app_handle().set_activation_policy(
                                tauri::ActivationPolicy::Accessory,
                            );
                        }
                    }
                }
                tauri::WindowEvent::Focused(false) => {
                    // Tray popup: close on outside click / focus loss
                    if window.label() == "tray-popup" {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_input_devices,
            get_output_devices,
            get_default_devices,
            start_monitoring,
            stop_monitoring,
            set_monitoring_volume,
            set_monitoring_model,
            get_system_input_volume,
            set_system_input_volume,
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
            commands::ns_models::get_available_ns_models,
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
