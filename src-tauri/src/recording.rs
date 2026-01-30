use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const SAMPLE_RATE: usize = 48000;
pub const CHANNELS: usize = 2; // Stereo

#[derive(serde::Serialize, Clone)]
pub struct RecordableApp {
    pub id: String,
    pub name: String,
    pub bundle_id: String,
}

pub struct RecordingState {
    pub writer: Arc<Mutex<Option<WavWriter>>>,
    pub mic_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub app_buffer: Arc<Mutex<VecDeque<f32>>>,
    pub worker: Option<std::thread::JoinHandle<()>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            writer: Arc::new(Mutex::new(None)),
            mic_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(SAMPLE_RATE * 10))),
            app_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(SAMPLE_RATE * 10))),
            worker: None,
        }
    }
}

pub struct WavWriter {
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    output_path: PathBuf,
}

impl WavWriter {
    pub fn new(output_path: PathBuf) -> Result<Self, String> {
        let spec = hound::WavSpec {
            channels: CHANNELS as u16,
            sample_rate: SAMPLE_RATE as u32,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = hound::WavWriter::create(&output_path, spec)
            .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

        Ok(Self {
            writer,
            output_path,
        })
    }

    pub fn write_samples(&mut self, left: &[f32], right: &[f32]) -> Result<(), String> {
        if left.len() != right.len() {
            return Err("Left and right channel length mismatch".to_string());
        }

        // Interleave and write samples
        for i in 0..left.len() {
            // Convert f32 (-1.0 to 1.0) to i16
            let left_sample = (left[i].clamp(-1.0, 1.0) * 32767.0) as i16;
            let right_sample = (right[i].clamp(-1.0, 1.0) * 32767.0) as i16;
            
            self.writer
                .write_sample(left_sample)
                .map_err(|e| format!("Failed to write left sample: {}", e))?;
            self.writer
                .write_sample(right_sample)
                .map_err(|e| format!("Failed to write right sample: {}", e))?;
        }

        Ok(())
    }

    pub fn finalize(self) -> Result<PathBuf, String> {
        self.writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;
        
        Ok(self.output_path)
    }
}

// For now, return a placeholder app list
// TODO: Implement ScreenCaptureKit integration for actual per-app capture
pub fn get_recordable_apps() -> Result<Vec<RecordableApp>, String> {
    // Placeholder - in real implementation, use ScreenCaptureKit to list running apps
    Ok(vec![
        RecordableApp {
            id: "system".to_string(),
            name: "System Audio (Loopback)".to_string(),
            bundle_id: "system".to_string(),
        },
    ])
}
