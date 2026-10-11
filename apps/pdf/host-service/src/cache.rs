//! The render cache (SERVICE.md "Reading"): `pdf.page` writes each page it
//! renders to `.cache/pages/<doc>/<page>@<dpi>.png` in the caller's storage.
//!
//! - At most [`MAX_BYTES`] (16 MiB) and [`MAX_FILES`] renders per storage;
//!   the oldest renders (by when they were written) go first.
//! - The file cap is the service's own: an app's storage holds at most 256
//!   entries for the app's own `fs` writes and `files.import` (makepad's
//!   `splash_storage::MAX_ENTRIES`), so a cache of hundreds of thumbnails
//!   would leave the app unable to save its library or import a PDF.
//! - Before any write of the service would fail for room
//!   (`storage_full:`), the whole cache is cleared ([`make_room`]).
//! - The cache is the service's: a render replaces a stale render of the
//!   same page and resolution, from any surface. It never walks or writes
//!   through a link: a cache folder that is a link is refused, and only
//!   regular files under plain folders are counted or removed.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use octosense_engine_area::Area;

use crate::codes::{self, fail, invalid, Code};

/// The cache, relative to the storage.
pub(crate) const DIR: &str = ".cache/pages";
/// The most bytes of renders one storage keeps.
pub(crate) const MAX_BYTES: u64 = 16 << 20;
/// The most renders one storage keeps.
pub(crate) const MAX_FILES: usize = 64;

/// Where `pdf.page` writes page `page` of document `handle` at `dpi`.
pub(crate) fn render_path(handle: &str, page: u64, dpi: u32) -> String {
    format!("{DIR}/{handle}/{page}@{dpi}.png")
}

/// Whether `path` is a plain folder (`Ok(false)`: not there); a link or a
/// file in the way is refused.
fn plain_dir(path: &Path, shown: &str) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(true),
        Ok(_) => Err(invalid(format!("the render cache folder `{shown}` is not a plain folder"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(invalid(format!("the render cache folder `{shown}`: {e}"))),
    }
}

/// The cache's folder in `area`, when it is there and plain all the way.
fn root(area: &Area) -> Result<Option<PathBuf>, String> {
    let dot = area.root.join(".cache");
    if !plain_dir(&dot, ".cache")? {
        return Ok(None);
    }
    let pages = dot.join("pages");
    Ok(plain_dir(&pages, DIR)?.then_some(pages))
}

struct Render {
    path: PathBuf,
    len: u64,
    written: SystemTime,
}

/// Every render in the cache: regular files in the documents' plain
/// folders. Links and anything else are neither counted nor followed.
fn renders(root: &Path) -> Vec<Render> {
    let mut out = Vec::new();
    let Ok(docs) = std::fs::read_dir(root) else { return out };
    for doc in docs.flatten() {
        if !std::fs::symlink_metadata(doc.path()).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(doc.path()) else { continue };
        for file in files.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(file.path()) else { continue };
            if meta.is_file() {
                out.push(Render { path: file.path(), len: meta.len(), written: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH) });
            }
        }
    }
    out
}

/// Remove a document's folder when it is empty (never a link).
fn drop_if_empty(dir: &Path) {
    if std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
        let _ = std::fs::remove_dir(dir);
    }
}

/// The limits [`evict`] keeps to: the cache's own, or a test's.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub bytes: u64,
    pub files: usize,
}

pub(crate) const LIMITS: Limits = Limits { bytes: MAX_BYTES, files: MAX_FILES };

/// Make room in the cache at `root` for one more render of `incoming`
/// bytes at `keep` (which it replaces, so it does not count): the oldest
/// renders go first until the cache with it fits `limits`.
pub(crate) fn evict(root: &Path, keep: &Path, incoming: u64, limits: Limits) {
    let mut all: Vec<Render> = renders(root).into_iter().filter(|r| r.path != keep).collect();
    all.sort_by_key(|r| r.written);
    let mut bytes: u64 = all.iter().map(|r| r.len).sum();
    let mut files = all.len();
    for old in &all {
        if bytes.saturating_add(incoming) <= limits.bytes && files < limits.files {
            break;
        }
        if std::fs::remove_file(&old.path).is_ok() {
            bytes = bytes.saturating_sub(old.len);
            files -= 1;
            if let Some(dir) = old.path.parent().filter(|d| Some(*d) != keep.parent()) {
                drop_if_empty(dir);
            }
        }
    }
}

