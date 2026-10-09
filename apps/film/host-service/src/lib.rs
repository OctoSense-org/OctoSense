//! `octosense-film-service` — the `film` host service (ADR 0013).
//!
//! filmcraft's video engine (their crates, unchanged, pinned) behind typed
//! `film.*` methods. Pure Rust and fully headless: the engine's own
//! demuxers (MP4/MOV, Matroska/WebM, MXF, MPEG TS/PS, Ogg), decoders
//! (H.264, HEVC, VP9, AV1, ProRes, DNx, AAC, Opus, …) and encoders
//! (H.264, AAC, ProRes, PCM, GIF) — no ffmpeg, no system codecs, no GPU,
//! no network. Every call is a fresh, stateless session.
//!
//! Methods (all under the `film` family; paths relative to the caller's
//! film area):
//! - `info {path}` → container, streams and duration as JSON
//! - `frame {path, at_ms?, out, max_side?}` → the frame at `at_ms`
//!   through the engine's program renderer, written to `out` as PNG
//! - `export {path, out, format?, start_ms?, end_ms?, audio?}` → the
//!   range transcoded by the engine's own encoders (`h264` MP4 with AAC,
//!   `prores` MOV, `wav`, `gif`), at most [`MAX_EXPORT_MS`] per call
//! - `project.info {path}` → a `.fcproj` opened headlessly and inspected
//!   (`project.inspect` / `sequence.inspect`)
//!
//! Containment: the service works only inside `<host dir>/film` — the
//! shared `.host` directory also holds Mail's and Calendar's data, which
//! this service must never see. Boundary paths are checked here (`..`,
//! absolute paths and symlink escapes are refused, the sheets service's
//! stance rooted one level down), and every read the engine makes on its
//! own — media a project file points at included — passes
//! [`AreaServices`], which refuses anything outside the area again.
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use filmcraft_engine::project::{resolve_auto_points, ItemId, SequenceSettings, TrackKind};
use filmcraft_engine::time::{FrameRate, Tick, TimeRange};
use filmcraft_engine::{Services, Session};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value as Json};

/// The largest media file the service reads (bytes).
const MAX_MEDIA_BYTES: u64 = 512 << 20;
/// The largest project file `project.info` opens (bytes).
const MAX_PROJECT_BYTES: u64 = 64 << 20;
/// The longest range one `export` call encodes (milliseconds).
const MAX_EXPORT_MS: f64 = 300_000.0;
/// The longest edge `frame` produces, and its default.
const MAX_RENDER_SIDE: u32 = 4096;
const DEFAULT_RENDER_SIDE: u32 = 1024;
/// The export formats offered: what the engine encodes with its own code
/// (H.264+AAC in MP4, ProRes 422 HQ in MOV, PCM WAV, animated GIF).
const FORMATS: [&str; 4] = ["h264", "prores", "wav", "gif"];

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct FilmService;

/// Register the `film` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(FilmService));
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
        if !may_call(&call.app_id) {
            reply.send(Err("The film service serves system apps only.".into()));
            return;
        }
        let method = call.method().to_string();
        let args = call.args.clone();
        let host_dir = call.host_dir.clone();
        reply.send(dispatch(&method, &args, &host_dir));
    }
}

fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    match method {
        "info" => info(args, host_dir),
        "frame" => frame(args, host_dir),
        "export" => export(args, host_dir),
        "project.info" => project_info(args, host_dir),
        other => Err(format!("film.{other} is not a method of the film service")),
    }
}

// ---------------------------------------------------------------------------
// containment
// ---------------------------------------------------------------------------

/// The service's area inside the caller's host directory, created on first
/// use and returned canonicalized: `<host_dir>/film`.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let a = host_dir.join("film");
    std::fs::create_dir_all(&a).map_err(|e| format!("the film area: {e}"))?;
    a.canonicalize().map_err(|e| format!("the film area: {e}"))
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
        return Err(format!("`{what}` stays inside the app's film area"));
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
        return Err(format!("`{what}` stays inside the app's film area"));
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

