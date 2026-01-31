use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
use screencapturekit::stream::sc_stream::SCStream;

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
    #[cfg(target_os = "macos")]
    pub app_audio_stream: Arc<Mutex<Option<SCStream>>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            writer: Arc::new(Mutex::new(None)),
            mic_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(SAMPLE_RATE * 10))),
            app_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(SAMPLE_RATE * 10))),
            worker: None,
            #[cfg(target_os = "macos")]
            app_audio_stream: Arc::new(Mutex::new(None)),
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

#[cfg(target_os = "macos")]
pub fn get_recordable_apps() -> Result<Vec<RecordableApp>, String> {
    use screencapturekit::prelude::*;
    
    let content = SCShareableContent::get()
        .map_err(|e| format!("Failed to get shareable content: {:?}", e))?;
    
    let running_apps = content.applications();
    
    let mut apps: Vec<RecordableApp> = running_apps
        .iter()
        .filter_map(|app| {
            let bundle_id = app.bundle_identifier();
            let app_name = app.application_name();
            let pid = app.process_id();
            
            // Skip system processes and apps without bundle IDs
            if bundle_id.is_empty() || app_name.is_empty() {
                return None;
            }
            
            Some(RecordableApp {
                id: format!("{}_{}", bundle_id, pid),
                name: app_name,
                bundle_id,
            })
        })
        .collect();
    
    // Sort by name for better UX
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    
    // Add "None" option at the beginning
    apps.insert(0, RecordableApp {
        id: "none".to_string(),
        name: "None (Mic only)".to_string(),
        bundle_id: "none".to_string(),
    });
    
    Ok(apps)
}

#[cfg(not(target_os = "macos"))]
pub fn get_recordable_apps() -> Result<Vec<RecordableApp>, String> {
    Ok(vec![
        RecordableApp {
            id: "none".to_string(),
            name: "None (Mic only)".to_string(),
            bundle_id: "none".to_string(),
        },
    ])
}

#[cfg(target_os = "macos")]
pub fn start_app_audio_capture(
    app_id: &str,
    app_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<SCStream, String> {
    use screencapturekit::prelude::*;
    
    // Parse app_id to get bundle_id and pid
    let parts: Vec<&str> = app_id.split('_').collect();
    if parts.len() < 2 {
        return Err("Invalid app_id format".to_string());
    }
    let bundle_id = parts[0..parts.len()-1].join("_");
    let pid: i32 = parts[parts.len()-1].parse()
        .map_err(|_| "Invalid PID in app_id".to_string())?;
    
    // Get shareable content
    let content = SCShareableContent::get()
        .map_err(|e| format!("Failed to get shareable content: {:?}", e))?;
    
    // Store applications and displays to avoid lifetime issues
    let apps = content.applications();
    let displays = content.displays();
    
    // Find the app by bundle_id and pid
    let app = apps
        .iter()
        .find(|a| a.bundle_identifier() == bundle_id && a.process_id() == pid)
        .ok_or_else(|| format!("Application not found: {} (PID: {})", bundle_id, pid))?;
    
    // Get first display for the filter
    let display = displays
        .first()
        .ok_or_else(|| "No displays found".to_string())?;
    
    // Create content filter for the app
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_including_applications(&[app], &[])
        .build();
    
    // Configure stream for audio-only capture at 48kHz stereo
    let config = SCStreamConfiguration::new()
        .with_captures_audio(true)
        .with_sample_rate(SAMPLE_RATE as i32)
        .with_channel_count(2);
    
    // Create stream with audio handler
    struct AudioHandler {
        buffer: Arc<Mutex<VecDeque<f32>>>,
    }
    
    impl SCStreamOutputTrait for AudioHandler {
        fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
            if !matches!(of_type, SCStreamOutputType::Audio) {
                return;
            }
            
            // Extract audio samples from CMSampleBuffer
            if let Some(audio_buffer_list) = sample.audio_buffer_list() {
                let num_buffers = audio_buffer_list.num_buffers();
                
                for i in 0..num_buffers {
                    if let Some(audio_buffer) = audio_buffer_list.buffer(i) {
                        let data = audio_buffer.data();
                        let num_channels = audio_buffer_list.get(i)
                            .map(|b| b.number_channels)
                            .unwrap_or(1);
                        
                        // Convert PCM data to f32 samples
                        // Audio is typically in f32 format from ScreenCaptureKit
                        let samples = unsafe {
                            std::slice::from_raw_parts(
                                data.as_ptr() as *const f32,
                                data.len() / std::mem::size_of::<f32>(),
                            )
                        };
                        
                        // If stereo, downmix to mono; if mono, use as-is
                        let mut buffer = self.buffer.lock().unwrap();
                        if num_channels == 2 {
                            for chunk in samples.chunks(2) {
                                let mono = (chunk[0] + chunk[1]) / 2.0;
                                if buffer.len() >= SAMPLE_RATE * 10 {
                                    buffer.pop_front();
                                }
                                buffer.push_back(mono);
                            }
                        } else {
                            for &sample in samples {
                                if buffer.len() >= SAMPLE_RATE * 10 {
                                    buffer.pop_front();
                                }
                                buffer.push_back(sample);
                            }
                        }
                    }
                }
            }
        }
    }
    
    let handler = AudioHandler {
        buffer: app_buffer,
    };
    
    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(handler, SCStreamOutputType::Audio);
    stream.start_capture()
        .map_err(|e| format!("Failed to start capture: {:?}", e))?;
    
    Ok(stream)
}

#[cfg(not(target_os = "macos"))]
pub fn start_app_audio_capture(
    _app_id: &str,
    _app_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<(), String> {
    Err("App audio capture is only supported on macOS".to_string())
}
