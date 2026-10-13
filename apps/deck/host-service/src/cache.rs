//! The render cache (SERVICE.md "Reading and drawing", "Cache"), as PDF
//! Tools' (decision 6): `deck.view` writes each slide it draws, and
//! `deck.layouts` each layout picture, to `.cache/slides/<doc>/` in the
//! caller's storage, and `deck.cover` a deck's first slide to
//! `.cache/covers/` ([`crate::covers`]).
//!
//! - At most [`MAX_BYTES`] (16 MiB) and [`MAX_FILES`] renders per storage,
//!   slide renders, layout pictures and covers together, at most
//!   [`MAX_COVERS`] of them covers. When the cache needs room, slide renders
//!   (and layout pictures) go before covers, each oldest first (by when they
//!   were written).
//! - The file cap is the service's own: an app's storage holds at most 256
//!   entries for the app's own writes (makepad's
//!   `splash_storage::MAX_ENTRIES`), and the library, its pictures and its
//!   exports need room too.
//! - A document's renders go when it closes ([`forget`]) and when a later
//!   open finds it gone ([`prune`]); both reach `.cache/slides` alone, so
//!   covers stay across closes and runs. A render of an older revision of
//!   the same slide, size and mode goes when the new one is written
//!   ([`drop_older`]).
//! - Before any write of the service would fail for room (`storage_full:`),
//!   the whole cache, covers too, is cleared ([`make_room`]).
//! - The cache is the service's: a render replaces a stale render at the
//!   same name. It never walks or writes through a link: a cache folder that
//!   is a link is refused, and only regular files under plain folders are
//!   counted or removed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use octosense_engine_area::Area;

use crate::codes::{self, invalid, too_large};

/// The cache's slide renders and layout pictures, relative to the storage.
pub(crate) const DIR: &str = ".cache/slides";
/// Its covers, relative to the storage.
pub(crate) const COVERS: &str = ".cache/covers";
/// The most bytes of renders one storage keeps.
pub(crate) const MAX_BYTES: u64 = 16 << 20;
/// The most renders one storage keeps.
pub(crate) const MAX_FILES: usize = 64;
/// The most covers among them.
pub(crate) const MAX_COVERS: usize = 32;

/// Where a render named `file` of document `handle` goes.
pub(crate) fn render_path(handle: &str, file: &str) -> String {
    format!("{DIR}/{handle}/{file}")
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

/// The cache's own folder in `area`, `.cache`, when it is there and plain.
fn cache_dir(area: &Area) -> Result<Option<PathBuf>, String> {
    let dot = area.root.join(".cache");
    Ok(plain_dir(&dot, ".cache")?.then_some(dot))
}

/// The slide renders' folder in `area`, when it is there and plain all the
/// way. [`forget`] and [`prune`] reach nothing else.
fn root(area: &Area) -> Result<Option<PathBuf>, String> {
    let Some(dot) = cache_dir(area)? else { return Ok(None) };
    let slides = dot.join("slides");
    Ok(plain_dir(&slides, DIR)?.then_some(slides))
}

/// The covers' folder in `area`, when it is there and plain all the way.
pub(crate) fn covers_root(area: &Area) -> Result<Option<PathBuf>, String> {
    let Some(dot) = cache_dir(area)? else { return Ok(None) };
    let covers = dot.join("covers");
    Ok(plain_dir(&covers, COVERS)?.then_some(covers))
}

struct Render {
    path: PathBuf,
    len: u64,
    written: SystemTime,
    /// A cover (`.cache/covers`), not a slide render.
    cover: bool,
}

/// The regular files directly in `dir`, as renders.
fn files_in(dir: &Path, cover: bool, out: &mut Vec<Render>) {
    let Ok(files) = std::fs::read_dir(dir) else { return };
    for file in files.flatten() {
        let Ok(meta) = std::fs::symlink_metadata(file.path()) else { continue };
        if meta.is_file() {
            out.push(Render { path: file.path(), len: meta.len(), written: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), cover });
        }
    }
}

