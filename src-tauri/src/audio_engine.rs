use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;
use virtual_mic_ipc::*;

/// Shared memory manager for virtual microphone output
pub struct SharedMemoryWriter {
    ptr: *mut u8,
    fd: i32,
    writer: RingBufferWriter,
}

impl SharedMemoryWriter {
    /// Create or open shared memory for virtual microphone
    pub fn new() -> Result<Self, String> {
        unsafe {
            // Try to unlink any existing shared memory first
            let name = std::ffi::CString::new(SHM_NAME).unwrap();
            libc::shm_unlink(name.as_ptr());
            
            // Create new shared memory
            let fd = libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_RDWR,
                0o644,
            );
            
            if fd < 0 {
                return Err(format!("Failed to create shared memory: {}", 
                    std::io::Error::last_os_error()));
            }
            
            // Set size
            let size = shared_memory_size();
            if libc::ftruncate(fd, size as i64) != 0 {
                libc::close(fd);
                return Err(format!("Failed to set shared memory size: {}", 
                    std::io::Error::last_os_error()));
            }
            
            // Map memory
            let ptr = libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            
            if ptr == libc::MAP_FAILED {
                libc::close(fd);
                return Err(format!("Failed to map shared memory: {}", 
                    std::io::Error::last_os_error()));
            }
            
            let ptr = ptr as *mut u8;
            
            // Initialize header
            let header_ptr = ptr as *mut Header;
            *header_ptr = Header::init();
            
            // Create writer
            let writer = RingBufferWriter::from_ptr(ptr);
            
            Ok(Self { ptr, fd, writer })
        }
    }
    
    /// Write audio frames to the ring buffer
    pub fn write(&mut self, frames: &[f32]) -> usize {
        self.writer.write(frames)
    }
    
    /// Get current fill level
    pub fn fill_level(&self) -> u32 {
        self.writer.fill_level()
    }
}

impl Drop for SharedMemoryWriter {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                let size = shared_memory_size();
                libc::munmap(self.ptr as *mut _, size);
                self.ptr = ptr::null_mut();
            }
            
            if self.fd >= 0 {
                libc::close(self.fd);
                self.fd = -1;
            }
            
            // Unlink shared memory
            let name = std::ffi::CString::new(SHM_NAME).unwrap();
            libc::shm_unlink(name.as_ptr());
        }
    }
}

unsafe impl Send for SharedMemoryWriter {}

