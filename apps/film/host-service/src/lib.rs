//! `octosense-film-service` — the `film` host service (ADR 0013).
//!
//! filmcraft's video engine (their crates, unchanged, pinned) behind typed
//! `film.*` methods. Pure Rust and fully headless: the engine's own
//! demuxers (MP4/MOV, Matroska/WebM, MXF, MPEG TS/PS, Ogg), decoders
//! (H.264, HEVC, VP9, AV1, ProRes, DNx, AAC, Opus, …) and encoders
//! (H.264, AAC, ProRes, PCM, GIF) — no ffmpeg, no system codecs, no GPU,
//! no network. Every call is a fresh, stateless session.
//!
//! Methods (all under the `film` family; paths relative to the call's
//! area):
//! - `info {path}` → container, streams and duration as JSON
//! - `frame {path, at_ms?, out, max_side?}` → the frame at `at_ms`
//!   through the engine's program renderer, written to `out` as PNG
//! - `export {path, out, format?, start_ms?, end_ms?, audio?}` → the
//!   range transcoded by the engine's own encoders (`h264` MP4 with AAC,
//!   `prores` MOV, `wav`, `gif`), at most [`MAX_EXPORT_MS`] per call
//! - `project.info {path}` → a `.fcproj` opened headlessly and inspected
//!   (`project.inspect` / `sequence.inspect`)
//! - `run {path, cmds: [{id, params?}], out?, format?, at_ms?, max_side?,
//!   start_ms?, end_ms?, audio?}` → `{results, out, …}` — the command door
//!   (ADR 0013, #418): run commands of filmcraft's registry on one session
//!   over `path` (a media file laid on a new sequence, as `frame` and
//!   `export` do, or a `.fcproj`), then write `out`: a `.fcproj` (the
//!   engine's own project writer), a `.png` (the frame at `at_ms`, as
//!   `frame`), or media (`.mp4`/`.mov`/`.wav`/`.gif` or `format`, as
//!   `export`, over `start_ms..end_ms` of the active sequence). Only what
//!   the door's allowlist admits runs ([`door`]): commands the reviewed
//!   classification (`skill/safety.json`) classes `safe`, and the reviewed
//!   caption read, whose `path` must name a file inside the area; every
//!   other id is refused before any command runs. After every command the
//!   project and the session are fenced ([`fence`]): nothing in them may
//!   point the engine at a file or folder outside the area.
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/film`. Boundary paths are checked
//! here (`..`, absolute paths and symlink escapes are refused), and every
//! read the engine makes on its own — media a project file points at
//! included — passes [`AreaServices`], which refuses anything outside the
//! area again. Writes keep the area's rules (a write that may not replace,
//! an agent's, only creates new files, within the quota): a frame through
//! [`Area::write`], the engine's own writes through [`AreaServices`], and an
//! export, which the engine streams to a path itself, into a staging folder
//! inside the area first ([`octosense_engine_area::Stage`]).
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use filmcraft_engine::project::{resolve_auto_points, EffectInstance, ItemId, ItemKind, Param, ParamValue, SequenceSettings, TrackKind};
use filmcraft_engine::time::{FrameRate, Tick, TimeRange};
use filmcraft_engine::{Services, Session};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, FileRead, Inner, InnerRule, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The largest media file the service reads (bytes).
const MAX_MEDIA_BYTES: u64 = 512 << 20;
/// The largest project file `project.info` and `run` open, and `run` writes
/// (bytes).
const MAX_PROJECT_BYTES: u64 = 64 << 20;
/// The longest range one `export` call encodes (milliseconds).
const MAX_EXPORT_MS: f64 = 300_000.0;
/// The longest edge `frame` produces, and its default.
const MAX_RENDER_SIDE: u32 = 4096;
const DEFAULT_RENDER_SIDE: u32 = 1024;
/// The export formats offered: what the engine encodes with its own code
/// (H.264+AAC in MP4, ProRes 422 HQ in MOV, PCM WAV, animated GIF).
const FORMATS: [&str; 4] = ["h264", "prores", "wav", "gif"];

/// What the film engine's reviewer settled for the door beyond the classes.
///
/// - **One reviewed read**: `captions.import {path, name?}` reads only the
///   caption file `path` names, whole, through the session's services (so
///   [`AreaServices`] checks it again when the engine reads), parses it in
///   memory and adds a caption track (`captions.rs` `captions.import`,
///   `import`). Every other `file` command writes, walks folders, keeps a
///   path in session state, or reads with `std::fs` past the services
///   (`lut.import`, the Lumetri LUT browses, presets, templates).
/// - **No setter**: no `safe` film command sets an app-wide variable. The
///   preferences and everything kept in them (`prefs.*`, the proxy and
///   scrubbing toggles, panel views, shortcuts, user sound presets) are
///   classed `host`. The keyed `safe` setters edit the project only:
///   `essentialSound.set {key}` the selected clips' Essential Sound
///   settings (`EssentialSound::set` takes only that struct's fields and
///   checks its EQ and reverb preset names against the built-ins),
///   `metadata.set {field}` an item's log fields (read back only as
///   numbers and labels), and `sequence.settings`,
///   `file.projectSettings.general` a name, enumerated or numeric values.
/// - **Inner ids**: a named effect, transition or preset must be one the
///   engine builds in. FilmCraft has no effect plug-ins (`effect_defs` is
///   compiled in; Settings ▸ Plugins installs nothing), and a door session
///   never loads user effect or sound presets (they come only from a data
///   directory, which the service never gives the engine, or from `file`
///   and `host` commands), so these rules hold the door to the built-ins
///   even if a later engine adds either.
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[FileRead { id: "captions.import", params: &["path"] }],
    setters: &[],
    inner: &[
        Inner { id: "effects.apply", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "sequence.applyVideoTransition", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "sequence.applyAudioTransition", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "effects.setDefaultTransition", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "mixer.addInsert", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        // `presets.apply` takes the name as `preset`, else as `name`.
        Inner { id: "presets.apply", param: "preset", rule: InnerRule::Effect { builtin: builtin_effect_preset } },
        Inner { id: "presets.apply", param: "name", rule: InnerRule::Effect { builtin: builtin_effect_preset } },
        Inner { id: "lumetri.applyPreset", param: "name", rule: InnerRule::Effect { builtin: builtin_lumetri_preset } },
        Inner { id: "essentialSound.applyPreset", param: "preset", rule: InnerRule::Effect { builtin: builtin_sound_preset } },
    ],
};

/// A built-in effect or transition, by every spelling the engine resolves
/// one (`effects.apply`'s id or display name, `find_transition`'s trimmed
/// lowercase id or name, `mixer.addInsert`'s id).
fn builtin_effect(name: &str) -> bool {
    let trimmed = name.trim();
    let lower = trimmed.to_ascii_lowercase();
    filmcraft_engine::project::effect_defs().iter().any(|d| d.id == name || d.id == lower || d.name.eq_ignore_ascii_case(trimmed))
}

/// A built-in effect preset (`presets.apply` matches names exactly).
fn builtin_effect_preset(name: &str) -> bool {
    filmcraft_engine::presets::builtin_presets().iter().any(|p| p.name == name)
}

/// A built-in Lumetri preset (compiled in, matched case-insensitively).
fn builtin_lumetri_preset(name: &str) -> bool {
    filmcraft_engine::render::lumetri_presets::find(name).is_some()
}

/// A built-in Essential Sound preset (matched case-insensitively).
fn builtin_sound_preset(name: &str) -> bool {
    filmcraft_engine::project::essential::builtin_presets().iter().any(|p| p.name.eq_ignore_ascii_case(name))
}

/// The command door's gate: filmcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("film", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct FilmService;

/// Register the `film` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(FilmService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/film` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `film.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.film` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for FilmService {
    fn family(&self) -> &'static str {
        "film"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The film service serves system apps only.".into());
    }
    let area = Arc::new(areas.area(call, "film").map_err(|e| format!("film: {e}"))?);
    dispatch_in(call.method(), &call.args, area.clone())
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: Arc<Area>) -> Result<Json, String> {
    match method {
        "info" => info(args, &area),
        "frame" => frame(args, &area),
        "export" => export(args, &area),
        "project.info" => project_info(args, &area),
        "run" => run(args, &area),
        other => Err(format!("film.{other} is not a method of the film service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/film`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "film");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("the film area: {e}"))?;
    dispatch_in(method, args, Arc::new(area))
}

// ---------------------------------------------------------------------------
// containment
// ---------------------------------------------------------------------------

/// The call's area, canonicalized: what every path is checked against.
fn canonical(area: &Area) -> Result<PathBuf, String> {
    area.root.canonicalize().map_err(|e| format!("the call's folder: {e}"))
}

/// A path strictly inside the (canonical) area: relative, no `..`, no
/// absolute component; the resolved path stays under the area even through
/// symlinks.
fn contained(area: &Path, rel: &str, what: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("`{what}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("`{what}` stays inside this call's film area"));
    }
    let joined = area.join(rel_path);
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
    let resolved = deepest.canonicalize().map_err(|e| format!("{rel}: {e}"))?;
    if !resolved.starts_with(area) {
        return Err(format!("`{what}` stays inside this call's film area"));
    }
    Ok(joined)
}

/// `args[key]` as an existing input file inside the area, canonical, with
/// its size capped.
fn input(area: &Path, args: &Json, key: &str, cap: u64) -> Result<(PathBuf, String), String> {
    let rel = args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("`{key}` is required"))?;
    let p = contained(area, rel, key)?;
    let m = std::fs::metadata(&p).map_err(|e| format!("{rel}: {e}"))?;
    if !m.is_file() {
        return Err(format!("{rel} is not a file"));
    }
    if m.len() > cap {
        return Err(format!("{rel} is {} bytes; the film service reads at most {cap}", m.len()));
    }
    let abs = p.canonicalize().map_err(|e| format!("{rel}: {e}"))?;
    Ok((abs, rel.to_string()))
}

/// `args[key]` as an output path inside the (canonical) area, admitted by
/// the call's rules before the engine works.
fn output(area: &Area, canon: &Path, args: &Json, key: &str) -> Result<(PathBuf, String), String> {
    let rel = args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("`{key}` is required"))?;
    let p = contained(canon, rel, key)?;
    area.check(&canon_to_area(area, canon, &p), 0)?;
    Ok((p, rel.to_string()))
}

/// A path under the canonical area as the call's area spells it, so the
/// area's rules (and its messages) see the caller's own path.
fn canon_to_area(area: &Area, canon: &Path, p: &Path) -> PathBuf {
    p.strip_prefix(canon).map(|rel| area.root.join(rel)).unwrap_or_else(|_| p.to_path_buf())
}

/// Whether `path`, as the engine would use it, stays inside the canonical
/// area `area`: an absolute path whose deepest existing ancestor, links
/// resolved, is under it (a relative one would resolve against the
/// process's working directory, never the area).
fn inside(area: &Path, path: &str) -> bool {
    let p = Path::new(path);
    if !p.is_absolute() {
        return false;
    }
    let mut deepest = p.to_path_buf();
    while !deepest.exists() {
        match deepest.parent() {
            Some(parent) => deepest = parent.to_path_buf(),
            None => return false,
        }
    }
    deepest.canonicalize().is_ok_and(|resolved| resolved.starts_with(area))
}

/// The engine's file access, rooted in the area. The engine only ever gets
/// paths this service blessed, but a project file can carry any path
/// (media to relink), so every path the engine asks for is resolved —
/// through symlinks, to the deepest existing ancestor for writes — and
/// refused unless it stays inside the area. What the engine writes keeps
/// the call's rules ([`Area::write`]). No directory listing, no volumes, no
/// home directory.
struct AreaServices {
    /// Canonical.
    area: PathBuf,
    /// The call's area, whose rules the engine's writes keep.
    rules: Arc<Area>,
}

impl AreaServices {
    fn allowed(&self, path: &str) -> std::io::Result<PathBuf> {
        if inside(&self.area, path) {
            Ok(PathBuf::from(path))
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{path} is outside the film area")))
        }
    }
}

impl Services for AreaServices {
    fn read_file(&self, path: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.allowed(path)?)
    }
    fn write_file(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        let p = self.allowed(path)?;
        let p = canon_to_area(&self.rules, &self.area, &p);
        self.rules.write(&p, data).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))
    }
    fn file_size(&self, path: &str) -> std::io::Result<u64> {
        let m = std::fs::metadata(self.allowed(path)?)?;
        if m.is_file() { Ok(m.len()) } else { Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{path} is not a file"))) }
    }
    fn read_range(&self, path: &str, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(self.allowed(path)?)?;
        f.seek(SeekFrom::Start(offset))?;
        let mut out = Vec::with_capacity(len);
        f.take(len as u64).read_to_end(&mut out)?;
        Ok(out)
    }
    /// Media are read in place from the open file: a clip costs its index.
    fn reader(&self, path: &str) -> Option<std::io::Result<filmcraft_media::SharedReader>> {
        Some(self.allowed(path).and_then(|p| {
            filmcraft_media::reader::FileReader::open(&p).map(|r| Arc::new(r) as filmcraft_media::SharedReader)
        }))
    }
}

