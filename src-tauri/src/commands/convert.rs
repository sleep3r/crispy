use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
use tauri::async_runtime::spawn_blocking;
use tauri::AppHandle;

fn ffmpeg_missing_message() -> String {
    #[cfg(target_os = "windows")]
    {
        "FFmpeg is not installed. Press Win+R and run: winget install ffmpeg".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "FFmpeg is not installed. Please install FFmpeg to use the conversion feature.".to_string()
    }
}

#[cfg(target_os = "windows")]
fn resolve_ffmpeg_path() -> Option<PathBuf> {
    let mut probe = Command::new("ffmpeg.exe");
    probe.arg("-version");
    #[cfg(target_os = "windows")]
    {
        probe.creation_flags(CREATE_NO_WINDOW);
    }
    if let Ok(output) = probe.output() {
        if output.status.success() {
            return Some(PathBuf::from("ffmpeg.exe"));
        }
    }

    if let Some(local_app) = std::env::var_os("LOCALAPPDATA") {
        let candidate = PathBuf::from(local_app)
            .join("Microsoft")
            .join("WinGet")
            .join("Links")
            .join("ffmpeg.exe");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let candidate = PathBuf::from(user_profile)
            .join("scoop")
            .join("shims")
            .join("ffmpeg.exe");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
fn resolve_ffmpeg_path() -> Option<PathBuf> {
    if let Ok(output) = Command::new("ffmpeg").arg("-version").output() {
        if output.status.success() {
            return Some(PathBuf::from("ffmpeg"));
        }
    }

    // Packaged macOS apps don't inherit shell PATH. Check common locations.
    let candidates = [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ];

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            if let Ok(output) = Command::new(&path).arg("-version").output() {
                if output.status.success() {
                    return Some(path);
                }
            }
        }
    }

    None
}

#[tauri::command]
pub async fn convert_to_wav(app: AppHandle, input_path: String) -> Result<String, String> {
    let recordings_dir = crate::paths::recordings_dir(&app)?;
    crate::paths::ensure_dir(&recordings_dir)?;
    let ffmpeg_path = resolve_ffmpeg_path().ok_or_else(ffmpeg_missing_message)?;
    spawn_blocking(move || {
        let input = Path::new(&input_path);
        
        if !input.exists() {
            return Err("Input file does not exist".to_string());
        }

        // Generate output filename
        let input_stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid input filename")?;
        
        let now = chrono::Local::now();
        let output_filename = format!("{}_{}.wav", input_stem, now.format("%Y%m%d_%H%M%S"));
        let output_path = recordings_dir.join(output_filename);

        // Run ffmpeg conversion
        // Convert to 48kHz, stereo, 16-bit PCM WAV
        let mut command = Command::new(&ffmpeg_path);
        command.args([
                "-i",
                input_path.as_str(),
                "-ar",
                "48000",
                "-ac",
                "2",
                "-acodec",
                "pcm_s16le",
                "-y",
                output_path.to_str().ok_or("Invalid output path")?,
            ]);
        #[cfg(target_os = "windows")]
        {
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let output = command
            .output()
            .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("FFmpeg conversion failed: {}", stderr));
        }

        Ok(output_path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Task failed to join: {}", e))?
}

#[tauri::command]
pub async fn check_ffmpeg() -> Result<bool, String> {
    spawn_blocking(|| {
        resolve_ffmpeg_path()
            .and_then(|path| {
                let mut cmd = Command::new(path);
                cmd.arg("-version");
                #[cfg(target_os = "windows")]
                {
                    cmd.creation_flags(CREATE_NO_WINDOW);
                }
                cmd.output()
                    .ok()
                    .map(|o| o.status.success())
            })
            .unwrap_or(false)
    })
    .await
    .map_err(|e| e.to_string())
}
