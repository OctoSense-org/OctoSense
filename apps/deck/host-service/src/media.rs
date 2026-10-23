//! Playback (SERVICE.md "Playback", maintainer approved 13 Oct 2026):
//! `deck.media {doc, slide, id}` hands Quick Deck a video or audio clip of
//! an open document as a file in its storage, which the app plays with the
//! runtime's `Video` widget (the operating system's own player).
//!
//! **Bytes only.** The clip's bytes come from the open document's model,
//! the item the shape's `MediaClip` names (`Presentation::media`), exactly
//! as the deck stores them, and are written as they are. No engine command
//! runs and no engine function is called: nothing here decodes, probes or
//! transcodes a clip. deckcraft's decoders (`deckcraft-media`) are reached
//! only through `media.info`, `media.posterFrame`, `insert.audio` and
//! `insert.video`, which stay held (#448). `kind`, `autoplay`, `loop` and
//! `poster` are the model's own fields, and `ext` comes from the deck's own
//! name for the clip or its declared content type ([`ext_of`]), never from
//! the bytes.
//!
//! **The media cache**, beside the render cache ([`crate::cache`]): each
//! clip at `.cache/media/<SHA-256 of its bytes>.<ext>`, at most
//! [`MAX_BYTES`] (32 MiB) and [`MAX_FILES`] clips per storage. When it needs
//! room, clips no open document uses go first, and the ones open documents
//! use only when that is not enough; each, the clip used longest ago first
//! (written, or handed out again by a later call). A clip already there with
//! the same bytes is handed out again, not written twice. A document's clips
//! go when it closes or is released, with its renders ([`forget`]), unless
//! another document open in the same storage uses them, and any a released
//! document left behind go when a document next opens there ([`prune`]). If
//! a clip doesn't fit what is left of the storage, the render cache and then
//! the media cache are cleared before the write; a clip that still doesn't
//! fit is `storage_full:`, before anything is staged, and so is one larger
//! than the whole media cache, at once, before it is even hashed, with
//! nothing cleared, since even an empty cache can't hold it.
//!
//! **Writes** are staged inside the storage and moved into place by a link
//! that refuses anything already there (`octosense_engine_area::Stage`), so
//! a write only ever creates a file. The folders on the way must be plain: a
//! link there is refused, and only regular files directly in `.cache/media`
//! are counted, handed out or removed. A clip's path is the checked folder
//! joined with a name the service makes itself (a hex digest and an
//! extension from [`EXTS`]), never resolved through what is there. Whatever
//! else sits at that name (a link, never followed, or a file that doesn't
//! hold the clip's bytes) is a stale cache entry: it goes, as an evicted
//! clip would, and the clip is written anew.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use deckcraft_model::{MediaClip, MediaItem, Presentation, ShapeId, ShapeKind};
use octosense_engine_area::Area;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};

use crate::args;
use crate::cache;
use crate::codes::{self, fail, invalid, Code};
use crate::docs;
use crate::Ctx;

/// The media cache's folder, relative to the storage.
pub(crate) const DIR: &str = ".cache/media";
/// The most bytes of clips one storage keeps.
pub(crate) const MAX_BYTES: u64 = 32 << 20;
/// The most clips one storage keeps.
pub(crate) const MAX_FILES: usize = 8;
/// The extensions a clip is written with (SERVICE.md "Playback").
pub(crate) const EXTS: [&str; 7] = ["mp4", "m4v", "mov", "mp3", "m4a", "aac", "wav"];
/// `storage_full:`'s words for a clip (SERVICE.md "Playback").
pub(crate) const FULL: &str = "There isn't room in Quick Deck's storage to play this clip.";

/// `deck.media {doc, slide, id}`: `{path, kind, ext, bytes, autoplay, loop,
/// poster}`.
pub(crate) fn media(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.media";
    args::only(a, &["doc", "slide", "id"], M)?;
    let slide = args::id(a, "slide", M)?;
    let id = args::id(a, "id", M)?;
    // The clips open documents use, read before the call holds the table:
    // eviction spares them while it can.
    let in_use = docs::clips_in_use(&docs::owner(cx.area)).unwrap_or_default();
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(slide)?;
        let deck = doc.deck()?;
        let clip = match deck.slides[i].shape(ShapeId(id)).map(|sh| &sh.kind) {
            Some(ShapeKind::Media(clip)) => clip,
            Some(_) => return Err(fail(Code::UnknownId, format!("shape {id} on slide {slide} is not a video or audio clip"))),
            None => return Err(fail(Code::UnknownId, format!("slide {slide} has no shape {id} (an undo may have removed it)"))),
        };
        let item = clip_bytes(&deck, clip)?;
        let ext = ext_of(item).ok_or_else(|| {
            fail(Code::Unsupported, format!("this clip is not a kind Quick Deck plays ({}), by its name or by its declared type", EXTS.join(", ")))
        })?;
        let rel = write(cx.area, &item.data, ext, &in_use, M)?;
        if !doc.clips.contains(&rel) {
            doc.clips.push(rel.clone());
        }
        Ok(json!({
            "path": rel,
            "kind": if clip.video { "video" } else { "audio" },
            "ext": ext,
            "bytes": item.data.len(),
            "autoplay": clip.autoplay,
            "loop": clip.loop_play,
            "poster": clip.poster.and_then(|p| deck.media(p)).is_some_and(|p| !p.data.is_empty()),
        }))
    })
}