// ---------------------------------------------------------------------------
// the engine session
// ---------------------------------------------------------------------------

fn session(canon: &Path, rules: &Arc<Area>) -> Session {
    Session::new(Arc::new(AreaServices { area: canon.to_path_buf(), rules: rules.clone() }))
}

/// Import `abs` and lay the whole clip on a fresh sequence sized like it
/// (V1/A1 from zero, the engine's own recipe), leaving it active.
fn clip_sequence(s: &mut Session, abs: &Path) -> Result<filmcraft_media::MediaInfo, String> {
    let r = s.execute("file.import", json!({"paths": [abs.to_string_lossy()]})).map_err(|e| e.to_string())?;
    if let Some(e) = r["errors"].as_array().and_then(|a| a.first()) {
        return Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()));
    }
    let id = r["items"].as_array().and_then(|a| a.first()).and_then(Json::as_u64).ok_or("nothing was imported")?;
    let id = ItemId(id);
    let info = s.project.item(id).and_then(|i| i.as_media()).map(|m| m.info.clone()).ok_or("not a media item")?;
    lay_on_sequence(s, id, &info)?;
    Ok(info)
}

/// A new sequence holding the whole of item `id`, active when it returns.
fn lay_on_sequence(s: &mut Session, id: ItemId, info: &filmcraft_media::MediaInfo) -> Result<(), String> {
    let (w, h, rate) = match &info.video {
        Some(v) => (v.width, v.height, v.frame_rate),
        // Audio-only media get a nominal picture.
        None => (640, 480, FrameRate::FPS_24),
    };
    let mut p = (*s.project).clone();
    let seq = p.new_sequence("Film", SequenceSettings { width: w, height: h, frame_rate: rate, ..Default::default() }, 1, 1, None);
    let range = TimeRange::new(Tick::ZERO, info.duration);
    let vi = match info.video.is_some() {
        true => Some(p.make_track_item(id, TrackKind::Video, Tick::ZERO, range, rate).ok_or("the clip's picture did not place")?),
        false => None,
    };
    let ai = info.audio.is_some().then(|| p.make_track_item(id, TrackKind::Audio, Tick::ZERO, range, rate)).flatten();
    let q = p.sequence_mut(seq).ok_or("no sequence")?;
    if let Some(mut vi) = vi {
        for e in &mut vi.effects {
            resolve_auto_points(e, (w, h), (w, h));
        }
        q.video_tracks[0].items.push(vi);
    }
    if let Some(ai) = ai {
        q.audio_tracks[0].items.push(ai);
    }
    s.project = Arc::new(p);
    s.state.active_sequence = Some(seq);
    s.state.open_sequences = vec![seq];
    Ok(())
}

// ---------------------------------------------------------------------------
// methods
// ---------------------------------------------------------------------------

/// `film.info {path}` → the engine's probe of the file: container, video
/// and audio streams, duration (ticks and `duration_ms`), timecode, size.
fn info(args: &Json, rules: &Arc<Area>) -> Result<Json, String> {
    let err = |e: String| format!("film.info: {e}");
    let area = canonical(rules).map_err(err)?;
    let (abs, rel) = input(&area, args, "path", MAX_MEDIA_BYTES).map_err(err)?;
    let bytes: Arc<[u8]> = std::fs::read(&abs).map_err(|e| err(format!("{rel}: {e}")))?.into();
    let name = abs.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| rel.clone());
    let src = filmcraft_codecs::open_bytes(&name, bytes).map_err(|e| err(format!("{rel}: {e}")))?;
    let mut v = serde_json::to_value(src.info()).map_err(|e| err(e.to_string()))?;
    v["duration_ms"] = json!((src.info().duration.seconds() * 1000.0).round());
    v["file"] = json!(rel);
    Ok(v)
}

/// `film.frame {path, at_ms?, out, max_side?}` → the frame at `at_ms`
/// (default 0), rendered by the engine's program compositor over black and
/// written to `out` as PNG with its longest edge at most `max_side`.
fn frame(args: &Json, rules: &Arc<Area>) -> Result<Json, String> {
    let err = |e: String| format!("film.frame: {e}");
    let area = canonical(rules).map_err(err)?;
    let (abs, _) = input(&area, args, "path", MAX_MEDIA_BYTES).map_err(err)?;
    let (out_abs, out_rel) = output(rules, &area, args, "out").map_err(err)?;
    let (at_ms, max_side) = frame_args(args).map_err(err)?;

    let mut s = session(&area, rules);
    let info = clip_sequence(&mut s, &abs).map_err(err)?;
    let v = info.video.as_ref().ok_or_else(|| err("the file has no video stream".into()))?;
    let duration_ms = info.duration.seconds() * 1000.0;
    let png = program_png(&s, (v.width, v.height), v.frame_rate, duration_ms, at_ms, max_side).map_err(err)?;
    rules.write(&canon_to_area(rules, &area, &out_abs), &png).map_err(err)?;
    let (w, h) = png_size(&png);
    Ok(json!({"out": out_rel, "width": w, "height": h, "at_ms": at_ms, "bytes": png.len()}))
}

/// `at_ms` (default 0) and `max_side` (default [`DEFAULT_RENDER_SIDE`],
/// clamped to 16..[`MAX_RENDER_SIDE`]) of a frame call.
fn frame_args(args: &Json) -> Result<(f64, u32), String> {
    let at_ms = args["at_ms"].as_f64().unwrap_or(0.0);
    if !at_ms.is_finite() || at_ms < 0.0 {
        return Err("`at_ms` is a time in milliseconds from the start".into());
    }
    let max_side = args["max_side"].as_u64().unwrap_or(DEFAULT_RENDER_SIDE as u64).min(MAX_RENDER_SIDE as u64) as u32;
    Ok((at_ms, max_side.max(16)))
}

/// The program frame at `at_ms` of the session's active sequence
/// (`width`×`height` at `rate`, `duration_ms` long), rendered by the
/// engine's program compositor over black, as PNG with its longest edge at
/// most `max_side`.
fn program_png(s: &Session, (w, h): (u32, u32), rate: FrameRate, duration_ms: f64, at_ms: f64, max_side: u32) -> Result<Vec<u8>, String> {
    if at_ms > duration_ms {
        return Err(format!("`at_ms` {at_ms} is past the end ({duration_ms:.0} ms)"));
    }
    // The display time of the last frame, not one past it.
    let frame_ms = rate.tick_of(1).seconds() * 1000.0;
    let t = Tick::from_seconds_f64(at_ms.min((duration_ms - frame_ms).max(0.0)) / 1000.0);
    let long = w.max(h);
    let scale = if long > max_side { max_side as f32 / long as f32 } else { 1.0 };
    let img = s.try_render_program_at(scale, t).map_err(|e| e.to_string())?;
    png_at_most(img.w as u32, img.h as u32, img.over_black_rgba8(), max_side)
}

