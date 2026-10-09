//! Read the system output mixer without launching an AppleScript interpreter.
//! Called only by the status worker: audio drivers may block property reads.
use std::{ffi::c_void, mem::size_of, ptr};

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
}

// AudioHardwareService's virtual main volume handles devices whose volume
// exists only on individual channels (for example a built-in stereo output).
#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioHardwareServiceGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
}

fn read<T: Default>(
    object: u32,
    selector: &[u8; 4],
    scope: &[u8; 4],
    virtual_main: bool,
) -> Option<T> {
    let address = Address {
        selector: u32::from_be_bytes(*selector),
        scope: u32::from_be_bytes(*scope),
        element: 0,
    };
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // The call writes exactly one scalar of the type associated with selector.
    let result = unsafe {
        let get = if virtual_main {
            AudioHardwareServiceGetPropertyData
        } else {
            AudioObjectGetPropertyData
        };
        get(
            object,
            &address,
            0,
            ptr::null(),
            &mut size,
            (&mut value as *mut T).cast(),
        )
    };
    (result == 0 && size as usize == size_of::<T>()).then_some(value)
}

pub(super) fn sample_volume() -> (Option<u32>, bool) {
    let Some(device) = read::<u32>(1, b"dOut", b"glob", false).filter(|id| *id != 0) else {
        return (None, false);
    };
    let level = read::<f32>(device, b"vmvc", b"outp", true)
        .or_else(|| read::<f32>(device, b"volm", b"outp", false))
        .filter(|v| v.is_finite())
        .map(|v| (v.clamp(0.0, 1.0) * 100.0).round() as u32);
    let muted = read::<u32>(device, b"mute", b"outp", false).is_some_and(|v| v != 0);
    (level, muted)
}