/// Audio processing state
pub struct AudioEngine {
    pub stream: Option<cpal::Stream>,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            stream: None,
        }
    }
    
    /// Start audio capture and processing
    pub fn start(
        &mut self,
        device_name: String,
        app_handle: tauri::AppHandle,
    ) -> Result<(), String> {
        // Stop any existing stream
        self.stop();
        
        // Initialize shared memory
        let shm_writer = SharedMemoryWriter::new()?;
        
        let host = cpal::default_host();
        
        // Find the device
        let device = if device_name == "Default" {
            host.default_input_device()
        } else {
            host.input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| {
                    #[allow(deprecated)]
                    d.name().map(|n| n == device_name).unwrap_or(false)
                })
        }
        .ok_or("Failed to find input device")?;
        
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let sample_rate = config.sample_rate();
        let channels = config.channels() as usize;
        
        eprintln!("Input config: {} Hz, {} channels, {:?}", 
            sample_rate, channels, config.sample_format());
        
        // Shared state for the audio callback
        let shm_writer = Arc::new(Mutex::new(shm_writer));
        let last_emit = Arc::new(Mutex::new(Instant::now()));
        
        // Create resampler buffer if needed
        let needs_resample = sample_rate != SAMPLE_RATE as u32;
        let resample_ratio = sample_rate as f64 / SAMPLE_RATE as f64;
        
        let err_fn = |err| eprintln!("Audio stream error: {}", err);
        
        // Build stream based on sample format
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let shm = shm_writer.clone();
                let last = last_emit.clone();
                let app = app_handle.clone();
                
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        process_audio_f32(data, channels, sample_rate, needs_resample, 
                            resample_ratio, &shm, &last, &app);
                    },
                    err_fn,
                    None,
                )
            },
            cpal::SampleFormat::I16 => {
                let shm = shm_writer.clone();
                let last = last_emit.clone();
                let app = app_handle.clone();
                
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        let float_data: Vec<f32> = data.iter()
                            .map(|&s| s as f32 / 32768.0)
                            .collect();
                        process_audio_f32(&float_data, channels, sample_rate, needs_resample,
                            resample_ratio, &shm, &last, &app);
                    },
                    err_fn,
                    None,
                )
            },
            cpal::SampleFormat::U16 => {
                let shm = shm_writer.clone();
                let last = last_emit.clone();
                let app = app_handle.clone();
                
                device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &_| {
                        let float_data: Vec<f32> = data.iter()
                            .map(|&s| (s as f32 - 32768.0) / 32768.0)
                            .collect();
                        process_audio_f32(&float_data, channels, sample_rate, needs_resample,
                            resample_ratio, &shm, &last, &app);
                    },
                    err_fn,
                    None,
                )
            },
            _ => return Err(format!("Unsupported sample format: {}", config.sample_format())),
        }
        .map_err(|e| e.to_string())?;
        
        stream.play().map_err(|e| e.to_string())?;
        
        self.stream = Some(stream);
        // Keep shm_writer in Arc for the callback, we don't need to store it
        // The callback closure owns the Arc
        
        Ok(())
    }
    
    /// Stop audio capture
    pub fn stop(&mut self) {
        self.stream = None;
        // Note: shm_writer cleanup happens in the Arc held by the callback
    }
}


/// Process audio data: downmix to mono, resample if needed, write to ring buffer
fn process_audio_f32(
    data: &[f32],
    channels: usize,
    _sample_rate: u32,
    needs_resample: bool,
    resample_ratio: f64,
    shm_writer: &Arc<Mutex<SharedMemoryWriter>>,
    last_emit: &Arc<Mutex<Instant>>,
    app_handle: &tauri::AppHandle,
) {
    // Downmix to mono
    let frame_count = data.len() / channels;
    let mut mono_buffer = Vec::with_capacity(frame_count);
    
    for i in 0..frame_count {
        let mut sum = 0.0;
        for ch in 0..channels {
            sum += data[i * channels + ch];
        }
        mono_buffer.push(sum / channels as f32);
    }
    
    // Resample if needed
    let output_buffer = if needs_resample {
        simple_resample(&mono_buffer, resample_ratio)
    } else {
        mono_buffer
    };
    
    // Compute RMS for UI meter
    let mut sum_squares = 0.0;
    for &sample in &output_buffer {
        sum_squares += sample * sample;
    }
    let rms = (sum_squares / output_buffer.len() as f32).sqrt();
    
    // Emit level update (throttled to 60Hz)
    {
        let mut last = last_emit.lock().unwrap();
        if last.elapsed() >= Duration::from_millis(16) {
            *last = Instant::now();
            let _ = app_handle.emit("microphone-level", rms);
        }
    }
    
    // Write to shared memory ring buffer
    let mut writer = shm_writer.lock().unwrap();
    let written = writer.write(&output_buffer);
    
    if written < output_buffer.len() {
        // Buffer full - this is logged but not critical
        // The plugin will see the overrun counter
    }
}

/// Simple linear resampler for sample rate conversion
fn simple_resample(input: &[f32], ratio: f64) -> Vec<f32> {
    if ratio == 1.0 {
        return input.to_vec();
    }
    
    let output_len = (input.len() as f64 / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);
    
    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos.floor() as usize;
        let frac = src_pos - src_idx as f64;
        
        if src_idx + 1 < input.len() {
            // Linear interpolation
            let sample = input[src_idx] * (1.0 - frac as f32) + 
                        input[src_idx + 1] * frac as f32;
            output.push(sample);
        } else if src_idx < input.len() {
            output.push(input[src_idx]);
        }
    }
    
    output
}