/// RGBA8 as PNG, downscaled so the longest side is at most `max_side`.
fn png_at_most(w: u32, h: u32, rgba: Vec<u8>, max_side: u32) -> Result<Vec<u8>, String> {
    let mut img = image::RgbaImage::from_raw(w, h, rgba).ok_or("bad frame buffer")?;
    if w.max(h) > max_side {
        let s = max_side as f32 / w.max(h) as f32;
        let (nw, nh) = (((w as f32 * s) as u32).max(1), ((h as f32 * s) as u32).max(1));
        img = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| format!("png: {e}"))?;
    Ok(out.into_inner())
}

/// Width and height from a PNG header (bytes 16..24).
fn png_size(png: &[u8]) -> (u32, u32) {
    let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    if png.len() >= 24 { (be(16), be(20)) } else { (0, 0) }
}

/// The export format for an output path's extension.
fn format_for(path: &str) -> Option<&'static str> {
    match path.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" => Some("h264"),
        "mov" => Some("prores"),
        "wav" => Some("wav"),
        "gif" => Some("gif"),
        _ => None,
    }
}

/// `film.export {path, out, format?, start_ms?, end_ms?, audio?}` → the
/// range re-encoded by the engine's own encoders. `format` defaults from
/// the `out` extension; the range defaults to the whole clip and is capped
/// at [`MAX_EXPORT_MS`].
fn export(args: &Json, rules: &Arc<Area>) -> Result<Json, String> {
    let err = |e: String| format!("film.export: {e}");
    let area = canonical(rules).map_err(err)?;
    let (abs, _) = input(&area, args, "path", MAX_MEDIA_BYTES).map_err(err)?;
    let (out_abs, out_rel) = output(rules, &area, args, "out").map_err(err)?;
    let format = media_format(args, &out_rel).map_err(err)?;

    let mut s = session(&area, rules);
    let info = clip_sequence(&mut s, &abs).map_err(err)?;
    let duration_ms = info.duration.seconds() * 1000.0;
    let (start_ms, end_ms) = range_ms(args, duration_ms, "clip").map_err(err)?;
    let audio = args["audio"].as_bool().unwrap_or(true);
    let (bytes, result) = export_staged(&mut s, rules, &canon_to_area(rules, &area, &out_abs), &format, (start_ms, end_ms), audio, None).map_err(err)?;
    Ok(json!({"out": out_rel, "format": format, "start_ms": start_ms, "end_ms": end_ms, "bytes": bytes, "result": result}))
}

/// The export format of a media call: `format`, else from `out`'s
/// extension; one of [`FORMATS`].
fn media_format(args: &Json, out_rel: &str) -> Result<String, String> {
    let format = match args["format"].as_str() {
        Some(f) => f.to_string(),
        None => format_for(out_rel).ok_or_else(|| format!("give `format` ({}) or an out path ending .mp4/.mov/.wav/.gif", FORMATS.join(" | ")))?.to_string(),
    };
    if !FORMATS.contains(&format.as_str()) {
        return Err(format!("`{format}` is not offered ({})", FORMATS.join(" | ")));
    }
    Ok(format)
}

/// `start_ms..end_ms` of an export over something `duration_ms` long (the
/// `what`): the whole of it by default, at most [`MAX_EXPORT_MS`].
fn range_ms(args: &Json, duration_ms: f64, what: &str) -> Result<(f64, f64), String> {
    let start_ms = args["start_ms"].as_f64().unwrap_or(0.0);
    let end_ms = args["end_ms"].as_f64().unwrap_or(duration_ms);
    if !start_ms.is_finite() || !end_ms.is_finite() || start_ms < 0.0 || start_ms >= end_ms {
        return Err(format!("the range runs from `start_ms` to `end_ms` within the {what} (0..{duration_ms:.0})"));
    }
    if end_ms - start_ms > MAX_EXPORT_MS {
        return Err(format!("at most {MAX_EXPORT_MS:.0} ms per export ({:.0} ms asked); give `start_ms`/`end_ms`", end_ms - start_ms));
    }
    if end_ms > duration_ms + 1.0 {
        return Err(format!("`end_ms` {end_ms} is past the end ({duration_ms:.0} ms)"));
    }
    Ok((start_ms, end_ms))
}

/// The file extension the engine gives an export of `format`
/// (`ExportSettings::extension`); a staged path without it would get it
/// appended.
fn extension_of(format: &str) -> &'static str {
    match format {
        "h264" => "mp4",
        "prores" => "mov",
        "wav" => "wav",
        _ => "gif",
    }
}

/// Encode `start_ms..end_ms` of the session's active sequence with the
/// engine's own encoders, at `size` when given (else the sequence's), and
/// place it at `out` (the area's spelling). The engine streams an export to
/// a path itself (`file.exportMedia`, the exporter's `std::fs::File::create`):
/// a staging folder inside the area, then into place under the call's rules
/// ([`octosense_engine_area::Stage::commit`]). The settings come from these
/// parameters alone: no preset, no `settings` patch, so no image overlay or
/// caption sidecar path can enter. Answers the bytes and the encoder's
/// report.
fn export_staged(s: &mut Session, rules: &Area, out: &Path, format: &str, (start_ms, end_ms): (f64, f64), audio: bool, size: Option<(u32, u32)>) -> Result<(u64, Json), String> {
    let stage = rules.stage()?;
    let staged = stage.path(format!("export.{}", extension_of(format)));
    let mut params = json!({
        "path": staged.to_string_lossy(),
        "format": format,
        "audio": audio,
        "range": "custom",
        "startSeconds": start_ms / 1000.0,
        "endSeconds": end_ms / 1000.0,
        "wait": true,
    });
    if let Some((w, h)) = size {
        params["width"] = json!(w);
        params["height"] = json!(h);
    }
    let r = s.execute("file.exportMedia", params).map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(&staged).map(|m| m.len()).map_err(|e| format!("{}: {e}", rules.shown(out)))?;
    stage.commit(&[(staged, out.to_path_buf())])?;
    Ok((bytes, r["result"].clone()))
}

/// `film.project.info {path}` → a FilmCraft project opened headlessly:
/// `project.inspect`, and `sequence.inspect` when a sequence is active.
/// Media the project points at outside the area stay offline (refused by
/// [`AreaServices`]).
fn project_info(args: &Json, rules: &Arc<Area>) -> Result<Json, String> {
    let err = |e: String| format!("film.project.info: {e}");
    let area = canonical(rules).map_err(err)?;
    let (abs, rel) = input(&area, args, "path", MAX_PROJECT_BYTES).map_err(err)?;
    // A project naming a scratch disk or an ingest folder would make the
    // engine list it on opening: refused first, as `run` refuses it.
    let bytes = std::fs::read(&abs).map_err(|e| err(format!("{rel}: {e}")))?;
    stored_folders(&bytes).map_err(|e| err(format!("`{rel}`: {e}")))?;
    let mut s = session(&area, rules);
    s.execute("file.open", json!({"path": abs.to_string_lossy()})).map_err(|e| err(e.to_string()))?;
    let mut project = s.execute("project.inspect", json!({})).map_err(|e| err(e.to_string()))?;
    // The engine names the project by the host's own path: the caller's is
    // the area-relative one.
    if let Some(path) = project.get_mut("path").filter(|p| p.is_string()) {
        *path = json!(rel);
    }
    let sequence = s.execute("sequence.inspect", json!({})).unwrap_or(Json::Null);
    Ok(json!({"file": rel, "project": project, "sequence": sequence}))
}

// ---------------------------------------------------------------------------
// the command door
// ---------------------------------------------------------------------------

/// The folders a project names, each a place file commands and the render
/// previews write or read with `std::fs`: (what, its keys under the project
/// file's `settings`).
const PROJECT_FOLDERS: [(&str, [&str; 2]); 5] = [
    ("ingest destination", ["ingest", "destination"]),
    ("captured-media scratch disk", ["scratch", "captured"]),
    ("video-preview scratch disk", ["scratch", "videoPreviews"]),
    ("audio-preview scratch disk", ["scratch", "audioPreviews"]),
    ("auto-save scratch disk", ["scratch", "autoSave"]),
];

fn names_a_folder(what: &str, dir: &str) -> String {
    format!("the project's {what} is set (`{dir}`), and the door runs no project that points the engine at a folder")
}

/// What a text parameter of the pinned engine's effects holds, as the
/// renderer uses it. Reviewed from `filmcraft-project`'s effect and graphic
/// layer definitions and `filmcraft-render`; the test
/// `every_text_parameter_of_the_engine_is_reviewed` fails when the engine
/// gains one this table does not name.
#[derive(Clone, Copy, PartialEq, Debug)]
enum TextParam {
    /// Drawn or named, never opened: a title's or layer's text, a layer's
    /// name, a burn-in prefix, or a font family and style, which
    /// `filmcraft_text::fonts::resolve` looks up among the bundled faces and
    /// the OS font folders by name, never by path.
    Drawn,
    /// A Lumetri LUT reference, which `filmcraft_render::luts::resolve`
    /// reads as `builtin:<id>` (made from code) or `lib:<id>` (a LUT
    /// embedded in the project), never as a file.
    Lut,
}

