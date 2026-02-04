#![allow(deprecated)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use nnnoiseless::{DenoiseState, FRAME_SIZE as RNNOISE_FRAME_SIZE};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

use crate::recording;

#[derive(serde::Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

pub struct AudioMonitorState {
    pub input_stream: Option<cpal::Stream>,
    pub output_stream: Option<cpal::Stream>,
    shared: Option<Arc<Mutex<NsState>>>,
    pub last_input_rate: Option<f32>,
    pub last_output_rate: Option<f32>,
}

impl AudioMonitorState {
    pub fn new() -> Self {
        Self {
            input_stream: None,
            output_stream: None,
            shared: None,
            last_input_rate: None,
            last_output_rate: None,
        }
    }
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
        let max_len = input_rate as usize;
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

// --- Device list commands (no state) ---

#[tauri::command]
pub fn get_input_devices() -> Result<Vec<AudioDevice>, String> {
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
pub fn get_output_devices() -> Result<Vec<AudioDevice>, String> {
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
pub struct DefaultDevices {
    pub default_input: Option<String>,
    pub blackhole_output: Option<String>,
}

#[tauri::command]
pub fn get_default_devices() -> Result<DefaultDevices, String> {
    let host = cpal::default_host();

    let default_input = host
        .default_input_device()
        .and_then(|device| device.name().ok());

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

// --- Monitoring: pub fns called from main with state ---

pub fn start_monitoring(
    audio: Arc<Mutex<AudioMonitorState>>,
    recording_mic_buffer: Arc<Mutex<VecDeque<f32>>>,
    app_handle: tauri::AppHandle,
    device_name: String,
    output_device_name: String,
    model_name: String,
    volume: f32,
) -> Result<(), String> {
    if device_name.trim().is_empty() {
        return Err("No input device selected".to_string());
    }

    {
        let mut mon = audio.lock().unwrap();
        mon.input_stream = None;
        mon.output_stream = None;
        mon.shared = None;
    }

    let host = cpal::default_host();

    let device = if device_name == "Default" {
        host.default_input_device()
    } else {
        host.input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
    }
    .ok_or("Failed to find input device")?;

    // Try to force 48kHz to avoid pitch issues
    let default_config = device.default_input_config().map_err(|e| e.to_string())?;
    
    // Check if we can use 48kHz
    let config = if let Ok(mut configs) = device.supported_input_configs() {
        if let Some(range) = configs.find(|c| c.min_sample_rate() <= 48000 && c.max_sample_rate() >= 48000) {
            // Device supports 48kHz - use it
            range.with_sample_rate(48000)
        } else {
            eprintln!("Warning: Device doesn't support 48kHz, using default ({}Hz)", default_config.sample_rate());
            default_config
        }
    } else {
        eprintln!("Warning: Could not query supported configs, using default");
        default_config
    };

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
            (
                Some(output_config),
                Some(output_channels),
                Some(output_sample_format),
                Some(output_stream_config),
            )
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
        cpal::SampleFormat::F32 => build_input_stream_f32(
            &device,
            &input_config,
            input_channels,
            shared.clone(),
            recording_mic_buffer.clone(),
            last_emit.clone(),
            app_handle.clone(),
            err_fn,
        )?,
        cpal::SampleFormat::I16 => build_input_stream_i16(
            &device,
            &input_config,
            input_channels,
            shared.clone(),
            recording_mic_buffer.clone(),
            last_emit.clone(),
            app_handle.clone(),
            err_fn,
        )?,
        cpal::SampleFormat::U16 => build_input_stream_u16(
            &device,
            &input_config,
            input_channels,
            shared.clone(),
            recording_mic_buffer.clone(),
            last_emit.clone(),
            app_handle.clone(),
            err_fn,
        )?,
        _ => return Err(format!("Unsupported sample format: {}", input_sample_format)),
    };

    let output_stream = if let (Some(output_device), Some(output_stream_config), Some(output_channels), Some(output_sample_format), Some(shared_out)) =
        (
            output_device,
            output_stream_config,
            output_channels,
            output_sample_format,
            shared.clone(),
        )
    {
        let s = match output_sample_format {
            cpal::SampleFormat::F32 => output_device
                .build_output_stream(
                    &output_stream_config,
                    move |data: &mut [f32], _: &_| {
                        let mut shared = shared_out.lock().unwrap();
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
                .map_err(|e| e.to_string())?,
            cpal::SampleFormat::I16 => output_device
                .build_output_stream(
                    &output_stream_config,
                    move |data: &mut [i16], _: &_| {
                        let mut shared = shared_out.lock().unwrap();
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
                .map_err(|e| e.to_string())?,
            cpal::SampleFormat::U16 => output_device
                .build_output_stream(
                    &output_stream_config,
                    move |data: &mut [u16], _: &_| {
                        let mut shared = shared_out.lock().unwrap();
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
                .map_err(|e| e.to_string())?,
            _ => return Err(format!("Unsupported output sample format: {}", output_sample_format)),
        };
        Some(s)
    } else {
        None
    };

    input_stream.play().map_err(|e| e.to_string())?;
    if let Some(ref s) = output_stream {
        s.play().map_err(|e| e.to_string())?;
    }

    let mut mon = audio.lock().unwrap();
    mon.input_stream = Some(input_stream);
    mon.output_stream = output_stream;
    mon.shared = shared.clone();
    mon.last_input_rate = Some(config.sample_rate() as f32);
    mon.last_output_rate = output_config.as_ref().map(|c| c.sample_rate() as f32);

    Ok(())
}

fn push_mono_to_buffers(
    shared: Option<&Arc<Mutex<NsState>>>,
    rec_buffer: &Mutex<VecDeque<f32>>,
    mono: f32,
    sum: &mut f32,
    frames: &mut f32,
) {
    if let Some(shared) = shared {
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
    *sum += mono * mono;
    *frames += 1.0;
}

fn build_input_stream_f32<F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    input_channels: usize,
    shared: Option<Arc<Mutex<NsState>>>,
    rec_buffer: Arc<Mutex<VecDeque<f32>>>,
    last_emit: Arc<Mutex<Instant>>,
    app_handle: tauri::AppHandle,
    err_fn: F,
) -> Result<cpal::Stream, String>
where
    F: FnMut(cpal::StreamError) + Send + 'static,
{
    device
        .build_input_stream(
            config,
            move |data: &[f32], _: &_| {
                let mut sum = 0.0;
                let mut frames = 0.0;
                for frame in data.chunks(input_channels) {
                    let mono = frame.iter().sum::<f32>() / input_channels as f32;
                    push_mono_to_buffers(shared.as_ref(), &rec_buffer, mono, &mut sum, &mut frames);
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
        .map_err(|e| e.to_string())
}

fn build_input_stream_i16<F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    input_channels: usize,
    shared: Option<Arc<Mutex<NsState>>>,
    rec_buffer: Arc<Mutex<VecDeque<f32>>>,
    last_emit: Arc<Mutex<Instant>>,
    app_handle: tauri::AppHandle,
    err_fn: F,
) -> Result<cpal::Stream, String>
where
    F: FnMut(cpal::StreamError) + Send + 'static,
{
    device
        .build_input_stream(
            config,
            move |data: &[i16], _: &_| {
                let mut sum = 0.0;
                let mut frames = 0.0;
                for frame in data.chunks(input_channels) {
                    let mono = frame.iter().map(|&s| s as f32 / 32768.0).sum::<f32>()
                        / input_channels as f32;
                    push_mono_to_buffers(shared.as_ref(), &rec_buffer, mono, &mut sum, &mut frames);
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
        .map_err(|e| e.to_string())
}

fn build_input_stream_u16<F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    input_channels: usize,
    shared: Option<Arc<Mutex<NsState>>>,
    rec_buffer: Arc<Mutex<VecDeque<f32>>>,
    last_emit: Arc<Mutex<Instant>>,
    app_handle: tauri::AppHandle,
    err_fn: F,
) -> Result<cpal::Stream, String>
where
    F: FnMut(cpal::StreamError) + Send + 'static,
{
    device
        .build_input_stream(
            config,
            move |data: &[u16], _: &_| {
                let mut sum = 0.0;
                let mut frames = 0.0;
                for frame in data.chunks(input_channels) {
                    let mono = frame
                        .iter()
                        .map(|&s| (s as f32 - 32768.0) / 32768.0)
                        .sum::<f32>()
                        / input_channels as f32;
                    push_mono_to_buffers(shared.as_ref(), &rec_buffer, mono, &mut sum, &mut frames);
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
        .map_err(|e| e.to_string())
}

pub fn stop_monitoring(audio: Arc<Mutex<AudioMonitorState>>) -> Result<(), String> {
    let mut mon = audio.lock().unwrap();
    mon.input_stream = None;
    mon.output_stream = None;
    mon.shared = None;
    Ok(())
}

pub fn set_monitoring_volume(audio: Arc<Mutex<AudioMonitorState>>, volume: f32) -> Result<(), String> {
    let mon = audio.lock().unwrap();
    if let Some(shared) = mon.shared.as_ref() {
        let mut shared = shared.lock().unwrap();
        shared.set_volume(volume);
    }
    Ok(())
}

pub fn set_monitoring_model(
    audio: Arc<Mutex<AudioMonitorState>>,
    model_name: String,
) -> Result<(), String> {
    let mon = audio.lock().unwrap();
    let shared = mon.shared.as_ref().ok_or("Monitoring not started")?;
    let (vol, input_rate, output_rate) = {
        let guard = shared.lock().unwrap();
        let v = guard.volume();
        let ir = mon.last_input_rate.unwrap_or(48000.0);
        let or = mon.last_output_rate.unwrap_or(48000.0);
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

// --- System volume (macOS) ---

#[tauri::command]
pub fn get_system_input_volume() -> Result<u8, String> {
    #[cfg(target_os = "macos")]
    {
        let v = crate::system_input_volume::get_system_input_volume()?;
        Ok((v * 100.0).round() as u8)
    }
    #[cfg(not(target_os = "macos"))]
    Err("System input volume is only supported on macOS.".to_string())
}

#[tauri::command]
pub fn set_system_input_volume(volume: u8) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let v = (volume.min(100) as f32) / 100.0;
        crate::system_input_volume::set_system_input_volume(v)
    }
    #[cfg(not(target_os = "macos"))]
    let _ = volume;
    #[cfg(not(target_os = "macos"))]
    Err("System input volume is only supported on macOS.".to_string())
}

// --- BlackHole status ---

#[derive(serde::Serialize)]
pub struct BlackHoleStatus {
    pub installed: bool,
    pub paths: Vec<String>,
}

#[tauri::command]
pub fn get_blackhole_status() -> Result<BlackHoleStatus, String> {
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
