//! `octosense-pdf-service` — the `pdf` host service (ADR 0013).
//!
//! pdfcraft's PDF engine through its own headless automation layer, in two
//! shapes:
//!
//! - **Files.** A call that names a `path` is a fresh session that opens
//!   the file, acts and closes: `info`, `text`, `render`, `merge`, `split`.
//!   The system agent's five `pdf.*` tools (`tools.json`) are these.
//! - **Open documents** (PDF Tools v2, `apps/pdftools/design/SERVICE.md`):
//!   `open` keeps the engine's document, its edits and undo history,
//!   between calls under a handle that later calls name as `doc`; `close`,
//!   `state`, `page`, `find`, `lines`, `comments`, `comment`, `fields`,
//!   `fill`, `fill_sign`, `pages`, `edit_text`, `undo`, `redo`, `save` and
//!   `export` work on it, and `info` and `text` take `{doc}` too. Who holds
//!   an open document, on which thread, and what releases it: [`docs`].
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]): an
//! app's storage for its own requests. Without a resolver a call works in
//! the legacy `<host dir>/pdf`. Every path is relative to the area and kept
//! inside it, first by this service's own check ([`contained`]) and then by
//! the engine's root-confined resolver, which refuses `..`, absolute paths
//! and link escapes before any I/O. What the engine writes lands in a
//! staging folder inside the area first and is moved into place all or
//! nothing ([`octosense_engine_area::Stage`]), within what is left of the
//! storage. A write never replaces a file, except an app's own foreground
//! call where it always could (`merge`, `split`, `render`), the service's
//! own render cache ([`cache`]), and SERVICE.md's incremental `save` over
//! the document's own file. Before a write would fail for room, the render
//! cache is cleared.
//!
//! **Scripts.** A reference inside a PDF (a remote go-to, a launch action,
//! an external file specification) is reported, never followed, and a
//! document's own scripts never run: every session turns the engine's
//! JavaScript off before it opens anything ([`session`]), so an XFA form's
//! initialize and calculate scripts (which the engine runs on opening by
//! default, in its sandbox), field scripts and document scripts stay
//! inert, through every method.
//!
//! **Engine commands** are called only with arguments this service builds
//! from reviewed values, never with a caller's object; every v2 method
//! refuses an argument it does not take ([`args::only`]). Each command's
//! class in `skill/safety.json` and the review of it are beside the
//! methods that call it ([`reading`], [`review`], [`change`]).
//!
//! **Errors** start with a stable code ([`codes`]): `not_found:`,
//! `damaged:`, `protected:`, `storage_full:`, `too_many_open:`,
//! `unknown_doc:`, `unsaved:`, `invalid:`, `too_large:`.
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

mod args;
mod cache;
mod change;
mod codes;
mod docs;
mod reading;
mod review;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{Area, Slot, Stage};
use pdfcraft_automation::{Automation, Content, ToolError};
use serde_json::{json, Value as Json};

use codes::{fail, invalid, Code};

/// The longest page edge `render` produces.
const MAX_RENDER_SIDE: u64 = 4096;
/// The largest PDF the service opens (bytes).
const MAX_PDF_BYTES: u64 = 128 << 20;
/// The most files one `merge` combines.
const MAX_MERGE_FILES: usize = 16;
/// The most files one `split` writes.
const MAX_SPLIT_FILES: u64 = 256;
/// The most pages one `text` call extracts.
const MAX_TEXT_PAGES: usize = args::MAX_PAGES;

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct PdfService;

