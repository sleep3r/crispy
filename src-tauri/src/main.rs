#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cpal::traits::{DeviceTrait, HostTrait};

#[derive(serde::Serialize)]
struct AudioDevice {
    id: String,
    name: String,
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
                    // Filter out duplicate names if necessary, but for now just list them
                    result.push(AudioDevice {
                        id: name.clone(),
                        name,
                    });
                }
            }
            // Dedup based on name to avoid confusion in UI if cpal returns multiple handles
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_input_devices,
            get_output_devices
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
