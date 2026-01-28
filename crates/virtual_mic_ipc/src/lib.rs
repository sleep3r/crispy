use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::ptr;

/// Magic number to identify Crispy virtual mic shared memory
pub const CRISPY_MAGIC: u32 = 0x43525350; // "CRSP"

/// Protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Sample rate in Hz
pub const SAMPLE_RATE: u32 = 48000;

/// Number of channels (mono)
pub const CHANNELS: u32 = 1;

/// Sample format (Float32)
pub const SAMPLE_FORMAT: u32 = 0; // 0 = Float32

/// Buffer capacity in frames (samples)
/// 10ms * 48kHz = 480 frames per block
/// Store 200ms = 9600 frames to handle jitter
pub const CAPACITY_FRAMES: u32 = 9600;

/// Shared memory name
pub const SHM_NAME: &str = "/crispy_virtual_mic";

/// Shared memory layout
/// 
/// Ring buffer section (follows header)
/// Size: CAPACITY_FRAMES * CHANNELS * sizeof(f32)
/// Access via raw pointer arithmetic
#[repr(C)]
pub struct SharedMemory {
    /// Header section
    pub header: Header,
}

/// Header structure at the start of shared memory
#[repr(C)]
pub struct Header {
    /// Magic number for validation (CRISPY_MAGIC)
    pub magic: u32,
    
    /// Protocol version
    pub version: u32,
    
    /// Sample rate in Hz
    pub sample_rate: u32,
    
    /// Number of channels
    pub channels: u32,
    
    /// Sample format (0 = Float32)
    pub format: u32,
    
    /// Ring buffer capacity in frames
    pub capacity_frames: u32,
    
    /// Write index (in frames) - app writes here
    pub write_index: AtomicU32,
    
    /// Read index (in frames) - plugin reads here
    pub read_index: AtomicU32,
    
    /// Underrun counter (plugin tried to read but no data)
    pub underrun_count: AtomicU64,
    
    /// Overrun counter (app tried to write but buffer full)
    pub overrun_count: AtomicU64,
    
    /// Sequence counter (monotonic frame counter from app)
    pub sequence: AtomicU64,
}

impl Header {
    /// Initialize a new header
    pub fn init() -> Self {
        Self {
            magic: CRISPY_MAGIC,
            version: PROTOCOL_VERSION,
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            format: SAMPLE_FORMAT,
            capacity_frames: CAPACITY_FRAMES,
            write_index: AtomicU32::new(0),
            read_index: AtomicU32::new(0),
            underrun_count: AtomicU64::new(0),
            overrun_count: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
        }
    }
    
    /// Validate header magic and version
    pub fn validate(&self) -> bool {
        self.magic == CRISPY_MAGIC && self.version == PROTOCOL_VERSION
    }
}

/// Ring buffer writer (app side)
pub struct RingBufferWriter {
    header: *mut Header,
    buffer: *mut f32,
    capacity: u32,
}

impl RingBufferWriter {
    /// Create a writer from shared memory pointer
    ///
    /// # Safety
    /// ptr must point to valid shared memory with proper layout
    pub unsafe fn from_ptr(ptr: *mut u8) -> Self {
        let header = ptr as *mut Header;
        let buffer_offset = std::mem::size_of::<Header>();
        let buffer = ptr.add(buffer_offset) as *mut f32;
        let capacity = (*header).capacity_frames;
        
        Self {
            header,
            buffer,
            capacity,
        }
    }
    
    /// Write frames to the ring buffer
    /// Returns number of frames actually written
    pub fn write(&mut self, frames: &[f32]) -> usize {
        let header = unsafe { &*self.header };
        
        let write_idx = header.write_index.load(Ordering::Acquire);
        let read_idx = header.read_index.load(Ordering::Acquire);
        
        // Calculate available space
        let available = if write_idx >= read_idx {
            self.capacity - (write_idx - read_idx) - 1
        } else {
            read_idx - write_idx - 1
        };
        
        let to_write = frames.len().min(available as usize);
        
        if to_write == 0 {
            // Buffer full - increment overrun
            header.overrun_count.fetch_add(1, Ordering::Relaxed);
            return 0;
        }
        
        // Write in two parts if wrapping
        let start = write_idx as usize;
        
        unsafe {
            if start + to_write <= self.capacity as usize {
                // No wrap - single copy
                ptr::copy_nonoverlapping(
                    frames.as_ptr(),
                    self.buffer.add(start),
                    to_write,
                );
            } else {
                // Wrap - two copies
                let first_part = self.capacity as usize - start;
                let second_part = to_write - first_part;
                
                ptr::copy_nonoverlapping(
                    frames.as_ptr(),
                    self.buffer.add(start),
                    first_part,
                );
                ptr::copy_nonoverlapping(
                    frames.as_ptr().add(first_part),
                    self.buffer,
                    second_part,
                );
            }
        }
        
        // Update write index
        let new_write = (write_idx + to_write as u32) % self.capacity;
        header.write_index.store(new_write, Ordering::Release);
        
        // Update sequence
        header.sequence.fetch_add(to_write as u64, Ordering::Relaxed);
        
        to_write
    }
    