/// Register the `pdf` service with App Hub's host-service registry, and
/// its listener for closing isolates: the open documents an isolate holds
/// go when it closes ([`docs`]).
pub fn register() {
    octosense_appstore::services::on_isolate_closed(docs::isolate_closed);
    register_host_service(Box::new(PdfService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/pdf` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `pdf.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.pdf` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for PdfService {
    fn family(&self) -> &'static str {
        "pdf"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        // App Hub calls this inline, holding its registry's lock: a panic
        // in the engine must not unwind through it (and poison every
        // service's registry), so it becomes this call's error.
        let isolate = reply.isolate_key();
        let answer = catch_unwind(AssertUnwindSafe(|| serve(&AREAS, &call, isolate)))
            .unwrap_or_else(|_| Err(fail(Code::Damaged, "the PDF engine failed unexpectedly")));
        reply.send(answer);
    }
}

/// One call: where it works, who made it, and from which isolate.
pub(crate) struct Ctx<'a> {
    pub area: &'a Area,
    pub app: &'a str,
    /// The requesting isolate's heap key (App Hub's `Replier::isolate_key`;
    /// a key no isolate has for an agent's or a component's call): what a
    /// document it opens is bound to ([`docs`]).
    pub isolate: usize,
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall, isolate: usize) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err(invalid("the pdf service serves system apps only"));
    }
    // Documents whose isolate closed during a call, or whose caller's
    // storage scope changed or went, go first, on every call ([`docs`]).
    docs::sweep();
    let area = areas.area(call, "pdf").map_err(|e| invalid(format!("this call has no folder to work in: {e}")))?;
    let cx = Ctx { area: &area, app: &call.app_id, isolate };
    dispatch_in(call.method(), &call.args, &cx).map(|answer| area.relative_json(answer)).map_err(|error| area.relative_text(&error))
}

/// A fresh engine session whose every file read and write is root-confined
/// to the area by the engine's own resolver, with the document's own
/// scripts off (the engine's Preferences ▸ JavaScript switch, on by
/// default).
fn session(area: &Area) -> Result<Automation, String> {
    let mut a = Automation::new().with_root(area.root.clone()).map_err(|e| invalid(format!("this call's folder: {e}")))?;
    run(&mut a, "js_enabled", &json!({"enabled": false})).map_err(|e| fail(Code::Damaged, format!("the engine's scripting could not be turned off: {e}")))?;
    if a.session().javascript() {
        return Err(fail(Code::Damaged, "the engine's scripting could not be turned off"));
    }
    Ok(a)
}

fn dispatch_in(method: &str, args: &Json, cx: &Ctx) -> Result<Json, String> {
    match method {
        "info" => info(args, cx),
        "text" => text(args, cx),
        "render" => render(args, cx.area),
        "merge" => merge(args, cx.area),
        "split" => split(args, cx.area),
        "open" => docs::open(args, cx),
        "close" => docs::close(args, cx),
        "state" => docs::state(args, cx),
        "page" => reading::page(args, cx),
        "find" => reading::find(args, cx),
        "lines" => reading::lines(args, cx),
        "comments" => review::comments(args, cx),
        "comment" => review::comment(args, cx),
        "fields" => review::fields(args, cx),
        "fill" => review::fill(args, cx),
        "fill_sign" => review::fill_sign(args, cx),
        "pages" => change::pages(args, cx),
        "edit_text" => change::edit_text(args, cx),
        "undo" => change::history(args, cx, false),
        "redo" => change::history(args, cx, true),
        "save" => change::save(args, cx),
        "export" => change::export(args, cx),
        other => Err(invalid(format!("pdf.{other} is not a method of the pdf service"))),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/pdf`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "pdf");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("pdf: {e}"))?;
    dispatch_in(method, args, &Ctx { area: &area, app: "os.fixture", isolate: 0 })
}

/// A staging path (`name` inside `stage`) as the engine names paths: relative
/// to its root, the area's.
fn staged(area: &Area, stage: &Stage<'_>, name: &str) -> Result<String, String> {
    let dir = stage.dir().strip_prefix(&area.root).map_err(|_| invalid("the staging folder is outside the area"))?;
    Ok(dir.join(name).to_string_lossy().into_owned())
}

/// A path strictly inside the area: relative, normal components only, and
/// the resolved deepest existing ancestor must stay under the area even
/// through symlinks (the stance the sheet service and the files host tools
/// take). The engine's resolver enforces the same bound again at I/O time.
fn contained(area: &Area, rel: &str, key: &str, m: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(invalid(format!("{m} needs `{key}`")));
    }
    let outside = || invalid(format!("{m}: `{key}` stays inside this app's storage"));
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(outside());
    }
    let root = &area.root;
    let joined = root.join(rel_path);
    let check_root = root.canonicalize().map_err(|e| invalid(format!("{m}: this call's folder: {e}")))?;
    let deepest = {
        let mut p = joined.clone();
        while !p.exists() {
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => break,
            }
        }
        p
    };
    let resolved = deepest.canonicalize().map_err(|_| outside())?;
    if !resolved.starts_with(&check_root) {
        return Err(outside());
    }
    Ok(joined)
}

