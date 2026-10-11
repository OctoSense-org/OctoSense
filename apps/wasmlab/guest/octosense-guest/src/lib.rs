//! The guest side of OctoSense's WebAssembly functions: what a Rust crate
//! needs so an app can call its functions through the shell's runtime
//! (`crates/wasm-host`, whose docs define the ABI).
//!
//! ```ignore
//! octosense_guest::abi!();
//!
//! fn shout(input: &[u8]) -> Result<Vec<u8>, String> {
//!     Ok(input.to_ascii_uppercase())
//! }
//! octosense_guest::export!(shout);
//!
//! #[derive(serde::Deserialize)]
//! struct Order { items: Vec<u32> }
//! fn total(order: Order) -> Result<u32, String> {
//!     Ok(order.items.iter().sum())
//! }
//! octosense_guest::export_json!(total);
//! ```
//!
//! Build the crate as a `cdylib` for `wasm32-unknown-unknown`. The module
//! exports `memory`, `octo_alloc`, `octo_free` and each exported function,
//! and imports only `octo.log`.

use std::alloc::{alloc, dealloc, Layout};

#[doc(hidden)]
pub use serde_json;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "octo")]
extern "C" {
    #[link_name = "log"]
    fn octo_log(ptr: *const u8, len: usize);
}

/// Writes a line to the app's log. The host keeps a bounded number of
/// lines, each cut to a bounded length.
pub fn log(line: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        octo_log(line.as_ptr(), line.len())
    }
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{line}");
}

/// Exports `octo_alloc` and `octo_free`, which the host uses to pass input
/// in and take output out. Invoke it once in the crate.
#[macro_export]
macro_rules! abi {
    () => {
        const _: () = {
            #[export_name = "octo_alloc"]
            pub extern "C" fn __octo_alloc(len: usize) -> *mut u8 {
                $crate::__alloc(len)
            }

            #[export_name = "octo_free"]
            pub unsafe extern "C" fn __octo_free(ptr: *mut u8, len: usize) {
                $crate::__free(ptr, len)
            }
        };
    };
}

/// Exports `fn(&[u8]) -> Result<Vec<u8>, String>` under its own name.
#[macro_export]
macro_rules! export {
    ($name:ident) => {
        const _: () = {
            #[export_name = stringify!($name)]
            pub extern "C" fn __octo_export(ptr: *const u8, len: usize) -> i64 {
                $crate::__init();
                let input = unsafe { $crate::__input(ptr, len) };
                $crate::__output($name(input))
            }
        };
    };
}

/// Exports `fn(In) -> Result<Out, String>` under its own name, where `In`
/// is deserialized from the input as JSON and `Out` serialized to it.
#[macro_export]
macro_rules! export_json {
    ($name:ident) => {
        const _: () = {
            #[export_name = stringify!($name)]
            pub extern "C" fn __octo_export(ptr: *const u8, len: usize) -> i64 {
                $crate::__init();
                let input = unsafe { $crate::__input(ptr, len) };
                $crate::__output(
                    $crate::serde_json::from_slice(input)
                        .map_err(|e| {
                            ::std::format!("the input is not what {} takes: {e}", stringify!($name))
                        })
                        .and_then($name)
                        .and_then(|out| {
                            $crate::serde_json::to_vec(&out).map_err(|e| e.to_string())
                        }),
                )
            }
        };
    };
}

#[doc(hidden)]
pub fn __alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::dangling().as_ptr();
    }
    // SAFETY: the size is non-zero and the alignment is 1.
    unsafe { alloc(Layout::from_size_align_unchecked(len, 1)) }
}

/// # Safety
/// `ptr` and `len` are a buffer from [`__alloc`] or [`__output`].
#[doc(hidden)]
pub unsafe fn __free(ptr: *mut u8, len: usize) {
    if len != 0 {
        dealloc(ptr, Layout::from_size_align_unchecked(len, 1));
    }
}

/// # Safety
/// `ptr` and `len` are the input buffer the host wrote.
#[doc(hidden)]
pub unsafe fn __input<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// Packs a result for the host: a status byte (`0` ok, `1` error) and the
/// output or the error text, as `(ptr << 32) | len`.
#[doc(hidden)]
pub fn __output(result: Result<Vec<u8>, String>) -> i64 {
    let mut buf = Vec::new();
    match result {
        Ok(bytes) => {
            buf.reserve_exact(bytes.len() + 1);
            buf.push(0);
            buf.extend_from_slice(&bytes);
        }
        Err(text) => {
            buf.reserve_exact(text.len() + 1);
            buf.push(1);
            buf.extend_from_slice(text.as_bytes());
        }
    }
    let boxed = buf.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut u8 as usize;
    ((ptr as u64) << 32 | len as u64) as i64
}

/// Logs a panic's message before the module traps, so the app can tell why.
#[doc(hidden)]
pub fn __init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}")))));
}
