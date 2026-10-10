//! A component's storage folder under a byte budget, and its file writes
//! done on the calling thread.
//!
//! wasmtime-wasi gives a component its folder ([`super::Grants`]). This
//! replaces the calls that grow a file — `write`, `set-size`, and the
//! streams `write-via-stream` and `append-via-stream` hand out — with ones
//! that charge the growth against the call's [`Budget`], and the calls that
//! free bytes — `open-at` truncating a file, `unlink-file-at` — with ones
//! that give them back. Growth past the budget fails inside the guest: as a
//! full disk (`ENOSPC`) from a descriptor call, and as an I/O error (`EIO`)
//! from a stream, since wasi-libc reports any failed stream write as one.
//! The budget records the refusal, so the caller can say why. Overwriting,
//! truncating and deleting always work, so a component whose app is at its
//! quota can still free space.
//!
//! What counts is what the shell's storage measure counts: the lengths of
//! the folder's regular files.

use std::sync::{Arc, Mutex};

use bytes::Bytes;
use wasmtime::component::{Linker, Resource};
use wasmtime::StoreContextMut;
use wasmtime_wasi::filesystem::WasiFilesystemView;
use wasmtime_wasi::p2::bindings::sync::filesystem::types::{
    self as fs, Host as _, HostDescriptor as _,
};
use wasmtime_wasi::p2::{DynOutputStream, OutputStream, Pollable, StreamError, StreamResult};
use wasmtime_wasi::runtime::in_tokio;

use super::State;

/// The filesystem interface wasmtime-wasi links; a guest's older 0.2
/// imports resolve to it.
const TYPES: &str = "wasi:filesystem/types@0.2.12";

/// A full disk, to the guest: `ENOSPC` where WASI maps it (Linux, Android,
/// macOS, iOS); elsewhere an error of that kind.
fn full() -> std::io::Error {
    #[cfg(unix)]
    {
        std::io::Error::from_raw_os_error(28)
    }
    #[cfg(not(unix))]
    {
        std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "the storage budget is used up",
        )
    }
}

/// What an instance may still add to its storage folder (`None`: no
/// ceiling), shared with the file streams it has open.
#[derive(Clone, Debug, Default)]
pub(crate) struct Budget(Arc<Mutex<Ledger>>);

#[derive(Debug, Default)]
struct Ledger {
    left: Option<u64>,
    /// A write was refused since the caller last asked.
    refused: bool,
}

impl Budget {
    fn ledger(&self) -> std::sync::MutexGuard<'_, Ledger> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn set(&self, left: Option<u64>) {
        self.ledger().left = left;
    }

    pub(crate) fn left(&self) -> Option<u64> {
        self.ledger().left
    }

    /// Whether a write was refused since the last time this was asked.
    pub(crate) fn take_refused(&self) -> bool {
        std::mem::take(&mut self.ledger().refused)
    }

    /// Spend `bytes`, or refuse them as a full disk.
    fn spend(&self, bytes: u64) -> std::io::Result<()> {
        let mut ledger = self.ledger();
        match ledger.left {
            Some(n) if bytes > n => {
                ledger.refused = true;
                Err(full())
            }
            Some(n) => {
                ledger.left = Some(n - bytes);
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn refund(&self, bytes: u64) {
        if let Some(n) = self.ledger().left.as_mut() {
            *n = n.saturating_add(bytes);
        }
    }
}

type Answer<T> = wasmtime::Result<(Result<T, fs::ErrorCode>,)>;

/// Shadow the linker's growing and freeing filesystem calls with charged
/// ones. Call after `wasmtime_wasi::p2::add_to_linker_sync`.
pub(crate) fn link(linker: &mut Linker<State>) -> wasmtime::Result<()> {
    linker.allow_shadowing(true);
    let mut types = linker.instance(TYPES)?;
    types.func_wrap(
        "[method]descriptor.write-via-stream",
        |mut store: StoreContextMut<'_, State>, (fd, offset): (Resource<fs::Descriptor>, u64)| {
            charged_stream(store.data_mut(), fd, Some(offset))
        },
    )?;
    types.func_wrap(
        "[method]descriptor.append-via-stream",
        |mut store: StoreContextMut<'_, State>, (fd,): (Resource<fs::Descriptor>,)| {
            charged_stream(store.data_mut(), fd, None)
        },
    )?;
    types.func_wrap(
        "[method]descriptor.write",
        |mut store: StoreContextMut<'_, State>,
         (fd, buffer, offset): (Resource<fs::Descriptor>, Vec<u8>, u64)|
         -> Answer<u64> {
            let state = store.data_mut();
            let budget = state.budget.clone();
            let mut files = state.filesystem();
            let size = match files.stat(borrow(&fd)) {
                Ok(stat) => stat.size,
                Err(e) => return Ok((Err(files.convert_error_code(e)?),)),
            };
            let grows = offset
                .saturating_add(buffer.len() as u64)
                .saturating_sub(size);
            if budget.spend(grows).is_err() {
                return Ok((Err(fs::ErrorCode::InsufficientSpace),));
            }
            match files.write(fd, buffer, offset) {
                Ok(written) => Ok((Ok(written),)),
                Err(e) => {
                    budget.refund(grows);
                    Ok((Err(files.convert_error_code(e)?),))
                }
            }
        },
    )?;
    types.func_wrap(
        "[method]descriptor.set-size",
        |mut store: StoreContextMut<'_, State>,
         (fd, size): (Resource<fs::Descriptor>, u64)|
         -> Answer<()> {
            let state = store.data_mut();
            let budget = state.budget.clone();
            let mut files = state.filesystem();
            let was = match files.stat(borrow(&fd)) {
                Ok(stat) => stat.size,
                Err(e) => return Ok((Err(files.convert_error_code(e)?),)),
            };
            let grows = size.saturating_sub(was);
            if budget.spend(grows).is_err() {
                return Ok((Err(fs::ErrorCode::InsufficientSpace),));
            }
            match files.set_size(fd, size) {
                Ok(()) => {
                    budget.refund(was.saturating_sub(size));
                    Ok((Ok(()),))
                }
                Err(e) => {
                    budget.refund(grows);
                    Ok((Err(files.convert_error_code(e)?),))
                }
            }
        },
    )?;
    types.func_wrap(
        "[method]descriptor.open-at",
        |mut store: StoreContextMut<'_, State>,
         (fd, path_flags, path, open_flags, flags): (
            Resource<fs::Descriptor>,
            fs::PathFlags,
            String,
            fs::OpenFlags,
            fs::DescriptorFlags,
        )|
         -> Answer<Resource<fs::Descriptor>> {
            let state = store.data_mut();
            let budget = state.budget.clone();
            let mut files = state.filesystem();
            let freed = if open_flags.contains(fs::OpenFlags::TRUNCATE) {
                file_size_at(&mut files, &fd, path_flags, &path)
            } else {
                0
            };
            match files.open_at(fd, path_flags, path, open_flags, flags) {
                Ok(opened) => {
                    budget.refund(freed);
                    Ok((Ok(opened),))
                }
                Err(e) => Ok((Err(files.convert_error_code(e)?),)),
            }
        },
    )?;
    types.func_wrap(
        "[method]descriptor.unlink-file-at",
        |mut store: StoreContextMut<'_, State>,
         (fd, path): (Resource<fs::Descriptor>, String)|
         -> Answer<()> {
            let state = store.data_mut();
            let budget = state.budget.clone();
            let mut files = state.filesystem();
            let freed = file_size_at(&mut files, &fd, fs::PathFlags::empty(), &path);
            match files.unlink_file_at(fd, path) {
                Ok(()) => {
                    budget.refund(freed);
                    Ok((Ok(()),))
                }
                Err(e) => Ok((Err(files.convert_error_code(e)?),)),
            }
        },
    )?;
    linker.allow_shadowing(false);
    Ok(())
}