/// A contained output path the call may write, refused before the engine
/// works when the area's rules would refuse it.
fn out_path(area: &Area, rel: &str, key: &str, m: &str) -> Result<PathBuf, String> {
    let out = contained(area, rel, key, m)?;
    area.check(&out, 0).map_err(codes::area)?;
    Ok(out)
}

/// The containment check plus the size cap, for a file a method will read.
fn checked_input(area: &Area, rel: &str, key: &str, m: &str) -> Result<(), String> {
    let path = contained(area, rel, key, m)?;
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(fail(Code::NotFound, format!("{rel} is not in this app's storage"))),
        Err(e) => return Err(invalid(format!("{rel}: {e}"))),
    };
    if !meta.is_file() {
        return Err(invalid(format!("{rel} is not a file")));
    }
    if meta.len() > MAX_PDF_BYTES {
        return Err(fail(Code::TooLarge, format!("{rel} is larger than the service reads ({} MB)", MAX_PDF_BYTES >> 20)));
    }
    Ok(())
}

/// Run one engine tool and keep its JSON result.
fn run(a: &mut Automation, tool: &str, args: &Json) -> Result<Json, ToolError> {
    for c in a.call(tool, args)? {
        if let Content::Json(v) = c {
            return Ok(v);
        }
    }
    Err(ToolError::Failed(format!("{tool}: the engine returned no result")))
}

/// Open `path` (already checked) and return the session's document id and
/// page count.
fn open(a: &mut Automation, path: &str) -> Result<(Json, u64), String> {
    let opened = run(a, "doc_open", &json!({ "path": path })).map_err(|e| codes::unreadable(&e))?;
    let pages = opened["pages"].as_u64().unwrap_or(0);
    Ok((opened["doc"].clone(), pages))
}

/// `{path}` or `{doc}`, not both: which one a reading method was given.
fn path_or_doc<'a>(args: &'a Json, m: &str) -> Result<Option<&'a str>, String> {
    match (args.get("path").filter(|v| !v.is_null()), args.get("doc").filter(|v| !v.is_null())) {
        (Some(_), Some(_)) => Err(invalid(format!("{m} takes `path` or `doc`, not both"))),
        (_, Some(_)) => Ok(None),
        _ => args::need_str(args, "path", m).map(Some),
    }
}

/// The `pages` a text call names, or all of a document of at most
/// [`MAX_TEXT_PAGES`].
fn text_pages(args: &Json, total: u64, m: &str) -> Result<Option<Vec<u64>>, String> {
    match args::opt_pages(args, "pages", m)? {
        Some(pages) => Ok(Some(pages)),
        None if total as usize > MAX_TEXT_PAGES => {
            Err(fail(Code::TooLarge, format!("{m}: the document has {total} pages; pass `pages` ({MAX_TEXT_PAGES} per call)")))
        }
        None => Ok(None),
    }
}

fn info(args: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.info";
    let Some(path) = path_or_doc(args, M)? else {
        args::only(args, &["doc"], M)?;
        return docs::with_doc(args, cx, M, |doc| {
            let mut out = doc.call("doc_info", json!({})).map_err(|e| codes::refused(&e))?;
            // The engine reads the file from wherever it last saved it (a
            // staging folder): the document's file is the service's to name.
            out["file"] = json!(doc.file);
            out["document"]["path"] = json!(doc.file);
            Ok(out)
        });
    };
    checked_input(cx.area, path, "path", M)?;
    let mut a = session(cx.area)?;
    let (doc, _) = open(&mut a, path)?;
    let mut out = run(&mut a, "doc_info", &json!({ "doc": doc })).map_err(|e| codes::unreadable(&e))?;
    out["file"] = json!(path);
    Ok(out)
}

