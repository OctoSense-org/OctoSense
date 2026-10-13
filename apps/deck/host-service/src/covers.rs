//! Covers (SERVICE.md "Reading and drawing", `deck.cover {path, width?}`):
//! slide 1 of a deck in the caller's storage, drawn once into the render
//! cache's `.cache/covers/` and kept there across closes and runs, so a
//! library shows its covers without opening each deck on every start, as
//! `pdf.cover` does (`apps/pdf/host-service/src/covers.rs`).
//!
//! - **The key.** A cover's file name is
//!   `<name>-<size>-<modified>@<width>.png`: `<name>` a hash of the deck's
//!   storage-relative name (16 lowercase hex digits), `<size>` its bytes and
//!   `<modified>` its modification time in nanoseconds since the epoch. A
//!   deck that changes (saved, or replaced under the same name) gets another
//!   key, and writing its new cover removes the covers of its other
//!   versions by the exact prefix `<name>-`, which no other deck's covers
//!   carry; a cover at another width of the same version stays.
//! - **A kept cover** answers with no engine work: the file's metadata and
//!   its PNG header ([`cache::kept_png`]).
//! - **A new cover** reads the file as `deck.open` does, through the import
//!   guard before the engine sees a byte (`crate::read_guarded`), within the
//!   door's ceilings on what a session holds, and draws slide 1 with
//!   deckcraft-render's `render_slide` (no command, no open document: no
//!   handle, nothing counted toward a caller's 8). Its one write is the
//!   service's own cache file, through [`cache::store`]'s bounds and link
//!   checks.
//! - **Caps.** 48 to 3840 pixels wide (default 320), the longest side at
//!   most 4096 and at most 16 megapixels; covers share the render cache's
//!   16 MiB and 64 files, at most 32 of them, and slide renders go before
//!   covers when the cache needs room ([`cache`]).

use std::path::{Component, Path};
use std::time::UNIX_EPOCH;

use deckcraft_engine::cmd::file as engine_file;
use deckcraft_engine::render::{self as render, RenderOpts};
use serde_json::{json, Value as Json};

use crate::args;
use crate::cache;
use crate::codes::{fail, invalid, Code};
use crate::reading;
use crate::Ctx;

/// A cover's default width, in pixels.
pub(crate) const DEFAULT_WIDTH: i64 = 320;

/// The key's first part for the storage-relative name `name`: FNV-1a over
/// its normal components joined with `/`, as 16 lowercase hex digits.
pub(crate) fn name_key(name: &str) -> String {
    let joined: Vec<String> = Path::new(name)
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in joined.join("/").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// `deck.cover {path, width?}`.
pub(crate) fn cover(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.cover";
    args::only(a, &["path", "width"], M)?;
    let path = args::need_str(a, "path", M)?;
    let width = args::opt_int_in(a, "width", M, reading::WIDTH.0, reading::WIDTH.1)?.unwrap_or(DEFAULT_WIDTH) as u64;
    let (file, len) = crate::input(cx.area, path, "path", M)?;
    let meta = std::fs::metadata(&file).map_err(|e| invalid(format!("{path}: {e}")))?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    let prefix = format!("{}-", name_key(path));
    let current = format!("{prefix}{len}-{modified}@");
    let rel = format!("{}/{current}{width}.png", cache::COVERS);
    if let Some((w, h)) = cache::kept_png(cx.area, &rel) {
        return Ok(json!({ "path": rel, "width": w, "height": h }));
    }
    let (name, deck, _) = crate::read_guarded(cx.area, path, "path", M)?;
    let mut session = deckcraft_engine::Session::new();
    engine_file::add_opened(&mut session, &name, None, deck);
    if let Some(why) = crate::Held::of(&session).over() {
        return Err(crate::too_large_deck(why));
    }
    let deck = session.doc().map_err(|_| fail(Code::Damaged, "the engine opened no deck"))?.doc.clone();
    if deck.slides.is_empty() {
        return Err(fail(Code::Damaged, format!("{path} has no slides")));
    }
    let (scale, _, _) = reading::size_for(&deck, width, M)?;
    let img = render::render_slide(&deck, 0, &RenderOpts { scale, ..Default::default() });
    let png = img.to_png();
    // This deck's covers of other versions are stale: they go before the new
    // one takes room.
    cache::drop_covers(cx.area, &prefix, &current);
    cache::store(cx.area, &rel, &png, M)?;
    Ok(json!({ "path": rel, "width": img.width, "height": img.height }))
}