/// The reviewed text parameters: (effect id, parameter id, what it holds).
const TEXT_PARAMS: [(&str, &str, TextParam); 9] = [
    ("lumetri", "input_lut", TextParam::Lut),
    ("lumetri", "look_lut", TextParam::Lut),
    ("metadata_burnin", "prefix", TextParam::Drawn),
    ("simple_text", "text", TextParam::Drawn),
    ("graphic_text", "name", TextParam::Drawn),
    ("graphic_text", "text", TextParam::Drawn),
    ("graphic_text", "font", TextParam::Drawn),
    ("graphic_text", "font_style", TextParam::Drawn),
    ("graphic_shape", "name", TextParam::Drawn),
];

fn text_param(effect: &str, param: &str) -> Option<TextParam> {
    TEXT_PARAMS.iter().find(|(e, p, _)| *e == effect && *p == param).map(|(_, _, kind)| *kind)
}

/// Every text value one parameter holds (its value and its keyframes')
/// passes the review: a LUT reference stays a reference, a drawn text is
/// drawn, and a text parameter the review does not name (a later engine's,
/// or one a project file made up) may hold only a path inside the area.
fn fence_param(effect: &str, param: &str, prm: &Param, area: &Path) -> Result<(), String> {
    for value in std::iter::once(&prm.value).chain(prm.keyframes.iter().map(|k| &k.value)) {
        let ParamValue::Text(text) = value else { continue };
        match text_param(effect, param) {
            Some(TextParam::Drawn) => {}
            Some(TextParam::Lut) => {
                if !(text.is_empty() || text.starts_with("builtin:") || text.starts_with("lib:")) {
                    return Err(format!(
                        "`{effect}` `{param}` is `{text}`: a LUT is `builtin:<id>` or `lib:<id>` (a LUT embedded in the project), and the door never lets a project name a LUT file"
                    ));
                }
            }
            None => {
                if !text.is_empty() && !inside(area, text) {
                    return Err(format!("`{effect}` `{param}` holds `{text}`, which is not a path inside this call's folder"));
                }
            }
        }
    }
    Ok(())
}

fn fence_effect(e: &EffectInstance, area: &Path) -> Result<(), String> {
    e.params.iter().try_for_each(|(param, prm)| fence_param(&e.effect, param, prm, area))
}

fn fence_lanes(lanes: &BTreeMap<String, Param>, area: &Path) -> Result<(), String> {
    lanes.iter().try_for_each(|(lane, prm)| fence_param("mixer", lane, prm, area))
}

/// The fence the door runs after every command (and on the project it
/// opens): nothing in the project or the session may point the engine at a
/// file or folder outside the area. The engine reads media and proxies only
/// through [`AreaServices`], which keeps those outside the area offline;
/// what it reads or writes with `std::fs` past its services is held here:
///
/// - the project's folders ([`PROJECT_FOLDERS`]): the scratch disks (render
///   previews are listed, read, moved and written there, captured audio and
///   auto-saves written) and the ingest destination (imports copy into it)
///   must all be unset;
/// - the session's paths: no queued export (the queue encodes to the paths
///   and image overlays its entries hold), no Media Browser selection (a
///   path-less import takes those host paths), no pending proxy, ingest or
///   Project Manager job;
/// - every effect's text parameters, on clips, transitions, tracks, the
///   master and the source graphics, values and keyframes ([`fence_param`]),
///   and the mixers' automation lanes.
fn fence(s: &Session, area: &Path) -> Result<(), String> {
    let p = &*s.project;
    let st = &p.settings;
    let folders = [&st.ingest.destination, &st.scratch.captured, &st.scratch.video_previews, &st.scratch.audio_previews, &st.scratch.auto_save];
    for ((what, _), dir) in PROJECT_FOLDERS.iter().zip(folders) {
        if let Some(dir) = dir.as_deref().filter(|d| !d.is_empty()) {
            return Err(names_a_folder(what, dir));
        }
    }
    if !s.export_queue.items.is_empty() {
        return Err("an export is queued, and the door encodes only its own `out`".into());
    }
    if !s.browser.selection.is_empty() {
        return Err("the Media Browser holds a selection of files, which a later import would read".into());
    }
    if !s.media_jobs.is_empty() {
        return Err("a proxy, ingest or Project Manager job is pending".into());
    }
    for item in p.items.values() {
        let ItemKind::Sequence(q) = &item.kind else { continue };
        for t in q.video_tracks.iter().chain(&q.audio_tracks).chain(&q.submix_tracks) {
            for e in t.items.iter().flat_map(|it| &it.effects).chain(t.transitions.iter().map(|x| &x.effect)).chain(&t.effects) {
                fence_effect(e, area)?;
            }
            fence_lanes(&t.mixer.lanes, area)?;
        }
        for e in &q.master_effects {
            fence_effect(e, area)?;
        }
        fence_lanes(&q.master_mixer.lanes, area)?;
    }
    for e in p.source_graphics.values().flat_map(|g| &g.layers) {
        fence_effect(e, area)?;
    }
    Ok(())
}

/// The folders a project file names, read before the engine opens it.
/// Opening points the engine's render-preview store at `<the Video Previews
/// scratch disk>/<name>` and lists that folder with `std::fs`
/// (`install_project` → `PreviewStore::set_dir`), so a project naming any
/// folder ([`PROJECT_FOLDERS`]) is refused before the engine sees it, as
/// [`fence`] refuses one after every command. The project lives under
/// `project` in the file's envelope (a schema-1 file is the bare project);
/// no schema upgrade moves these fields.
fn stored_folders(bytes: &[u8]) -> Result<(), String> {
    let doc: Json = serde_json::from_slice(bytes).map_err(|e| format!("not a FilmCraft project: {e}"))?;
    let settings = &doc.get("project").unwrap_or(&doc)["settings"];
    for (what, [group, key]) in PROJECT_FOLDERS {
        if let Some(dir) = settings[group][key].as_str().filter(|d| !d.is_empty()) {
            return Err(names_a_folder(what, dir));
        }
    }
    Ok(())
}

/// The engine's own project writer, caught in memory. `file.saveCopy`
/// serializes the project (`filmcraft_format::encode_with_view`, with what
/// is open) and hands the bytes to its services' `write_file`, without
/// adopting the path (no preview move, no recent-projects entry). For that
/// one command this stands in for the session's services: it keeps the one
/// write to `path` and refuses every other file access, so the engine
/// touches no disk; the service then writes the bytes under the call's rules.
struct Capture {
    path: String,
    bytes: Mutex<Option<Vec<u8>>>,
}

impl Services for Capture {
    fn read_file(&self, path: &str) -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{path}: the project writer reads nothing")))
    }
    fn write_file(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        if path != self.path {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{path}: the project writer writes only the project")));
        }
        *self.bytes.lock().unwrap_or_else(|e| e.into_inner()) = Some(data.to_vec());
        Ok(())
    }
}

/// The session's project as `.fcproj` bytes, named after `out` as Save As
/// names a project after its file.
fn project_bytes(s: &mut Session, out: &Path) -> Result<Vec<u8>, String> {
    if let Some(stem) = out.file_stem().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty() && *n != s.project.name) {
        let p = Arc::make_mut(&mut s.project);
        p.root.name.clone_from(&stem);
        p.name = stem;
    }
    let capture = Arc::new(Capture { path: out.to_string_lossy().into_owned(), bytes: Mutex::new(None) });
    let own = std::mem::replace(&mut s.services, capture.clone());
    let saved = s.execute("file.saveCopy", json!({"path": capture.path}));
    s.services = own;
    saved.map_err(|e| e.to_string())?;
    let bytes = capture.bytes.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or("the engine wrote no project")?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(format!("the project is {} bytes; the film service writes at most {MAX_PROJECT_BYTES}", bytes.len()));
    }
    Ok(bytes)
}

/// The frame size a door export encodes at: the sequence's, scaled down
/// (even sides) so its longest edge is at most [`MAX_RENDER_SIDE`]. A
/// sequence may be set as large as 16K, and the encoders work on whole
/// frames.
fn export_size(w: u32, h: u32) -> Option<(u32, u32)> {
    let long = w.max(h);
    if long <= MAX_RENDER_SIDE {
        return None;
    }
    let k = MAX_RENDER_SIDE as f64 / long as f64;
    let even = |v: u32| (((v as f64 * k) / 2.0).round() as u32 * 2).max(2);
    Some((even(w), even(h)))
}

