// Noise suppression (NS) models: list only. No download; models are built-in or from rnnnoise.

use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct NsModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Returns available noise suppression models (dummy, noisy, rnnnoise).
/// Kept separate from transcription (TS) models for clarity.
#[tauri::command]
pub fn get_available_ns_models() -> Vec<NsModelInfo> {
    vec![
        NsModelInfo {
            id: "dummy".to_string(),
            name: "None".to_string(),
            description: "No processing".to_string(),
        },
        NsModelInfo {
            id: "noisy".to_string(),
            name: "Test noise".to_string(),
            description: "Adds test noise (debug)".to_string(),
        },
        NsModelInfo {
            id: "rnnnoise".to_string(),
            name: "RNN Noise".to_string(),
            description: "RNNoise neural network denoiser (48 kHz)".to_string(),
        },
    ]
}