/// Another handle on the descriptor `fd` borrows, for a call before the
/// one it is passed to.
fn borrow(fd: &Resource<fs::Descriptor>) -> Resource<fs::Descriptor> {
    Resource::new_borrow(fd.rep())
}

/// The bytes the regular file at `path` holds, as the storage measure
/// counts them (0 for anything else, or nothing there).
fn file_size_at<V: fs::HostDescriptor>(
    files: &mut V,
    fd: &Resource<fs::Descriptor>,
    path_flags: fs::PathFlags,
    path: &str,
) -> u64 {
    match files.stat_at(borrow(fd), path_flags, path.to_string()) {
        Ok(stat) if stat.type_ == fs::DescriptorType::RegularFile => stat.size,
        _ => 0,
    }
}

/// The stream `write-via-stream` (`offset`) or `append-via-stream`
/// (`None`) hands out, charged.
fn charged_stream(
    state: &mut State,
    fd: Resource<fs::Descriptor>,
    offset: Option<u64>,
) -> Answer<Resource<DynOutputStream>> {
    let budget = state.budget.clone();
    let mut files = state.filesystem();
    let size = match files.stat(borrow(&fd)) {
        Ok(stat) => stat.size,
        Err(e) => return Ok((Err(files.convert_error_code(e)?),)),
    };
    let opened = match offset {
        Some(offset) => files.write_via_stream(fd, offset),
        None => files.append_via_stream(fd),
    };
    let stream = match opened {
        Ok(stream) => stream,
        Err(e) => return Ok((Err(files.convert_error_code(e)?),)),
    };
    let inner = files.table.delete(stream)?;
    let charged: DynOutputStream = Box::new(Charged {
        inner,
        budget,
        position: offset,
        size,
    });
    Ok((Ok(files.table.push(charged)?),))
}

/// A file's output stream that charges what it adds to the file, and
/// writes on the calling thread: the plain stream hands every write to a
/// thread pool and waits for it on the next call.
struct Charged {
    inner: DynOutputStream,
    budget: Budget,
    /// Where the next write lands; `None` when appending.
    position: Option<u64>,
    /// The file's size as far as this stream knows.
    size: u64,
}

#[wasmtime_wasi::async_trait]
impl Pollable for Charged {
    async fn ready(&mut self) {
        self.inner.ready().await
    }
}

#[wasmtime_wasi::async_trait]
impl OutputStream for Charged {
    fn write(&mut self, bytes: Bytes) -> StreamResult<()> {
        let len = bytes.len() as u64;
        let end = self.position.unwrap_or(self.size).saturating_add(len);
        let grows = end.saturating_sub(self.size);
        self.budget
            .spend(grows)
            .map_err(|e| StreamError::LastOperationFailed(e.into()))?;
        if let Err(e) = in_tokio(self.inner.blocking_write_and_flush(bytes)) {
            self.budget.refund(grows);
            return Err(e);
        }
        if let Some(position) = &mut self.position {
            *position = end;
        }
        self.size = self.size.max(end);
        Ok(())
    }

    fn flush(&mut self) -> StreamResult<()> {
        self.inner.flush()
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        self.inner.check_write()
    }
}
