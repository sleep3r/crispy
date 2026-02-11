use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use screencapturekit::stream::sc_stream::SCStream;

pub const SAMPLE_RATE: usize = 48000;
pub const CHANNELS: usize = 2; // Stereo

/// Resample audio from one sample rate to another using linear interpolation
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn resample_audio(samples: &[f32], from_rate: usize, to_rate: usize) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }
    
    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);
    
    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_index = src_pos.floor() as usize;
        let frac = src_pos - src_index as f64;
        
        if src_index + 1 < samples.len() {
            // Linear interpolation between two samples
            let sample1 = samples[src_index];
            let sample2 = samples[src_index + 1];
            output.push(sample1 + (sample2 - sample1) * frac as f32);
        } else if src_index < samples.len() {
            // Last sample, no interpolation needed
            output.push(samples[src_index]);
        }
    }
    
    output
}

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
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub app_audio_stream: Arc<Mutex<Option<SCStream>>>,
    #[cfg(target_os = "windows")]
    pub app_audio_stop: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_os = "windows")]
    pub app_audio_worker: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            writer: Arc::new(Mutex::new(None)),
            mic_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(SAMPLE_RATE * 10))),
            app_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(SAMPLE_RATE * 10))),
            worker: None,
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            app_audio_stream: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "windows")]
            app_audio_stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(target_os = "windows")]
            app_audio_worker: Arc::new(Mutex::new(None)),
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

    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
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

#[cfg(target_os = "windows")]
pub fn get_recordable_apps() -> Result<Vec<RecordableApp>, String> {
    crate::windows_audio::get_recordable_apps_windows()
}

#[cfg(not(any(all(target_os = "macos", target_arch = "aarch64"), target_os = "windows")))]
pub fn get_recordable_apps() -> Result<Vec<RecordableApp>, String> {
    Ok(vec![
        RecordableApp {
            id: "none".to_string(),
            name: "None (Mic only)".to_string(),
            bundle_id: "none".to_string(),
        },
    ])
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
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
        detected_sample_rate: Arc<Mutex<Option<usize>>>,
    }
    
    impl SCStreamOutputTrait for AudioHandler {
        fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
            if !matches!(of_type, SCStreamOutputType::Audio) {
                return;
            }
            
            // Extract audio samples from CMSampleBuffer
            if let Some(audio_buffer_list) = sample.audio_buffer_list() {
                let num_buffers = audio_buffer_list.num_buffers();
                if num_buffers == 0 {
                    return;
                }

                let mono_samples: Option<Vec<f32>> = if num_buffers >= 2 {
                    let Some(left) = audio_buffer_list.buffer(0) else {
                        return;
                    };
                    let Some(right) = audio_buffer_list.buffer(1) else {
                        return;
                    };

                    let left_samples = unsafe {
                        std::slice::from_raw_parts(
                            left.data().as_ptr() as *const f32,
                            left.data().len() / std::mem::size_of::<f32>(),
                        )
                    };
                    let right_samples = unsafe {
                        std::slice::from_raw_parts(
                            right.data().as_ptr() as *const f32,
                            right.data().len() / std::mem::size_of::<f32>(),
                        )
                    };

                    let len = left_samples.len().min(right_samples.len());
                    Some(
                        (0..len)
                            .map(|i| (left_samples[i] + right_samples[i]) / 2.0)
                            .collect(),
                    )
                } else {
                    let Some(audio_buffer) = audio_buffer_list.buffer(0) else {
                        return;
                    };
                    let num_channels = audio_buffer_list
                        .get(0)
                        .map(|b| b.number_channels as usize)
                        .unwrap_or(1);

                    let samples = unsafe {
                        std::slice::from_raw_parts(
                            audio_buffer.data().as_ptr() as *const f32,
                            audio_buffer.data().len() / std::mem::size_of::<f32>(),
                        )
                    };

                    if num_channels >= 2 {
                        Some(
                            samples
                                .chunks(num_channels)
                                .map(|chunk| {
                                    let mut sum = 0.0;
                                    for &s in chunk {
                                        sum += s;
                                    }
                                    sum / num_channels as f32
                                })
                                .collect(),
                        )
                    } else {
                        Some(samples.to_vec())
                    }
                };

                let Some(mono_samples) = mono_samples else {
                    return;
                };

                let actual_sample_rate = {
                    let mut guard = self.detected_sample_rate.lock().unwrap();
                    if let Some(rate) = *guard {
                        rate
                    } else {
                        let duration = sample.duration();
                        let num_samples = mono_samples.len();
                        let computed = if duration.value > 0 && duration.timescale > 0 {
                            let duration_secs =
                                duration.value as f64 / duration.timescale as f64;
                            if duration_secs > 0.0 {
                                (num_samples as f64 / duration_secs).round() as usize
                            } else {
                                44100
                            }
                        } else {
                            44100
                        };

                        let rate = if computed.abs_diff(48000) < 200 {
                            48000
                        } else if computed.abs_diff(44100) < 200 {
                            44100
                        } else {
                            44100
                        };
                        *guard = Some(rate);
                        rate
                    }
                };

                let final_samples = if actual_sample_rate != SAMPLE_RATE {
                    resample_audio(&mono_samples, actual_sample_rate, SAMPLE_RATE)
                } else {
                    mono_samples
                };

                // Push to buffer
                let mut buffer = self.buffer.lock().unwrap();
                for sample in final_samples {
                    if buffer.len() >= SAMPLE_RATE * 10 {
                        buffer.pop_front();
                    }
                    buffer.push_back(sample);
                }
            }
        }
    }
    
    let handler = AudioHandler {
        buffer: app_buffer,
        detected_sample_rate: Arc::new(Mutex::new(None)),
    };
    
    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(handler, SCStreamOutputType::Audio);
    stream.start_capture()
        .map_err(|e| format!("Failed to start capture: {:?}", e))?;
    
    Ok(stream)
}

