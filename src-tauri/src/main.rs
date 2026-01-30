#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

#[derive(serde::Serialize)]
struct AudioDevice {
    id: String,
    name: String,
}

struct AudioMonitorState {
    stream: Arc<Mutex<Option<cpal::Stream>>>,
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

#[tauri::command]
#[allow(deprecated)]
fn start_monitoring(
    state: tauri::State<AudioMonitorState>,
    app_handle: tauri::AppHandle,
    device_name: String,
) -> Result<(), String> {
    if device_name.trim().is_empty() {
        return Err("No input device selected".to_string());
    }

    // Stop any existing stream first
    {
        let mut stream_lock = state.stream.lock().unwrap();
        *stream_lock = None;
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
    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let last_emit = Arc::new(Mutex::new(Instant::now()));

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let last_emit = last_emit.clone();
            let app_handle = app_handle.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| {
                    let mut sum = 0.0;
                    for &sample in data {
                        sum += sample * sample;
                    }
                    let rms = (sum / data.len() as f32).sqrt();

                    let mut last = last_emit.lock().unwrap();
                    if last.elapsed() >= Duration::from_millis(16) {
                        *last = Instant::now();
                        let _ = app_handle.emit("microphone-level", rms);
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let last_emit = last_emit.clone();
            let app_handle = app_handle.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &_| {
                    let mut sum = 0.0;
                    for &sample in data {
                        let sample_f32 = sample as f32 / 32768.0;
                        sum += sample_f32 * sample_f32;
                    }
                    let rms = (sum / data.len() as f32).sqrt();

                    let mut last = last_emit.lock().unwrap();
                    if last.elapsed() >= Duration::from_millis(16) {
                        *last = Instant::now();
                        let _ = app_handle.emit("microphone-level", rms);
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let last_emit = last_emit.clone();
            let app_handle = app_handle.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[u16], _: &_| {
                    let mut sum = 0.0;
                    for &sample in data {
                        let sample_f32 = (sample as f32 - 32768.0) / 32768.0;
                        sum += sample_f32 * sample_f32;
                    }
                    let rms = (sum / data.len() as f32).sqrt();

                    let mut last = last_emit.lock().unwrap();
                    if last.elapsed() >= Duration::from_millis(16) {
                        *last = Instant::now();
                        let _ = app_handle.emit("microphone-level", rms);
                    }
                },
                err_fn,
                None,
            )
        }
        _ => return Err(format!("Unsupported sample format: {}", config.sample_format())),
    }
    .map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;

    *state.stream.lock().unwrap() = Some(stream);

    Ok(())
}

#[tauri::command]
fn stop_monitoring(state: tauri::State<AudioMonitorState>) -> Result<(), String> {
    let mut stream_lock = state.stream.lock().unwrap();
    *stream_lock = None;
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

fn main() {
    tauri::Builder::default()
        .manage(AudioMonitorState {
            stream: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            get_input_devices,
            get_output_devices,
            start_monitoring,
            stop_monitoring,
            get_blackhole_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
