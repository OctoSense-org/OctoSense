//! Covers (SERVICE.md "Reading", `pdf.cover {path, dpi?}`): page 1 of a
//! PDF in the caller's storage, drawn once into the render cache's
//! `.cache/covers/` and kept there across closes and runs, so a library
//! shows its covers without opening each PDF on every start.
//!
//! - **The key.** A cover's file name is
//!   `<name>-<size>-<modified>@<dpi>.png`: `<name>` a hash of the PDF's
//!   storage-relative name (16 lowercase hex digits), `<size>` its bytes and
//!   `<modified>` its modification time in nanoseconds since the epoch, all
//!   digits. A PDF that changes (saved, or replaced under the same name)
//!   gets another key, and writing its new cover removes the covers of its
//!   other versions by the exact prefix `<name>-`, which no other PDF's
//!   covers carry ([`cache::drop_covers`]); a cover at another dpi of the
//!   same version stays.
//! - **A kept cover** answers with no engine work: the file's metadata and
//!   its PNG header ([`cache::kept_png`]).
//! - **A new cover** is drawn as `pdf.info {path}` reads a file: a fresh
//!   session with JavaScript off ([`crate::session`]), `doc_open` (`code`,
//!   run with scripts off) and `page_render` (`safe`), and no open
//!   document: no handle, nothing in the open-document table, and nothing
//!   counted toward a caller's 8. Its one write is the service's own cache
//!   file, through [`cache::store`]'s bounds and link checks.
//! - **Caps.** 24 to 150 dpi (default 48), at most 16 megapixels; covers
//!   share the render cache's 16 MiB and 64 files, at most 32 of them, and
//!   page renders go before covers when the cache needs room ([`cache`]).
//!   The whole cache, covers included, is cleared before a write would fail
//!   with `storage_full:`.

use std::path::{Component, Path};
use std::time::UNIX_EPOCH;

use pdfcraft_automation::Content;
use serde_json::{json, Value as Json};

use crate::args;
use crate::cache;
use crate::codes::{self, fail, Code};
use crate::reading;
use crate::Ctx;

/// `pdf.cover`'s resolutions (SERVICE.md: 24 to 150, default 48).
pub(crate) const DPI_RANGE: (u32, u32) = (24, 150);
pub(crate) const DEFAULT_DPI: u32 = 48;

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

/// `pdf.cover {path, dpi?}`.
pub(crate) fn cover(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.cover";
    args::only(a, &["path", "dpi"], M)?;
    let path = args::need_str(a, "path", M)?;
    let dpi = args::dpi(a, "dpi", M, DEFAULT_DPI, DPI_RANGE)?;
    crate::checked_input(cx.area, path, "path", M)?;
    let file = crate::contained(cx.area, path, "path", M)?;
    let meta = std::fs::metadata(&file).map_err(|e| codes::invalid(format!("{path}: {e}")))?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    let prefix = format!("{}-", name_key(path));
    let current = format!("{prefix}{}-{modified}@", meta.len());
    let rel = format!("{}/{current}{dpi}.png", cache::COVERS);
    if let Some((width, height)) = cache::kept_png(cx.area, &rel) {
        return Ok(json!({ "path": rel, "width": width, "height": height, "dpi": dpi }));
    }
    let mut engine = crate::session(cx.area)?;
    let (doc, pages) = crate::open(&mut engine, path)?;
    if pages == 0 {
        return Err(fail(Code::Damaged, format!("{path} has no pages")));
    }
    let info = crate::run(&mut engine, "doc_info", &json!({ "doc": doc })).map_err(|e| codes::unreadable(&e))?;
    let first = crate::docs::sizes_of(&info).first().copied().ok_or_else(|| fail(Code::Damaged, format!("{path} has no pages")))?;
    reading::check_pixels(first, dpi, 1)?;
    let contents = engine.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": dpi })).map_err(|e| codes::refused(&e))?;
    let Some(Content::Png { data, width, height }) = contents.into_iter().find(|c| matches!(c, Content::Png { .. })) else {
        return Err(fail(Code::Damaged, format!("{path}'s first page could not be rendered")));
    };
    // This PDF's covers of other versions are stale: they go before the
    // new one takes room.
    cache::drop_covers(cx.area, &prefix, &current);
    cache::store(cx.area, &rel, &data)?;
    Ok(json!({ "path": rel, "width": width, "height": height, "dpi": dpi }))
}