    /// Get current buffer fill level in frames
    pub fn fill_level(&self) -> u32 {
        let header = unsafe { &*self.header };
        let write_idx = header.write_index.load(Ordering::Acquire);
        let read_idx = header.read_index.load(Ordering::Acquire);
        
        if write_idx >= read_idx {
            write_idx - read_idx
        } else {
            self.capacity - (read_idx - write_idx)
        }
    }
}

unsafe impl Send for RingBufferWriter {}
unsafe impl Sync for RingBufferWriter {}

/// Ring buffer reader (plugin side)
pub struct RingBufferReader {
    header: *const Header,
    buffer: *const f32,
    capacity: u32,
}

impl RingBufferReader {
    /// Create a reader from shared memory pointer
    ///
    /// # Safety
    /// ptr must point to valid shared memory with proper layout
    pub unsafe fn from_ptr(ptr: *const u8) -> Self {
        let header = ptr as *const Header;
        let buffer_offset = std::mem::size_of::<Header>();
        let buffer = ptr.add(buffer_offset) as *const f32;
        let capacity = (*header).capacity_frames;
        
        Self {
            header,
            buffer,
            capacity,
        }
    }
    
    /// Read frames from the ring buffer
    /// Returns number of frames actually read
    /// Fills remaining with silence if underrun
    pub fn read(&self, frames: &mut [f32]) -> usize {
        let header = unsafe { &*self.header };
        
        let write_idx = header.write_index.load(Ordering::Acquire);
        let read_idx = header.read_index.load(Ordering::Acquire);
        
        // Calculate available data
        let available = if write_idx >= read_idx {
            write_idx - read_idx
        } else {
            self.capacity - (read_idx - write_idx)
        };
        
        let to_read = frames.len().min(available as usize);
        
        if to_read < frames.len() {
            // Underrun - fill remainder with silence
            header.underrun_count.fetch_add(1, Ordering::Relaxed);
            for i in to_read..frames.len() {
                frames[i] = 0.0;
            }
        }
        
        if to_read == 0 {
            return 0;
        }
        
        // Read in two parts if wrapping
        let start = read_idx as usize;
        
        unsafe {
            if start + to_read <= self.capacity as usize {
                // No wrap - single copy
                ptr::copy_nonoverlapping(
                    self.buffer.add(start),
                    frames.as_mut_ptr(),
                    to_read,
                );
            } else {
                // Wrap - two copies
                let first_part = self.capacity as usize - start;
                let second_part = to_read - first_part;
                
                ptr::copy_nonoverlapping(
                    self.buffer.add(start),
                    frames.as_mut_ptr(),
                    first_part,
                );
                ptr::copy_nonoverlapping(
                    self.buffer,
                    frames.as_mut_ptr().add(first_part),
                    second_part,
                );
            }
        }
        
        // Update read index
        let new_read = (read_idx + to_read as u32) % self.capacity;
        header.read_index.store(new_read, Ordering::Release);
        
        to_read
    }
    
    /// Get current buffer fill level in frames
    pub fn fill_level(&self) -> u32 {
        let header = unsafe { &*self.header };
        let write_idx = header.write_index.load(Ordering::Acquire);
        let read_idx = header.read_index.load(Ordering::Acquire);
        
        if write_idx >= read_idx {
            write_idx - read_idx
        } else {
            self.capacity - (read_idx - write_idx)
        }
    }
    
    /// Get underrun count
    pub fn underrun_count(&self) -> u64 {
        let header = unsafe { &*self.header };
        header.underrun_count.load(Ordering::Relaxed)
    }
    
    /// Get overrun count
    pub fn overrun_count(&self) -> u64 {
        let header = unsafe { &*self.header };
        header.overrun_count.load(Ordering::Relaxed)
    }
}

unsafe impl Send for RingBufferReader {}
unsafe impl Sync for RingBufferReader {}

/// Calculate total shared memory size
pub const fn shared_memory_size() -> usize {
    std::mem::size_of::<Header>() + (CAPACITY_FRAMES as usize * CHANNELS as usize * std::mem::size_of::<f32>())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_header_init() {
        let header = Header::init();
        assert!(header.validate());
        assert_eq!(header.sample_rate, SAMPLE_RATE);
        assert_eq!(header.channels, CHANNELS);
    }
}