/// The deck's item holding `clip`'s bytes, or `unsupported:` when the deck
/// holds none: a clip linked to a file elsewhere, which the service never
/// follows, or one whose item is missing or empty.
fn clip_bytes<'a>(deck: &'a Presentation, clip: &MediaClip) -> Result<&'a MediaItem, String> {
    match deck.media(clip.media) {
        Some(item) if !item.data.is_empty() => Ok(item),
        Some(item) if item.link.is_some() => Err(fail(Code::Unsupported, "this clip is linked to a file outside the deck, which Quick Deck doesn't open")),
        _ => Err(fail(Code::Unsupported, "the deck holds no bytes for this clip")),
    }
}

/// The extension a clip is written with: the deck's own for it (its item's
/// name's), when that is one of [`EXTS`], else the one its declared content
/// type names; `None` for neither. Never read from the bytes.
pub(crate) fn ext_of(item: &MediaItem) -> Option<&'static str> {
    let own = Path::new(&item.name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if let Some(ext) = EXTS.iter().copied().find(|e| *e == own) {
        return Some(ext);
    }
    let declared = item.content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    Some(match declared.as_str() {
        "video/mp4" => "mp4",
        "video/x-m4v" => "m4v",
        "video/quicktime" => "mov",
        "audio/mpeg" | "audio/mp3" | "audio/x-mp3" | "audio/mpeg3" | "audio/x-mpeg-3" => "mp3",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/aac" | "audio/x-aac" | "audio/aacp" => "aac",
        "audio/wav" | "audio/wave" | "audio/x-wav" | "audio/vnd.wave" => "wav",
        _ => return None,
    })
}

/// `storage_full:`, in the contract's words.
fn full() -> String {
    fail(Code::StorageFull, FULL)
}

/// Write `bytes` to the media cache as `<SHA-256>.<ext>` (`ext` one of
/// [`EXTS`]), or find them there already: the storage-relative path.
/// `in_use` are the clips open documents use, which eviction spares while it
/// can.
fn write(area: &Area, bytes: &[u8], ext: &'static str, in_use: &[String], m: &str) -> Result<String, String> {
    let len = bytes.len() as u64;
    // Larger than the whole media cache, it can be neither there nor written:
    // refused before anything else, hashing included.
    if len > MAX_BYTES {
        return Err(full());
    }
    // The path is built from the checked folder and the service's own name,
    // so nothing at that name is followed, a link included.
    let (dir, _) = folder(area)?;
    let name = format!("{:x}.{ext}", Sha256::digest(bytes));
    let target = dir.join(&name);
    let rel = format!("{DIR}/{name}");
    if reuse(&target, bytes) {
        return Ok(rel);
    }
    // Anything else at the clip's own name is a stale entry: it goes (a link
    // itself, never what it points to), and the clip is written anew.
    match std::fs::symlink_metadata(&target) {
        Ok(meta) if meta.is_dir() => return Err(invalid(format!("{m}: `{rel}` is a folder in the media cache"))),
        Ok(meta) => {
            std::fs::remove_file(&target).map_err(|e| codes::io(m, &e))?;
            if meta.is_file() {
                area.credit(meta.len());
            }
        }
        Err(_) => {}
    }
    area.credit(evict(&dir, &target, len, in_use));
    // Not room enough in the storage: the render cache goes first, then the
    // media cache; a clip that still doesn't fit is refused before anything
    // is staged.
    cache::make_room(area, len);
    if area.room().is_some_and(|room| len > room) {
        area.credit(clear(area));
    }
    if area.room().is_some_and(|room| len > room) {
        return Err(full());
    }
    let create = Area::new(area.root.clone(), area.room(), false);
    let stage = crate::stage(&create)?;
    let staged = stage.path("clip");
    std::fs::write(&staged, bytes).map_err(|e| match e.kind() {
        std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded => full(),
        _ => codes::io(m, &e),
    })?;
    stage.commit(&[(staged, target)]).map_err(|e| if octosense_engine_area::no_room(&e) || e.contains("No space left") { full() } else { codes::area(e) })?;
    Ok(rel)
}