fn text(args: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.text";
    let Some(path) = path_or_doc(args, M)? else {
        args::only(args, &["doc", "pages"], M)?;
        return docs::with_doc(args, cx, M, |doc| {
            let mut call = json!({});
            if let Some(pages) = text_pages(args, doc.sizes.len() as u64, M)? {
                call["pages"] = json!(pages);
            }
            doc.call("text_extract", call).map_err(|e| codes::refused(&e))
        });
    };
    checked_input(cx.area, path, "path", M)?;
    let mut a = session(cx.area)?;
    let (doc, total) = open(&mut a, path)?;
    let mut call = json!({ "doc": doc });
    if let Some(pages) = text_pages(args, total, M)? {
        call["pages"] = json!(pages);
    }
    run(&mut a, "text_extract", &call).map_err(|e| codes::refused(&e))
}

fn render(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.render";
    let path = args::need_str(args, "path", M)?;
    let out = args::need_str(args, "out", M)?;
    let page = args::positive(args, "page", M)?;
    let max_side = args::opt_int(args, "max_side", M)?.unwrap_or(1024).clamp(16, MAX_RENDER_SIDE as i64) as u64;
    checked_input(area, path, "path", M)?;
    let out_abs = out_path(area, out, "out", M)?;
    let mut a = session(area)?;
    let (doc, _) = open(&mut a, path)?;
    // The page's size in points decides the dpi that fits `max_side`.
    let inspected = run(&mut a, "doc_info", &json!({ "doc": doc })).map_err(|e| codes::unreadable(&e))?;
    let dims = &inspected["pages"][(page - 1) as usize];
    let side = dims["width"].as_f64().unwrap_or(0.0).max(dims["height"].as_f64().unwrap_or(0.0));
    if side <= 0.0 {
        return Err(invalid(format!("the document has no page {page}")));
    }
    let dpi = (72.0 * max_side as f64 / side).clamp(1.0, 600.0);
    let contents = a.call("page_render", &json!({ "doc": doc, "page": page, "dpi": dpi })).map_err(|e| codes::refused(&e))?;
    let Some(Content::Png { data, width, height }) = contents.into_iter().find(|c| matches!(c, Content::Png { .. })) else {
        return Err(fail(Code::Damaged, format!("page {page} could not be rendered")));
    };
    cache::make_room(area, (data.len() as u64).saturating_sub(cache::regular_len(&out_abs)));
    area.write(&out_abs, &data).map_err(codes::area)?;
    Ok(json!({ "out": out, "width": width, "height": height, "bytes": data.len() }))
}

/// A merge input's page range (`"1-4, 9"`): `None` for every page (none
/// given, `""` or `"all"`). Dashes a person types (`–`, `—`) read as `-`.
fn range(text: &str, m: &str) -> Result<Option<String>, String> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("all") {
        return Ok(None);
    }
    let t: String = t.chars().map(|c| if matches!(c, '–' | '—' | '‒' | '−') { '-' } else { c }).collect();
    if t.len() > 256 || !t.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '-' | ' ')) {
        return Err(invalid(format!("{m}: `pages` is a range of page numbers such as \"1-4, 9\"")));
    }
    Ok(Some(t))
}

fn merge(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.merge";
    let out = args::need_str(args, "out", M)?;
    let out_abs = out_path(area, out, "out", M)?;
    let items = args["paths"].as_array().ok_or_else(|| invalid(format!("{M}: `paths` is a list of files")))?;
    if items.len() < 2 || items.len() > MAX_MERGE_FILES {
        return Err(invalid(format!("{M}: `paths` is 2 to {MAX_MERGE_FILES} files")));
    }
    let mut paths: Vec<&str> = Vec::new();
    let mut ranges: Vec<Option<String>> = Vec::new();
    for item in items {
        let (path, pages) = match item {
            Json::String(p) => (p.as_str(), None),
            Json::Object(o) => {
                args::only(item, &["path", "pages"], M)?;
                let p = o.get("path").and_then(Json::as_str).ok_or_else(|| invalid(format!("{M}: every input names its `path`")))?;
                let pages = match o.get("pages") {
                    None | Some(Json::Null) => None,
                    Some(Json::String(r)) => range(r, M)?,
                    Some(_) => return Err(invalid(format!("{M}: `pages` is a range such as \"1-4, 9\""))),
                };
                (p, pages)
            }
            _ => return Err(invalid(format!("{M}: every input is a path or {{path, pages}}"))),
        };
        if path.is_empty() {
            return Err(invalid(format!("{M}: every input names a file")));
        }
        checked_input(area, path, "paths", M)?;
        paths.push(path);
        ranges.push(pages);
    }
    // The engine writes the merge itself: into a staging folder, then into
    // place under the area's rules.
    let stage = area.stage().map_err(codes::area)?;
    let mut a = session(area)?;
    let mut call = json!({ "paths": paths, "out": staged(area, &stage, "merged.pdf")?, "open": false });
    if ranges.iter().any(Option::is_some) {
        call["pages"] = json!(ranges);
    }
    let combined = run(&mut a, "doc_combine", &call).map_err(|e| codes::read_or_refused(&e))?;
    change::commit(area, &stage, &[(stage.path("merged.pdf"), out_abs)])?;
    Ok(json!({ "out": out, "bytes": combined["bytes"] }))
}

