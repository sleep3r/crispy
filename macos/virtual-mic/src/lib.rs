use std::ptr;
use std::sync::atomic::Ordering;
use virtual_mic_ipc::*;

/// Global shared memory state
static mut SHM_PTR: *mut u8 = ptr::null_mut();
static mut SHM_FD: i32 = -1;

/// Initialize shared memory connection
/// Returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn crispy_init_shm() -> i32 {
    unsafe {
        // Close existing if any
        if !SHM_PTR.is_null() {
            crispy_cleanup_shm();
        }
        
        // Open shared memory (read-only for plugin)
        let name = std::ffi::CString::new(SHM_NAME).unwrap();
        let fd = libc::shm_open(name.as_ptr(), libc::O_RDONLY, 0);
        
        if fd < 0 {
            return -1;
        }
        
        SHM_FD = fd;
        
        // Map memory
        let size = shared_memory_size();
        let ptr = libc::mmap(
            ptr::null_mut(),
            size,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd,
            0,
        );
        
        if ptr == libc::MAP_FAILED {
            libc::close(fd);
            SHM_FD = -1;
            return -1;
        }
        
        SHM_PTR = ptr as *mut u8;
        
        // Validate header
        let header = &*(SHM_PTR as *const Header);
        if !header.validate() {
            crispy_cleanup_shm();
            return -1;
        }
        
        0
    }
}

/// Clean up shared memory
#[no_mangle]
pub extern "C" fn crispy_cleanup_shm() {
    unsafe {
        if !SHM_PTR.is_null() {
            let size = shared_memory_size();
            libc::munmap(SHM_PTR as *mut _, size);
            SHM_PTR = ptr::null_mut();
        }
        
        if SHM_FD >= 0 {
            libc::close(SHM_FD);
            SHM_FD = -1;
        }
    }
}

/// Check if shared memory is available
#[no_mangle]
pub extern "C" fn crispy_is_shm_available() -> i32 {
    unsafe {
        if SHM_PTR.is_null() {
            0
        } else {
            1
        }
    }
}

/// Read audio frames from the ring buffer
/// Returns number of frames actually read
/// Fills with silence on underrun
#[no_mangle]
pub extern "C" fn crispy_read_frames(buffer: *mut f32, frame_count: u32) -> u32 {
    unsafe {
        if SHM_PTR.is_null() {
            // No shared memory - return silence
            ptr::write_bytes(buffer, 0, frame_count as usize);
            return 0;
        }
        
        let reader = RingBufferReader::from_ptr(SHM_PTR as *const u8);
        let slice = std::slice::from_raw_parts_mut(buffer, frame_count as usize);
        reader.read(slice) as u32
    }
}

/// Get current buffer fill level in frames
#[no_mangle]
pub extern "C" fn crispy_get_fill_level() -> u32 {
    unsafe {
        if SHM_PTR.is_null() {
            return 0;
        }
        
        let reader = RingBufferReader::from_ptr(SHM_PTR as *const u8);
        reader.fill_level()
    }
}

/// Get underrun count
#[no_mangle]
pub extern "C" fn crispy_get_underrun_count() -> u64 {
    unsafe {
        if SHM_PTR.is_null() {
            return 0;
        }
        
        let reader = RingBufferReader::from_ptr(SHM_PTR as *const u8);
        reader.underrun_count()
    }
}

/// Get overrun count
#[no_mangle]
pub extern "C" fn crispy_get_overrun_count() -> u64 {
    unsafe {
        if SHM_PTR.is_null() {
            return 0;
        }
        
        let reader = RingBufferReader::from_ptr(SHM_PTR as *const u8);
        reader.overrun_count()
    }
}

/// Get current read index
#[no_mangle]
pub extern "C" fn crispy_get_read_index() -> u32 {
    unsafe {
        if SHM_PTR.is_null() {
            return 0;
        }
        
        let header = &*(SHM_PTR as *const Header);
        header.read_index.load(Ordering::Acquire)
    }
}

/// Get current write index
#[no_mangle]
pub extern "C" fn crispy_get_write_index() -> u32 {
    unsafe {
        if SHM_PTR.is_null() {
            return 0;
        }
        
        let header = &*(SHM_PTR as *const Header);
        header.write_index.load(Ordering::Acquire)
    }
}