/// Whether `path` is a plain folder (`Ok(false)`: not there); a link or a
/// file in the way is refused.
fn plain_dir(path: &Path, shown: &str) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(true),
        Ok(_) => Err(invalid(format!("the media cache folder `{shown}` is not a plain folder"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(invalid(format!("the media cache folder `{shown}`: {e}"))),
    }
}

/// The media cache's folder in `area`, `.cache/media`, and whether it is
/// there: refused when it, or `.cache`, is there but isn't a plain folder (a
/// link, a file).
fn folder(area: &Area) -> Result<(PathBuf, bool), String> {
    let dot = area.root.join(".cache");
    let dir = dot.join("media");
    let there = plain_dir(&dot, ".cache")? && plain_dir(&dir, DIR)?;
    Ok((dir, there))
}

/// The media cache's folder in `area`, when it is there and plain all the
/// way.
fn root(area: &Area) -> Result<Option<PathBuf>, String> {
    let (dir, there) = folder(area)?;
    Ok(there.then_some(dir))
}

/// One clip in the media cache.
struct Kept {
    path: PathBuf,
    len: u64,
    /// When it was written, or last handed out again ([`reuse`]).
    used: SystemTime,
}

/// The regular files directly in the media cache's folder `dir`. Links and
/// anything else are neither counted nor followed.
fn kept(dir: &Path) -> Vec<Kept> {
    let mut out = Vec::new();
    for file in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(meta) = std::fs::symlink_metadata(file.path()) else { continue };
        if meta.is_file() {
            out.push(Kept { path: file.path(), len: meta.len(), used: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH) });
        }
    }
    out
}

/// Whether the entry at `path` is a regular file, never a link, holding
/// exactly `bytes`. If it is, it counts as just used, so it goes last when
/// the cache needs room.
fn reuse(path: &Path, bytes: &[u8]) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return false };
    if !meta.is_file() || meta.len() != bytes.len() as u64 {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    // The file opened is the one looked at, not a link raced into its place.
    if !same_file(&meta, &file) {
        return false;
    }
    let mut buf = vec![0u8; 1 << 16];
    let mut at = 0usize;
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if bytes.get(at..at + n) != Some(&buf[..n]) {
                    return false;
                }
                at += n;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    if at != bytes.len() {
        return false;
    }
    let _ = file.set_modified(SystemTime::now());
    true
}

#[cfg(unix)]
fn same_file(looked: &std::fs::Metadata, file: &std::fs::File) -> bool {
    use std::os::unix::fs::MetadataExt;
    file.metadata().is_ok_and(|m| m.dev() == looked.dev() && m.ino() == looked.ino())
}

#[cfg(not(unix))]
fn same_file(_: &std::fs::Metadata, file: &std::fs::File) -> bool {
    file.metadata().is_ok_and(|m| m.is_file())
}

/// Make room in the media cache's folder `dir` for a clip of `incoming`
/// bytes at `keep` (which does not count), until the cache with it holds at
/// most [`MAX_BYTES`] and [`MAX_FILES`]: clips no open document uses go
/// first, and those open documents use (`in_use`) only when that is not
/// enough; each, the clip used longest ago first. The bytes it freed.
fn evict(dir: &Path, keep: &Path, incoming: u64, in_use: &[String]) -> u64 {
    let mut all: Vec<Kept> = kept(dir).into_iter().filter(|k| k.path != keep).collect();
    all.sort_by_cached_key(|k| (rel_of(&k.path).is_some_and(|rel| in_use.contains(&rel)), k.used));
    let mut bytes: u64 = all.iter().map(|k| k.len).sum();
    let mut files = all.len();
    let mut freed = 0;
    for old in &all {
        if bytes.saturating_add(incoming) <= MAX_BYTES && files < MAX_FILES {
            break;
        }
        if std::fs::remove_file(&old.path).is_ok() {
            bytes = bytes.saturating_sub(old.len);
            files -= 1;
            freed += old.len;
        }
    }
    freed
}

/// Clear the media cache in `area`: the bytes it held.
fn clear(area: &Area) -> u64 {
    let Ok(Some(dir)) = root(area) else { return 0 };
    kept(&dir).iter().filter(|k| std::fs::remove_file(&k.path).is_ok()).map(|k| k.len).sum()
}

/// A clip's storage-relative path, as [`media`] answers it and documents
/// keep it.
fn rel_of(path: &Path) -> Option<String> {
    path.file_name().and_then(|n| n.to_str()).map(|n| format!("{DIR}/{n}"))
}

/// Remove the clips a document that went used (`gone`), but not those
/// another document open in the same storage uses (`used`).
pub(crate) fn forget(area: &Area, gone: &[String], used: &[String]) {
    let Ok(Some(dir)) = root(area) else { return };
    for k in kept(&dir) {
        if rel_of(&k.path).is_some_and(|rel| gone.contains(&rel) && !used.contains(&rel)) {
            let _ = std::fs::remove_file(&k.path);
        }
    }
}

/// Remove every clip in `area`'s media cache that no document open there
/// uses (`used`): clips of documents released without a close, by an
/// earlier run.
pub(crate) fn prune(area: &Area, used: &[String]) {
    let Ok(Some(dir)) = root(area) else { return };
    for k in kept(&dir) {
        if !rel_of(&k.path).is_some_and(|rel| used.contains(&rel)) {
            let _ = std::fs::remove_file(&k.path);
        }
    }
}