fn split(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.split";
    let path = args::need_str(args, "path", M)?;
    let out_dir = args::need_str(args, "out_dir", M)?;
    checked_input(area, path, "path", M)?;
    let out_dir_abs = contained(area, out_dir, "out_dir", M)?;
    let every = args["every"].as_u64();
    let before = args["before"].as_array();
    let mut a = session(area)?;
    let (doc, total) = open(&mut a, path)?;
    // The engine writes the parts itself: into a staging folder, then all
    // of them into `out_dir` under the area's rules, or none.
    let stage = area.stage().map_err(codes::area)?;
    let staging = staged(area, &stage, "")?;
    let call_args = match (every, before) {
        (Some(n), None) if n >= 1 => {
            if total.div_ceil(n) > MAX_SPLIT_FILES {
                return Err(fail(Code::TooLarge, format!("splitting {total} pages every {n} makes too many files (at most {MAX_SPLIT_FILES})")));
            }
            json!({ "doc": doc, "out_dir": staging, "every": n })
        }
        (None, Some(b)) if !b.is_empty() && (b.len() as u64) < MAX_SPLIT_FILES => {
            json!({ "doc": doc, "out_dir": staging, "before": b })
        }
        _ => return Err(invalid(format!("{M}: pass exactly one of `every` (pages per file, ≥ 1) or `before` (1-based page numbers)"))),
    };
    let parts = run(&mut a, "doc_split", &call_args).map_err(|e| codes::refused(&e))?;
    let moves: Vec<(PathBuf, PathBuf)> = stage.files().into_iter().map(|rel| (stage.path(&rel), out_dir_abs.join(rel))).collect();
    change::commit(area, &stage, &moves)?;
    // The engine reports where it wrote, in the staging folder; callers
    // speak area-relative paths, where the parts are now.
    let files: Vec<Json> = parts["files"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut f| {
            if let Some(name) = f["path"].as_str().and_then(|p| Path::new(p).file_name()) {
                f["path"] = json!(Path::new(out_dir).join(name).to_string_lossy());
            }
            f
        })
        .collect();
    Ok(json!({ "files": files }))
}

