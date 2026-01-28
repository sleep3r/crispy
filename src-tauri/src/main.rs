#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio_engine;

use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, Mutex};
use audio_engine::AudioEngine;

#[derive(serde::Serialize)]
struct AudioDevice {
    id: String,
    name: String,
}

struct AudioMonitorState {
    engine: Arc<Mutex<AudioEngine>>,
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
fn start_monitoring(
    state: tauri::State<AudioMonitorState>,
    app_handle: tauri::AppHandle,
    device_name: String,
) -> Result<(), String> {
    let mut engine = state.engine.lock().unwrap();
    engine.start(device_name, app_handle)
}

#[tauri::command]
fn stop_monitoring(state: tauri::State<AudioMonitorState>) -> Result<(), String> {
    let mut engine = state.engine.lock().unwrap();
    engine.stop();
    Ok(())
}

#[derive(serde::Serialize)]
struct VirtualMicStatus {
    plugin_installed: bool,
    plugin_path: String,
    shared_memory_active: bool,
}

#[tauri::command]
fn get_virtual_mic_status() -> Result<VirtualMicStatus, String> {
    let plugin_path = "/Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver";
    let plugin_installed = std::path::Path::new(plugin_path).exists();
    
    // Check if shared memory exists
    let shared_memory_active = unsafe {
        let name = std::ffi::CString::new(virtual_mic_ipc::SHM_NAME).unwrap();
        let fd = libc::shm_open(name.as_ptr(), libc::O_RDONLY, 0);
        if fd >= 0 {
            libc::close(fd);
            true
        } else {
            false
        }
    };
    
    Ok(VirtualMicStatus {
        plugin_installed,
        plugin_path: plugin_path.to_string(),
        shared_memory_active,
    })
}

fn main() {
    tauri::Builder::default()
        .manage(AudioMonitorState {
            engine: Arc::new(Mutex::new(AudioEngine::new())),
        })
        .invoke_handler(tauri::generate_handler![
            get_input_devices,
            get_output_devices,
            start_monitoring,
            stop_monitoring,
            get_virtual_mic_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