/// `args[key]` as an output path inside the area, its parent created.
fn output(area: &Path, args: &Json, key: &str) -> Result<(PathBuf, String), String> {
    let rel = args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("`{key}` is required"))?;
    let p = contained(area, rel, key)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok((p, rel.to_string()))
}

/// The engine's file access, rooted in the area. The engine only ever gets
/// paths this service blessed, but a project file can carry any path
/// (media to relink), so every path the engine asks for is resolved —
/// through symlinks, to the deepest existing ancestor for writes — and
/// refused unless it stays inside the area. No directory listing, no
/// volumes, no home directory.
struct AreaServices {
    /// Canonical.
    area: PathBuf,
}

impl AreaServices {
    fn allowed(&self, path: &str) -> std::io::Result<PathBuf> {
        let deny = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{path} is outside the film area"));
        let p = Path::new(path);
        if !p.is_absolute() {
            return Err(deny());
        }
        let mut deepest = p.to_path_buf();
        while !deepest.exists() {
            match deepest.parent() {
                Some(parent) => deepest = parent.to_path_buf(),
                None => break,
            }
        }
        let resolved = deepest.canonicalize()?;
        if resolved.starts_with(&self.area) { Ok(p.to_path_buf()) } else { Err(deny()) }
    }
}

impl Services for AreaServices {
    fn read_file(&self, path: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.allowed(path)?)
    }
    fn write_file(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        std::fs::write(self.allowed(path)?, data)
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

fn session(area: &Path) -> Session {
    Session::new(Arc::new(AreaServices { area: area.to_path_buf() }))
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
fn info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let err = |e: String| format!("film.info: {e}");
    let area = area(host_dir).map_err(err)?;
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
fn frame(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let err = |e: String| format!("film.frame: {e}");
    let area = area(host_dir).map_err(err)?;
    let (abs, _) = input(&area, args, "path", MAX_MEDIA_BYTES).map_err(err)?;
    let (out_abs, out_rel) = output(&area, args, "out").map_err(err)?;
    let at_ms = args["at_ms"].as_f64().unwrap_or(0.0);
    if !at_ms.is_finite() || at_ms < 0.0 {
        return Err(err("`at_ms` is a time in milliseconds from the start".into()));
    }
    let max_side = (args["max_side"].as_u64().unwrap_or(DEFAULT_RENDER_SIDE as u64) as u32).clamp(16, MAX_RENDER_SIDE);

    let mut s = session(&area);
    let info = clip_sequence(&mut s, &abs).map_err(err)?;
    let v = info.video.as_ref().ok_or_else(|| err("the file has no video stream".into()))?;
    let duration_ms = info.duration.seconds() * 1000.0;
    if at_ms > duration_ms {
        return Err(err(format!("`at_ms` {at_ms} is past the end ({duration_ms:.0} ms)")));
    }
    // The display time of the last frame, not one past it.
    let frame_ms = v.frame_rate.tick_of(1).seconds() * 1000.0;
    let t = Tick::from_seconds_f64(at_ms.min((duration_ms - frame_ms).max(0.0)) / 1000.0);
    let long = v.width.max(v.height);
    let scale = if long > max_side { max_side as f32 / long as f32 } else { 1.0 };
    let img = s.try_render_program_at(scale, t).map_err(|e| err(e.to_string()))?;
    let png = png_at_most(img.w as u32, img.h as u32, img.over_black_rgba8(), max_side).map_err(err)?;
    std::fs::write(&out_abs, &png).map_err(|e| err(format!("{out_rel}: {e}")))?;
    let (w, h) = png_size(&png);
    Ok(json!({"out": out_rel, "width": w, "height": h, "at_ms": at_ms, "bytes": png.len()}))
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
fn export(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let err = |e: String| format!("film.export: {e}");
    let area = area(host_dir).map_err(err)?;
    let (abs, _) = input(&area, args, "path", MAX_MEDIA_BYTES).map_err(err)?;
    let (out_abs, out_rel) = output(&area, args, "out").map_err(err)?;
    let format = match args["format"].as_str() {
        Some(f) => f.to_string(),
        None => format_for(&out_rel).ok_or_else(|| err(format!("give `format` ({}) or an out path ending .mp4/.mov/.wav/.gif", FORMATS.join(" | "))))?.to_string(),
    };
    if !FORMATS.contains(&format.as_str()) {
        return Err(err(format!("`{format}` is not offered ({})", FORMATS.join(" | "))));
    }

    let mut s = session(&area);
    let info = clip_sequence(&mut s, &abs).map_err(err)?;
    let duration_ms = info.duration.seconds() * 1000.0;
    let start_ms = args["start_ms"].as_f64().unwrap_or(0.0);
    let end_ms = args["end_ms"].as_f64().unwrap_or(duration_ms);
    if !start_ms.is_finite() || !end_ms.is_finite() || start_ms < 0.0 || start_ms >= end_ms {
        return Err(err(format!("the range runs from `start_ms` to `end_ms` within the clip (0..{duration_ms:.0})")));
    }
    if end_ms - start_ms > MAX_EXPORT_MS {
        return Err(err(format!("at most {MAX_EXPORT_MS:.0} ms per export ({:.0} ms asked); give `start_ms`/`end_ms`", end_ms - start_ms)));
    }
    if end_ms > duration_ms + 1.0 {
        return Err(err(format!("`end_ms` {end_ms} is past the end ({duration_ms:.0} ms)")));
    }
    let r = s
        .execute(
            "file.exportMedia",
            json!({
                "path": out_abs.to_string_lossy(),
                "format": format,
                "audio": args["audio"].as_bool().unwrap_or(true),
                "range": "custom",
                "startSeconds": start_ms / 1000.0,
                "endSeconds": end_ms / 1000.0,
                "wait": true,
            }),
        )
        .map_err(|e| err(e.to_string()))?;
    let bytes = std::fs::metadata(&out_abs).map(|m| m.len()).map_err(|e| err(format!("{out_rel}: {e}")))?;
    Ok(json!({"out": out_rel, "format": format, "start_ms": start_ms, "end_ms": end_ms, "bytes": bytes, "result": r["result"]}))
}

/// `film.project.info {path}` → a FilmCraft project opened headlessly:
/// `project.inspect`, and `sequence.inspect` when a sequence is active.
/// Media the project points at outside the area stay offline (refused by
/// [`AreaServices`]).
fn project_info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let err = |e: String| format!("film.project.info: {e}");
    let area = area(host_dir).map_err(err)?;
    let (abs, rel) = input(&area, args, "path", MAX_PROJECT_BYTES).map_err(err)?;
    let mut s = session(&area);
    s.execute("file.open", json!({"path": abs.to_string_lossy()})).map_err(|e| err(e.to_string()))?;
    let project = s.execute("project.inspect", json!({})).map_err(|e| err(e.to_string()))?;
    let sequence = s.execute("sequence.inspect", json!({})).unwrap_or(Json::Null);
    Ok(json!({"file": rel, "project": project, "sequence": sequence}))
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
        let film = area(host()).unwrap();
        let mut s = session(&film);
        let abs = film.join(clip());
        s.execute("file.import", json!({"paths": [abs.to_string_lossy()]})).unwrap();
        s.execute("file.saveAs", json!({"path": film.join("p.fcproj").to_string_lossy()})).unwrap();

        let v = dispatch("project.info", &json!({"path": "p.fcproj"}), host()).unwrap();
        assert_eq!(v["file"], json!("p.fcproj"));
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
            let svc = AreaServices { area: area(host()).unwrap() };
            assert!(svc.read_file(&host().join("film/esc/beside.txt").to_string_lossy()).is_err());
            assert!(svc.read_file("/etc/hosts").is_err());
            assert!(svc.read_file(&host().join("film/clip.mp4").to_string_lossy()).is_ok());
        }
    }

    #[test]
    fn the_area_is_the_film_subdir_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let a = area(dir.path()).unwrap();
        assert!(a.is_dir());
        assert_eq!(a.file_name().unwrap(), "film");
        assert_eq!(a, dir.path().canonicalize().unwrap().join("film"));
        // Idempotent, and never the host dir itself (Mail's and
        // Calendar's data live beside it).
        assert_eq!(area(dir.path()).unwrap(), a);
        assert_ne!(a, dir.path().canonicalize().unwrap());
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
}