/// What a `run` call writes.
enum Out {
    Project,
    Frame { at_ms: f64, max_side: u32 },
    Media { format: String },
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over `path`, the project fenced after each
/// ([`fence`]); then `out` written under the area's rules.
fn run(args: &Json, rules: &Arc<Area>) -> Result<Json, String> {
    let err = |e: String| format!("film.run: {e}");
    let area = canonical(rules).map_err(err)?;
    let admitted = door()?.admit_all(&args["cmds"], rules)?;
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(_) => {
            let (out_abs, out_rel) = output(rules, &area, args, "out").map_err(err)?;
            let ext = out_rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
            let kind = match ext.as_str() {
                "fcproj" | "png" if args.get("format").is_some_and(|f| !f.is_null()) => {
                    return Err(err(format!("`format` is for media; `{out_rel}` is written as .{ext}")));
                }
                "fcproj" => Out::Project,
                "png" => {
                    let (at_ms, max_side) = frame_args(args).map_err(err)?;
                    Out::Frame { at_ms, max_side }
                }
                _ if args["format"].is_null() && format_for(&out_rel).is_none() => {
                    return Err(err(format!(
                        "`out` ends .fcproj (the project), .png (a frame) or .mp4/.mov/.wav/.gif (media); or give `format` ({})",
                        FORMATS.join(" | ")
                    )));
                }
                _ => Out::Media { format: media_format(args, &out_rel).map_err(err)? },
            };
            Some((out_abs, out_rel, kind))
        }
        None if args.get("format").is_some_and(|f| !f.is_null()) => return Err(err("`format` needs an `out`".into())),
        None => None,
    };
    let rel = args["path"].as_str().unwrap_or_default();
    let project = rel.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("fcproj"));
    let (abs, rel) = input(&area, args, "path", if project { MAX_PROJECT_BYTES } else { MAX_MEDIA_BYTES }).map_err(err)?;

    let mut s = session(&area, rules);
    if project {
        let bytes = std::fs::read(&abs).map_err(|e| err(format!("{rel}: {e}")))?;
        stored_folders(&bytes).map_err(|e| err(format!("`{rel}`: {e}")))?;
        s.execute("file.open", json!({"path": abs.to_string_lossy()})).map_err(|e| err(format!("{rel}: {e}")))?;
    } else {
        clip_sequence(&mut s, &abs).map_err(err)?;
    }
    fence(&s, &area).map_err(|e| err(format!("`{rel}`: {e}")))?;
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        let r = s.execute(&id, params).map_err(|e| format!("film.run {id}: {e}"))?;
        fence(&s, &area).map_err(|e| format!("film.run {id}: {e}"))?;
        results.push(json!({"id": id, "result": r}));
    }

    let Some((out_abs, out_rel, kind)) = out else { return Ok(json!({"results": results, "out": Json::Null})) };
    let dest = canon_to_area(rules, &area, &out_abs);
    let mut answer = json!({"results": results, "out": out_rel});
    match kind {
        Out::Project => {
            let bytes = project_bytes(&mut s, &out_abs).map_err(err)?;
            rules.write(&dest, &bytes).map_err(err)?;
            answer["format"] = json!("fcproj");
            answer["bytes"] = json!(bytes.len());
        }
        Out::Frame { at_ms, max_side } => {
            let (size, rate, duration_ms) = active_shape(&s).map_err(err)?;
            let png = program_png(&s, size, rate, duration_ms, at_ms, max_side).map_err(err)?;
            rules.write(&dest, &png).map_err(err)?;
            let (w, h) = png_size(&png);
            for (k, v) in [("width", json!(w)), ("height", json!(h)), ("at_ms", json!(at_ms)), ("bytes", json!(png.len()))] {
                answer[k] = v;
            }
        }
        Out::Media { format } => {
            let ((w, h), _, duration_ms) = active_shape(&s).map_err(err)?;
            let (start_ms, end_ms) = range_ms(args, duration_ms, "sequence").map_err(err)?;
            let audio = args["audio"].as_bool().unwrap_or(true);
            let (bytes, _) = export_staged(&mut s, rules, &dest, &format, (start_ms, end_ms), audio, export_size(w, h)).map_err(err)?;
            for (k, v) in [("format", json!(format)), ("start_ms", json!(start_ms)), ("end_ms", json!(end_ms)), ("bytes", json!(bytes))] {
                answer[k] = v;
            }
        }
    }
    Ok(answer)
}

