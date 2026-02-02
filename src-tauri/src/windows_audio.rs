// Windows audio capture using WASAPI
//
// CURRENT STATUS:
// - ✅ Process enumeration works (shows list of running applications)
// - ⏳ Audio capture from specific apps - IN DEVELOPMENT
//
// TODO: Implement actual WASAPI loopback capture for selected processes
// This requires:
// 1. IAudioClient initialization for the target process
// 2. Loopback capture configuration
// 3. Audio streaming from the captured buffer to app_buffer
// 4. Handle sample rate conversion if needed
//
// For now, users can see available apps but capture will fail with a message
// that this feature is coming soon.

#[cfg(target_os = "windows")]
use std::collections::VecDeque;
#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::CloseHandle;

#[cfg(target_os = "windows")]
use crate::recording::RecordableApp;

#[cfg(target_os = "windows")]
pub fn get_recordable_apps_windows() -> Result<Vec<RecordableApp>, String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|e| format!("Failed to create process snapshot: {:?}", e))?;

        if snapshot.is_invalid() {
            return Err("Invalid snapshot handle".to_string());
        }

        let mut apps = Vec::new();
        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };

        if Process32First(snapshot, &mut entry).is_ok() {
            loop {
                // Convert process name from fixed-size array to String
                let process_name = String::from_utf8_lossy(
                    &entry
                        .szExeFile
                        .iter()
                        .take_while(|&&c| c != 0)
                        .map(|&c| c as u8)
                        .collect::<Vec<u8>>(),
                )
                .to_string();

                // Filter out system processes and keep only user applications
                if !process_name.is_empty()
                    && entry.th32ProcessID > 0
                    && !is_system_process(&process_name)
                {
                    let name = process_name
                        .trim_end_matches(".exe")
                        .to_string();
                    
                    apps.push(RecordableApp {
                        id: format!("{}_{}", name, entry.th32ProcessID),
                        name: name.clone(),
                        bundle_id: name,
                    });
                }

                if Process32Next(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);

        // Sort by name
        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        
        // Remove duplicates by name (keep first occurrence)
        apps.dedup_by(|a, b| a.name.to_lowercase() == b.name.to_lowercase());

        // Add "None" option at the beginning
        apps.insert(
            0,
            RecordableApp {
                id: "none".to_string(),
                name: "None (Mic only)".to_string(),
                bundle_id: "none".to_string(),
            },
        );

        Ok(apps)
    }
}

#[cfg(target_os = "windows")]
fn is_system_process(name: &str) -> bool {
    let system_processes = [
        "system",
        "registry",
        "smss.exe",
        "csrss.exe",
        "wininit.exe",
        "services.exe",
        "lsass.exe",
        "svchost.exe",
        "dwm.exe",
        "conhost.exe",
        "winlogon.exe",
        "fontdrvhost.exe",
        "spoolsv.exe",
        "runtimebroker.exe",
        "taskhostw.exe",
        "sihost.exe",
        "ctfmon.exe",
        "searchindexer.exe",
        "searchprotocolhost.exe",
        "searchfilterhost.exe",
        "dllhost.exe",
        "taskmgr.exe",
        "mmc.exe",
        "wudfhost.exe",
        "audiodg.exe",
        "backgroundtaskhost.exe",
        "winstore.app.exe",
        "applicationframehost.exe",
        "securityhealthsystray.exe",
        "securityhealthservice.exe",
        "msedge.exe", // Edge может генерировать много процессов
        "msedgewebview2.exe",
    ];

    let name_lower = name.to_lowercase();
    
    // Filter system processes
    if system_processes.iter().any(|&sys| name_lower == sys) {
        return true;
    }
    
    // Filter obvious non-GUI processes
    if name_lower.ends_with("host.exe") 
        || name_lower.ends_with("service.exe")
        || name_lower.ends_with("helper.exe")
        || name_lower.contains("background")
        || name_lower.contains("update")
    {
        return true;
    }
    
    false
}

// Stub for app audio capture on Windows
// TODO: Implement actual audio capture using WASAPI loopback
#[cfg(target_os = "windows")]
pub fn start_app_audio_capture_windows(
    _app_id: &str,
    _app_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<(), String> {
    // For now, return an error indicating it's not yet implemented
    // In the future, this should use WASAPI loopback capture
    Err("App audio capture on Windows is coming soon. Use system audio loopback for now.".to_string())
}