#[cfg(test)]
mod doc_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// The value of form field `name` in `path`, as `session` opens it.
    fn field(a: &mut Automation, path: &str, name: &str) -> Json {
        let (doc, _) = open(a, path).unwrap();
        let fields = run(a, "form_fields", &json!({"doc": doc})).unwrap();
        let list = if fields.is_array() { fields } else { fields["fields"].clone() };
        list.as_array().unwrap().iter().find(|f| f["name"] == name).map(|f| f["value"].clone()).unwrap_or_else(|| panic!("no field {name}: {list}"))
    }

    /// A document's own scripts never run in a service session. The
    /// hostile fixture is the engine's own scripted XFA form, whose
    /// initialize script sets `qty` when the form opens with scripting on,
    /// as the engine's default session does; through the service it opens
    /// as the file has it, by every method that opens it.
    #[test]
    fn a_documents_own_scripts_never_run() {
        let dir = tempfile::tempdir().unwrap();
        let form = pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template());
        std::fs::write(dir.path().join("form.pdf"), &form).unwrap();
        let mut scripted = Automation::new().with_root(dir.path()).unwrap();
        assert!(scripted.session().javascript(), "the engine runs scripts by default");
        assert_eq!(field(&mut scripted, "form.pdf", "qty"), json!("2"), "the fixture's script runs in the engine's default session");
        let area = Area::new(dir.path(), None, false);
        let mut a = session(&area).unwrap();
        assert!(!a.session().javascript(), "every service session turns scripts off");
        assert_ne!(field(&mut a, "form.pdf", "qty"), json!("2"), "no script ran");
        let cx = Ctx { area: &area, app: "os.fixture", isolate: 0 };
        dispatch_in("info", &json!({"path": "form.pdf"}), &cx).unwrap();
        dispatch_in("text", &json!({"path": "form.pdf"}), &cx).unwrap();
        dispatch_in("render", &json!({"path": "form.pdf", "page": 1, "out": "form.png", "max_side": 64}), &cx).unwrap();
    }

    /// A tiny, valid two-page PDF written by this test: Helvetica text on
    /// each page, exact stream lengths and xref offsets, no compression —
    /// so the suite needs no fixtures on disk.
    pub(crate) fn tiny_pdf(page1: &str, page2: &str) -> Vec<u8> {
        let (s1, s2) = (
            format!("BT /F1 12 Tf 20 50 Td ({page1}) Tj ET"),
            format!("BT /F1 12 Tf 20 50 Td ({page2}) Tj ET"),
        );
        let page = |contents: u32| {
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] \
                 /Resources << /Font << /F1 7 0 R >> >> /Contents {contents} 0 R >>"
            )
        };
        let stream = |s: &str| format!("<< /Length {} >>\nstream\n{s}\nendstream", s.len());
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_string(),
            page(4),
            stream(&s1),
            page(6),
            stream(&s2),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
        }
        let xref = pdf.len();
        pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for off in &offsets {
            pdf.extend(format!("{off:010} 00000 n \n").bytes());
        }
        pdf.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF", objs.len() + 1).bytes());
        pdf
    }

    /// pdfcraft's own `doc_info` names the absolute path it opened
    /// (`document.path`); through the service, answers and errors read
    /// relative to the area.
    #[test]
    fn answers_never_show_the_host_path_of_the_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        std::fs::create_dir_all(host.join("pdf")).unwrap();
        std::fs::write(host.join("pdf/a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        let call = |args: Json| ServiceCall {
            app_id: "os.pdftools".into(),
            service: "pdf.info".into(),
            args,
            from_sheet: false,
            may_prompt: true,
            host_dir: host.to_path_buf(),
        };
        let host = host.display().to_string();
        let answer = serve(&Slot::new(), &call(json!({"path": "a.pdf"})), 0).unwrap();
        assert_eq!(answer["document"]["pages"], json!(2), "{answer}");
        assert!(!answer.to_string().contains(&host), "{answer}");
        let error = serve(&Slot::new(), &call(json!({"path": "missing.pdf"})), 0).unwrap_err();
        assert!(!error.contains(&host), "{error}");
        assert!(error.starts_with("not_found: "), "{error}");
    }

    #[test]
    fn info_text_render_merge_split_on_a_real_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        std::fs::create_dir_all(host.join("pdf")).unwrap();
        std::fs::write(host.join("pdf/a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        std::fs::write(host.join("pdf/b.pdf"), tiny_pdf("Second file", "Last page")).unwrap();

        let doc = dispatch("info", &json!({"path": "a.pdf"}), host).unwrap();
        assert_eq!(doc["document"]["pages"], json!(2), "{doc}");
        assert_eq!(doc["file"], json!("a.pdf"));
        assert_eq!(doc["pages"][0]["width"], json!(200.0), "{doc}");

        let text = dispatch("text", &json!({"path": "a.pdf"}), host).unwrap();
        let all = text["pages"].to_string();
        assert!(all.contains("Hello PDF") && all.contains("Page two"), "{text}");
        let one = dispatch("text", &json!({"path": "a.pdf", "pages": [2]}), host).unwrap();
        assert_eq!(one["pages"][0]["page"], json!(2), "{one}");
        assert!(one["pages"][0]["text"].as_str().unwrap().contains("Page two"), "{one}");

        let rend = dispatch("render", &json!({"path": "a.pdf", "page": 1, "out": "prev.png", "max_side": 64}), host).unwrap();
        assert_eq!(rend["out"], json!("prev.png"));
        assert!(rend["bytes"].as_u64().unwrap() > 0, "{rend}");
        let (w, h) = (rend["width"].as_u64().unwrap(), rend["height"].as_u64().unwrap());
        assert!((16..=65).contains(&w.max(h)), "{rend}");
        assert!(host.join("pdf/prev.png").metadata().unwrap().len() > 0, "the PNG lands inside the area");

        let merged = dispatch("merge", &json!({"paths": ["a.pdf", "b.pdf"], "out": "m.pdf"}), host).unwrap();
        assert_eq!(merged["out"], json!("m.pdf"));
        assert!(merged["bytes"].as_u64().unwrap() > 0, "{merged}");
        let minfo = dispatch("info", &json!({"path": "m.pdf"}), host).unwrap();
        assert_eq!(minfo["document"]["pages"], json!(4), "{minfo}");

        let split = dispatch("split", &json!({"path": "m.pdf", "out_dir": "parts", "every": 2}), host).unwrap();
        let files = split["files"].as_array().unwrap();
        assert_eq!(files.len(), 2, "{split}");
        assert_eq!(files[0]["first_page"], json!(1), "{split}");
        assert_eq!(files[1]["last_page"], json!(4), "{split}");
        for f in files {
            let rel = f["path"].as_str().unwrap();
            assert!(!rel.starts_with('/'), "paths come back area-relative: {rel}");
            assert!(host.join("pdf").join(rel).metadata().unwrap().len() > 0, "{rel}");
        }
    }

    #[test]
    fn paths_outside_the_pdf_area_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for bad in ["../up.pdf", "/etc/up.pdf", "a/../../up.pdf", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("text", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("render", &json!({"path": bad, "page": 1, "out": "o.png"}), host).is_err(), "{bad}");
            assert!(dispatch("merge", &json!({"paths": [bad, bad], "out": "m.pdf"}), host).is_err(), "{bad}");
            assert!(dispatch("split", &json!({"path": bad, "out_dir": "p", "every": 1}), host).is_err(), "{bad}");
        }
        // Outputs are confined too, even with a fine input.
        std::fs::create_dir_all(host.join("pdf")).unwrap();
        std::fs::write(host.join("pdf/a.pdf"), tiny_pdf("x", "y")).unwrap();
        for bad in ["../o.png", "/tmp/o.png", "a/../../o.png"] {
            assert!(dispatch("render", &json!({"path": "a.pdf", "page": 1, "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("merge", &json!({"paths": ["a.pdf", "a.pdf"], "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("split", &json!({"path": "a.pdf", "out_dir": bad, "every": 1}), host).is_err(), "{bad}");
        }
    }

    #[test]
    fn area_is_the_pdf_subdirectory_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = Slot::new().area(&service_call("info", json!({}), host, false), "pdf").unwrap();
        assert_eq!(a.root, host.join("pdf"));
        assert!(a.root.is_dir(), "created on first use");
        std::fs::write(a.root.join("a.pdf"), tiny_pdf("in the area", "p2")).unwrap();
        for _ in 0..2 {
            serve(&Slot::new(), &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "o.png"}), host, false), 0).unwrap();
        }
        assert!(a.root.join("o.png").is_file(), "writes land inside the area, replacing as before");
        assert!(!host.join("o.png").exists(), "nothing lands beside the area in the shared host dir");
    }

    /// A call as App Hub hands it to the service.
    pub(crate) fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("pdf.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
    }

    /// A resolver shaped like the shell's: every call works in `root`, an
    /// app's own foreground call may replace a file and an agent's may not,
    /// within `quota`.
    pub(crate) fn resolver(root: &Path, quota: Option<u64>) -> Slot {
        let slot = Slot::new();
        let root = root.to_path_buf();
        slot.set(Some(std::sync::Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, quota, call.may_prompt)))));
        slot
    }

    pub(crate) fn no_staging_left(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(octosense_engine_area::STAGING_PREFIX))
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too; what the engine
    /// writes itself lands where it was asked, and no staging folder stays.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        std::fs::write(root.join("b.pdf"), tiny_pdf("Second file", "Last page")).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let doc = serve(&areas, &service_call("info", json!({"path": "a.pdf"}), &host, false), 0).unwrap();
        assert_eq!(doc["document"]["pages"], json!(2), "{doc}");
        let merged = serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "b.pdf"], "out": "out/m.pdf"}), &host, false), 0).unwrap();
        assert!(merged["bytes"].as_u64().unwrap() > 0, "{merged}");
        assert!(root.join("out/m.pdf").is_file());
        let split = serve(&areas, &service_call("split", json!({"path": "out/m.pdf", "out_dir": "parts", "every": 2}), &host, false), 0).unwrap();
        let files = split["files"].as_array().unwrap();
        assert_eq!(files.len(), 2, "{split}");
        for f in files {
            let rel = f["path"].as_str().unwrap();
            assert!(rel.starts_with("parts/") && root.join(rel).is_file(), "{rel}");
        }
        serve(&areas, &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "p1.png", "max_side": 32}), &host, false), 0).unwrap();
        assert!(root.join("p1.png").is_file() && !host.exists() && !root.join("pdf").exists());
        assert!(no_staging_left(&root));
        std::fs::write(dir.path().join("beside.pdf"), tiny_pdf("x", "y")).unwrap();
        for bad in ["../beside.pdf", "/etc/hosts", "out/../../beside.pdf"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false), 0).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "b.pdf"], "out": bad}), &host, true), 0).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": bad, "every": 1}), &host, true), 0).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.pdf"}), &host, false), 0).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "up/o.png"}), &host, true), 0).is_err());
            assert!(serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": "up", "every": 1}), &host, true), 0).is_err());
            assert!(!dir.path().join("o.png").exists() && !dir.path().join("a-part1.pdf").exists());
        }
    }

    /// An agent's call never replaces a file, a split's parts included (all
    /// or none land); an app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        std::fs::write(dir.path().join("taken.pdf"), b"keep me").unwrap();
        let areas = resolver(dir.path(), None);
        let refused = serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "a.pdf"], "out": "taken.pdf"}), dir.path(), false), 0).unwrap_err();
        assert!(refused.contains("`taken.pdf` already exists"), "{refused}");
        std::fs::write(dir.path().join("p.png"), b"keep").unwrap();
        assert!(serve(&areas, &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "p.png"}), dir.path(), false), 0).is_err());
        // One part's name is taken: no part lands.
        std::fs::create_dir(dir.path().join("parts")).unwrap();
        std::fs::write(dir.path().join("parts/a-part2.pdf"), b"theirs").unwrap();
        let refused = serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": "parts", "every": 1}), dir.path(), false), 0).unwrap_err();
        assert!(refused.contains("already exists"), "{refused}");
        assert!(!dir.path().join("parts/a-part1.pdf").exists(), "all or nothing");
        assert_eq!(std::fs::read(dir.path().join("parts/a-part2.pdf")).unwrap(), b"theirs");
        assert_eq!(std::fs::read(dir.path().join("taken.pdf")).unwrap(), b"keep me");
        assert!(no_staging_left(dir.path()));
        serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "a.pdf"], "out": "taken.pdf"}), dir.path(), true), 0).unwrap();
        assert!(std::fs::read(dir.path().join("taken.pdf")).unwrap().starts_with(b"%PDF"));
        serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": "parts", "every": 1}), dir.path(), true), 0).unwrap();
        assert!(std::fs::read(dir.path().join("parts/a-part2.pdf")).unwrap().starts_with(b"%PDF"));
    }

    /// What a call writes, the engine's own output included, must fit what
    /// is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        let tight = resolver(dir.path(), Some(64));
        for (method, args) in [
            ("merge", json!({"paths": ["a.pdf", "a.pdf"], "out": "m.pdf"})),
            ("split", json!({"path": "a.pdf", "out_dir": "parts", "every": 1})),
            ("render", json!({"path": "a.pdf", "page": 1, "out": "p.png"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true), 0).unwrap_err();
            assert!(refused.starts_with("storage_full: ") && refused.contains("bytes left"), "{method}: {refused}");
        }
        assert!(!dir.path().join("m.pdf").exists() && !dir.path().join("p.png").exists() && !dir.path().join("parts/a-part1.pdf").exists());
        assert!(no_staging_left(dir.path()));
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.files"));
        assert!(!may_call("org.example.app"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "pdf", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("pdf.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