/// Every render in the cache folder `dot` (`.cache`): regular files in the
/// documents' plain folders under `slides/`, and in a plain `covers/`.
/// Links and anything else are neither counted nor followed.
fn renders(dot: &Path) -> Vec<Render> {
    let mut out = Vec::new();
    let plain = |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
    let slides = dot.join("slides");
    if plain(&slides) {
        if let Ok(docs) = std::fs::read_dir(&slides) {
            for doc in docs.flatten() {
                if plain(&doc.path()) {
                    files_in(&doc.path(), false, &mut out);
                }
            }
        }
    }
    let covers = dot.join("covers");
    if plain(&covers) {
        files_in(&covers, true, &mut out);
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
    pub covers: usize,
}

pub(crate) const LIMITS: Limits = Limits { bytes: MAX_BYTES, files: MAX_FILES, covers: MAX_COVERS };

/// Make room in the cache folder `dot` (`.cache`) for one more render of
/// `incoming` bytes at `keep` (which it replaces, so it does not count):
/// covers past their cap go first, oldest first (a new cover at `keep`
/// counts); then slide renders before covers, each oldest first, until the
/// cache with it fits `limits`.
pub(crate) fn evict(dot: &Path, keep: &Path, incoming: u64, limits: Limits) {
    let keeps_cover = keep.parent() == Some(dot.join("covers").as_path());
    let mut all: Vec<Render> = renders(dot).into_iter().filter(|r| r.path != keep).collect();
    // Slide renders before covers, each oldest first.
    all.sort_by_key(|r| (r.cover, r.written));
    let cover_room = limits.covers.saturating_sub(usize::from(keeps_cover));
    let mut covers = all.iter().filter(|r| r.cover).count();
    let mut left: Vec<Render> = Vec::with_capacity(all.len());
    for old in all {
        if old.cover && covers > cover_room && remove_render(&old, keep) {
            covers -= 1;
        } else {
            left.push(old);
        }
    }
    let mut bytes: u64 = left.iter().map(|r| r.len).sum();
    let mut files = left.len();
    for old in &left {
        if bytes.saturating_add(incoming) <= limits.bytes && files < limits.files {
            break;
        }
        if remove_render(old, keep) {
            bytes = bytes.saturating_sub(old.len);
            files -= 1;
        }
    }
}

/// Remove one render, and a slide render's document folder once it is
/// empty (not the folder `keep` goes in).
fn remove_render(old: &Render, keep: &Path) -> bool {
    if std::fs::remove_file(&old.path).is_err() {
        return false;
    }
    if let Some(dir) = old.path.parent().filter(|d| !old.cover && Some(*d) != keep.parent()) {
        drop_if_empty(dir);
    }
    true
}

/// Clear the whole cache, slide renders and covers; the bytes it held.
fn clear(area: &Area) -> u64 {
    let Ok(Some(dot)) = cache_dir(area) else { return 0 };
    let mut freed = 0;
    for old in renders(&dot) {
        if std::fs::remove_file(&old.path).is_ok() {
            freed += old.len;
        }
        if let Some(dir) = old.path.parent().filter(|_| !old.cover) {
            drop_if_empty(dir);
        }
    }
    freed
}

/// A render kept at `rel` in the cache: its width and height from its PNG
/// header, if it is a regular file under plain folders whose header reads as
/// a PNG within the cache's bounds; `None` otherwise (draw it).
pub(crate) fn kept_png(area: &Area, rel: &str) -> Option<(u64, u64)> {
    cache_dir(area).ok()??;
    let path = area.root.join(rel);
    if let Some(dir) = path.parent() {
        if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
            return None;
        }
    }
    let meta = std::fs::symlink_metadata(&path).ok().filter(|m| m.is_file() && m.len() <= MAX_BYTES)?;
    let mut head = [0u8; 24];
    std::fs::File::open(&path).ok()?.read_exact(&mut head).ok()?;
    if meta.len() < 33 || &head[..8] != b"\x89PNG\r\n\x1a\n" || &head[12..16] != b"IHDR" {
        return None;
    }
    let be = |at: usize| u64::from(u32::from_be_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]));
    Some((be(16), be(20))).filter(|(w, h)| *w > 0 && *h > 0)
}

/// Remove the files in the plain folder `dir` whose names start with
/// `prefix` but not with `current`: a render's or cover's older versions,
/// never another's.
fn drop_matching(dir: &Path, prefix: &str, current: &str) {
    let Ok(files) = std::fs::read_dir(dir) else { return };
    for file in files.flatten() {
        let name = file.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with(prefix) && !name.starts_with(current) && std::fs::symlink_metadata(file.path()).is_ok_and(|m| m.is_file()) {
            let _ = std::fs::remove_file(file.path());
        }
    }
}

/// Remove the covers in `.cache/covers` whose names start with `prefix`
/// (one deck's covers, [`crate::covers`]) but not with `current` (its
/// current version's): a deck's older covers, never another deck's.
pub(crate) fn drop_covers(area: &Area, prefix: &str, current: &str) {
    if let Ok(Some(dir)) = covers_root(area) {
        drop_matching(&dir, prefix, current);
    }
}

/// Remove document `handle`'s renders whose names start with `prefix` (one
/// slide, size and mode) but not with `current` (its current revision's).
pub(crate) fn drop_older(area: &Area, handle: &str, prefix: &str, current: &str) {
    let Ok(Some(root)) = root(area) else { return };
    let dir = root.join(handle);
    if std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
        drop_matching(&dir, prefix, current);
    }
}

/// Before a write of `need` bytes in `area`: when the storage has less room
/// left, clear the render cache first and give the call the room it freed.
/// The write's own check then answers `storage_full:` if that was not
/// enough.
pub(crate) fn make_room(area: &Area, need: u64) {
    if area.room().is_some_and(|room| need > room) {
        area.credit(clear(area));
    }
}

/// The size of the regular file at `path` (0: none, or a link).
pub(crate) fn regular_len(path: &Path) -> u64 {
    std::fs::symlink_metadata(path).ok().filter(|m| m.is_file()).map_or(0, |m| m.len())
}

/// Whether the render at `rel` is still the `len` bytes this service wrote:
/// the cache may have been cleared or trimmed since.
pub(crate) fn still_there(area: &Area, rel: &str, len: u64) -> bool {
    std::fs::symlink_metadata(area.root.join(rel)).is_ok_and(|m| m.is_file() && m.len() == len)
}

/// Write `png` to the cache at `rel` (under [`DIR`] or [`COVERS`]), within
/// the cache's bounds and the storage's room.
pub(crate) fn store(area: &Area, rel: &str, png: &[u8], m: &str) -> Result<(), String> {
    let len = png.len() as u64;
    if len > MAX_BYTES {
        return Err(too_large(format!("{m}: this picture is {} MB, more than the {} MB render cache holds: draw it smaller", len >> 20, MAX_BYTES >> 20)));
    }
    let target = crate::inside(area, rel, "path", m)?;
    // The folders on the way must be plain: a link would have the cache
    // count, trim or write files somewhere else in the storage.
    root(area)?;
    covers_root(area)?;
    if let Some(dir) = target.parent() {
        plain_dir(dir, &area.shown(dir))?;
    }
    if let Some(dot) = cache_dir(area)? {
        evict(&dot, &target, len, LIMITS);
    }
    make_room(area, len.saturating_sub(regular_len(&target)));
    // The service's own cache: a stale render at the same name goes, by an
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
