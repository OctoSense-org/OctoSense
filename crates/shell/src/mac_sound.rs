//! Where a Mac's sound goes: its output devices, the one in use, and a choice
//! of another, as the Sound menu in a Mac's menu bar offers them. The Mac owns
//! them; the shell reads them when the menu opens and asks CoreAudio for a
//! change when a person picks one.

/// An output device: its CoreAudio id, its name, whether sound goes there
/// now, and what it is, for its glyph.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Output {
    pub id: u32,
    pub name: String,
    pub current: bool,
    pub kind: Kind,
}

/// What an output is, as a Mac's Sound menu draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Speakers,
    Headphones,
    Display,
}

/// What a device is, from how it is connected (CoreAudio's transport type)
/// and what it is called: headphones by name or over Bluetooth, a display
/// over HDMI or DisplayPort or by name, speakers otherwise.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn kind_of(transport: u32, name: &str) -> Kind {
    let name = name.to_lowercase();
    if ["headphone", "airpods", "headset", "earbud", "buds", "beats"].iter().any(|word| name.contains(word)) {
        return Kind::Headphones;
    }
    if ["display", " tv", "tv "].iter().any(|word| name.contains(word)) || name == "tv" {
        return Kind::Display;
    }
    match &transport.to_be_bytes() {
        b"hdmi" | b"dprt" => Kind::Display,
        b"blue" | b"bled" => Kind::Headphones,
        _ => Kind::Speakers,
    }
}

#[cfg(target_os = "macos")]
mod core_audio {
    use std::ffi::c_void;

    #[repr(C)]
    pub struct Address {
        pub selector: u32,
        pub scope: u32,
        pub element: u32,
    }

    const fn code(c: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*c)
    }
    pub const SYSTEM: u32 = 1;
    pub const DEVICES: u32 = code(b"dev#");
    pub const DEFAULT_OUTPUT: u32 = code(b"dOut");
    pub const STREAMS: u32 = code(b"stm#");
    pub const NAME: u32 = code(b"lnam");
    pub const TRANSPORT: u32 = code(b"tran");
    pub const GLOBAL: u32 = code(b"glob");
    pub const OUTPUT: u32 = code(b"outp");
    pub const UTF8: u32 = 0x0800_0100;

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        pub fn AudioObjectGetPropertyDataSize(id: u32, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: *mut u32) -> i32;
        pub fn AudioObjectGetPropertyData(id: u32, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: *mut u32, data: *mut c_void) -> i32;
        pub fn AudioObjectSetPropertyData(id: u32, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: u32, data: *const c_void) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        pub fn CFStringGetLength(string: *const c_void) -> isize;
        pub fn CFStringGetCString(string: *const c_void, buffer: *mut u8, size: isize, encoding: u32) -> u8;
        pub fn CFRelease(object: *const c_void);
    }

    pub fn global(selector: u32) -> Address {
        Address { selector, scope: GLOBAL, element: 0 }
    }

    /// A CFString's text; the string stays the caller's.
    pub unsafe fn text(string: *const c_void) -> Option<String> {
        let room = CFStringGetLength(string) * 4 + 1;
        let mut buffer = vec![0u8; room.max(1) as usize];
        if CFStringGetCString(string, buffer.as_mut_ptr(), room, UTF8) == 0 {
            return None;
        }
        buffer.truncate(buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len()));
        String::from_utf8(buffer).ok()
    }
}

/// The Mac's output devices, in CoreAudio's order: only those with an output
/// stream, so a microphone isn't listed.
#[cfg(target_os = "macos")]
pub(crate) fn outputs() -> Vec<Output> {
    use core_audio::*;
    use std::ffi::c_void;
    use std::ptr::null;
    unsafe {
        let mut current = 0u32;
        let mut size = 4u32;
        if AudioObjectGetPropertyData(SYSTEM, &global(DEFAULT_OUTPUT), 0, null(), &mut size, &mut current as *mut u32 as *mut c_void) != 0 {
            return Vec::new();
        }
        let mut size = 0u32;
        if AudioObjectGetPropertyDataSize(SYSTEM, &global(DEVICES), 0, null(), &mut size) != 0 {
            return Vec::new();
        }
        let mut ids = vec![0u32; size as usize / 4];
        if AudioObjectGetPropertyData(SYSTEM, &global(DEVICES), 0, null(), &mut size, ids.as_mut_ptr() as *mut c_void) != 0 {
            return Vec::new();
        }
        ids.truncate(size as usize / 4);
        ids.into_iter()
            .filter_map(|id| {
                let streams = Address { selector: STREAMS, scope: OUTPUT, element: 0 };
                let mut size = 0u32;
                if AudioObjectGetPropertyDataSize(id, &streams, 0, null(), &mut size) != 0 || size == 0 {
                    return None;
                }
                let mut name: *const c_void = null();
                let mut size = std::mem::size_of::<*const c_void>() as u32;
                if AudioObjectGetPropertyData(id, &global(NAME), 0, null(), &mut size, &mut name as *mut *const c_void as *mut c_void) != 0 || name.is_null() {
                    return None;
                }
                // The name is a copy, the caller's to release.
                let words = text(name);
                CFRelease(name);
                let mut transport = 0u32;
                let mut size = 4u32;
                if AudioObjectGetPropertyData(id, &global(TRANSPORT), 0, null(), &mut size, &mut transport as *mut u32 as *mut c_void) != 0 {
                    transport = 0;
                }
                let words = words?;
                Some(Output { id, kind: kind_of(transport, &words), name: words, current: id == current })
            })
            .collect()
    }
}

/// Sends the Mac's sound to the device `id`, as choosing it in a Mac's Sound
/// menu does. False when CoreAudio refuses.
#[cfg(target_os = "macos")]
pub(crate) fn choose_output(id: u32) -> bool {
    use core_audio::*;
    unsafe { AudioObjectSetPropertyData(SYSTEM, &global(DEFAULT_OUTPUT), 0, std::ptr::null(), 4, &id as *const u32 as *const std::ffi::c_void) == 0 }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn outputs() -> Vec<Output> {
    Vec::new()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn choose_output(_id: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{kind_of, Kind};

    #[test]
    fn an_output_is_drawn_as_what_it_is() {
        let code = |c: &[u8; 4]| u32::from_be_bytes(*c);
        assert_eq!(kind_of(code(b"bltn"), "Mac mini Speakers"), Kind::Speakers);
        assert_eq!(kind_of(code(b"bltn"), "External Headphones"), Kind::Headphones);
        assert_eq!(kind_of(code(b"blue"), "Someone's AirPods Pro"), Kind::Headphones);
        assert_eq!(kind_of(code(b"hdmi"), "LG HDR 4K"), Kind::Display);
        assert_eq!(kind_of(code(b"usb "), "Studio Display Speakers"), Kind::Display);
        assert_eq!(kind_of(code(b"airp"), "Living Room TV"), Kind::Display);
        assert_eq!(kind_of(code(b"virt"), "Recorder"), Kind::Speakers);
        assert_eq!(kind_of(0, "Television"), Kind::Speakers, "a word with tv in it is not a TV");
    }

    #[test]
    fn sound_goes_to_one_output_at_most_and_each_has_a_name() {
        let outputs = super::outputs();
        assert!(outputs.iter().filter(|output| output.current).count() <= 1, "{outputs:?}");
        assert!(outputs.iter().all(|output| !output.name.trim().is_empty()), "{outputs:?}");
    }
}