/// The active sequence's frame size, rate and duration (ms).
fn active_shape(s: &Session) -> Result<((u32, u32), FrameRate, f64), String> {
    let id = s.state.active_sequence.ok_or("no sequence is active")?;
    let q = s.project.sequence(id).ok_or("no sequence is active")?;
    Ok(((q.settings.width, q.settings.height), q.settings.frame_rate, q.duration().seconds() * 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;

    /// One host directory for the whole suite, with a real MP4 written by
    /// the engine's own H.264 and AAC encoders (no fixture bytes, no
    /// ffmpeg): half a second of the procedural Plasma demo scene at
    /// 160x90, exported into the film area.
    static HOST: LazyLock<(tempfile::TempDir, String)> = LazyLock::new(|| {
        let dir = tempfile::tempdir().unwrap();
        let film = dir.path().join("film");
        std::fs::create_dir_all(&film).unwrap();
        let mut s = Session::default();
        let r = s.execute("file.importDemoFootage", json!({"scene": "Plasma"})).unwrap();
        let id = ItemId(r["items"][0].as_u64().unwrap());
        let info = s.project.item(id).unwrap().as_media().unwrap().info.clone();
        lay_on_sequence(&mut s, id, &info).unwrap();
        s.execute(
            "file.exportMedia",
            json!({
                "path": film.join("clip.mp4").to_string_lossy(),
                "format": "h264", "width": 160, "height": 90,
                "range": "custom", "startSeconds": 0.0, "endSeconds": 0.5,
                "wait": true,
            }),
        )
        .unwrap();
        (dir, "clip.mp4".into())
    });

    fn host() -> &'static Path {
        HOST.0.path()
    }

    fn clip() -> &'static str {
        &HOST.1
    }

    #[test]
    fn info_reads_the_container_streams_and_duration() {
        let v = dispatch("info", &json!({"path": clip()}), host()).unwrap();
        assert_eq!(v["video"]["width"], json!(160), "{v}");
        assert_eq!(v["video"]["height"], json!(90));
        let ms = v["duration_ms"].as_f64().unwrap();
        assert!((400.0..=700.0).contains(&ms), "{ms}");
        assert!(v["audio"].is_object(), "the demo scene has sound: {v}");
        assert!(v["container"].as_str().is_some_and(|c| !c.is_empty()));
        assert_eq!(v["file"], json!(clip()));
    }

    #[test]
    fn frame_renders_a_png_through_the_program_monitor() {
        let v = dispatch("frame", &json!({"path": clip(), "at_ms": 250, "out": "shots/f.png", "max_side": 64}), host()).unwrap();
        assert_eq!(v["width"], json!(64), "{v}");
        assert_eq!(v["height"], json!(36));
        let png = std::fs::read(host().join("film/shots/f.png")).unwrap();
        assert_eq!(png.len() as u64, v["bytes"].as_u64().unwrap());
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (64, 36));
        let (lo, hi) = img.pixels().fold((255u8, 0u8), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])));
        assert!(hi > lo, "a decoded plasma frame is not flat");
    }

    #[test]
    fn export_reencodes_with_the_engines_own_encoders() {
        // A WAV of the first quarter second…
        let v = dispatch("export", &json!({"path": clip(), "out": "sound.wav", "end_ms": 250}), host()).unwrap();
        assert_eq!(v["format"], json!("wav"), "{v}");
        assert!(v["bytes"].as_u64().unwrap() > 44);
        // …an H.264+AAC MP4 of the same range, readable back through `info`…
        let v = dispatch("export", &json!({"path": clip(), "out": "cut.mp4", "start_ms": 0, "end_ms": 250}), host()).unwrap();
        assert_eq!(v["format"], json!("h264"));
        let back = dispatch("info", &json!({"path": "cut.mp4"}), host()).unwrap();
        assert!(back["video"].is_object(), "{back}");
        let ms = back["duration_ms"].as_f64().unwrap();
        assert!((150.0..=400.0).contains(&ms), "{ms}");
        // …and the range cap holds.
        let e = dispatch("export", &json!({"path": clip(), "out": "x.mp4", "start_ms": 0, "end_ms": 600_000}), host()).unwrap_err();
        assert!(e.contains("at most"), "{e}");
    }

    #[test]
    fn project_info_opens_a_saved_project() {
        // Written by the engine itself: import the clip, save the project
        // into the area (through the contained services).
        let film = host().join("film").canonicalize().unwrap();
        let rules = Arc::new(Area::new(&film, None, true));
        let mut s = session(&film, &rules);
        let abs = film.join(clip());
        s.execute("file.import", json!({"paths": [abs.to_string_lossy()]})).unwrap();
        s.execute("file.saveAs", json!({"path": film.join("p.fcproj").to_string_lossy()})).unwrap();

        let v = dispatch("project.info", &json!({"path": "p.fcproj"}), host()).unwrap();
        assert_eq!(v["file"], json!("p.fcproj"));
        assert!(!v.to_string().contains(&*film.to_string_lossy()), "no host path in the answer: {v}");
        let project = v["project"].to_string();
        assert!(project.contains("clip"), "the imported clip is in the tree: {project}");
        // As `tools.json` declares the answer.
        assert!(v["project"].is_object(), "{v}");
        assert!(v["sequence"].is_object() || v["sequence"].is_null(), "{v}");
    }

    #[test]
    fn paths_stay_inside_the_film_area() {
        for bad in ["../up.mp4", "/etc/hosts", "a/../../up.mp4"] {
            let e = dispatch("info", &json!({"path": bad}), host()).unwrap_err();
            assert!(e.contains("film area") || e.contains("required"), "{bad}: {e}");
        }
        // Writes too.
        let e = dispatch("frame", &json!({"path": clip(), "out": "../f.png"}), host()).unwrap_err();
        assert!(e.contains("film area"), "{e}");
        // A symlink inside the area pointing out is an escape: `beside.txt`
        // sits beside the area, where Mail's and Calendar's data live.
        #[cfg(unix)]
        {
            std::fs::write(host().join("beside.txt"), b"not the film service's").unwrap();
            std::os::unix::fs::symlink(host(), host().join("film/esc")).unwrap();
            let e = dispatch("info", &json!({"path": "esc/beside.txt"}), host()).unwrap_err();
            assert!(e.contains("film area"), "{e}");
            // The engine-side guard refuses strays on its own (a project
            // file could carry any path), while area files stay readable.
            let film = host().join("film").canonicalize().unwrap();
            let svc = AreaServices { area: film.clone(), rules: Arc::new(Area::new(&film, None, true)) };
            assert!(svc.read_file(&host().join("film/esc/beside.txt").to_string_lossy()).is_err());
            assert!(svc.read_file("/etc/hosts").is_err());
            assert!(svc.read_file(&host().join("film/clip.mp4").to_string_lossy()).is_ok());
        }
    }

    #[test]
    fn the_area_is_the_film_subdir_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let a = Slot::new().area(&service_call("info", json!({}), dir.path(), false), "film").unwrap();
        assert!(a.root.is_dir());
        assert_eq!(a.root, dir.path().join("film"));
        // Never the host dir itself (Mail's and Calendar's data live
        // beside it), and as before: replacing allowed, no quota.
        assert!(a.may_replace && a.quota_left.is_none());
        assert_ne!(canonical(&a).unwrap(), dir.path().canonicalize().unwrap());
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("film.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
    }

    /// A resolver shaped like the shell's: every call works in `root`, an
    /// app's own foreground call may replace a file and an agent's may not,
    /// within `quota`.
    fn resolver(root: &Path, quota: Option<u64>) -> Slot {
        let slot = Slot::new();
        let root = root.to_path_buf();
        slot.set(Some(Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, quota, call.may_prompt)))));
        slot
    }

    /// The suite's clip, copied into a caller's folder.
    fn clip_in(root: &Path) -> &'static str {
        std::fs::create_dir_all(root).unwrap();
        std::fs::copy(host().join("film").join(clip()), root.join(clip())).unwrap();
        clip()
    }

    fn no_staging_left(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(octosense_engine_area::STAGING_PREFIX))
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too; an export the engine
    /// streams itself lands where it was asked.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        let clip = clip_in(&root);
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let v = serve(&areas, &service_call("info", json!({"path": clip}), &host, false)).unwrap();
        assert_eq!(v["video"]["width"], json!(160), "{v}");
        serve(&areas, &service_call("frame", json!({"path": clip, "out": "shots/f.png", "max_side": 32}), &host, false)).unwrap();
        let e = serve(&areas, &service_call("export", json!({"path": clip, "out": "cut/a.wav", "end_ms": 100}), &host, false)).unwrap();
        assert!(root.join("shots/f.png").is_file() && root.join("cut/a.wav").is_file(), "{e}");
        assert_eq!(e["bytes"].as_u64().unwrap(), std::fs::metadata(root.join("cut/a.wav")).unwrap().len());
        assert!(!host.exists() && !root.join("film").exists() && no_staging_left(&root));
        std::fs::write(dir.path().join("beside.mp4"), b"x").unwrap();
        for bad in ["../beside.mp4", "/etc/hosts", "cut/../../beside.mp4"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("export", json!({"path": clip, "out": bad, "end_ms": 100}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.mp4"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("frame", json!({"path": clip, "out": "up/f.png"}), &host, true)).is_err());
            assert!(serve(&areas, &service_call("export", json!({"path": clip, "out": "up/x.wav", "end_ms": 100}), &host, true)).is_err());
            assert!(!dir.path().join("f.png").exists() && !dir.path().join("x.wav").exists());
        }
    }

    /// An agent's call never replaces a file, before the engine works; an
    /// app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("taken.wav"), b"keep me").unwrap();
        std::fs::write(dir.path().join("taken.png"), b"keep me").unwrap();
        for (method, args) in [
            ("export", json!({"path": clip, "out": "taken.wav", "end_ms": 100})),
            ("frame", json!({"path": clip, "out": "taken.png"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        assert_eq!(std::fs::read(dir.path().join("taken.wav")).unwrap(), b"keep me");
        assert!(no_staging_left(dir.path()));
        serve(&areas, &service_call("export", json!({"path": clip, "out": "taken.wav", "end_ms": 100}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.wav")).unwrap().starts_with(b"RIFF"));
    }

    /// What a call writes, the engine's streamed export included, must fit
    /// what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let tight = resolver(dir.path(), Some(64));
        for (method, args) in [
            ("export", json!({"path": clip, "out": "a.wav", "end_ms": 100})),
            ("frame", json!({"path": clip, "out": "f.png"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        assert!(!dir.path().join("a.wav").exists() && !dir.path().join("f.png").exists() && no_staging_left(dir.path()));
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.films"));
        assert!(!may_call("org.example.app"));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "film", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("film.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }

    // -----------------------------------------------------------------------
    // the command door
    // -----------------------------------------------------------------------

    /// A `film.run` call, as App Hub hands it to the service.
    fn door_run(areas: &Slot, args: Json, root: &Path, may_prompt: bool) -> Result<Json, String> {
        serve(areas, &service_call("run", args, root, may_prompt))
    }

    /// The first video clip of the sequence a `run` over `path` starts on.
    /// Sessions are deterministic, so a later call names the same clip.
    fn first_clip(areas: &Slot, root: &Path, path: &str) -> u64 {
        let v = door_run(areas, json!({"path": path, "cmds": [{"id": "sequence.inspect"}]}), root, false).unwrap();
        v["results"][0]["result"]["video"][0]["items"][0]["clip"].as_u64().unwrap_or_else(|| panic!("{v}"))
    }

    /// The door runs allowlisted commands in a temporary area — a built-in
    /// effect, a numeric parameter, a marker — and writes each kind of
    /// `out`: the project (read back with `project.info`, and as the next
    /// call's `path`), media and a frame.
    #[test]
    fn the_door_runs_allowlisted_commands_and_writes_each_kind_of_out() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let c = first_clip(&areas, dir.path(), clip);
        let edits = json!([
            {"id": "effects.apply", "params": {"clips": [c], "effect": "Gaussian Blur"}},
            {"id": "effects.setParam", "params": {"clip": c, "effect": "gaussian_blur", "param": "blurriness", "value": 12.5}},
            {"id": "markers.add", "params": {"name": "Door marker"}},
        ]);
        let v = door_run(&areas, json!({"path": clip, "cmds": edits, "out": "edit.fcproj"}), dir.path(), false).unwrap();
        assert_eq!(v["out"], json!("edit.fcproj"), "{v}");
        assert_eq!(v["format"], json!("fcproj"));
        assert_eq!(v["results"].as_array().unwrap().len(), 3);
        assert_eq!(v["results"][0]["id"], json!("effects.apply"));
        assert_eq!(v["bytes"].as_u64().unwrap(), std::fs::metadata(dir.path().join("edit.fcproj")).unwrap().len());
        let back = serve(&areas, &service_call("project.info", json!({"path": "edit.fcproj"}), dir.path(), false)).unwrap();
        assert_eq!(back["project"]["name"], json!("edit"), "named after its file, as Save As names one: {back}");
        let seq = &back["sequence"];
        assert_eq!(seq["markers"][0]["name"], json!("Door marker"), "{seq}");
        let effects = seq["video"][0]["items"][0]["effects"].as_array().unwrap();
        let blur = effects.iter().find(|e| e["effect"] == json!("gaussian_blur")).unwrap_or_else(|| panic!("{seq}"));
        assert_eq!(blur["params"]["blurriness"]["value"], json!("Float(12.5)"), "{blur}");

        let mp4 = door_run(&areas, json!({"path": clip, "cmds": edits, "out": "edit.mp4", "end_ms": 250}), dir.path(), false).unwrap();
        assert_eq!((mp4["out"].as_str(), mp4["format"].as_str()), (Some("edit.mp4"), Some("h264")), "{mp4}");
        assert_eq!((mp4["start_ms"].as_f64(), mp4["end_ms"].as_f64()), (Some(0.0), Some(250.0)));
        assert_eq!(mp4["bytes"].as_u64().unwrap(), std::fs::metadata(dir.path().join("edit.mp4")).unwrap().len());
        let info = serve(&areas, &service_call("info", json!({"path": "edit.mp4"}), dir.path(), false)).unwrap();
        assert!(info["video"].is_object(), "{info}");
        let gif = door_run(&areas, json!({"path": clip, "cmds": edits, "out": "edit.gif", "audio": false}), dir.path(), false).unwrap();
        assert_eq!(gif["format"], json!("gif"), "{gif}");
        assert!(std::fs::read(dir.path().join("edit.gif")).unwrap().starts_with(b"GIF"));

        let png = door_run(&areas, json!({"path": clip, "cmds": edits, "out": "shots/edit.png", "at_ms": 250, "max_side": 64}), dir.path(), false).unwrap();
        assert_eq!((png["width"].as_u64(), png["height"].as_u64(), png["at_ms"].as_f64()), (Some(64), Some(36), Some(250.0)), "{png}");
        let img = image::load_from_memory(&std::fs::read(dir.path().join("shots/edit.png")).unwrap()).unwrap();
        assert_eq!((img.width(), img.height()), (64, 36));
        assert_eq!(png["bytes"].as_u64().unwrap(), std::fs::metadata(dir.path().join("shots/edit.png")).unwrap().len());

        // The saved project is a door input in turn, inspected as
        // `project.info` inspects one; without `out` nothing is written.
        let p = door_run(&areas, json!({"path": "edit.fcproj", "cmds": [{"id": "project.inspect"}, {"id": "sequence.inspect"}]}), dir.path(), false).unwrap();
        assert!(p["out"].is_null(), "{p}");
        assert_eq!(p["results"][0]["result"]["path"], json!("edit.fcproj"), "never the host's path: {p}");
        assert_eq!(p["results"][1]["result"]["markers"][0]["name"], json!("Door marker"));
        assert!(no_staging_left(dir.path()));
    }

    /// A still picture is a door input too: laid on a sequence for its
    /// still duration, rendered as a frame and encoded as media.
    #[test]
    fn a_still_picture_runs_through_the_door() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let still = image::RgbaImage::from_fn(32, 18, |x, y| image::Rgba([(x * 8) as u8, (y * 14) as u8, 128, 255]));
        still.save(dir.path().join("still.png")).unwrap();
        let v = door_run(&areas, json!({"path": "still.png", "cmds": [{"id": "project.inspect"}], "out": "f.png", "max_side": 16}), dir.path(), false).unwrap();
        assert_eq!((v["width"].as_u64(), v["height"].as_u64()), (Some(16), Some(9)), "{v}");
        let v = door_run(&areas, json!({"path": "still.png", "cmds": [], "out": "still.gif", "end_ms": 200}), dir.path(), false).unwrap();
        assert_eq!((v["format"].as_str(), v["end_ms"].as_f64()), (Some("gif"), Some(200.0)), "{v}");
        assert!(std::fs::read(dir.path().join("still.gif")).unwrap().starts_with(b"GIF"));
    }

    /// Every class but `safe` (and the reviewed caption read) is refused,
    /// and so is an id the classification does not know, before any command
    /// runs: a refused id anywhere in the list writes nothing.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        // film's classification has no `code` class: no film command runs
        // another by a name it is handed.
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        assert_eq!(safety["classes"]["code"]["count"], json!(0));
        let outside = tempfile::tempdir().unwrap();
        let elsewhere = outside.path().to_string_lossy().into_owned();
        std::fs::write(outside.path().join("secret.mp4"), b"not yours").unwrap();
        let secret = outside.path().join("secret.mp4").to_string_lossy().into_owned();
        let refused = |id: &str, params: Json| {
            door_run(&areas, json!({"path": clip, "cmds": [{"id": "markers.add"}, {"id": id, "params": params}], "out": "x.fcproj"}), dir.path(), false).unwrap_err()
        };
        for (id, params, class) in [
            ("prefs.set", json!({"key": "mediaCache.location", "value": elsewhere}), "host"),
            ("prefs.reset", json!({}), "host"),
            ("media.toggleProxies", json!({}), "host"),
            ("fonts.list", json!({}), "host"),
            ("shortcuts.set", json!({"command": "file.import", "key": "Cmd+I"}), "host"),
            ("audio.voiceover.start", json!({}), "device"),
            ("multicam.transmitView", json!({}), "device"),
            ("transcript.downloadModel", json!({"model": "tiny"}), "network"),
        ] {
            let e = refused(id, params);
            assert!(e.contains(&format!("`{id}` is classed {class}")) && e.contains("never runs it"), "{id}: {e}");
        }
        for (id, params) in [
            ("file.import", json!({"paths": [secret]})),
            ("file.saveAs", json!({"path": "elsewhere.fcproj"})),
            ("graphics.template.apply", json!({"template": secret})),
            ("export.queue.add", json!({"path": secret})),
            ("project.ingestSettings", json!({"enabled": true, "destination": elsewhere})),
            ("file.projectSettings.scratchDisks", json!({"videoPreviews": elsewhere})),
            ("lut.import", json!({"path": secret})),
            ("lumetri.setInputLut", json!({"path": secret})),
            ("mediaBrowser.select", json!({"paths": [secret]})),
            ("presets.import", json!({"path": secret})),
        ] {
            let e = refused(id, params);
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        let e = refused("film.secret", json!({}));
        assert!(e.contains("not a reviewed film command"), "{e}");
        assert!(!dir.path().join("x.fcproj").exists(), "nothing written");
        assert!(no_staging_left(dir.path()));
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "markers.add"})).collect();
        let e = door_run(&areas, json!({"path": clip, "cmds": too_many}), dir.path(), false).unwrap_err();
        assert!(e.contains("at most 64"), "{e}");
    }

    /// The fence after each command: a path a command plants in the project
    /// (a LUT parameter given a file) refuses the call, with nothing
    /// written, and so does a project file whose scratch disks name a folder,
    /// before the engine opens it.
    #[test]
    fn the_fence_refuses_a_path_a_command_plants_or_a_project_names() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let c = first_clip(&areas, dir.path(), clip);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("look.cube"), b"LUT_3D_SIZE 2\n").unwrap();
        let cube = outside.path().join("look.cube").to_string_lossy().into_owned();
        std::fs::write(dir.path().join("mine.cube"), b"LUT_3D_SIZE 2\n").unwrap();
        let mine = dir.path().join("mine.cube").to_string_lossy().into_owned();
        let lut = |param: &str, value: &str| {
            json!({"path": clip, "cmds": [
                {"id": "effects.apply", "params": {"clips": [c], "effect": "lumetri"}},
                {"id": "effects.setParam", "params": {"clip": c, "effect": "lumetri", "param": param, "value": value}},
                {"id": "markers.add"},
            ], "out": "graded.fcproj"})
        };
        for (param, value) in [("input_lut", cube.as_str()), ("look_lut", "/etc/hosts"), ("input_lut", "../look.cube"), ("look_lut", mine.as_str())] {
            let e = door_run(&areas, lut(param, value), dir.path(), false).unwrap_err();
            assert!(e.starts_with("film.run effects.setParam: ") && e.contains(&format!("`lumetri` `{param}`")), "{param} = {value}: {e}");
        }
        assert!(!dir.path().join("graded.fcproj").exists() && no_staging_left(dir.path()), "nothing written");
        // A LUT reference passes: built in, or embedded in the project.
        let v = door_run(&areas, lut("input_lut", "builtin:slog3-sgamut3cine-to-rec709"), dir.path(), false).unwrap();
        assert_eq!(v["out"], json!("graded.fcproj"), "{v}");

        // A project file whose scratch disk names a folder outside: the
        // engine never opens it (it would list that folder for previews).
        let root = dir.path().canonicalize().unwrap();
        let rules = Arc::new(Area::new(&root, None, true));
        let mut s = session(&root, &rules);
        s.execute("file.projectSettings.scratchDisks", json!({"videoPreviews": outside.path().to_string_lossy()})).unwrap();
        s.execute("file.saveCopy", json!({"path": root.join("scratch.fcproj").to_string_lossy()})).unwrap();
        let e = door_run(&areas, json!({"path": "scratch.fcproj", "cmds": [{"id": "project.inspect"}]}), dir.path(), false).unwrap_err();
        assert!(e.contains("`scratch.fcproj`") && e.contains("video-preview scratch disk"), "{e}");
        // `project.info` opens no such project either.
        let e = serve(&areas, &service_call("project.info", json!({"path": "scratch.fcproj"}), dir.path(), false)).unwrap_err();
        assert!(e.starts_with("film.project.info: ") && e.contains("video-preview scratch disk"), "{e}");
        // A project file that already holds a planted path is fenced as it
        // opens, before any command runs.
        let mut s = session(&root, &rules);
        clip_sequence(&mut s, &root.join(clip)).unwrap();
        let seq = s.state.active_sequence.unwrap();
        let c = s.project.sequence(seq).unwrap().video_tracks[0].items[0].id.0;
        s.execute("effects.apply", json!({"clips": [c], "effect": "lumetri"})).unwrap();
        s.execute("effects.setParam", json!({"clip": c, "effect": "lumetri", "param": "look_lut", "value": cube})).unwrap();
        s.execute("file.saveCopy", json!({"path": root.join("planted.fcproj").to_string_lossy()})).unwrap();
        let e = door_run(&areas, json!({"path": "planted.fcproj", "cmds": [{"id": "project.inspect"}], "out": "again.fcproj"}), dir.path(), false).unwrap_err();
        assert!(e.contains("`planted.fcproj`") && e.contains("`lumetri` `look_lut`"), "{e}");
        assert!(!dir.path().join("again.fcproj").exists());
    }

    /// What the fence holds beyond the commands a door call can run: the
    /// project's folders, the session's paths (a queued export, a Media
    /// Browser selection), and text parameters a project file made up.
    #[test]
    fn the_fence_holds_the_projects_folders_and_the_sessions_paths() {
        use filmcraft_engine::export_tools::{QueueItem, QueueStatus};
        let dir = tempfile::tempdir().unwrap();
        let area = dir.path().canonicalize().unwrap();
        let rules = Arc::new(Area::new(&area, None, false));
        let fresh = || session(&area, &rules);
        assert!(fence(&fresh(), &area).is_ok());
        let mut s = fresh();
        Arc::make_mut(&mut s.project).settings.ingest.destination = Some(area.join("ingest").to_string_lossy().into_owned());
        assert!(fence(&s, &area).unwrap_err().contains("ingest destination is set"), "a folder anywhere, inside too");
        let mut s = fresh();
        Arc::make_mut(&mut s.project).settings.scratch.auto_save = Some("Same as Project".into());
        assert!(fence(&s, &area).unwrap_err().contains("auto-save scratch disk"));
        let mut s = fresh();
        s.browser.selection.push("/elsewhere/a.mov".into());
        assert!(fence(&s, &area).unwrap_err().contains("Media Browser"));
        let mut s = fresh();
        let project = s.project.clone();
        s.export_queue.items.push(QueueItem {
            id: 1,
            sequence: ItemId(1),
            sequence_name: "S".into(),
            preset: String::new(),
            settings: Default::default(),
            status: QueueStatus::Ready,
            job: None,
            error: None,
            project,
        });
        assert!(fence(&s, &area).unwrap_err().contains("export is queued"));
        // A text parameter the review does not name holds only a path inside
        // the area; a reviewed drawn text holds anything.
        let with = |param: &str, text: &str| {
            let mut s = fresh();
            let p = Arc::make_mut(&mut s.project);
            let seq = p.new_sequence("S", SequenceSettings::default(), 1, 1, None);
            let mut e = filmcraft_engine::project::find_effect("simple_text").unwrap().instance();
            e.params.insert(param.into(), Param::new(ParamValue::Text(text.into())));
            p.sequence_mut(seq).unwrap().master_effects.push(e);
            fence(&s, &area)
        };
        assert!(with("file", "/etc/hosts").unwrap_err().contains("`simple_text` `file` holds `/etc/hosts`"));
        assert!(with("file", "pictures/a.png").is_err(), "a relative path would resolve against the working directory");
        assert!(with("file", &area.join("a.png").to_string_lossy()).is_ok());
        assert!(with("text", "/etc/hosts").is_ok(), "drawn, never opened");
    }

    /// The reviewed caption read takes a file inside the area only (the
    /// gate hands the engine its resolved path), of bounded size, and adds
    /// its captions to the sequence.
    #[test]
    fn the_doors_caption_read_keeps_to_the_area() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        std::fs::create_dir(dir.path().join("subs")).unwrap();
        let srt = "1\n00:00:00,000 --> 00:00:00,400\nHello from the door\n\n";
        std::fs::write(dir.path().join("subs/a.srt"), srt).unwrap();
        let v = door_run(
            &areas,
            json!({"path": clip, "cmds": [{"id": "captions.import", "params": {"path": "subs/a.srt"}}, {"id": "captions.list"}], "out": "captioned.fcproj"}),
            dir.path(),
            false,
        )
        .unwrap();
        assert_eq!(v["results"][0]["result"]["captions"], json!(1), "{v}");
        assert!(v["results"][1]["result"].to_string().contains("Hello from the door"), "{v}");
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("b.srt"), srt).unwrap();
        let secret = outside.path().join("b.srt").to_string_lossy().into_owned();
        for bad in [secret.as_str(), "../b.srt", "subs/../../b.srt", "subs/missing.srt"] {
            let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "captions.import", "params": {"path": bad}}]}), dir.path(), false).unwrap_err();
            assert!(e.contains("captions.import") && e.contains("`path`"), "{bad}: {e}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path().join("b.srt"), dir.path().join("subs/link.srt")).unwrap();
            let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "captions.import", "params": {"path": "subs/link.srt"}}]}), dir.path(), false).unwrap_err();
            assert!(e.contains("`path`"), "a link out is refused: {e}");
        }
        // What one call's reviewed reads total is bounded (a sparse file:
        // only its length matters).
        let big = std::fs::File::create(dir.path().join("subs/big.srt")).unwrap();
        big.set_len(octosense_engine_area::door::MAX_READ_BYTES + 1).unwrap();
        let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "captions.import", "params": {"path": "subs/big.srt"}}]}), dir.path(), false).unwrap_err();
        assert!(e.contains("captions.import") && e.contains("total more than"), "{e}");
    }

    /// `out` keeps the area's rules for every kind: an agent's call never
    /// replaces a file (an app's own may), what is written fits the quota,
    /// and `path` and `out` stay inside the area.
    #[test]
    fn the_doors_out_keeps_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        for taken in ["taken.fcproj", "taken.wav", "taken.png"] {
            std::fs::write(dir.path().join(taken), b"keep me").unwrap();
            let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "markers.add"}], "out": taken, "end_ms": 100}), dir.path(), false).unwrap_err();
            assert!(e.contains("already exists"), "{taken}: {e}");
            assert_eq!(std::fs::read(dir.path().join(taken)).unwrap(), b"keep me");
        }
        door_run(&areas, json!({"path": clip, "cmds": [], "out": "taken.fcproj"}), dir.path(), true).unwrap();
        assert!(std::fs::read(dir.path().join("taken.fcproj")).unwrap().starts_with(b"{"), "an app's own call may replace");
        let tight = resolver(dir.path(), Some(64));
        for out in ["q.fcproj", "q.wav", "q.png"] {
            let e = door_run(&tight, json!({"path": clip, "cmds": [], "out": out, "end_ms": 100}), dir.path(), true).unwrap_err();
            assert!(e.contains("bytes left"), "{out}: {e}");
            assert!(!dir.path().join(out).exists(), "{out}");
        }
        let outside = tempfile::tempdir().unwrap();
        std::fs::copy(dir.path().join(clip), outside.path().join("beside.mp4")).unwrap();
        for bad in ["../x.fcproj", "/etc/x.fcproj", "a/../../x.fcproj"] {
            assert!(door_run(&areas, json!({"path": clip, "cmds": [], "out": bad}), dir.path(), true).is_err(), "{bad}");
        }
        let beside = outside.path().join("beside.mp4").to_string_lossy().into_owned();
        for bad in [beside.as_str(), "../beside.mp4", "", "nothing.mp4"] {
            assert!(door_run(&areas, json!({"path": bad, "cmds": []}), dir.path(), true).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), dir.path().join("up")).unwrap();
            assert!(door_run(&areas, json!({"path": "up/beside.mp4", "cmds": []}), dir.path(), true).is_err());
            for out in ["up/x.fcproj", "up/x.wav", "up/x.png"] {
                assert!(door_run(&areas, json!({"path": clip, "cmds": [], "out": out, "end_ms": 100}), dir.path(), true).is_err(), "{out}");
            }
            assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1, "nothing landed beside the area");
        }
        let e = door_run(&areas, json!({"path": clip, "cmds": [], "out": "x.txt"}), dir.path(), true).unwrap_err();
        assert!(e.contains(".fcproj") && e.contains(".png"), "{e}");
        let e = door_run(&areas, json!({"path": clip, "cmds": [], "out": "x.png", "format": "gif"}), dir.path(), true).unwrap_err();
        assert!(e.contains("`format` is for media"), "{e}");
        assert!(no_staging_left(dir.path()));
    }

    /// The door's gate is built from the generated classification and the
    /// reviewed settlements: inner ids name only built-in effects and
    /// presets.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        for id in ["effects.apply", "effects.setParam", "markers.add", "project.inspect", "sequence.inspect", "captions.import"] {
            assert!(door.runs(id), "{id}");
        }
        for id in ["file.import", "file.saveAs", "prefs.set", "lut.import", "graphics.template.apply", "export.queue.add", "transcript.downloadModel"] {
            assert!(!door.runs(id), "{id}");
        }
        assert!(door.runnable().len() > 500, "{}", door.runnable().len());
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let preset = filmcraft_engine::presets::builtin_presets()[0].name.clone();
        let look = filmcraft_engine::render::lumetri_presets::presets()[0].name;
        let sound = filmcraft_engine::project::essential::builtin_presets()[0].name.clone();
        for (id, params) in [
            ("effects.apply", json!({"effect": "gaussian_blur"})),
            ("effects.apply", json!({"effect": "Gaussian Blur"})),
            ("sequence.applyVideoTransition", json!({"effect": "Cross Dissolve"})),
            ("sequence.applyAudioTransition", json!({"effect": "constant_power"})),
            ("effects.setDefaultTransition", json!({"effect": "cross_dissolve"})),
            ("mixer.addInsert", json!({"strip": "A1", "effect": "phaser"})),
            ("presets.apply", json!({"preset": preset})),
            ("presets.apply", json!({"name": preset})),
            ("lumetri.applyPreset", json!({"name": look})),
            ("essentialSound.applyPreset", json!({"preset": sound})),
        ] {
            assert!(door.admit(id, &params, &area).is_ok(), "{id} {params}");
        }
        for (id, params) in [
            ("effects.apply", json!({"effect": "/Library/Plug-Ins/evil.ofx"})),
            ("effects.apply", json!({"effect": ["gaussian_blur", "evil"]})),
            ("sequence.applyVideoTransition", json!({"effect": "evil"})),
            ("mixer.addInsert", json!({"effect": 7})),
            ("presets.apply", json!({"preset": "My Preset"})),
            ("presets.apply", json!({"preset": preset, "name": "evil"})),
            ("lumetri.applyPreset", json!({"name": "~/Looks/evil.look"})),
            ("essentialSound.applyPreset", json!({"preset": "user preset"})),
        ] {
            let e = door.admit(id, &params, &area).unwrap_err();
            assert!(e.contains("is not an effect the engine builds in"), "{id} {params}: {e}");
        }
    }

    /// Every text parameter of the pinned engine's effects is in the
    /// fence's review ([`TEXT_PARAMS`]): an engine that gains one fails here
    /// until someone reads what the renderer does with it.
    #[test]
    fn every_text_parameter_of_the_engine_is_reviewed() {
        let mut found: Vec<(&str, &str)> = filmcraft_engine::project::effect_defs()
            .iter()
            .flat_map(|d| d.params.iter().filter(|p| matches!(p.kind, filmcraft_engine::project::ParamKind::Text)).map(move |p| (d.id, p.id)))
            .collect();
        found.sort();
        let mut reviewed: Vec<(&str, &str)> = TEXT_PARAMS.iter().map(|(e, p, _)| (*e, *p)).collect();
        reviewed.sort();
        assert_eq!(found, reviewed);
    }

    /// Every `film.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        // A four-second clip the examples name, made by the engine itself,
        // and its captions.
        let mut s = Session::default();
        let r = s.execute("file.importDemoFootage", json!({"scene": "Plasma"})).unwrap();
        let id = ItemId(r["items"][0].as_u64().unwrap());
        let info = s.project.item(id).unwrap().as_media().unwrap().info.clone();
        lay_on_sequence(&mut s, id, &info).unwrap();
        let talk = dir.path().join("talk.mp4");
        s.execute(
            "file.exportMedia",
            json!({"path": talk.to_string_lossy(), "format": "h264", "width": 160, "height": 90, "range": "custom", "startSeconds": 0.0, "endSeconds": 4.0, "wait": true}),
        )
        .unwrap();
        std::fs::write(dir.path().join("talk.srt"), "1\n00:00:00,500 --> 00:00:02,000\nHello there\n\n2\n00:00:02,500 --> 00:00:03,500\nThanks for watching\n").unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("film.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 4, "{ran} examples");
        let thumb = std::fs::read(dir.path().join("thumb.png")).unwrap();
        assert_eq!(png_size(&thumb), (160, 90), "at most `max_side`: a small clip's frame is not enlarged");
        let project = serve(&areas, &service_call("run", json!({"path": "talk.fcproj", "cmds": [{"id": "captions.list"}]}), dir.path(), false)).unwrap();
        assert!(project["results"][0]["result"].to_string().contains("Thanks for watching"), "{project}");
    }
}
