use std::f32::consts::PI;
use std::io::Write;
use std::thread;
use std::time::Duration;
use virtual_mic_ipc::*;

/// Test writer that generates a 440Hz sine wave into shared memory
fn main() {
    println!("Crispy Virtual Mic - Tone Writer");
    println!("==================================");
    println!();
    
    // Open/create shared memory
    let shm_fd = unsafe {
        let name = std::ffi::CString::new(SHM_NAME).unwrap();
        let fd = libc::shm_open(
            name.as_ptr(),
            libc::O_CREAT | libc::O_RDWR,
            0o644,
        );
        
        if fd < 0 {
            eprintln!("Failed to open shared memory: {}", std::io::Error::last_os_error());
            std::process::exit(1);
        }
        
        fd
    };
    
    // Set size
    let size = shared_memory_size();
    unsafe {
        if libc::ftruncate(shm_fd, size as i64) != 0 {
            eprintln!("Failed to set shared memory size: {}", std::io::Error::last_os_error());
            libc::close(shm_fd);
            std::process::exit(1);
        }
    }
    
    // Map memory
    let ptr = unsafe {
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            shm_fd,
            0,
        );
        
        if ptr == libc::MAP_FAILED {
            eprintln!("Failed to map shared memory: {}", std::io::Error::last_os_error());
            libc::close(shm_fd);
            std::process::exit(1);
        }
        
        ptr as *mut u8
    };
    
    println!("Shared memory opened: {}", SHM_NAME);
    println!("Size: {} bytes", size);
    println!();
    
    // Initialize header
    unsafe {
        let header_ptr = ptr as *mut Header;
        *header_ptr = Header::init();
    }
    
    println!("Header initialized:");
    println!("  Sample rate: {} Hz", SAMPLE_RATE);
    println!("  Channels: {}", CHANNELS);
    println!("  Capacity: {} frames ({} ms)", CAPACITY_FRAMES, CAPACITY_FRAMES * 1000 / SAMPLE_RATE);
    println!();
    
    // Create writer
    let mut writer = unsafe { RingBufferWriter::from_ptr(ptr) };
    
    // Generate 440Hz sine wave
    let frequency = 440.0_f32;
    let mut phase = 0.0_f32;
    let phase_delta = 2.0 * PI * frequency / SAMPLE_RATE as f32;
    
    // Write in 10ms blocks (480 frames at 48kHz)
    let block_size = (SAMPLE_RATE / 100) as usize; // 10ms
    let mut buffer = vec![0.0f32; block_size];
    
    println!("Generating 440Hz tone...");
    println!("Block size: {} frames ({} ms)", block_size, block_size * 1000 / SAMPLE_RATE as usize);
    println!("Target fill level: ~60ms ({} frames)", SAMPLE_RATE * 60 / 1000);
    println!();
    println!("Press Ctrl+C to stop");
    println!();
    
    let mut iteration = 0;
    
    loop {
        // Generate sine wave block
        for sample in buffer.iter_mut() {
            *sample = (phase.sin() * 0.5).clamp(-1.0, 1.0); // 50% amplitude
            phase += phase_delta;
            if phase >= 2.0 * PI {
                phase -= 2.0 * PI;
            }
        }
        
        // Write to ring buffer
        let written = writer.write(&buffer);
        
        if written < buffer.len() {
            // Buffer full - this is expected initially
            if iteration % 10 == 0 {
                print!("\rBuffer full (overrun), waiting... ");
                std::io::stdout().flush().unwrap();
            }
            thread::sleep(Duration::from_millis(5));
        } else {
            // Successfully written
            let fill = writer.fill_level();
            let fill_ms = fill * 1000 / SAMPLE_RATE;
            
            if iteration % 10 == 0 {
                print!("\rWritten: {} frames, Fill: {} frames ({} ms)  ", 
                       written, fill, fill_ms);
                std::io::stdout().flush().unwrap();
            }
            
            // Sleep for block duration
            thread::sleep(Duration::from_millis(10));
        }
        
        iteration += 1;
    }
}