#[cfg(target_os = "windows")]
pub fn start_app_audio_capture(
    app_id: &str,
    app_buffer: Arc<Mutex<VecDeque<f32>>>,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Result<std::thread::JoinHandle<()>, String> {
    crate::windows_audio::start_app_audio_capture_windows(app_id, app_buffer, stop_flag)
}

#[cfg(not(any(all(target_os = "macos", target_arch = "aarch64"), target_os = "windows")))]
pub fn start_app_audio_capture(
    _app_id: &str,
    _app_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<(), String> {
    Err("App audio capture is not supported on this platform".to_string())
}

// Non-macOS (and macOS x86) stub defined above.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_writer_creates_file() {
        let dir = std::env::temp_dir().join("crispy_test_wavwriter_create");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_create.wav");

        let writer = WavWriter::new(path.clone()).unwrap();
        assert_eq!(writer.output_path(), &path);
        let finalized_path = writer.finalize().unwrap();
        assert_eq!(finalized_path, path);
        assert!(path.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_writer_writes_silence() {
        let dir = std::env::temp_dir().join("crispy_test_wavwriter_silence");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_silence.wav");

        let mut writer = WavWriter::new(path.clone()).unwrap();
        let left = vec![0.0f32; 48000];
        let right = vec![0.0f32; 48000];
        writer.write_samples(&left, &right).unwrap();
        writer.finalize().unwrap();

        // Verify the file is a valid WAV we can read back
        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, CHANNELS as u16);
        assert_eq!(spec.sample_rate, SAMPLE_RATE as u32);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);

        // 48000 samples * 2 channels = 96000 total samples
        let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len(), 48000 * 2);
        // All silence
        assert!(samples.iter().all(|&s| s == 0));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_writer_writes_audio_data() {
        let dir = std::env::temp_dir().join("crispy_test_wavwriter_data");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_data.wav");

        let mut writer = WavWriter::new(path.clone()).unwrap();
        // Write a simple tone: +0.5 left, -0.5 right
        let left = vec![0.5f32; 100];
        let right = vec![-0.5f32; 100];
        writer.write_samples(&left, &right).unwrap();
        writer.finalize().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len(), 200); // 100 * 2 channels

        // Check interleaved: left, right, left, right...
        let expected_left = (0.5f32 * 32767.0) as i16;
        let expected_right = (-0.5f32 * 32767.0) as i16;
        for i in 0..100 {
            assert_eq!(samples[i * 2], expected_left);
            assert_eq!(samples[i * 2 + 1], expected_right);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_writer_clamps_samples() {
        let dir = std::env::temp_dir().join("crispy_test_wavwriter_clamp");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_clamp.wav");

        let mut writer = WavWriter::new(path.clone()).unwrap();
        // Values outside -1.0..1.0 should be clamped
        let left = vec![2.0f32, -3.0f32];
        let right = vec![1.5f32, -1.5f32];
        writer.write_samples(&left, &right).unwrap();
        writer.finalize().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
        // All should be clamped to i16 max/min
        assert_eq!(samples[0], 32767);  // 2.0 clamped to 1.0
        assert_eq!(samples[1], 32767);  // 1.5 clamped to 1.0
        assert_eq!(samples[2], -32767); // -3.0 clamped to -1.0
        assert_eq!(samples[3], -32767); // -1.5 clamped to -1.0

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_writer_rejects_mismatched_channels() {
        let dir = std::env::temp_dir().join("crispy_test_wavwriter_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_mismatch.wav");

        let mut writer = WavWriter::new(path.clone()).unwrap();
        let left = vec![0.0f32; 100];
        let right = vec![0.0f32; 50]; // different length
        let result = writer.write_samples(&left, &right);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mismatch"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wav_writer_multiple_writes() {
        let dir = std::env::temp_dir().join("crispy_test_wavwriter_multi");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_multi.wav");

        let mut writer = WavWriter::new(path.clone()).unwrap();
        // Write 3 batches of 100 samples each
        for _ in 0..3 {
            let left = vec![0.25f32; 100];
            let right = vec![-0.25f32; 100];
            writer.write_samples(&left, &right).unwrap();
        }
        writer.finalize().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len(), 600); // 300 * 2 channels

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recording_constants() {
        assert_eq!(SAMPLE_RATE, 48000);
        assert_eq!(CHANNELS, 2);
    }

    #[test]
    fn recording_state_initializes_with_empty_buffers() {
        let state = RecordingState::new();
        assert!(state.writer.lock().unwrap().is_none());
        assert!(state.mic_buffer.lock().unwrap().is_empty());
        assert!(state.app_buffer.lock().unwrap().is_empty());
        assert!(state.worker.is_none());
    }
}