/// Clear the whole cache; the bytes it held.
fn clear(area: &Area) -> u64 {
    let Ok(Some(root)) = root(area) else { return 0 };
    let mut freed = 0;
    for old in renders(&root) {
        if std::fs::remove_file(&old.path).is_ok() {
            freed += old.len;
        }
        if let Some(dir) = old.path.parent() {
            drop_if_empty(dir);
        }
    }
    freed
}

/// Before a write of `need` bytes in `area`: when the storage has less
/// room left, clear the render cache first and give the call the room it
/// freed. The write's own check then answers `storage_full:` if that was
/// not enough.
pub(crate) fn make_room(area: &Area, need: u64) {
    if area.room().is_some_and(|room| need > room) {
        area.credit(clear(area));
    }
}

/// The size of the regular file at `path` (0: none, or a link).
pub(crate) fn regular_len(path: &Path) -> u64 {
    std::fs::symlink_metadata(path).ok().filter(|m| m.is_file()).map_or(0, |m| m.len())
}

/// Whether the render at `rel` is still the `len` bytes this service
/// wrote: the cache may have been cleared or trimmed since.
pub(crate) fn still_there(area: &Area, rel: &str, len: u64) -> bool {
    std::fs::symlink_metadata(area.root.join(rel)).is_ok_and(|m| m.is_file() && m.len() == len)
}

/// Write `png` to the cache at `rel` (from [`render_path`]), within the
/// cache's bounds and the storage's room.
pub(crate) fn store(area: &Area, rel: &str, png: &[u8]) -> Result<(), String> {
    let len = png.len() as u64;
    if len > MAX_BYTES {
        return Err(fail(Code::TooLarge, format!("this page renders to {} MB, more than the {} MB render cache holds: choose a lower dpi", len >> 20, MAX_BYTES >> 20)));
    }
    let target = crate::contained(area, rel, "path", "pdf.page")?;
    // The folders on the way must be plain: a link would have the cache
    // count, trim or write files somewhere else in the storage.
    root(area)?;
    if let Some(dir) = target.parent() {
        plain_dir(dir, &area.shown(dir))?;
    }
    if let Some(root) = root(area)? {
        evict(&root, &target, len, LIMITS);
    }
    make_room(area, len.saturating_sub(regular_len(&target)));
    // The service's own cache: a stale render of the same page goes, by an
    // atomic rename that replaces a link rather than writing through it.
    Area::new(area.root.clone(), area.room(), true).write(&target, png).map_err(codes::area)
}

/// Remove the renders of document `handle` (it closed).
pub(crate) fn forget(area: &Area, handle: &str) {
    if let Ok(Some(root)) = root(area) {
        remove_doc(&root.join(handle));
    }
}

/// Remove the renders of every document in `area` whose handle is not in
/// `open`: documents closed, released, or opened by an earlier run.
pub(crate) fn prune(area: &Area, open: &[String]) {
    let Ok(Some(root)) = root(area) else { return };
    let Ok(docs) = std::fs::read_dir(&root) else { return };
    for doc in docs.flatten() {
        if !open.iter().any(|h| doc.file_name().to_str() == Some(h.as_str())) {
            remove_doc(&doc.path());
        }
    }
}

/// A document's cache folder and the regular files in it; a link there is
/// removed itself, never followed.
fn remove_doc(dir: &Path) {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => {
            if let Ok(files) = std::fs::read_dir(dir) {
                for file in files.flatten() {
                    if std::fs::symlink_metadata(file.path()).is_ok_and(|m| !m.is_dir()) {
                        let _ = std::fs::remove_file(file.path());
                    }
                }
            }
            let _ = std::fs::remove_dir(dir);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(dir);
        }
        Err(_) => {}
    }
}
