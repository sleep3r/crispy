use std::path::Path;
use std::process::Command;
use tauri::async_runtime::spawn_blocking;
use tauri::AppHandle;

fn get_ffmpeg_command() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "ffmpeg.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "ffmpeg"
    }
}

#[tauri::command]
pub async fn convert_to_wav(app: AppHandle, input_path: String) -> Result<String, String> {
    let recordings_dir = crate::paths::recordings_dir(&app)?;
    crate::paths::ensure_dir(&recordings_dir)?;
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

        // Check if ffmpeg is available
        let ffmpeg = get_ffmpeg_command();
        let check = Command::new(ffmpeg)
            .arg("-version")
            .output();

        if check.is_err() {
            #[cfg(target_os = "windows")]
            {
                return Err(
                    "FFmpeg is not installed. Press Win+R and run: winget install ffmpeg".to_string(),
                );
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err(
                    "FFmpeg is not installed. Please install FFmpeg to use the conversion feature.".to_string(),
                );
            }
        }

        // Run ffmpeg conversion
        // Convert to 48kHz, stereo, 16-bit PCM WAV
        let output = Command::new(ffmpeg)
            .args([
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
            ])
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
        let ffmpeg = get_ffmpeg_command();
        Command::new(ffmpeg)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
    .await
    .map_err(|e| e.to_string())
}
