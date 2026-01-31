//! macOS: get/set system default input device volume (Core Audio).
//! This is the same level as in System Settings → Sound → Input.

#![cfg(target_os = "macos")]

use coreaudio_sys::{
    kAudioDevicePropertyScopeInput, kAudioDevicePropertyVolumeScalar,
    kAudioHardwarePropertyDefaultInputDevice, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectGetPropertyData,
    AudioObjectSetPropertyData, AudioObjectPropertyAddress, Float32, UInt32,
};
use std::mem;
use std::ptr;

const ELEMENT_MAIN: u32 = kAudioObjectPropertyElementMain as u32;

fn default_input_device_id() -> Result<u32, String> {
    let mut device_id: u32 = 0;
    let mut size = mem::size_of::<u32>() as UInt32;
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDefaultInputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: ELEMENT_MAIN,
    };
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut device_id as *mut _ as *mut _,
        )
    };
    if status != 0 {
        return Err(format!("Core Audio default input device: {}", status));
    }
    Ok(device_id)
}

/// Get system default input device volume (0.0 .. 1.0).
/// Not all devices support volume control; returns error if unsupported.
pub fn get_system_input_volume() -> Result<f32, String> {
    let device_id = default_input_device_id()?;
    let mut volume: Float32 = 0.0;
    let mut size = mem::size_of::<Float32>() as UInt32;
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyVolumeScalar,
        mScope: kAudioDevicePropertyScopeInput,
        mElement: ELEMENT_MAIN,
    };
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut volume as *mut _ as *mut _,
        )
    };
    if status != 0 {
        return Err(format!("Core Audio get input volume: {}", status));
    }
    Ok(volume)
}

/// Set system default input device volume (0.0 .. 1.0).
pub fn set_system_input_volume(volume: f32) -> Result<(), String> {
    let device_id = default_input_device_id()?;
    let volume = volume.clamp(0.0, 1.0);
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyVolumeScalar,
        mScope: kAudioDevicePropertyScopeInput,
        mElement: ELEMENT_MAIN,
    };
    let size = mem::size_of::<Float32>() as UInt32;
    let status = unsafe {
        AudioObjectSetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            size,
            &volume as *const _ as *const _,
        )
    };
    if status != 0 {
        return Err(format!("Core Audio set input volume: {}", status));
    }
    Ok(())
}
