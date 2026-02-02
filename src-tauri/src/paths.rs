use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn fallback_documents_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|p| p.join("Documents"))
    }

    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|p| p.join("Documents"))
    }
}

/// Best-effort "Documents" dir:
/// 1) Tauri's document_dir (known folder)
/// 2) Fallback to USERPROFILE/Documents on Windows or HOME/Documents on Unix
pub fn documents_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = app.path().document_dir() {
        return Ok(dir);
    }
    fallback_documents_dir().ok_or_else(|| "Cannot resolve Documents directory".to_string())
}

/// ~/Documents/Crispy (macOS/Linux) or %USERPROFILE%\\Documents\\Crispy (Windows fallback)
pub fn crispy_documents_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(documents_dir(app)?.join("Crispy"))
}

pub fn recordings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crispy_documents_root(app)?.join("Recordings"))
}

pub fn transcriptions_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crispy_documents_root(app)?.join("Transcriptions"))
}

pub fn ensure_dir(path: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|e| format!("Failed to create dir {}: {}", path.display(), e))
}
