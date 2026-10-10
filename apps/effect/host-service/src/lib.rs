//! `octosense-effect-service` — the `effect` host service (ADR 0013).
//!
//! effectcraft's motion-graphics engine (an After Effects-class compositor:
//! compositions, layers, keyframes, 300+ effects, expressions, Lottie)
//! through its headless automation backend. Every call is a fresh,
//! stateless session driven by the same command registry every effectcraft
//! frontend dispatches through.
//!
//! Methods (all under the `effect` family; paths relative to the call's
//! area):
//! - `info {path}` → the project summarised (items, comps, sizes, rates)
//! - `run {path?, cmds: [{id, params?}], out?, comp?, time?, max_side?,
//!   transparent?, include_expressions?}` → `{results, out, format, …}` —
//!   the command door (ADR 0013, #418): run commands of effectcraft's
//!   registry on the project at `path` (an `.ecproj`, or a Lottie `.json` /
//!   `.lottie` opened as a new composition, as `import_lottie` does, with
//!   what it made under `imported`), or on a new empty project, then write
//!   `out` by its extension: `.ecproj` the project; `.json` / `.lottie` a
//!   composition as Lottie (`comp`, `include_expressions`; with its `bytes`
//!   and the engine's `warnings`), as `export_lottie` does; `.png` one frame
//!   (`comp`, `time`, `max_side`, `transparent`; with `bytes`, `width`,
//!   `height`), as `render` does. Only what the door's allowlist admits runs
//!   ([`door`]), and every command is admitted before any runs: one refused
//!   command refuses the call, with nothing written.
//! - `commands {filter?}` → the engine's catalog entries the door runs
//! - `render {path, comp?, time?, out, max_side?, transparent?}` → one comp
//!   frame written to `out` as PNG
//! - `export_lottie {path, comp?, out, include_expressions?}` → a comp as
//!   Lottie `.json`/`.lottie`, with the engine's warnings list
//! - `import_lottie {path, out}` → a Lottie file opened as a composition
//!   and saved as an `.ecproj` project
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/effect`. Boundary paths are validated
//! before any I/O, and the engine side is gated too: its file I/O
//! ([`Services`]), media probing ([`Importer`]) and footage decoding
//! (`FootageSource`) each re-check their paths against the area — an image
//! sequence's every frame, not only its first — so a project that
//! references footage outside it renders placeholders instead of reading
//! it. 3D model footage is refused and renders nothing: the engine reads a
//! model's sibling files (buffers, textures, materials) by the paths inside
//! the model, with no gate of its own. Effect parameters that the engine
//! reads as a file by their own path, past those gates (a LUT, an OCIO
//! file transform or config, a mocha shape file), refuse the project unless
//! they hold the file's text inline, and so does an Essential Graphics value
//! that sets one inside a precomp, and an effect plug-in
//! ([`fence_effect_files`]). What the engine writes keeps the area's rules
//! ([`Area::write`]: a write that may not replace, an agent's, only creates
//! new files, within the quota).
//!
//! **The command door** `run` runs only the commands the reviewed
//! classification (`skill/safety.json`) classes `safe`: never a `file`,
//! `code`, `network`, `device` or `host` command, nor an id the
//! classification does not know, so the engine's command wrappers
//! (`engine.batch`, `file.runScript`, `learn.step`), its scripts, plug-in
//! loading and preferences stay out whatever list they arrive in; and
//! `effect.apply` only of an effect the engine builds in. What a command
//! plants in the project is fenced after every command, before a later one
//! or the write could draw it. The service serves system apps only until
//! ADR 0013's store capability is designed.
//!
//! **Work caps** (#418): engine work runs on the shell's UI thread, so no
//! door call may multiply work or memory without bound. The gate refuses a
//! command whose parameters ask for more than a reviewed limit ([`LIMITS`]:
//! sizes, frames and frame rates, motion-blur samples, copies, font size,
//! tracker and mesh regions, list lengths); `run` measures the project
//! against the same caps once it opens and after every command, whatever
//! route a value took ([`measure`]: project size, comp and solid sizes, the
//! precomp expansion and cycles, every capped or slider-ranged parameter
//! value and keyframe), checks what a command would do on the project as it
//! stands before it runs ([`precheck`]: copies, generated keys, analysed
//! frames, the engine's name search), and checks a frame's layer buffers
//! before drawing it ([`check_frame`]). The door's session evaluates no
//! expression: the commands that set or enable one are classed `code`, and
//! no frame or Lottie file of a composition that holds an enabled
//! expression is written ([`check_expressions`]).

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use effectcraft_automation::Backend;
use effectcraft_engine::effects::EffectSpec;
use effectcraft_engine::project::{Footage, ItemId};
use effectcraft_engine::raster::{AuxChannels, Image};
use effectcraft_engine::render::FootageSource;
use effectcraft_engine::time::Tick;
use effectcraft_engine::{Importer, Services, Session};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Inner, InnerRule, Limit, Measure, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The longest frame edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;
/// The largest file the engine's guarded I/O reads or writes (bytes):
/// projects and Lottie files are JSON, and frames are written by `render`.
const MAX_FILE_BYTES: u64 = 64 << 20;
/// The most bytes the session's decoded-frame cache and its processed-layer
/// cache each keep (the engine's default is 1 GiB each).
const CACHE_BYTES: usize = 256 << 20;
/// Undo levels of the command door's session (the engine's default is 32):
/// every edit keeps the project as it was, and one call needs undo only
/// within itself.
const DOOR_UNDO_LEVELS: u32 = 4;

/// What the effect engine's reviewer settled for the door beyond the
/// classes. `effect.apply` names its effect by id, display name or alias,
/// which the engine resolves with `lookup` over the built-in effects and
/// every registered plug-in: it must name a built-in no plug-in shadows
/// ([`builtin_effect`]). It is the only `safe` command that names an effect,
/// preset or command by a caller's string and could reach past the
/// built-ins: `effect.applyLast` re-applies the id `effect.apply` recorded;
/// ease presets (`keys.easePreset.apply`), text animation presets
/// (`layer.applyTextPreset`) and brush presets (`paint.brushPreset`) are
/// built-in data (and, for ease presets, the session's own list, empty
/// without a config store); the VR builders run fixed command ids. No `file`
/// command is reviewed to run: the door's files are its own `path` and
/// `out`. No `safe` command sets an app-wide variable by key (`prefs.*` are
/// `host`; `layer.setText`'s `font` only notes a recent font, which a
/// session without a config store never keeps).
///
/// Engine work runs on the shell's UI thread, so the parameters that
/// multiply it carry limits ([`LIMITS`], each with its reason): comp, solid,
/// placeholder and cube-map sizes, comp frames and frame rates, motion-blur
/// samples, font size, the values a command sets by path ([`ValueCap`]:
/// the Repeater's copies, a star's points, the tracker's regions), the
/// Puppet mesh's expansion, Liquify dabs, mask-interpolation rates, and the
/// lists a command repeats its work for. An Expression Selector is
/// refused (it plants an expression). What no parameter shows (a uid path,
/// a paste, the wiggler, a project or Lottie file, the work a command does
/// on the project as it stands) is bounded in `run` itself ([`measure`],
/// [`precheck`]).
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[],
    setters: &[],
    inner: &[Inner { id: "effect.apply", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } }],
    limits: LIMITS,
    copies_per_call: MAX_SHAPE_COPIES,
    held: &[],
};

// ------------------------------------------------------------------ caps

/// Pixels a composition, solid, placeholder or footage item may hold, and
/// one text or shape layer's buffer in a frame the door draws: 8,847,360
/// = DCI 4K (4096 × 2160), the largest everyday delivery size. An RGBA f32
/// frame that size is 142 MB, so a frame and one working copy stay near
/// 256 MB. The engine takes 4..30000 per comp side (900 M px, 14.4 GB a
/// frame) and any solid or placeholder size (read to 32 bits), and renders
/// a solid whole at its own size.
const MAX_FRAME_PIXELS: f64 = 4096.0 * 2160.0;
/// Frames a composition or placeholder may span (duration × frame rate):
/// 36,000 = ten minutes at 60 fps, twenty at 30, where motion graphics run
/// seconds to a few minutes. Analyses, the audio-to-keyframes conversion and
/// the frame-by-frame generators walk a comp's frames; the engine bounds a
/// duration only below (one frame) and by its tick range (9e6 s).
const MAX_COMP_FRAMES: f64 = 36_000.0;
/// The slowest frame rate a composition, placeholder or footage item may
/// have: every frame walk steps one frame, and audio is mixed one frame at
/// a time (48000 / fps samples, 384 MB a frame at the engine's 0.001 fps).
/// Footage takes any rate, where 0 or a negative rate makes a frame step
/// that a frame walk never leaves.
const MIN_FRAME_RATE: f64 = 1.0;
/// The fastest frame rate: 240 fps, twice high-frame-rate work (120); the
/// engine takes up to 1000 for a comp, any rate for footage (2.5e11 fps
/// makes a zero frame step).
const MAX_FRAME_RATE: f64 = 240.0;
/// Samples Per Frame: each sample renders a run of 3D layers, or a moving
/// shape or text layer's content, again. 32 = twice After Effects' default
/// (16); the engine clamps 2..64.
const MAX_MOTION_BLUR_SAMPLES: f64 = 32.0;
/// Adaptive Sample Limit: the most samples a moving 2D layer gets, each a
/// whole-frame warp of its buffer. 128 = After Effects' and the engine's
/// default; the engine clamps 16..256.
const MAX_ADAPTIVE_SAMPLE_LIMIT: f64 = 128.0;
/// A shape Repeater's copies: each copy transforms and draws everything
/// above it in its group again, and the renderer allocates per copy with no
/// ceiling of its own (1e9 copies abort on allocation). Everyday repeaters
/// make tens of copies.
const MAX_REPEATER_COPIES: f64 = 1_000.0;
/// What one shape group's nested Repeaters (and Offset Paths copies) may
/// multiply to, and the copies one call's copy limits may multiply to
/// ([`Reviewed::copies_per_call`]): nested repeaters multiply each other.
/// 10,000 drawn copies is about a quarter second of path work.
const MAX_SHAPE_COPIES: f64 = 10_000.0;
/// Font size (px): a text layer is drawn into one buffer the size of its
/// glyph bounds (the engine clamps it at 16384 × 16384, 4.3 GB), and
/// Per-character 3D holds a buffer per glyph (up to 4096² each) at once.
/// 1296 px = After Effects' own largest font size; the engine takes 10,000.
const MAX_FONT_SIZE: f64 = 1_296.0;
/// Items, layers and effect instances one project may hold. A plain solid
/// layer is about 5.4 KB of project, so this keeps a project well inside
/// the [`MAX_FILE_BYTES`] the service reads and writes; every edit copies
/// the composition it changes (kept [`DOOR_UNDO_LEVELS`] deep), every
/// effect instance runs on every frame of its layer, and the engine's
/// unique-name search compares every new layer's candidate names with every
/// layer of its comp. A call stops at the command that would grow the
/// project past it, so a duplicate loop stops too.
const MAX_PROJECT_SIZE: usize = 5_000;
/// Name comparisons one command may make naming its copies: the engine
/// names a copy `stem 2`, `stem 3`, … up to the first free name, comparing
/// each candidate with every name already there, so copying n layers that
/// share a stem into a comp of L costs about n² × L (doubling a comp of
/// 2,048 same-named layers, 10¹⁰). 10⁸ comparisons ≈ a few tenths of a
/// second.
const MAX_NAME_WORK: f64 = 1e8;
/// Keyframes one project may hold (about 175 B each, and key edits are
/// quadratic in a property's keys): the generators below add keys frame by
/// frame.
const MAX_KEYFRAMES: usize = 200_000;
/// Layers one frame of a composition may draw with its precompositions
/// expanded: every instance of a precomp renders its comp again (the
/// engine caches no precomp), a frame-blended one twice, so a few levels of
/// nesting multiply; the engine stops only at depth 16, and its own cycle
/// check walks every path through the nesting. 2,000 ≈ 40 instances of a
/// 50-layer precomp.
const MAX_FRAME_LAYERS: f64 = 2_000.0;
/// Keyframes one generator command may add (the wiggler, Exponential
/// Scale, mask interpolation, audio to keyframes add one per frame or per
/// step of a span the gate cannot see).
const MAX_GENERATED_KEYS: f64 = 10_000.0;
/// Frames a smoothing command may walk: the Smoother samples a span at
/// every frame and evaluates every sample again for each key it adds
/// (O(n²)); Motion Sketch and Record Puppet Pin smooth their keys in
/// O(n² log n). 2,000 frames ≈ 4 million evaluations.
const MAX_SMOOTH_FRAMES: f64 = 2_000.0;
/// Frames × source pixels one analysis may walk (tracking, Warp
/// Stabilizer, the 3D camera tracker, Roto Brush, scene edit detection,
/// auto-trace over the work area): each frame renders the layer at full
/// size and analyses it, on the calling thread with `wait`. 100 M px ≈ 48
/// frames of 1080p or 380 of 512 × 512; the engine walks every frame of the
/// layer within its comp.
const MAX_ANALYSIS_PIXELS: f64 = 100e6;
/// Entries of a list a command repeats its work for (`layers`, `items`,
/// `effects`, `points`): the engine keeps repeated entries, so one id listed
/// N times makes N copies.
const MAX_LIST: f64 = 1_000.0;
/// Liquify dabs one stroke may lay (one every eighth of the brush along
/// the stroke, unclipped to the layer): every dab copies the whole
/// distortion mesh (up to about 35,000 nodes) when the frame renders, and
/// every stroke is replayed. 20,000 dabs ≈ a second of mesh work.
const MAX_LIQUIFY_DABS: f64 = 20_000.0;

/// Whether `x` passes `max` (a NaN does).
fn past(x: f64, max: f64) -> bool {
    !matches!(x.partial_cmp(&max), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
}

/// Whether `x` falls short of `min` (a NaN does).
fn short_of(x: f64, min: f64) -> bool {
    !matches!(x.partial_cmp(&min), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
}

/// A numeric parameter as the door counts it: absent or null is `None`; a
/// number or numeric string counts by its size; anything else is refused.
fn number(p: &Json, key: &str) -> Result<Option<f64>, String> {
    let n = match p.get(key) {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Number(n)) => n.as_f64(),
        Some(Json::String(s)) => s.trim().parse::<f64>().ok(),
        Some(_) => None,
    };
    n.filter(|n| n.is_finite()).map(|n| Some(n.abs())).ok_or_else(|| format!("`{key}` is a number the door bounds"))
}

/// The frame rate a command sets: `frameRate`, or its alias `fps` (the
/// faster when both), refused under [`MIN_FRAME_RATE`] (a zero or negative
/// rate included).
fn frame_rate(p: &Json) -> Result<Option<f64>, String> {
    let mut rate: Option<f64> = None;
    for key in ["frameRate", "fps"] {
        let signed = match p.get(key) {
            None | Some(Json::Null) => continue,
            Some(Json::Number(n)) => n.as_f64(),
            Some(Json::String(s)) => s.trim().parse::<f64>().ok(),
            Some(_) => None,
        };
        let r = signed.filter(|r| r.is_finite()).ok_or_else(|| format!("`{key}` is a number the door bounds"))?;
        if r < MIN_FRAME_RATE {
            return Err(format!("`{key}` is {r} fps, slower than the {MIN_FRAME_RATE} fps the door allows (every frame walk steps one frame)"));
        }
        rate = Some(rate.map_or(r, |q: f64| q.max(r)));
    }
    Ok(rate)
}

/// `comp.new`'s frame in pixels, a side not given at the engine's default
/// (1920 × 1080).
fn new_comp_pixels(p: &Json) -> Result<Option<f64>, String> {
    Ok(Some(number(p, "width")?.unwrap_or(1920.0) * number(p, "height")?.unwrap_or(1080.0)))
}

/// `comp.new`'s frames: `duration` (default 10 s) × its frame rate
/// (default 29.97).
fn new_comp_frames(p: &Json) -> Result<Option<f64>, String> {
    Ok(Some(number(p, "duration")?.unwrap_or(10.0) * frame_rate(p)?.unwrap_or(29.97)))
}

/// The frames a settings command gives: the `duration` and frame rate it
/// names (one not named stays the item's own, which [`measure`] checks
/// after the command).
fn frames_set(p: &Json) -> Result<Option<f64>, String> {
    match (number(p, "duration")?, frame_rate(p)?) {
        (None, None) => Ok(None),
        (d, r) => Ok(Some(d.unwrap_or(1.0) * r.unwrap_or(1.0))),
    }
}

/// `file.importPlaceholder`'s frame in pixels (default 1920 × 1080).
fn placeholder_pixels(p: &Json) -> Result<Option<f64>, String> {
    new_comp_pixels(p)
}

/// `file.importPlaceholder`'s frames: `duration` (default 30 s) × its
/// frame rate (default 29.97).
fn placeholder_frames(p: &Json) -> Result<Option<f64>, String> {
    Ok(Some(number(p, "duration")?.unwrap_or(30.0) * frame_rate(p)?.unwrap_or(29.97)))
}

/// `comp.vr.createEnvironment`'s cube map in pixels: 3 × 2 faces of `size`
/// (default 1024).
fn environment_pixels(p: &Json) -> Result<Option<f64>, String> {
    let f = number(p, "size")?.unwrap_or(1024.0);
    Ok(Some(6.0 * f * f))
}

/// `comp.vr.extractCubemap`'s cube map in pixels: 3 × 2 faces of
/// `faceSize` (without one, a quarter of the comp's width, which
/// [`measure`] checks).
fn cubemap_pixels(p: &Json) -> Result<Option<f64>, String> {
    Ok(number(p, "faceSize")?.map(|f| 6.0 * f * f))
}

/// The largest component of an `[x, y]` parameter (or of a plain number).
fn largest_of(p: &Json, key: &str) -> Result<Option<f64>, String> {
    let Some(Json::Array(items)) = p.get(key) else { return number(p, key) };
    let mut most: Option<f64> = None;
    for v in items {
        let n = match v {
            Json::Number(n) => n.as_f64(),
            Json::String(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        }
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("`{key}` holds numbers the door bounds"))?;
        most = Some(most.map_or(n.abs(), |m: f64| m.max(n.abs())));
    }
    Ok(most)
}

/// `track.setPoint`'s feature region (its larger side).
fn feature_size(p: &Json) -> Result<Option<f64>, String> {
    largest_of(p, "featureSize")
}

/// `track.setPoint`'s search region (its larger side).
fn search_size(p: &Json) -> Result<Option<f64>, String> {
    largest_of(p, "searchSize")
}

/// An Expression Selector (`kind: expression`, any case) plants an
/// expression, which no door call may: the engine runs expressions without
/// a time or memory budget.
fn expression_selector(p: &Json) -> Result<Option<f64>, String> {
    match p.get("kind").and_then(Json::as_str) {
        Some(k) if k.eq_ignore_ascii_case("expression") => {
            Err("an Expression Selector runs an expression, and no door call plants one (expressions run without a time or memory budget)".into())
        }
        _ => Ok(None),
    }
}

/// The dabs a Liquify stroke lays: one every eighth of the brush (`size`,
/// default 64) along its `points`.
fn liquify_dabs(p: &Json) -> Result<Option<f64>, String> {
    let Some(points) = p.get("points").and_then(Json::as_array) else { return Ok(None) };
    let size = number(p, "size")?.unwrap_or(64.0);
    let step = (size * 0.5).max(1.0) * 0.25;
    let xy = |q: &Json| Some([q.get(0)?.as_f64()?, q.get(1)?.as_f64()?]);
    let pts: Vec<[f64; 2]> = points.iter().filter_map(xy).collect();
    let dabs = pts.windows(2).map(|w| ((w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) / step).ceil().max(1.0)).sum::<f64>();
    Ok(Some(dabs.max(1.0)))
}

/// The longest list parameter of a command that repeats its work per entry
/// (a repeated id included).
fn list_len(p: &Json) -> Result<Option<f64>, String> {
    Ok(["layers", "items", "effects", "points"].iter().filter_map(|k| p.get(*k).and_then(Json::as_array)).map(|a| a.len() as f64).reduce(f64::max))
}

/// A parameter whose value multiplies render work, capped wherever a
/// command sets it by `path` (`prop.set`, `prop.addKey`, `keys.set`): the
/// path's last segment names it by match id or display name, as the
/// engine resolves it. A uid or index path names nothing the gate can
/// read; [`measure`] checks every value, however it was set, after each
/// command.
struct ValueCap {
    /// Match ids and display names (any case).
    names: &'static [&'static str],
    max: f64,
    what: &'static str,
    copies: bool,
}

impl ValueCap {
    /// The size of the value `p` sets, when its path names this parameter.
    fn of(&self, p: &Json) -> Result<Option<f64>, String> {
        let Some(path) = p.get("path").and_then(Json::as_str) else { return Ok(None) };
        let leaf = path.rsplit(['/', '.']).find(|s| !s.is_empty()).unwrap_or("");
        let leaf = match leaf.rsplit_once('#') {
            Some((m, n)) if !m.is_empty() && n.parse::<usize>().is_ok() => m,
            _ => leaf,
        };
        if !self.names.iter().any(|n| n.eq_ignore_ascii_case(leaf.trim())) {
            return Ok(None);
        }
        Ok(match p.get("value") {
            Some(Json::Array(items)) => items.iter().filter_map(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())).map(f64::abs).reduce(f64::max),
            Some(Json::Number(n)) => n.as_f64().map(f64::abs),
            Some(Json::String(s)) => s.trim().parse::<f64>().ok().map(f64::abs),
            _ => None,
        })
    }
}

/// The Repeater's `copies` (and Offset Paths', which the engine clamps at
/// 1,000): [`MAX_REPEATER_COPIES`]. Copies multiply what the group holds,
/// and nested repeaters each other, so they count toward
/// [`Reviewed::copies_per_call`].
const REPEATER_COPIES: ValueCap = ValueCap { names: &["copies"], max: MAX_REPEATER_COPIES, what: "copies", copies: true };
fn repeater_copies(p: &Json) -> Result<Option<f64>, String> {
    REPEATER_COPIES.of(p)
}

/// A star's `points`: its path has that many vertices, which zig zag,
/// wiggle and the repeaters then multiply. 100 = the slider's own range
/// (which only `prop.set` applies; the renderer takes any count).
const STAR_POINTS: ValueCap = ValueCap { names: &["points"], max: 100.0, what: "star points", copies: false };
fn star_points(p: &Json) -> Result<Option<f64>, String> {
    STAR_POINTS.of(p)
}

/// A tracker's feature region (px): its pyramid crop and template grow
/// with its area. 512 = many times After Effects' default (about 40); the
/// engine floors it at 4 and sets no ceiling.
const FEATURE_SIZE: ValueCap = ValueCap { names: &["featureSize", "Feature Size"], max: 512.0, what: "pixels of feature region", copies: false };
fn feature_size_value(p: &Json) -> Result<Option<f64>, String> {
    FEATURE_SIZE.of(p)
}

/// A tracker's search region (px): every frame correlates the feature
/// exhaustively over it at the pyramid's top level, and crops it whole.
/// 1024 = many times After Effects' default (about 80); the engine floors
/// it at 4 and sets no ceiling.
const SEARCH_SIZE: ValueCap = ValueCap { names: &["searchSize", "Search Size"], max: 1024.0, what: "pixels of search region", copies: false };
fn search_size_value(p: &Json) -> Result<Option<f64>, String> {
    SEARCH_SIZE.of(p)
}

/// The Puppet mesh's expansion (px): the outline grid is padded by it on
/// every side before the mesh is traced. 200 = the slider's own valid
/// maximum (only `prop.set` applies it). By path it is checked by
/// [`measure`] alone: a mask's `expansion` shares the name and costs only
/// linear blur passes.
const MAX_PUPPET_EXPANSION: f64 = 200.0;

/// A parameter whose value multiplies the work of drawing a frame, as
/// [`measure`] finds it in the project, in its value and every keyframe
/// (every numeric component): where it lives (an effect id; `shape:<item>`
/// for a shape item; `*` anywhere) and its key there (the match path
/// inside the effect, `group/param`; `*/param` for a param of that match id
/// in any group), and the range the door allows.
struct ParamCap {
    owner: &'static str,
    param: &'static str,
    min: f64,
    max: f64,
    what: &'static str,
}

/// A ceiling on a parameter.
const fn cap(owner: &'static str, param: &'static str, max: f64, what: &'static str) -> ParamCap {
    ParamCap { owner, param, min: f64::NEG_INFINITY, max, what }
}

/// A floor on a parameter whose smaller values multiply work (a grid
/// spacing, a particle radius that sets the particle count).
const fn floor(owner: &'static str, param: &'static str, min: f64, what: &'static str) -> ParamCap {
    ParamCap { owner, param, min, max: f64::INFINITY, what }
}

/// Every parameter [`measure`] caps. Beyond these, every slider keeps its
/// declared valid range (the range `prop.set` already keeps; a keyframe, a
/// paste or a project file can hold anything, and no effect clamps at
/// render). These are the parameters whose declared range is itself unsafe,
/// or that have none, from reading each effect's render: what a value
/// multiplies is in the comment above its group. Buffer padding is held to
/// about 300 px a side (at 1080p a padded image is ≤ 70 MB, and the effects
/// hold 3–5 at once; the engine's own layer styles stop at 250 px).
const PARAM_CAPS: &[ParamCap] = &[
    // Shape items: copies multiply every path and draw above them (no
    // ceiling in the renderer); a star's points are its vertices.
    cap("shape:repeater", "copies", MAX_REPEATER_COPIES, "repeater copies"),
    cap("shape:star", "points", 100.0, "star points"),
    // Tracker regions: crop and exhaustive search per frame.
    cap("*", "*/featureSize", 512.0, "pixels of tracker feature region"),
    cap("*", "*/searchSize", 1024.0, "pixels of tracker search region"),
    // Puppet: the outline grid is padded by the expansion.
    cap("ec.distort.puppet", "*/expansion", MAX_PUPPET_EXPANSION, "pixels of Puppet mesh expansion"),
    // Blurs and glows: the buffer is padded by about 1.5 × the radius
    // (3–11 padded images), the box passes are O(radius) per row; valid
    // maxima 500–3000.
    cap("ec.blur.gaussian", "blurriness", 200.0, "px of blurriness"),
    cap("ec.obsolete.gaussianlegacy", "blurriness", 200.0, "px of blurriness"),
    cap("ec.blur.fastbox", "radius", 100.0, "px of blur radius"),
    cap("ec.blur.fastbox", "iterations", 5.0, "box iterations (the pad is radius × iterations)"),
    cap("ec.blur.directional", "length", 200.0, "px of blur length"),
    cap("ec.blur.channel", "redBlurriness", 150.0, "px of blurriness"),
    cap("ec.blur.channel", "greenBlurriness", 150.0, "px of blurriness"),
    cap("ec.blur.channel", "blueBlurriness", 150.0, "px of blurriness"),
    cap("ec.blur.channel", "alphaBlurriness", 150.0, "px of blurriness"),
    cap("ec.blur.compound", "maximumBlur", 50.0, "px of maximum blur (11 padded images)"),
    cap("ec.blur.cameralens", "blurRadius", 100.0, "px of blur radius"),
    cap("ec.blur.cameralens", "irisProperties/irisAspectRatio", 4.0, "of iris aspect ratio (it multiplies the pad)"),
    cap("ec.blur.cccross", "radiusX", 80.0, "px of blur radius (padded 3×)"),
    cap("ec.blur.cccross", "radiusY", 80.0, "px of blur radius (padded 3×)"),
    cap("ec.blur.camerashakedeblur", "blurDuration", 3.0, "frames of blur duration (two neighbour renders each)"),
    cap("ec.blur.camerashakedeblur", "searchRange", 12.0, "px of search range ((2r+1)² candidates per patch)"),
    cap("ec.stylize.glow", "radius", 150.0, "px of glow radius"),
    cap("ec.channel.minimax", "radius", 100.0, "px of radius"),
    cap("ec.matte.mattechoker", "geometricSoftness1", 10.0, "px of softness (padded by iterations × softness)"),
    cap("ec.matte.mattechoker", "geometricSoftness2", 10.0, "px of softness (padded by iterations × softness)"),
    cap("ec.matte.mattechoker", "iterations", 10.0, "iterations"),
    cap("ec.noise.median", "radius", 50.0, "px of median radius"),
    cap("ec.noise.dustscratches", "radius", 50.0, "px of median radius"),
    cap("ec.noise.medianlegacy", "radius", 50.0, "px of median radius"),
    // Effects that grow their buffer by a distance or a size.
    cap("ec.perspective.dropshadow", "distance", 120.0, "px of shadow distance (the pad)"),
    cap("ec.perspective.dropshadow", "softness", 120.0, "px of shadow softness (padded 1.5×)"),
    cap("ec.perspective.radialshadow", "projectionDistance", 15.0, "of projection distance (the pad is that % of the light's reach)"),
    cap("ec.perspective.radialshadow", "softness", 100.0, "px of shadow softness"),
    cap("ec.stylize.ccrepetile", "expandRight", 500.0, "px of tile expansion"),
    cap("ec.stylize.ccrepetile", "expandLeft", 500.0, "px of tile expansion"),
    cap("ec.stylize.ccrepetile", "expandDown", 500.0, "px of tile expansion"),
    cap("ec.stylize.ccrepetile", "expandUp", 500.0, "px of tile expansion"),
    cap("ec.stylize.motiontile", "outputWidth", 250.0, "% of output width"),
    cap("ec.stylize.motiontile", "outputHeight", 250.0, "% of output height"),
    cap("ec.utility.growbounds", "pixels", 500.0, "px of bounds growth"),
    cap("ec.distort.wavewarp", "height", 250.0, "px of wave height (the pad)"),
    cap("ec.distort.turbulentdisplace", "amount", 250.0, "px of displacement (the pad)"),
    cap("ec.distort.displacementmap", "maxHorizontal", 250.0, "px of displacement (the pad)"),
    cap("ec.distort.displacementmap", "maxVertical", 250.0, "px of displacement (the pad)"),
    cap("ec.distort.opticscompensation", "resize", 0.0, "(only Off: Max 2X, 4X and Unlimited pad by ½–3½ layers a side)"),
    cap("ec.distort.magnify", "size", 500.0, "px of magnifier size"),
    cap("ec.distort.ccslant", "slant", 100.0, "of slant"),
    cap("ec.distort.ccslant", "height", 200.0, "% of height"),
    cap("ec.distort.warp", "bend", 50.0, "of bend (the pad grows with bend + distortions)"),
    cap("ec.distort.warp", "horizontalDistortion", 50.0, "of horizontal distortion"),
    cap("ec.distort.warp", "verticalDistortion", 50.0, "of vertical distortion"),
    cap("ec.distort.upscale", "scale", 150.0, "% of upscale (the output is the scaled layer)"),
    // Time effects: each frame read renders the layer again and keeps three
    // copies of it (about 100 MB at 1080p). Most are held at their own
    // defaults; Time Displacement's defaults read 64 frames at once (2 × 1 s
    // × 60 fps + 1, the engine's own ceiling, several GB at 1080p), so the
    // door refuses it as it is applied: a quarter second at 16 fps reads 9.
    cap("ec.time.echo", "numberOfEchoes", 8.0, "echoes"),
    cap("ec.time.timedisplacement", "maxDisplacementTime", 0.25, "s of maximum displacement"),
    cap("ec.time.timedisplacement", "timeResolution", 16.0, "frames per second of displacement"),
    cap("ec.time.timewarp", "motionBlur/shutterSamples", 8.0, "shutter samples"),
    cap("ec.time.timewarp", "tuning/smoothing/smoothingIterations", 50.0, "smoothing iterations"),
    cap("ec.time.timewarp", "tuning/vectorDetail", 50.0, "of vector detail (flow tiles)"),
    floor("ec.time.timewarp", "tuning/blockSize", 8.0, "px of block size"),
    cap("ec.time.ccforcemotionblur", "motionBlurLevels", 8.0, "motion-blur levels"),
    cap("ec.time.ccwidetime", "forwardSteps", 3.0, "forward steps"),
    cap("ec.time.ccwidetime", "backwardSteps", 3.0, "backward steps"),
    cap("ec.time.pixelmotionblur", "shutterSamples", 16.0, "shutter samples (its default)"),
    cap("ec.matte.refinesoft", "motionBlur/motionBlurSamples", 16.0, "motion-blur samples (a refined neighbour render each; its default)"),
    cap("ec.matte.refinehard", "motionBlur/motionBlurSamples", 16.0, "motion-blur samples (a refined neighbour render each; its default)"),
    cap("ec.color.autolevels", "temporalSmoothing", 0.25, "s of temporal smoothing (two renders per frame of it)"),
    cap("ec.color.autocontrast", "temporalSmoothing", 0.25, "s of temporal smoothing (two renders per frame of it)"),
    cap("ec.color.autocolor", "temporalSmoothing", 0.25, "s of temporal smoothing (two renders per frame of it)"),
    cap("ec.color.shadowhighlight", "temporalSmoothing", 0.25, "s of temporal smoothing (two renders per frame of it)"),
    cap("ec.distort.warpstabilizer", "advanced/synthesizeInputRange", 1.0, "s of synthesize input range (two renders per frame of it; its default)"),
    // Noise and generators: octaves, cells, waves, segments per pixel.
    cap("ec.noise.fractal", "complexity", 10.0, "octaves of complexity"),
    cap("ec.noise.turbulent", "complexity", 10.0, "octaves of complexity"),
    cap("ec.generate.cellpattern", "cellPattern", 5.0, "(the HQ patterns evaluate 400 cells a pixel)"),
    cap("ec.generate.radiowaves", "waveMotion/frequency", 2.0, "waves per second (frequency × lifespan waves, each up to a 64-sided polygon per pixel)"),
    cap("ec.generate.radiowaves", "waveMotion/lifespan", 2.0, "s of wave lifespan"),
    cap("ec.generate.advancedlightning", "turbulence", 4.0, "of turbulence (above it segments grow)"),
    cap("ec.generate.fractal", "mandelbrot/mandelbrotMagnification", 10.0, "of magnification (iterations grow with it)"),
    cap("ec.generate.fractal", "julia/juliaMagnification", 10.0, "of magnification (iterations grow with it)"),
    cap("ec.generate.fractal", "highQualitySettings/samplingFactor", 2.0, "of sampling factor (factor² samples)"),
    cap("ec.generate.vegas", "segments", 100.0, "segments"),
    cap("ec.generate.vegas", "width", 20.0, "px of segment width"),
    cap("ec.generate.audiospectrum", "frequencyBands", 1024.0, "frequency bands"),
    cap("ec.generate.audiospectrum", "thickness", 100.0, "px of thickness"),
    cap("ec.generate.audiospectrum", "maximumHeight", 2000.0, "px of maximum height"),
    cap("ec.generate.audiowaveform", "displayedSamples", 1024.0, "displayed samples"),
    cap("ec.generate.audiowaveform", "thickness", 100.0, "px of thickness"),
    cap("ec.generate.audiowaveform", "maximumHeight", 2000.0, "px of maximum height"),
    cap("ec.obsolete.lightning", "segments", 16.0, "segments"),
    cap("ec.obsolete.lightning", "detailLevel", 4.0, "detail levels (segments double per level)"),
    cap("ec.obsolete.lightning", "branchSegments", 8.0, "branch segments"),
    cap("ec.transition.cardwipe", "rows", 100.0, "rows"),
    cap("ec.transition.cardwipe", "columns", 100.0, "columns"),
    // Simulations: particle, piece and cell counts, sprite sizes.
    cap("ec.sim.carddance", "rows", 100.0, "rows"),
    cap("ec.sim.carddance", "columns", 100.0, "columns"),
    cap("ec.sim.carddance", "xScale/xScaleOffset", 10.0, "of scale offset"),
    cap("ec.sim.carddance", "yScale/yScaleOffset", 10.0, "of scale offset"),
    cap("ec.sim.carddance", "xScale/xScaleMultiplier", 10.0, "of scale multiplier"),
    cap("ec.sim.carddance", "yScale/yScaleMultiplier", 10.0, "of scale multiplier"),
    cap("ec.sim.shatter", "shape/repetitions", 100.0, "repetitions"),
    cap("ec.sim.ccrainfall", "drops", 20_000.0, "drops"),
    cap("ec.sim.ccrainfall", "size", 10.0, "of drop size"),
    cap("ec.sim.ccrainfall", "speed", 10_000.0, "of drop speed (the streak length)"),
    cap("ec.sim.ccsnowfall", "flakes", 50_000.0, "flakes"),
    cap("ec.sim.ccsnowfall", "size", 20.0, "of flake size"),
    cap("ec.sim.ccbubbles", "bubbleAmount", 1000.0, "bubbles"),
    cap("ec.sim.ccbubbles", "bubbleSize", 5.0, "of bubble size"),
    cap("ec.sim.ccdrizzle", "dripRate", 50.0, "drips per second (drops = rate × longevity, no ceiling)"),
    cap("ec.sim.ccdrizzle", "longevity", 5.0, "s of longevity"),
    floor("ec.sim.ccstarburst", "gridSpacing", 2.0, "px of grid spacing"),
    cap("ec.sim.ccstarburst", "size", 300.0, "of star size"),
    floor("ec.sim.ccballaction", "gridSpacing", 4.0, "px of grid spacing"),
    cap("ec.sim.ccballaction", "ballSize", 200.0, "of ball size"),
    floor("ec.sim.ccpixelpolly", "gridSpacing", 2.0, "px of grid spacing"),
    cap("ec.sim.cchair", "density", 1000.0, "of hair density"),
    cap("ec.sim.cchair", "length", 200.0, "of hair length"),
    cap("ec.sim.ccparticleworld", "birthRate", 10.0, "of birth rate"),
    cap("ec.sim.ccparticleworld", "longevity", 10.0, "s of longevity"),
    cap("ec.sim.ccparticleworld", "particle/birthSize", 1.0, "of particle size (sprites up to the layer's size)"),
    cap("ec.sim.ccparticleworld", "particle/deathSize", 1.0, "of particle size (sprites up to the layer's size)"),
    cap("ec.sim.ccparticlesystems2", "birthRate", 20.0, "of birth rate"),
    cap("ec.sim.ccparticlesystems2", "longevity", 20.0, "s of longevity"),
    cap("ec.sim.ccparticlesystems2", "particle/birthSize", 5.0, "of particle size"),
    cap("ec.sim.ccparticlesystems2", "particle/deathSize", 5.0, "of particle size"),
    cap("ec.sim.ccmrmercury", "blobBirthSize", 1.0, "of blob size"),
    cap("ec.sim.ccmrmercury", "blobDeathSize", 1.0, "of blob size"),
    cap("ec.sim.ccmrmercury", "blobInfluence", 100.0, "of blob influence"),
    cap("ec.sim.particleplayground", "grid/particlesAcross", 100.0, "grid particles across (outside the engine's 60k cap)"),
    cap("ec.sim.particleplayground", "grid/particlesDown", 100.0, "grid particles down (outside the engine's 60k cap)"),
    floor("ec.sim.particleplayground", "layerExploder/radiusOfNewParticles", 2.0, "px of exploded particle radius (one particle per cell)"),
    cap("ec.sim.particleplayground", "repel/repelForceRadius", 50.0, "px of repel radius (O(N²) per step beyond it)"),
    cap("ec.sim.particleplayground", "cannon/particlesPerSecond", 500.0, "particles per second"),
    cap("ec.sim.particleplayground", "cannon/cannonParticleRadius", 50.0, "px of particle radius"),
    cap("ec.sim.particleplayground", "grid/gridParticleRadius", 50.0, "px of particle radius"),
    cap("ec.sim.particleplayground", "layerMap/timeOffset", 10.0, "s of layer-map time offset"),
    cap("ec.sim.waveworld", "simulation/gridResolution", 100.0, "of grid resolution"),
    cap("ec.sim.waveworld", "simulation/waveSpeed", 1.0, "of wave speed (sub-steps grow with it)"),
    cap("ec.sim.waveworld", "simulation/preRoll", 10.0, "s of pre-roll"),
    cap("ec.sim.foam", "zoom", 10.0, "of zoom"),
    cap("ec.sim.foam", "bubbles/size", 2.0, "of bubble size"),
    // Paint: dabs = stroke length / (diameter × spacing), each dab stamps
    // its diameter squared.
    cap("ec.paint.paint", "*/diameter", 200.0, "px of brush diameter"),
    cap("ec.paint.paint", "*/scale", 400.0, "% of stroke scale"),
];

/// The cap of the parameter at `key` (its match path) of `owner`, if
/// [`PARAM_CAPS`] has one.
fn param_cap(owner: &str, key: &str) -> Option<&'static ParamCap> {
    let leaf = key.rsplit('/').next().unwrap_or(key);
    PARAM_CAPS.iter().find(|c| {
        (c.owner == "*" || c.owner == owner)
            && match c.param.strip_prefix("*/") {
                Some(any) => any == leaf,
                None => c.param == key,
            }
    })
}

/// The effects whose point parameters grow the buffer by how far they
/// reach past the layer (CC Power Pin's corners, Magnify's and Radial
/// Shadow's centre and light, Puppet's pins, Bezier Warp's vertices): each
/// may lie at most [`MAX_POINT_OVERHANG`] outside the layer.
const PAD_POINT_EFFECTS: &[&str] = &["ec.distort.ccpowerpin", "ec.distort.magnify", "ec.perspective.radialshadow", "ec.distort.puppet", "ec.distort.bezierwarp"];

/// How far (px) a [`PAD_POINT_EFFECTS`] point may reach outside its layer:
/// the engine pads by the overhang up to 4096 px a side (Puppet's output up
/// to 16384²).
const MAX_POINT_OVERHANG: f64 = 500.0;

/// The effects that simulate from layer time 0 to the frame's time, step
/// by step (60 a second): drawing a frame at layer time T steps T × 60
/// times, so the door draws them only within [`MAX_SIM_SECONDS`] of the
/// layer's start.
const SIM_EFFECTS: &[&str] = &["ec.sim.ccparticleworld", "ec.sim.ccparticlesystems2", "ec.sim.ccmrmercury", "ec.sim.particleplayground", "ec.sim.waveworld", "ec.sim.foam"];

/// The latest layer time (s) a [`SIM_EFFECTS`] layer may be drawn at:
/// 300 s = 18,000 steps of up to 40,000 particles.
const MAX_SIM_SECONDS: f64 = 300.0;

/// The door's limits: the named ones, a [`MAX_LIST`] limit per command that
/// repeats its work for every entry of a list, and each [`ValueCap`] for
/// every command that sets a value by path.
macro_rules! limits {
    (named: [$($named:ident),* $(,)?], lists: [$($list:literal),* $(,)?], values: [$(($cap:ident, $f:ident)),* $(,)?] $(,)?) => {
        &[
            $($named,)*
            $(Limit { id: $list, what: "list entries", measure: Measure::Custom(list_len), max: MAX_LIST, copies: false },)*
            $(
                Limit { id: "prop.set", what: $cap.what, measure: Measure::Custom($f), max: $cap.max, copies: $cap.copies },
                Limit { id: "prop.addKey", what: $cap.what, measure: Measure::Custom($f), max: $cap.max, copies: $cap.copies },
                Limit { id: "keys.set", what: $cap.what, measure: Measure::Custom($f), max: $cap.max, copies: $cap.copies },
            )*
        ]
    };
}

/// A new composition's frame ([`MAX_FRAME_PIXELS`]; the engine clamps each
/// side to 4..30000 and defaults to 1920 × 1080).
const COMP_NEW_PIXELS: Limit = Limit { id: "comp.new", what: "pixels", measure: Measure::Custom(new_comp_pixels), max: MAX_FRAME_PIXELS, copies: false };
/// A new composition's frames ([`MAX_COMP_FRAMES`]; default 10 s at
/// 29.97 fps, no ceiling in the engine).
const COMP_NEW_FRAMES: Limit = Limit { id: "comp.new", what: "frames", measure: Measure::Custom(new_comp_frames), max: MAX_COMP_FRAMES, copies: false };
/// A new composition's frame rate ([`MIN_FRAME_RATE`]..[`MAX_FRAME_RATE`];
/// the engine takes 0.001..1000).
const COMP_NEW_RATE: Limit = Limit { id: "comp.new", what: "frames per second", measure: Measure::Custom(frame_rate), max: MAX_FRAME_RATE, copies: false };
/// A new composition's motion-blur samples per frame
/// ([`MAX_MOTION_BLUR_SAMPLES`]; default 16, the engine clamps 2..64).
const COMP_NEW_SAMPLES: Limit =
    Limit { id: "comp.new", what: "motion-blur samples", measure: Measure::Product(&["motionBlurSamples"]), max: MAX_MOTION_BLUR_SAMPLES, copies: false };
/// A new composition's adaptive sample limit ([`MAX_ADAPTIVE_SAMPLE_LIMIT`];
/// default 128, the engine clamps 16..256).
const COMP_NEW_ADAPTIVE: Limit =
    Limit { id: "comp.new", what: "adaptive motion-blur samples", measure: Measure::Product(&["adaptiveSampleLimit"]), max: MAX_ADAPTIVE_SAMPLE_LIMIT, copies: false };
/// Composition Settings' frame size ([`MAX_FRAME_PIXELS`]; a side not given
/// stays the comp's own, which [`measure`] checks after the command).
const COMP_SETTINGS_PIXELS: Limit = Limit { id: "comp.settings", what: "pixels", measure: Measure::Product(&["width", "height"]), max: MAX_FRAME_PIXELS, copies: false };
/// Composition Settings' frames ([`MAX_COMP_FRAMES`]).
const COMP_SETTINGS_FRAMES: Limit = Limit { id: "comp.settings", what: "frames", measure: Measure::Custom(frames_set), max: MAX_COMP_FRAMES, copies: false };
/// Composition Settings' frame rate.
const COMP_SETTINGS_RATE: Limit = Limit { id: "comp.settings", what: "frames per second", measure: Measure::Custom(frame_rate), max: MAX_FRAME_RATE, copies: false };
/// Composition Settings' motion-blur samples per frame.
const COMP_SETTINGS_SAMPLES: Limit =
    Limit { id: "comp.settings", what: "motion-blur samples", measure: Measure::Product(&["motionBlurSamples"]), max: MAX_MOTION_BLUR_SAMPLES, copies: false };
/// Composition Settings' adaptive sample limit.
const COMP_SETTINGS_ADAPTIVE: Limit =
    Limit { id: "comp.settings", what: "adaptive motion-blur samples", measure: Measure::Product(&["adaptiveSampleLimit"]), max: MAX_ADAPTIVE_SAMPLE_LIMIT, copies: false };
/// A new solid layer ([`MAX_FRAME_PIXELS`]; the engine renders a solid
/// whole at its own size, defaults to the comp's and sets no ceiling).
const SOLID_PIXELS: Limit = Limit { id: "layer.newSolid", what: "pixels", measure: Measure::Product(&["width", "height"]), max: MAX_FRAME_PIXELS, copies: false };
/// A new solid item (as [`SOLID_PIXELS`]).
const IMPORT_SOLID_PIXELS: Limit = Limit { id: "file.importSolid", what: "pixels", measure: Measure::Product(&["width", "height"]), max: MAX_FRAME_PIXELS, copies: false };
/// A solid replacing an item (as [`SOLID_PIXELS`]).
const REPLACE_SOLID_PIXELS: Limit =
    Limit { id: "file.replaceWithSolid", what: "pixels", measure: Measure::Product(&["width", "height"]), max: MAX_FRAME_PIXELS, copies: false };
/// Layer Settings' solid size (as [`SOLID_PIXELS`]).
const LAYER_SETTINGS_PIXELS: Limit = Limit { id: "layer.settings", what: "pixels", measure: Measure::Product(&["width", "height"]), max: MAX_FRAME_PIXELS, copies: false };
/// A new placeholder's frame ([`MAX_FRAME_PIXELS`]; default 1920 × 1080, no
/// ceiling in the engine). A comp made from it takes its size and rate.
const PLACEHOLDER_PIXELS: Limit =
    Limit { id: "file.importPlaceholder", what: "pixels", measure: Measure::Custom(placeholder_pixels), max: MAX_FRAME_PIXELS, copies: false };
/// A new placeholder's frames (default 30 s at 29.97 fps).
const PLACEHOLDER_FRAMES: Limit =
    Limit { id: "file.importPlaceholder", what: "frames", measure: Measure::Custom(placeholder_frames), max: MAX_COMP_FRAMES, copies: false };
/// A new placeholder's frame rate (the engine takes any, a zero or negative
/// one included).
const PLACEHOLDER_RATE: Limit =
    Limit { id: "file.importPlaceholder", what: "frames per second", measure: Measure::Custom(frame_rate), max: MAX_FRAME_RATE, copies: false };
/// A placeholder replacing an item: its frame.
const REPLACE_PLACEHOLDER_PIXELS: Limit =
    Limit { id: "file.replaceWithPlaceholder", what: "pixels", measure: Measure::Product(&["width", "height"]), max: MAX_FRAME_PIXELS, copies: false };
/// A placeholder replacing an item: its frames.
const REPLACE_PLACEHOLDER_FRAMES: Limit =
    Limit { id: "file.replaceWithPlaceholder", what: "frames", measure: Measure::Custom(frames_set), max: MAX_COMP_FRAMES, copies: false };
/// A placeholder replacing an item: its frame rate.
const REPLACE_PLACEHOLDER_RATE: Limit =
    Limit { id: "file.replaceWithPlaceholder", what: "frames per second", measure: Measure::Custom(frame_rate), max: MAX_FRAME_RATE, copies: false };
/// A VR environment's cube-map comp, 3 × 2 faces of `size` (default 1024;
/// the engine clamps 64..8192, a 24576 × 16384 comp at most).
const ENVIRONMENT_PIXELS: Limit =
    Limit { id: "comp.vr.createEnvironment", what: "cube-map pixels", measure: Measure::Custom(environment_pixels), max: MAX_FRAME_PIXELS, copies: false };
/// An extracted cube map, 3 × 2 faces of `faceSize` (the engine clamps
/// 16..8192).
const CUBEMAP_PIXELS: Limit =
    Limit { id: "comp.vr.extractCubemap", what: "cube-map pixels", measure: Measure::Custom(cubemap_pixels), max: MAX_FRAME_PIXELS, copies: false };
/// A new text layer's font size ([`MAX_FONT_SIZE`]).
const NEW_TEXT_SIZE: Limit = Limit { id: "layer.newText", what: "px of font size", measure: Measure::Product(&["size"]), max: MAX_FONT_SIZE, copies: false };
/// Edited text's font size ([`MAX_FONT_SIZE`]).
const SET_TEXT_SIZE: Limit = Limit { id: "layer.setText", what: "px of font size", measure: Measure::Product(&["size"]), max: MAX_FONT_SIZE, copies: false };
/// No Expression Selector ([`expression_selector`]).
const TEXT_SELECTOR: Limit = Limit { id: "text.addSelector", what: "expression selectors", measure: Measure::Custom(expression_selector), max: 1.0, copies: false };
/// No Expression Selector ([`expression_selector`]).
const LAYER_TEXT_SELECTOR: Limit =
    Limit { id: "layer.addTextSelector", what: "expression selectors", measure: Measure::Custom(expression_selector), max: 1.0, copies: false };
/// The Puppet mesh's expansion ([`MAX_PUPPET_EXPANSION`]; the command
/// writes it unclamped).
const PUPPET_MESH_EXPANSION: Limit =
    Limit { id: "puppet.mesh", what: "pixels of mesh expansion", measure: Measure::Product(&["expansion"]), max: MAX_PUPPET_EXPANSION, copies: false };
/// A new pin's mesh expansion (as [`PUPPET_MESH_EXPANSION`]).
const PUPPET_PIN_EXPANSION: Limit =
    Limit { id: "puppet.addPin", what: "pixels of mesh expansion", measure: Measure::Product(&["expansion"]), max: MAX_PUPPET_EXPANSION, copies: false };
/// A track point's feature region ([`FEATURE_SIZE`]).
const TRACK_FEATURE: Limit = Limit { id: "track.setPoint", what: "pixels of feature region", measure: Measure::Custom(feature_size), max: FEATURE_SIZE.max, copies: false };
/// A track point's search region ([`SEARCH_SIZE`]).
const TRACK_SEARCH: Limit = Limit { id: "track.setPoint", what: "pixels of search region", measure: Measure::Custom(search_size), max: SEARCH_SIZE.max, copies: false };
/// A Liquify stroke's dabs ([`MAX_LIQUIFY_DABS`]).
const LIQUIFY_STROKE: Limit = Limit { id: "liquify.stroke", what: "dabs", measure: Measure::Custom(liquify_dabs), max: MAX_LIQUIFY_DABS, copies: false };
/// Mask interpolation's keys per second: one interpolated mask path per
/// step of every span (the engine takes up to 1000 per second, ×2 with
/// fields). [`MAX_FRAME_RATE`].
const MASK_INTERPOLATE_RATE: Limit =
    Limit { id: "mask.interpolate", what: "keyframes per second", measure: Measure::Product(&["keyframeRate"]), max: MAX_FRAME_RATE, copies: false };
/// The default mask-interpolation rate (as [`MASK_INTERPOLATE_RATE`]).
const MASK_OPTIONS_RATE: Limit =
    Limit { id: "mask.interpolationOptions", what: "keyframes per second", measure: Measure::Product(&["keyframeRate"]), max: MAX_FRAME_RATE, copies: false };

/// Every limit of the door, in [`REVIEWED`]. The list limits are the
/// commands that repeat their work for every entry of a list ([`MAX_LIST`]):
/// layer copies, effect instances and pastes per target, item copies,
/// camera-solve layers, footage decodes.
const LIMITS: &[Limit] = limits!(
    named: [
        COMP_NEW_PIXELS,
        COMP_NEW_FRAMES,
        COMP_NEW_RATE,
        COMP_NEW_SAMPLES,
        COMP_NEW_ADAPTIVE,
        COMP_SETTINGS_PIXELS,
        COMP_SETTINGS_FRAMES,
        COMP_SETTINGS_RATE,
        COMP_SETTINGS_SAMPLES,
        COMP_SETTINGS_ADAPTIVE,
        SOLID_PIXELS,
        IMPORT_SOLID_PIXELS,
        REPLACE_SOLID_PIXELS,
        LAYER_SETTINGS_PIXELS,
        PLACEHOLDER_PIXELS,
        PLACEHOLDER_FRAMES,
        PLACEHOLDER_RATE,
        REPLACE_PLACEHOLDER_PIXELS,
        REPLACE_PLACEHOLDER_FRAMES,
        REPLACE_PLACEHOLDER_RATE,
        ENVIRONMENT_PIXELS,
        CUBEMAP_PIXELS,
        NEW_TEXT_SIZE,
        SET_TEXT_SIZE,
        TEXT_SELECTOR,
        LAYER_TEXT_SELECTOR,
        PUPPET_MESH_EXPANSION,
        PUPPET_PIN_EXPANSION,
        TRACK_FEATURE,
        TRACK_SEARCH,
        LIQUIFY_STROKE,
        MASK_INTERPOLATE_RATE,
        MASK_OPTIONS_RATE,
    ],
    lists: [
        "edit.duplicate",
        "effect.apply",
        "effect.applyLast",
        "effect.copy",
        "effect.paste",
        "keys.paste",
        "edit.pasteReversedKeyframes",
        "project.duplicate",
        "camera.createFromSolve",
        "camera.fromModel",
        "light.fromModel",
        "file.interpretFootage",
        "file.interpretProxy",
    ],
    values: [
        (REPEATER_COPIES, repeater_copies),
        (STAR_POINTS, star_points),
        (FEATURE_SIZE, feature_size_value),
        (SEARCH_SIZE, search_size_value),
    ],
);

/// The command door's gate: effectcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("effect", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Whether `spec` is one of the effects the engine builds in, not a
/// registered plug-in.
fn is_builtin_spec(spec: &EffectSpec) -> bool {
    effectcraft_engine::effects::registry().iter().any(|s| std::ptr::eq(s, spec))
}

/// Whether `name` (an id, a display name in any case, or an alias, as
/// `effect.apply` takes it) names an effect the engine builds in: it
/// resolves among the built-ins alone, the way the engine's `lookup` does,
/// and `lookup` itself — which also sees every registered plug-in, by id
/// ahead of any built-in's display name, and by display name in its sorted
/// list — lands on that same built-in, so no plug-in shadows it.
fn builtin_effect(name: &str) -> bool {
    use effectcraft_engine::effects as fx;
    let builtins = fx::registry();
    let by_id = |id: &str| builtins.iter().find(|s| s.id == id);
    let own = by_id(name)
        .or_else(|| builtins.iter().find(|s| s.name.eq_ignore_ascii_case(name)))
        .or_else(|| builtins.iter().find(|s| fx::aliases(s.id).iter().any(|a| a.eq_ignore_ascii_case(name))))
        .or_else(|| fx::migrate::EFFECT_NAME_ALIASES.iter().find(|(old, _)| old.eq_ignore_ascii_case(name)).and_then(|(_, id)| by_id(id)));
    match (own, fx::lookup(name)) {
        (Some(own), Some(found)) => std::ptr::eq(own, found),
        _ => false,
    }
}

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct EffectService;

/// Register the `effect` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(EffectService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/effect` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `effect.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.effect` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for EffectService {
    fn family(&self) -> &'static str {
        "effect"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The effect service serves system apps only.".into());
    }
    let area = Arc::new(areas.area(call, "effect").map_err(|e| format!("effect: {e}"))?);
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "render" => render(args, area),
        "run" => run(args, area),
        "commands" => commands(args, area),
        "export_lottie" => export_lottie(args, area),
        "import_lottie" => import_lottie(args, area),
        other => Err(format!("effect.{other} is not a method of the effect service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/effect`, as a call
/// without the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "effect");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("effect: {e}"))?;
    dispatch_in(method, args, &Arc::new(area))
}

/// A path strictly inside the call's area: relative, normal components
/// only, and resolved (through its deepest existing ancestor, so symlinks
/// cannot escape) under the canonical area.
fn contained(area: &Area, key: &str, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("effect: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("effect: `{key}` stays inside this call's effect area"));
    }
    let joined = area.root.join(rel_path);
    let root = area.root.canonicalize().map_err(|e| format!("effect: {e}"))?;
    allowed_in(&root, &joined.to_string_lossy()).map_err(|e| format!("effect: `{key}`: {e}"))?;
    Ok(joined)
}

/// A contained output path the call may write, refused before the engine
/// works when the area's rules would refuse it.
fn out_path(area: &Area, rel: &str) -> Result<PathBuf, String> {
    let out = contained(area, "out", rel)?;
    area.check(&out, 0).map_err(|e| format!("effect: {e}"))?;
    Ok(out)
}

/// `path`, if it resolves (through its deepest existing ancestor) inside
/// the canonical `root`.
fn allowed_in(root: &Path, path: &str) -> std::io::Result<PathBuf> {
    let outside = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the path is outside the call's effect area");
    let p = Path::new(path);
    let mut deepest = p.to_path_buf();
    while !deepest.exists() {
        match deepest.parent() {
            Some(parent) => deepest = parent.to_path_buf(),
            None => return Err(outside()),
        }
    }
    if !deepest.canonicalize()?.starts_with(root) {
        return Err(outside());
    }
    Ok(p.to_path_buf())
}

/// The engine-side gate (installed as the session's [`Services`]): every
/// read and write the engine performs — `file.open`, `file.saveAs`, the
/// Lottie commands and their extracted assets — re-checks that its path
/// resolves inside the area, and every write keeps the call's rules.
struct Guard {
    /// The canonical area.
    root: PathBuf,
    /// The call's area, whose rules the engine's writes keep.
    rules: Arc<Area>,
}

impl Guard {
    fn new(rules: &Arc<Area>) -> std::io::Result<Self> {
        Ok(Guard { root: rules.root.canonicalize()?, rules: rules.clone() })
    }

    /// The path, if it resolves (through its deepest existing ancestor)
    /// inside the area.
    fn allowed(&self, path: &str) -> std::io::Result<PathBuf> {
        allowed_in(&self.root, path)
    }

    /// The footage's every file inside the area: its path, and each frame
    /// of an image sequence (decoded by its own path).
    fn footage_inside(&self, footage: &Footage) -> bool {
        self.allowed(&footage.path).is_ok() && footage.sequence.iter().all(|frame| self.allowed(frame).is_ok())
    }
}

/// A 3D model's file: the engine reads its sibling resources by the paths
/// written inside it, outside any gate, so the service refuses models.
fn is_model(path: &str) -> bool {
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    effectcraft_media::MODEL_EXTENSIONS.contains(&ext.as_str())
}

impl Services for Guard {
    fn read_file(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let p = self.allowed(path)?;
        if std::fs::metadata(&p)?.len() > MAX_FILE_BYTES {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the file is larger than the effect service reads"));
        }
        std::fs::read(p)
    }

    fn write_file(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        if data.len() as u64 > MAX_FILE_BYTES {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the file is larger than the effect service writes"));
        }
        // `missing/../..` resolves inside the area only until the write
        // creates `missing`: no write path climbs.
        if Path::new(path).components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the path is outside the call's effect area"));
        }
        let p = self.allowed(path)?;
        self.rules.write(&p, data).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))
    }

    fn exists(&self, path: &str) -> bool {
        self.allowed(path).map(|p| p.exists()).unwrap_or(false)
    }
}

/// Media probing gated to the area: footage a project references outside
/// it never gets probed into the project.
struct ContainedImporter {
    guard: Arc<Guard>,
}

impl Importer for ContainedImporter {
    fn probe(&self, path: &str) -> Result<Footage, String> {
        self.guard.allowed(path).map_err(|e| e.to_string())?;
        if is_model(path) {
            return Err(format!("{path}: 3D models are not imported through the effect service"));
        }
        // A still with numbered siblings imports as a sequence: every
        // sibling must be inside the area before the engine reads one.
        for frame in effectcraft_media::sequence_files(path) {
            self.guard.allowed(&frame.to_string_lossy()).map_err(|e| e.to_string())?;
        }
        let footage = effectcraft_media::probe(path).map_err(|e| e.to_string())?;
        if !self.guard.footage_inside(&footage) {
            return Err(format!("{path}: the footage reaches outside the call's effect area"));
        }
        Ok(footage)
    }
}

/// Footage decoding gated to the area: a `Footage` whose path escapes it
/// decodes to nothing (the renderer draws its placeholder).
struct ContainedFootage {
    pool: effectcraft_media::MediaPool,
    guard: Arc<Guard>,
}

impl ContainedFootage {
    fn inside(&self, footage: &Footage) -> bool {
        self.guard.footage_inside(footage)
    }
}

impl FootageSource for ContainedFootage {
    fn frame(&self, item: ItemId, footage: &Footage, t: Tick) -> Option<Arc<Image>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::frame(&self.pool, item, footage, t)
    }

    fn audio(&self, item: ItemId, footage: &Footage, t: Tick, frames: usize, rate: u32) -> Option<Vec<f32>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::audio(&self.pool, item, footage, t, frames, rate)
    }

    fn aux(&self, item: ItemId, footage: &Footage, t: Tick) -> Option<Arc<AuxChannels>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::aux(&self.pool, item, footage, t)
    }

    /// No model renders through the service: the engine would read the
    /// model's sibling files by the paths inside it, outside any gate.
    fn model(&self, _item: ItemId, _footage: &Footage) -> Option<Arc<effectcraft_model::Model>> {
        None
    }

    fn vector_frame(&self, item: ItemId, footage: &Footage, scale: f64) -> Option<Arc<Image>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::vector_frame(&self.pool, item, footage, scale)
    }

    fn set_cache_budget(&self, bytes: usize) {
        FootageSource::set_cache_budget(&self.pool, bytes);
    }

    fn purge(&self) {
        FootageSource::purge(&self.pool);
    }

    // `set_conform_folder` is deliberately not forwarded: the default keeps
    // the pool's decoded-audio cache in memory instead of letting a command
    // point it at a folder.

    fn cache_budget(&self) -> Option<usize> {
        FootageSource::cache_budget(&self.pool)
    }
}

/// A fresh headless session whose file I/O, media probing and footage
/// decoding are all bound to the area. No exporter (the render queue's
/// encoders write wherever their output modules point, so `run` cannot
/// start one), no scripting, no plug-in loader, no config store (nothing a
/// command sets in the app's settings persists), no models folder, no
/// media browser of its own. Its decoded-frame and layer caches hold at most
/// [`CACHE_BYTES`] each (the engine's default is 1 GiB each).
fn session(area: &Arc<Area>) -> Result<Backend, String> {
    session_for(area, false)
}

/// [`session`]; for the command door (`door`), one that never evaluates an
/// expression and keeps [`DOOR_UNDO_LEVELS`] of undo. effectcraft runs
/// expressions as JavaScript with a per-loop iteration cap but no time or
/// memory budget, so no door call may run one: without an expression host
/// every value reads as its keyframed value (a query, an analysis, a
/// render), and [`check_expressions`] refuses to write a frame or a Lottie
/// file of a composition that holds an enabled expression rather than draw
/// it without. Every edit keeps the project as it was for undo, so a
/// call's edits of a large composition would otherwise keep 32 copies of it.
fn session_for(area: &Arc<Area>, door: bool) -> Result<Backend, String> {
    let guard = Arc::new(Guard::new(area).map_err(|e| format!("effect: {e}"))?);
    let mut s = Session {
        services: guard.clone(),
        footage: Arc::new(ContainedFootage { pool: effectcraft_media::MediaPool::with_budget(CACHE_BYTES), guard: guard.clone() }),
        importer: Some(Arc::new(ContainedImporter { guard })),
        expr: (!door).then(|| Arc::new(effectcraft_expr::Expressions) as Arc<dyn effectcraft_engine::render::ExprHost>),
        expr_check: (!door).then_some(effectcraft_expr::check_syntax as fn(&str) -> Result<(), String>),
        layer_cache: Arc::new(effectcraft_engine::render::LayerCache::new(CACHE_BYTES)),
        ..Default::default()
    };
    if door {
        s.prefs.general.undo_levels = DOOR_UNDO_LEVELS;
    }
    Ok(Backend::headless(s))
}

/// Open `args.path` (relative to the area) in the session, its effects'
/// file parameters fenced ([`fence_effect_files`]).
fn open(b: &mut Backend, area: &Area, args: &Json, method: &str) -> Result<String, String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let abs = contained(area, "path", &rel)?;
    b.exec("file.open", json!({"path": abs.to_string_lossy()})).map_err(|e| format!("effect.{method}: {e}"))?;
    fence_effect_files(b, method)?;
    Ok(rel)
}

/// Open the Lottie file at `lottie` (already contained in the area) as a
/// new composition of the session's project, made the active one: the
/// engine reads it through the [`Guard`] (inside the area, size-capped) and
/// writes its embedded images beside it under the area's rules. Its effects
/// are fenced like an opened project's.
fn import(b: &mut Backend, lottie: &Path, method: &str) -> Result<Json, String> {
    let r = b.exec("file.importLottie", json!({"path": lottie.to_string_lossy()})).map_err(|e| format!("effect.{method}: {e}"))?;
    fence_effect_files(b, method)?;
    Ok(json!({"comp": r["comp"], "items": r["items"], "warnings": r["warnings"]}))
}

/// How the engine reads an effect parameter that may name a file: each
/// loader takes the file's text inline, and reads anything else as a path,
/// with `std::fs`, outside the session's gated services.
#[derive(Clone, Copy)]
enum FileParam {
    /// A `.cube` LUT (`load_lut`): inline when it has a line break.
    Lut,
    /// An OCIO file transform (`ocio::load_file`): inline when it has a
    /// line break.
    OcioFile,
    /// An OCIO config (`load_config`): even inline, its search paths and
    /// file transforms name files, so any config is refused.
    OcioConfig,
    /// mocha shape data (`mocha_shape::load`): inline when it is JSON.
    MochaShapes,
}

impl FileParam {
    /// The parameter `key` (its match path inside the effect), when the
    /// engine may read it as a file: Apply Color LUT's `lut`, Lumetri's
    /// input LUT and look, OCIO's `file` and `configFile`, mocha's
    /// `shapeData`.
    fn of(key: &str) -> Option<FileParam> {
        let leaf = key.rsplit('/').next().unwrap_or(key);
        match (key, leaf) {
            (_, "lut") | ("basicCorrection/inputLutFile", _) | ("creative/lookFile", _) | (_, "inputLutFile") | (_, "lookFile") => Some(FileParam::Lut),
            (_, "file") => Some(FileParam::OcioFile),
            (_, "configFile") => Some(FileParam::OcioConfig),
            (_, "shapeData") => Some(FileParam::MochaShapes),
            _ => None,
        }
    }

    /// Whether the engine would take `value` inline (or ignore it) rather
    /// than read it as a path.
    fn inline(self, value: &str) -> bool {
        let t = value.trim();
        t.is_empty()
            || match self {
                FileParam::Lut | FileParam::OcioFile => value.contains('\n'),
                FileParam::OcioConfig => false,
                FileParam::MochaShapes => t.starts_with('{') || t.starts_with('['),
            }
    }
}

/// Refuse a project the engine would draw by reading a file by its own
/// path, or with an effect plug-in:
///
/// - a LUT, OCIO file or config, or mocha shape parameter holding a path
///   (in its value or any keyframe), or driven by an expression, which
///   could produce one when the frame renders;
/// - an Essential Properties value of a precomp layer whose control sets
///   such a parameter inside the precomp — directly, through a mirror or a
///   link, or through the Essential Properties of precomps nested deeper:
///   the renderer puts the instance's value into the parameter
///   (`essential::with_overrides`), so it is fenced as the parameter is;
/// - an effect instance whose id resolves to a registered plug-in rather
///   than a built-in effect (the renderer finds effects by id among both).
///
/// The engine reads those files with `std::fs`, outside the session's gated
/// services, so no area check could stop the read; the file's text inline
/// is drawn as before. It walks the session's project as it is now, so
/// `run` calls it after every command and before it writes.
fn fence_effect_files(b: &mut Backend, method: &str) -> Result<(), String> {
    use effectcraft_engine::project::{essential, GroupKind, ItemKind, LayerId, LayerSource, Node, PropGroup, Property, Uid, Value};

    /// Why the engine would read `p` as a file by its own path, if it would.
    fn reads_a_file(p: &Property, kind: FileParam) -> Option<&'static str> {
        if p.has_expression() {
            return Some("has an expression");
        }
        let named = std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)).any(|v| matches!(v, Value::Str(text) if !kind.inline(text)));
        named.then_some("names a file")
    }
    /// The file parameters of one effect instance: (match path, property, how it is read).
    fn file_params<'a>(g: &'a PropGroup, prefix: &str, out: &mut Vec<(String, &'a Property, FileParam)>) {
        for node in &g.children {
            match node {
                Node::Group(sub) => file_params(sub, &format!("{prefix}{}/", sub.match_id), out),
                Node::Prop(p) => {
                    let key = format!("{prefix}{}", p.match_id);
                    if let Some(kind) = FileParam::of(&key).or_else(|| FileParam::of(&p.match_id)) {
                        out.push((key, p, kind));
                    }
                }
            }
        }
    }
    /// An Essential Properties value of a precomp layer, and the properties
    /// inside the precomp its control sets when the layer renders.
    struct Instance<'a> {
        at: (ItemId, LayerId, Uid),
        layer: &'a str,
        prop: &'a Property,
        inner: ItemId,
        targets: Vec<(LayerId, Uid)>,
    }
    const READS: &str = "the engine would read a file by its own path, outside this call's folder; put the file's text in the parameter instead";

    let Some(session) = b.session() else { return Ok(()) };
    let project = &session.project;
    let comps = || {
        project.items.iter().filter_map(|(id, item)| match &item.kind {
            ItemKind::Comp(comp) => Some((*id, comp.as_ref())),
            _ => None,
        })
    };
    // Every property whose value reaches a file loader when a frame renders,
    // with what it feeds: (how the loader reads it, its key, the effect).
    let mut feeds: BTreeMap<(ItemId, LayerId, Uid), (FileParam, String, String)> = BTreeMap::new();
    for (cid, comp) in comps() {
        for layer in &comp.layers {
            let Some(effects) = layer.effects() else { continue };
            for effect in effects.groups() {
                if let GroupKind::Effect { effect: id } = &effect.kind {
                    if effectcraft_engine::effects::find(id).is_some_and(|spec| !is_builtin_spec(spec)) {
                        return Err(format!(
                            "effect.{method}: the effect `{id}` on layer `{}` is an effect plug-in, and no plug-in runs through the effect service",
                            layer.name
                        ));
                    }
                }
                let mut params = vec![];
                file_params(effect, "", &mut params);
                for (key, p, kind) in params {
                    if let Some(why) = reads_a_file(p, kind) {
                        return Err(format!("effect.{method}: the {} effect on layer `{}` {why} for `{key}`: {READS}", effect.match_id, layer.name));
                    }
                    if declares_too_large_a_table(p, kind) {
                        return Err(format!("effect.{method}: the {} effect on layer `{}` holds a LUT for `{key}` {TOO_LARGE}", effect.match_id, layer.name));
                    }
                    feeds.insert((cid, layer.id, p.uid), (kind, key, effect.match_id.clone()));
                }
            }
        }
    }
    let mut instances = vec![];
    for (cid, comp) in comps() {
        for layer in &comp.layers {
            let LayerSource::Comp { item: inner } = layer.source else { continue };
            let Some(group) = essential::group(layer) else { continue };
            let Some(eg) = project.comp(inner).and_then(|c| c.essential.as_ref()) else { continue };
            group.walk("", &mut |_, p| {
                if let Some(control) = essential::control_of(&p.match_id).and_then(|c| eg.resolve(c)) {
                    instances.push(Instance { at: (cid, layer.id, p.uid), layer: &layer.name, prop: p, inner, targets: control.kind.targets() });
                }
            });
        }
    }
    // An instance value feeds what its control's targets feed; a target may
    // itself be an instance value one precomp further in.
    loop {
        let mut grew = false;
        for i in &instances {
            if feeds.contains_key(&i.at) {
                continue;
            }
            let Some(fed) = i.targets.iter().find_map(|(l, u)| feeds.get(&(i.inner, *l, *u))).cloned() else { continue };
            if let Some(why) = reads_a_file(i.prop, fed.0) {
                return Err(format!(
                    "effect.{method}: the Essential Property `{}` on layer `{}` {why} for `{}` of the {} effect it sets inside its composition: {READS}",
                    i.prop.name, i.layer, fed.1, fed.2
                ));
            }
            if declares_too_large_a_table(i.prop, fed.0) {
                return Err(format!(
                    "effect.{method}: the Essential Property `{}` on layer `{}` holds a LUT for `{}` of the {} effect it sets inside its composition {TOO_LARGE}",
                    i.prop.name, i.layer, fed.1, fed.2
                ));
            }
            feeds.insert(i.at, fed);
            grew = true;
        }
        if !grew {
            break;
        }
    }
    Ok(())
}

/// The largest 3D LUT a `.cube` or `.csp` given inline may declare: the
/// engine's own `.cube` limit (256³ entries, about 200 MB).
const MAX_LUT_3D: f64 = 256.0;
/// The largest 1D LUT one may declare.
const MAX_LUT_1D: f64 = 65_536.0;
/// Why a LUT given inline is refused.
const TOO_LARGE: &str = "that declares a larger table than the door lets the engine build (a .csp is allocated from its declared size before a row is read, so a few bytes could ask for terabytes)";

/// Whether the LUT text inline in `p` (a `.cube`, or a `.csp` that OCIO
/// File Transform also takes) declares more than [`MAX_LUT_3D`]³ or
/// [`MAX_LUT_1D`] entries. The engine reads the `.csp` sizes and reserves
/// the whole table (`n³` entries) before it reads a row, with no ceiling;
/// a `.cube`'s rows must be present, and it stops past 256³ itself.
fn declares_too_large_a_table(p: &effectcraft_engine::project::Property, kind: FileParam) -> bool {
    use effectcraft_engine::project::Value;
    if !matches!(kind, FileParam::Lut | FileParam::OcioFile) {
        return false;
    }
    let too_large = |text: &str| {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let num = |l: Option<&str>| l.and_then(|l| l.split_whitespace().next()).and_then(|t| t.parse::<f64>().ok());
        if text.trim_start().starts_with("CSPLUTV100") {
            // CSPLUTV100, the kind, an optional metadata block, three shaper
            // curves (a count line and two value lines each), then the size.
            lines.next();
            let kind = lines.next().unwrap_or("");
            let mut lines = lines.peekable();
            if lines.peek().is_some_and(|l| l.starts_with("BEGIN METADATA")) {
                for l in lines.by_ref() {
                    if l.starts_with("END METADATA") {
                        break;
                    }
                }
            }
            for _ in 0..3 {
                for _ in 0..3 {
                    lines.next();
                }
            }
            return match num(lines.next()) {
                Some(n) if kind.starts_with("3D") => past(n, MAX_LUT_3D),
                Some(n) => past(n, MAX_LUT_1D),
                None => false,
            };
        }
        text.lines().map(str::trim).any(|l| {
            let mut t = l.split_whitespace();
            match (t.next(), t.next().and_then(|n| n.parse::<f64>().ok())) {
                (Some("LUT_3D_SIZE"), Some(n)) => past(n, MAX_LUT_3D),
                (Some("LUT_1D_SIZE"), Some(n)) => past(n, MAX_LUT_1D),
                _ => false,
            }
        })
    };
    std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)).any(|v| matches!(v, Value::Str(text) if kind.inline(text) && too_large(text)))
}

// ------------------------------------------------------------------ caps

/// What the caps measure in a project ([`measure`]).
#[derive(Debug, Default)]
struct Measured {
    /// Items and layers: the size [`MAX_PROJECT_SIZE`] bounds.
    size: usize,
    /// Keyframes of every property.
    keys: usize,
    /// The compositions that hold an enabled expression, by name.
    expressions: BTreeMap<ItemId, String>,
    /// The most copies the nested repeaters (and Offset Paths copies) of one
    /// shape group multiply to.
    shape_copies: f64,
    /// Per composition, the layers one of its frames draws, precompositions
    /// expanded (every instance of a precomp draws it again).
    expanded: BTreeMap<ItemId, f64>,
}

/// A number for a message: whole numbers without a fraction.
fn shown(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// The largest magnitude among a value's numeric components, and its
/// keyframes'.
fn largest(p: &effectcraft_engine::project::Property) -> f64 {
    std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)).flat_map(|v| v.components()).fold(0.0f64, |m, x| if x.is_finite() { m.max(x.abs()) } else { f64::INFINITY })
}

/// Every numeric component of a property's value and of its keyframes.
fn components(p: &effectcraft_engine::project::Property) -> impl Iterator<Item = f64> + '_ {
    std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)).flat_map(|v| v.components())
}

/// The copies a shape group's own Repeaters and Offset Paths multiply what
/// is above them by, times the most any group nested in it multiplies to.
fn shape_copies(g: &effectcraft_engine::project::PropGroup) -> f64 {
    use effectcraft_engine::project::Node;
    let mut own = 1.0f64;
    let mut inner = 1.0f64;
    for c in &g.children {
        let Node::Group(sub) = c else { continue };
        let copies = || sub.get("copies").map(largest).unwrap_or(1.0).max(1.0);
        if sub.match_id.starts_with("repeater") {
            own *= copies();
        } else if sub.match_id.starts_with("offset") {
            // The engine rounds Offset Paths' copies and clamps them to 1..1000.
            own *= copies().round().clamp(1.0, 1000.0);
        } else {
            inner = inner.max(shape_copies(sub));
        }
    }
    own * inner
}

/// The dabs the Liquify strokes in a `distortionMesh` text lay when a frame
/// renders: one every quarter of the brush radius along each stroke.
fn liquify_text_dabs(text: &str) -> f64 {
    effectcraft_engine::effects::distort4::parse_strokes(text)
        .iter()
        .map(|s| {
            let step = (s.size * 0.5).max(1.0) * 0.25;
            let dabs: f64 = s.points.windows(2).map(|w| ((w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) / step).ceil().max(1.0)).sum();
            dabs.max(1.0)
        })
        .sum()
}

/// One layer's walk in [`measure`].
struct Walk<'a> {
    method: &'a str,
    comp: &'a str,
    layer: &'a str,
    /// The layer's bounds in layer space (x0, y0, x1, y1).
    bounds: [f64; 4],
    keys: usize,
    effects: usize,
    expression: bool,
}

impl Walk<'_> {
    fn refuse(&self, prop: &str, why: String) -> String {
        format!("effect.{}: `{prop}` of layer `{}` in `{}` {why}", self.method, self.layer, self.comp)
    }
}

/// Walk the property group `g` of a layer: `owner` is the effect id or
/// `shape:<item>` the properties belong to (empty elsewhere), `prefix` their
/// match path inside it.
fn props(w: &mut Walk, g: &effectcraft_engine::project::PropGroup, owner: &str, prefix: &str) -> Result<(), String> {
    use effectcraft_engine::project::{GroupKind, Node, ParamUi, Value};
    for c in &g.children {
        let p = match c {
            Node::Group(sub) => {
                match &sub.kind {
                    GroupKind::Effect { effect } => {
                        w.effects += 1;
                        props(w, sub, effect, "")?;
                    }
                    _ if g.match_id == "contents" || owner.starts_with("shape:") => props(w, sub, &format!("shape:{}", sub.match_id), "")?,
                    // A path is kept only inside an effect, where the caps
                    // name nested parameters by it.
                    _ if owner.is_empty() => props(w, sub, owner, "")?,
                    _ => props(w, sub, owner, &format!("{prefix}{}/", sub.match_id))?,
                }
                continue;
            }
            Node::Prop(p) => p,
        };
        w.keys += p.keys.len();
        w.expression |= p.has_expression();
        let key: std::borrow::Cow<str> = if prefix.is_empty() { p.match_id.as_str().into() } else { format!("{prefix}{}", p.match_id).into() };
        // A slider keeps its declared valid range, as `prop.set` keeps it.
        if let ParamUi::Slider { min, max, .. } = p.ui {
            let tol = 1e-6 * min.abs().max(max.abs()).max(1.0);
            let scalars = std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)).filter_map(|v| match v {
                Value::Scalar(x) => Some(*x),
                _ => None,
            });
            for x in scalars {
                if !(x >= min - tol && x <= max + tol) {
                    return Err(w.refuse(&p.name, format!("is {}, outside its valid range {}..{} (the range `prop.set` keeps)", shown(x), shown(min), shown(max))));
                }
            }
        }
        if let Some(cap) = param_cap(owner, &key) {
            for x in components(p) {
                if past(x.abs(), cap.max) {
                    return Err(w.refuse(&p.name, format!("asks for {} {}, more than the {} the door allows", shown(x.abs()), cap.what, shown(cap.max))));
                }
                if short_of(x, cap.min) {
                    return Err(w.refuse(&p.name, format!("asks for {} {}, less than the {} the door allows", shown(x), cap.what, shown(cap.min))));
                }
            }
        }
        if p.ui == ParamUi::Point && PAD_POINT_EFFECTS.contains(&owner) {
            let [x0, y0, x1, y1] = w.bounds;
            for v in std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)) {
                let c = v.components();
                let (x, y) = (c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0));
                let reach = (x0 - x).max(x - x1).max(y0 - y).max(y - y1);
                if past(reach, MAX_POINT_OVERHANG) {
                    return Err(w.refuse(&p.name, format!("reaches {} px outside the layer, more than the {} px the door allows", shown(reach.round()), shown(MAX_POINT_OVERHANG))));
                }
            }
        }
        if owner == "ec.distort.liquify" && key.as_ref() == "distortionMesh" {
            for v in std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)) {
                let Value::Str(text) = v else { continue };
                let dabs = liquify_text_dabs(text);
                if past(dabs, MAX_LIQUIFY_DABS) {
                    return Err(w.refuse(&p.name, format!("lays {} dabs, more than the {} the door allows", shown(dabs), shown(MAX_LIQUIFY_DABS))));
                }
            }
        }
        if let Value::Text(_) = &p.value {
            let size = std::iter::once(&p.value)
                .chain(p.keys.iter().map(|k| &k.value))
                .filter_map(|v| match v {
                    Value::Text(doc) => Some(doc.runs().iter().map(|r| r.style.size).fold(doc.size, f64::max)),
                    _ => None,
                })
                .fold(0.0f64, f64::max);
            if past(size, MAX_FONT_SIZE) {
                return Err(w.refuse(&p.name, format!("is set in {} px type, more than the {} px the door allows", shown(size), shown(MAX_FONT_SIZE))));
            }
        }
    }
    Ok(())
}

/// Walk the project whole and refuse it when it holds more than the caps
/// allow: more than [`MAX_PROJECT_SIZE`] items and layers or
/// [`MAX_KEYFRAMES`] keyframes; a composition over [`MAX_FRAME_PIXELS`] or
/// [`MAX_COMP_FRAMES`], or with more motion-blur samples than
/// [`MAX_MOTION_BLUR_SAMPLES`] / [`MAX_ADAPTIVE_SAMPLE_LIMIT`]; a solid or
/// footage item over [`MAX_FRAME_PIXELS`]; a composition that contains
/// itself, or one of whose frames would draw more than [`MAX_FRAME_LAYERS`]
/// layers with its precompositions expanded; text over [`MAX_FONT_SIZE`];
/// a property past its reviewed cap ([`PARAM_CAPS`]) in its value or any
/// keyframe; nested repeaters past [`MAX_SHAPE_COPIES`].
///
/// It runs on the project as it is, so `run` calls it once the project is
/// open and after every command: whatever route a value took (a command's
/// parameter, a uid or index path the gate cannot read, the wiggler, a
/// paste, Essential Graphics, a project or Lottie file), the call stops
/// before a later command or the write could work on it. It is linear in
/// the project, whose size it bounds.
fn measure(project: &effectcraft_engine::project::Project, method: &str) -> Result<Measured, String> {
    use effectcraft_engine::project::{ItemKind, LayerSource};

    let mut m = Measured { size: project.items.len(), ..Default::default() };
    let name = |id: ItemId| project.item(id).map(|i| i.name.clone()).unwrap_or_else(|| format!("#{}", id.0));
    for (id, item) in &project.items {
        let (what, w, h) = match &item.kind {
            ItemKind::Comp(c) => ("composition", c.width, c.height),
            ItemKind::Solid(s) => ("solid", s.width, s.height),
            ItemKind::Footage(f) => ("footage", f.width, f.height),
            ItemKind::Folder => continue,
        };
        let px = w as f64 * h as f64;
        if px > MAX_FRAME_PIXELS {
            return Err(format!(
                "effect.{method}: the {what} `{}` is {w} × {h} = {} pixels, more than the {} the door allows",
                name(*id),
                shown(px),
                shown(MAX_FRAME_PIXELS)
            ));
        }
        // Footage keeps a rate the engine's frame walks can step by (a zero,
        // negative or vast rate makes a frame step they never leave).
        if let ItemKind::Footage(f) = &item.kind {
            let fps = f.frame_rate.as_f64();
            if f.has_video && !(fps > 0.0 && fps <= 1000.0) {
                return Err(format!("effect.{method}: the footage `{}` runs at {fps} fps, outside the 0–1000 fps the door allows", name(*id)));
            }
        }
        let ItemKind::Comp(c) = &item.kind else { continue };
        m.size += c.layers.len();
        let fps = c.frame_rate.as_f64();
        if !(MIN_FRAME_RATE..=MAX_FRAME_RATE).contains(&fps) {
            return Err(format!(
                "effect.{method}: the composition `{}` runs at {fps} fps, outside the {MIN_FRAME_RATE}–{MAX_FRAME_RATE} fps the door allows",
                name(*id)
            ));
        }
        let frames = c.duration.seconds() * fps;
        if past(frames, MAX_COMP_FRAMES) {
            return Err(format!(
                "effect.{method}: the composition `{}` is {} frames long ({} s at {} fps), more than the {} the door allows",
                name(*id),
                shown(frames.round()),
                c.duration.seconds(),
                fps,
                shown(MAX_COMP_FRAMES)
            ));
        }
        for (n, max, what) in [
            (c.motion_blur_samples as f64, MAX_MOTION_BLUR_SAMPLES, "motion-blur samples per frame"),
            (c.motion_blur_adaptive_limit as f64, MAX_ADAPTIVE_SAMPLE_LIMIT, "adaptive motion-blur samples"),
        ] {
            if n > max {
                return Err(format!("effect.{method}: the composition `{}` asks for {} {what}, more than the {} the door allows", name(*id), shown(n), shown(max)));
            }
        }
    }
    // The size bound walks the layers below; a project far past it (a file)
    // is refused before its properties are walked.
    if m.size > MAX_PROJECT_SIZE {
        return Err(format!(
            "effect.{method}: the project holds {} items and layers, more than the {MAX_PROJECT_SIZE} the door allows (a call stops once it would grow past them)",
            m.size
        ));
    }

    // Precompositions: no cycle, and a bounded expansion. The engine's own
    // cycle check walks every path through the precomps, so a frame of a
    // comp that nests one precomp many times in each of many levels would
    // take exponential time; a cycle the check misses recurses for ever.
    // Each comp's count is worked out once (`Some`), `None` while it is on
    // the path being walked.
    fn expand(
        project: &effectcraft_engine::project::Project,
        id: ItemId,
        memo: &mut BTreeMap<ItemId, Option<f64>>,
        path: &mut Vec<ItemId>,
    ) -> Result<f64, Vec<ItemId>> {
        match memo.get(&id) {
            Some(Some(n)) => return Ok(*n),
            Some(None) => {
                let at = path.iter().position(|p| *p == id).unwrap_or(0);
                return Err(path[at..].to_vec());
            }
            None => {}
        }
        let Some(c) = project.comp(id) else { return Ok(0.0) };
        memo.insert(id, None);
        path.push(id);
        let mut n = 0.0;
        for l in &c.layers {
            n += 1.0;
            if let LayerSource::Comp { item } = l.source {
                // A frame-blended precomp draws two of its frames.
                let blend = if c.enable_frame_blending && l.switches.frame_blend != effectcraft_engine::project::FrameBlend::Off { 2.0 } else { 1.0 };
                n += blend * expand(project, item, memo, path)?;
            }
        }
        path.pop();
        memo.insert(id, Some(n));
        Ok(n)
    }
    let mut memo = BTreeMap::new();
    for (id, _) in project.comps() {
        match expand(project, *id, &mut memo, &mut vec![]) {
            Ok(n) if n > MAX_FRAME_LAYERS => {
                return Err(format!(
                    "effect.{method}: a frame of the composition `{}` would draw {} layers with its precompositions expanded, more than the {} the door allows",
                    name(*id),
                    shown(n),
                    shown(MAX_FRAME_LAYERS)
                ));
            }
            Ok(n) => {
                m.expanded.insert(*id, n);
            }
            Err(cycle) => {
                let names: Vec<String> = cycle.iter().map(|c| format!("`{}`", name(*c))).collect();
                return Err(format!("effect.{method}: the composition {} contains itself ({}), which the engine cannot draw", names[0], names.join(" → ")));
            }
        }
    }

    // Properties: keyframes, expressions, effect instances, the reviewed
    // caps, slider ranges, reaching points, Liquify dabs, font sizes and
    // nested copies.
    for (id, c) in project.comps() {
        let comp = name(*id);
        for l in &c.layers {
            let (sw, sh) = effectcraft_engine::render::source_size(project, l);
            // Layers without a source rectangle (text, shapes) have
            // comp-sized bounds centred on their origin.
            let bounds = if sw == 0 {
                let (w, h) = (c.width as f64, c.height as f64);
                [-w / 2.0, -h / 2.0, w / 2.0, h / 2.0]
            } else {
                [0.0, 0.0, sw as f64, sh as f64]
            };
            let mut w = Walk { method, comp: &comp, layer: &l.name, bounds, keys: 0, effects: 0, expression: false };
            props(&mut w, &l.props, "", "")?;
            m.keys += w.keys;
            m.size += w.effects;
            if w.expression {
                m.expressions.insert(*id, comp.clone());
            }
            if let Some(contents) = l.props.sub("contents") {
                let copies = shape_copies(contents);
                if past(copies, MAX_SHAPE_COPIES) {
                    return Err(format!(
                        "effect.{method}: the repeaters of shape layer `{}` in `{comp}` multiply to {} copies, more than the {} the door allows",
                        l.name,
                        shown(copies),
                        shown(MAX_SHAPE_COPIES)
                    ));
                }
                m.shape_copies = m.shape_copies.max(copies);
            }
        }
    }
    if m.size > MAX_PROJECT_SIZE {
        return Err(format!(
            "effect.{method}: the project holds {} items, layers and effects, more than the {MAX_PROJECT_SIZE} the door allows (a call stops once it would grow past them)",
            m.size
        ));
    }
    if m.keys > MAX_KEYFRAMES {
        return Err(format!("effect.{method}: the project holds {} keyframes, more than the {MAX_KEYFRAMES} the door allows", m.keys));
    }
    Ok(m)
}

/// The project of a door session ([`measure`] it, or look at it).
fn project_of(b: &mut Backend) -> Arc<effectcraft_engine::project::Project> {
    b.session().map(|s| s.project.clone()).unwrap_or_default()
}

/// The composition a command works in, as the engine resolves it: `comp`
/// (an item id or a name), else the active one.
fn target_comp(s: &Session, p: &Json) -> Option<ItemId> {
    match p.get("comp") {
        Some(Json::Number(n)) => n.as_u64().map(ItemId).filter(|id| s.project.comp(*id).is_some()),
        Some(Json::String(name)) => s.project.items.values().find(|i| &i.name == name && i.as_comp().is_some()).map(|i| i.id),
        _ => s.active_comp_id(),
    }
}

/// The layer a command works on, as the engine resolves it: `layer` (or
/// the first of `layers`: an id, a 1-based index, `#n` or a name), else the
/// first selected layer.
fn target_layer(s: &Session, p: &Json) -> Option<(ItemId, effectcraft_engine::project::LayerId)> {
    use effectcraft_engine::project::LayerId;
    let cid = target_comp(s, p)?;
    let comp = s.project.comp(cid)?;
    let lid = match p.get("layer").or_else(|| p.get("layers").and_then(|a| a.get(0))) {
        Some(Json::Number(n)) => {
            let id = n.as_u64()?;
            if comp.layer(LayerId(id)).is_some() { LayerId(id) } else { comp.layers.get((id as usize).checked_sub(1)?)?.id }
        }
        Some(Json::String(name)) => match name.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()) {
            Some(i) => comp.layers.get(i.checked_sub(1)?)?.id,
            None => comp.layers.iter().find(|l| &l.name == name)?.id,
        },
        _ => *s.state.selected_layers.iter().find(|l| comp.layer(**l).is_some())?,
    };
    Some((cid, lid))
}

/// A layer's frames within its composition and its pixels (a layer without
/// a source rectangle is comp-sized): what an analysis of it walks.
fn layer_span(project: &effectcraft_engine::project::Project, comp: &effectcraft_engine::project::Comp, l: &effectcraft_engine::project::Layer) -> (f64, f64) {
    let (w, h) = effectcraft_engine::render::source_size(project, l);
    let (w, h) = if w == 0 { (comp.width, comp.height) } else { (w, h) };
    let (a, b) = (l.in_point.max(Tick::ZERO), l.out_point.min(comp.duration));
    let frames = if b > a { ((b - a).seconds() * comp.frame_rate.as_f64()).ceil() + 1.0 } else { 0.0 };
    (frames, w as f64 * h as f64)
}

/// The frame spans (comp frames between the first and last selected key)
/// of every property with at least `min` selected keys in the active comp.
fn selected_spans(s: &Session, min: usize) -> Vec<f64> {
    let Some(comp) = s.active_comp() else { return vec![] };
    let fr = comp.frame_rate;
    let mut groups: BTreeMap<(u64, u64), Vec<Tick>> = BTreeMap::new();
    for k in &s.state.selected_keys {
        groups.entry((k.layer.0, k.prop)).or_default().push(k.time);
    }
    groups
        .into_iter()
        .filter(|(_, ts)| ts.len() >= min)
        .filter_map(|((lid, _), ts)| {
            let l = comp.layer(effectcraft_engine::project::LayerId(lid))?;
            let (lo, hi) = (ts.iter().min()?, ts.iter().max()?);
            Some((fr.frame_at(l.comp_time(*hi)) - fr.frame_at(l.comp_time(*lo))).abs() as f64)
        })
        .collect()
}

/// The span (s) and step count of a capture (`motion.sketch`'s `points`,
/// `puppet.recordPin`'s `samples`: `[t, x, y]` with capture seconds, or
/// `[x, y]` one per frame) at `speed` % in a comp at `fps`: the keys it
/// makes, one per frame of the played-back span.
fn capture_keys(samples: Option<&Json>, speed: f64, fps: f64) -> f64 {
    let Some(points) = samples.and_then(Json::as_array) else { return 0.0 };
    let times: Vec<f64> = points.iter().filter_map(|q| q.as_array().filter(|a| a.len() >= 3).and_then(|a| a[0].as_f64())).collect();
    if times.is_empty() {
        return points.len() as f64;
    }
    let span = times.iter().copied().fold(f64::NEG_INFINITY, f64::max) - times.iter().copied().fold(f64::INFINITY, f64::min);
    // Played back faster or slower than captured: the longer of the two.
    let k = (speed / 100.0).max(100.0 / speed.max(1e-9));
    (span.abs() * k * fps).ceil() + 1.0
}

/// The name comparisons the engine makes giving `names` unique names among
/// `existing` (and each other): a name's candidates run through the names
/// that share its stem (its digits trimmed), each compared with every name
/// there.
fn name_work<'a>(existing: impl Iterator<Item = &'a str>, names: impl Iterator<Item = &'a str>) -> f64 {
    let stem = |n: &'a str| n.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end();
    let mut stems: BTreeMap<&str, f64> = BTreeMap::new();
    let mut total = 0.0;
    for n in existing {
        *stems.entry(stem(n)).or_default() += 1.0;
        total += 1.0;
    }
    let mut work = 0.0;
    for n in names {
        let same = stems.entry(stem(n)).or_default();
        work += (*same + 1.0) * (total + 1.0);
        *same += 1.0;
        total += 1.0;
    }
    work
}

/// Before an admitted command runs, refuse it when the work it would do on
/// the project as it stands passes a cap that no parameter shows the gate:
///
/// - a copy limit (the Repeater's copies): the copies its layer's
///   repeaters would multiply to ([`MAX_SHAPE_COPIES`]), from
///   `door.copies(id, params)` and the copies the layer already makes;
/// - a copy or paste: the project's size after it ([`MAX_PROJECT_SIZE`],
///   [`MAX_KEYFRAMES`]) and the comparisons naming its copies
///   ([`MAX_NAME_WORK`]), from the copies it would make of what is listed
///   or selected or on the clipboard;
/// - a generator: the keys it would add ([`MAX_GENERATED_KEYS`]) or the
///   frames it would smooth ([`MAX_SMOOTH_FRAMES`]), from the selected
///   keys' spans, the capture and the comp's rate;
/// - an analysis: the frames × pixels of the layer it walks
///   ([`MAX_ANALYSIS_PIXELS`]); a layer it cannot tell counts as the
///   largest of the comp's.
///
/// [`measure`] still checks what the command leaves; this keeps the
/// command itself from doing that work first.
fn precheck(b: &mut Backend, door: &Door, id: &str, params: &Json, measured: &Measured) -> Result<(), String> {
    let Some(s) = b.session() else { return Ok(()) };
    let refuse = |what: String| Err(format!("effect.run: `{id}` {what}"));
    let copies = door.copies(id, params);
    if copies > 1.0 {
        // The copies the target layer's repeaters make now, without the one
        // this sets.
        let held = target_layer(s, params)
            .and_then(|(cid, lid)| {
                let l = s.project.comp(cid)?.layer(lid)?;
                let contents = l.props.sub("contents")?;
                let old = match (params.get("prop").and_then(Json::as_u64), params.get("path").and_then(Json::as_str)) {
                    (Some(u), _) => l.props.find(u),
                    (None, Some(path)) => l.props.prop(path),
                    _ => None,
                }
                .map(largest)
                .unwrap_or(1.0)
                .max(1.0);
                Some(shape_copies(contents) / old)
            })
            .unwrap_or(measured.shape_copies.max(1.0));
        if held * copies > MAX_SHAPE_COPIES {
            return refuse(format!(
                "would make the repeaters multiply to {} copies, more than the {} the door allows",
                shown((held * copies).round()),
                shown(MAX_SHAPE_COPIES)
            ));
        }
    }
    let list = |key: &str| params.get(key).and_then(Json::as_array).map(Vec::len);
    let selected = s.state.selected_layers.len();
    let size_after = |growth: f64| -> Result<(), String> {
        let after = measured.size as f64 + growth;
        if after > MAX_PROJECT_SIZE as f64 {
            return Err(format!(
                "effect.run: `{id}` would grow the project to {} items, layers and effects, more than the {MAX_PROJECT_SIZE} the door allows",
                shown(after)
            ));
        }
        Ok(())
    };
    let keys_after = |growth: f64| -> Result<(), String> {
        let after = measured.keys as f64 + growth;
        if after > MAX_KEYFRAMES as f64 {
            return Err(format!("effect.run: `{id}` would grow the project to {} keyframes, more than the {MAX_KEYFRAMES} the door allows", shown(after)));
        }
        Ok(())
    };
    let clip_keys: f64 = s.state.key_clipboard.iter().map(|c| c.keys.len() as f64).sum();
    let naming = |work: f64| -> Result<(), String> {
        if work > MAX_NAME_WORK {
            return Err(format!(
                "effect.run: `{id}` would compare {} names giving its copies unique names, more than the {} the door allows in one command (copy fewer layers that share a name at once)",
                shown(work),
                shown(MAX_NAME_WORK)
            ));
        }
        Ok(())
    };
    // The layers a command copies in its comp: those listed, else the
    // selection.
    let copied_layers = || -> Vec<&effectcraft_engine::project::Layer> {
        let Some(comp) = target_comp(s, params).and_then(|c| s.project.comp(c)) else { return vec![] };
        let ids: Vec<Json> = match params.get("layers").and_then(Json::as_array) {
            Some(listed) => listed.clone(),
            None => s.state.selected_layers.iter().map(|l| json!(l.0)).collect(),
        };
        ids.iter()
            .filter_map(|v| target_layer(s, &json!({"comp": params.get("comp").cloned().unwrap_or(Json::Null), "layer": v})))
            .filter_map(|(_, lid)| comp.layer(lid))
            .collect()
    };
    let comp_names = || -> Vec<&str> { target_comp(s, params).and_then(|c| s.project.comp(c)).map(|c| c.layers.iter().map(|l| l.name.as_str()).collect()).unwrap_or_default() };
    let fps = s.active_comp().map(|c| c.frame_rate.as_f64()).unwrap_or(MAX_FRAME_RATE);
    match id {
        "edit.duplicate" => {
            size_after(list("layers").unwrap_or(selected.max(1)) as f64)?;
            let copies = copied_layers();
            naming(name_work(comp_names().into_iter(), copies.iter().map(|l| l.name.as_str())))?
        }
        "effect.apply" | "effect.applyLast" | "effect.paste" => {
            // The instances one layer gets: one per listing of it (the
            // engine keeps repeats) per effect named or on the clipboard,
            // each named as its effect is.
            let named: Vec<&str> = match id {
                "effect.paste" => s.state.effect_clipboard.iter().map(|g| g.name.as_str()).collect(),
                _ => {
                    let given: Vec<&str> = match params.get("effect") {
                        Some(Json::Array(items)) => items.iter().filter_map(Json::as_str).collect(),
                        Some(Json::String(n)) => vec![n.as_str()],
                        _ => vec!["Effect"],
                    };
                    given.into_iter().map(|n| effectcraft_engine::effects::lookup(n).map_or(n, |spec| spec.name)).collect()
                }
            };
            let targets = copied_layers();
            size_after((list("layers").unwrap_or(selected.max(1)) * named.len().max(1)) as f64)?;
            let mut work = 0.0;
            let mut seen = std::collections::BTreeSet::new();
            for l in &targets {
                if !seen.insert(l.id) {
                    continue;
                }
                let times = targets.iter().filter(|t| t.id == l.id).count();
                let existing: Vec<&str> = l.effects().map(|fx| fx.groups().map(|g| g.name.as_str()).collect()).unwrap_or_default();
                let new = (0..times).flat_map(|_| named.iter().copied());
                work += name_work(existing.into_iter(), new);
            }
            naming(work)?
        }
        "edit.paste" => {
            size_after((s.state.clipboard.len() + selected.max(1) * s.state.effect_clipboard.len()) as f64)?;
            keys_after(clip_keys * selected.max(1) as f64)?;
            naming(name_work(comp_names().into_iter(), s.state.clipboard.iter().map(|l| l.name.as_str())))?
        }
        "keys.paste" | "edit.pasteReversedKeyframes" => keys_after(clip_keys * list("layers").unwrap_or(selected.max(1)) as f64)?,
        "camera.createFromSolve" => {
            let points = list("points").unwrap_or(1);
            naming(name_work(comp_names().into_iter(), std::iter::repeat_n("Track Point", points)))?
        }
        "project.duplicate" => {
            let listed: Vec<ItemId> = match params.get("items").and_then(Json::as_array) {
                Some(items) => items
                    .iter()
                    .filter_map(|v| match v {
                        Json::Number(n) => n.as_u64().map(ItemId),
                        Json::String(n) => s.project.items.values().find(|i| &i.name == n).map(|i| i.id),
                        _ => None,
                    })
                    .collect(),
                None => s.state.project_selection.clone(),
            };
            let growth: usize = listed.iter().map(|i| 1 + s.project.comp(*i).map_or(0, |c| c.layers.len())).sum();
            size_after(growth as f64)?;
            let names: Vec<&str> = listed.iter().filter_map(|i| s.project.item(*i)).map(|i| i.name.as_str()).collect();
            naming(name_work(s.project.items.values().map(|i| i.name.as_str()), names.into_iter()))?
        }
        "keys.wiggle" => {
            let frequency = number(params, "frequency")?.unwrap_or(5.0).max(0.01);
            let step = (fps / frequency).round().max(1.0);
            let keys: f64 = selected_spans(s, 2).iter().map(|f| (f / step).ceil()).sum();
            if keys > MAX_GENERATED_KEYS {
                return refuse(format!("would add {} keyframes, more than the {} the door allows in one command", shown(keys), shown(MAX_GENERATED_KEYS)));
            }
        }
        "keys.exponentialScale" => {
            let keys: f64 = selected_spans(s, 2).iter().sum();
            if keys > MAX_GENERATED_KEYS {
                return refuse(format!("would add {} keyframes, more than the {} the door allows in one command", shown(keys), shown(MAX_GENERATED_KEYS)));
            }
        }
        "keys.smooth" => {
            // Each property's samples are evaluated again for every key the
            // smoother adds.
            let work: f64 = selected_spans(s, 3).iter().map(|f| (f + 1.0) * (f + 1.0)).sum();
            if work > MAX_SMOOTH_FRAMES * MAX_SMOOTH_FRAMES {
                return refuse(format!(
                    "would smooth {} frames, more than the {} the door allows in one command",
                    shown(work.sqrt().round()),
                    shown(MAX_SMOOTH_FRAMES)
                ));
            }
        }
        "mask.interpolate" => {
            let o = &s.state.mask_interp;
            let rate = number(params, "keyframeRate").ok().flatten().or(o.keyframe_rate).unwrap_or(fps).min(1000.0);
            let fields = params.get("keyframeFields").and_then(Json::as_bool).unwrap_or(o.keyframe_fields);
            let span = match params.get("times").and_then(Json::as_array) {
                Some(ts) => {
                    let ts: Vec<f64> = ts.iter().filter_map(Json::as_f64).collect();
                    ts.iter().copied().fold(f64::NEG_INFINITY, f64::max) - ts.iter().copied().fold(f64::INFINITY, f64::min)
                }
                None => selected_spans(s, 2).iter().copied().fold(0.0, f64::max) / fps.max(1e-9),
            };
            let keys = span.abs() * rate * if fields { 2.0 } else { 1.0 };
            if keys > MAX_GENERATED_KEYS {
                return refuse(format!("would add {} mask-path keyframes, more than the {} the door allows in one command", shown(keys.ceil()), shown(MAX_GENERATED_KEYS)));
            }
        }
        "motion.sketch" | "puppet.recordPin" => {
            let (samples, speed) = if id == "motion.sketch" {
                (params.get("points"), number(params, "captureSpeed")?.unwrap_or(100.0).max(1.0))
            } else {
                (params.get("samples"), number(params, "speed")?.unwrap_or(100.0).clamp(1.0, 10_000.0))
            };
            let comp_frames = s.active_comp().map_or(MAX_COMP_FRAMES, |c| (c.duration.seconds() * c.frame_rate.as_f64()).ceil() + 1.0);
            let pins = 1 + list("pins").unwrap_or(0);
            let keys = capture_keys(samples, speed, fps).min(comp_frames) * pins as f64;
            if keys > MAX_SMOOTH_FRAMES {
                return refuse(format!("would record and smooth {} keyframes, more than the {} the door allows in one command", shown(keys), shown(MAX_SMOOTH_FRAMES)));
            }
        }
        "keys.audioToKeyframes" => {
            let frames = target_comp(s, params)
                .and_then(|c| s.project.comp(c))
                .map_or(MAX_COMP_FRAMES, |c| ((c.work_area.1 - c.work_area.0).seconds() * c.frame_rate.as_f64()).ceil() + 1.0);
            if frames * 3.0 > MAX_GENERATED_KEYS {
                return refuse(format!(
                    "would add {} keyframes (three per work-area frame), more than the {} the door allows in one command",
                    shown(frames * 3.0),
                    shown(MAX_GENERATED_KEYS)
                ));
            }
        }
        "track.analyze" | "track.mask" | "track.camera" | "track.warpStabilizer" | "warp.analyze" | "camera.analyze" | "camera.deletePoints"
        | "roto.propagate" | "roto.freeze" | "roto.status" | "layer.sceneEditDetection" | "layer.autoTrace" => {
            let one_frame = match id {
                "track.analyze" | "track.mask" => params.get("direction").and_then(Json::as_str).is_some_and(|d| d.starts_with("frame")),
                "roto.status" => !params.get("compute").and_then(Json::as_bool).unwrap_or(false) && !params.get("matte").and_then(Json::as_bool).unwrap_or(false),
                "layer.autoTrace" => params.get("timeSpan").and_then(Json::as_str) != Some("workArea"),
                _ => false,
            };
            if one_frame {
                return Ok(());
            }
            let (frames, px) = match target_layer(s, params) {
                Some((cid, lid)) => s.project.comp(cid).zip(s.project.comp(cid).and_then(|c| c.layer(lid))).map(|(c, l)| layer_span(&s.project, c, l)).unwrap_or((0.0, 0.0)),
                None => {
                    // The layer is chosen by state the gate does not read (a
                    // tracker, a selection): the largest of the comp's.
                    let comps: Vec<_> = match target_comp(s, params) {
                        Some(c) => s.project.comp(c).into_iter().collect(),
                        None => s.project.comps().map(|(_, c)| c).collect(),
                    };
                    comps.iter().flat_map(|c| c.layers.iter().map(|l| layer_span(&s.project, c, l))).fold((0.0, 0.0), |a, b| if b.0 * b.1 > a.0 * a.1 { b } else { a })
                }
            };
            if frames * px > MAX_ANALYSIS_PIXELS {
                return refuse(format!(
                    "would analyse {} frames of {} pixels ({} pixels in all), more than the {} the door allows in one command: trim the layer to the frames to analyse",
                    shown(frames),
                    shown(px),
                    shown(frames * px),
                    shown(MAX_ANALYSIS_PIXELS)
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// The commands that draw a layer or a composition while they run, at full
/// size: [`check_frame`] looks at their comp before they do.
const DRAWING_COMMANDS: &[&str] = &[
    "effect.pickColor",
    "puppet.addPin",
    "puppet.info",
    "scopes.analyze",
    "layer.autoTrace",
    "layer.sceneEditDetection",
    "track.analyze",
    "track.mask",
    "track.camera",
    "track.warpStabilizer",
    "warp.analyze",
    "camera.analyze",
    "camera.deletePoints",
    "roto.propagate",
    "roto.freeze",
    "roto.status",
];

/// The compositions a frame of `comp` draws: it and every precomposition
/// it nests (the project is acyclic once [`measure`] passed it).
fn nested_comps(project: &effectcraft_engine::project::Project, comp: ItemId) -> std::collections::BTreeSet<ItemId> {
    let mut seen = std::collections::BTreeSet::new();
    let mut todo = vec![comp];
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(c) = project.comp(id) {
            todo.extend(c.layers.iter().filter_map(|l| match l.source {
                effectcraft_engine::project::LayerSource::Comp { item } => Some(item),
                _ => None,
            }));
        }
    }
    seen
}

/// Before `run` writes a frame or a Lottie file of a composition: refuse it
/// when the compositions it draws hold an enabled expression. The door's
/// session evaluates none ([`session_for`]), so the frame would be drawn
/// without them; Lottie export evaluates none either, and would drop them
/// or (`include_expressions`) hand them to a player that runs them.
fn check_expressions(b: &mut Backend, measured: &Measured, comp: Option<&Json>, method: &str) -> Result<(), String> {
    if measured.expressions.is_empty() {
        return Ok(());
    }
    let Some(s) = b.session() else { return Ok(()) };
    let cid = s.resolve_comp(comp).map_err(|e| format!("effect.{method}: {e}"))?;
    if let Some(name) = nested_comps(&s.project, cid).iter().find_map(|c| measured.expressions.get(c)) {
        return Err(format!(
            "effect.{method}: the composition `{name}` holds an enabled expression; the door writes no frame or Lottie file of it (expressions run without a time or memory budget, so the door evaluates none)"
        ));
    }
    Ok(())
}

/// Before `run` draws a frame of `comp` at `time` with `max_side`: the
/// engine renders the comp at the scale `max_side` gives (not at full
/// size), but every layer's own buffer at its own size times that scale (a
/// solid whole, footage at its native size, a text or shape layer at its
/// content's bounds up to 16384 × 16384, a precomp that preserves its
/// resolution at full size). Refuse a text or shape layer whose buffer
/// would pass [`MAX_FRAME_PIXELS`], and a simulation drawn past
/// [`MAX_SIM_SECONDS`] of its layer's time; [`measure`] has bounded the
/// comps, solids, footage and the precomp expansion.
fn check_frame(b: &mut Backend, comp: Option<&Json>, time: Option<f64>, max_side: u32, method: &str) -> Result<(), String> {
    use effectcraft_engine::project::{GroupKind, LayerSource};
    use effectcraft_engine::render::EvalCtx;
    let Some(s) = b.session() else { return Ok(()) };
    let cid = s.resolve_comp(comp).map_err(|e| format!("effect.{method}: {e}"))?;
    let t = time.map(Tick::from_seconds_f64).unwrap_or_else(|| s.time());
    let Some(c) = s.project.comp(cid) else { return Ok(()) };
    let long = c.width.max(c.height).max(1) as f64;
    let scale = if max_side == 0 { 1.0 } else { (max_side as f64 / long).min(1.0) };
    // (comp, comp time, scale, depth) still to look at.
    let mut todo = vec![(cid, t, scale, 0usize)];
    while let Some((id, t, scale, depth)) = todo.pop() {
        let Some(c) = s.project.comp(id) else { continue };
        let ctx = EvalCtx::new(&s.project, id, c, t);
        let name = || s.project.item(id).map(|i| i.name.clone()).unwrap_or_default();
        for l in &c.layers {
            if let Some(fx) = l.effects() {
                let sim = fx.groups().any(|g| g.enabled && matches!(&g.kind, GroupKind::Effect { effect } if SIM_EFFECTS.contains(&effect.as_str())));
                let lt = l.layer_time(t).seconds();
                if sim && l.switches.effects && lt.abs() > MAX_SIM_SECONDS {
                    return Err(format!(
                        "effect.{method}: layer `{}` in `{}` simulates from its start to {} s of its time at this frame, more than the {} s the door draws",
                        l.name,
                        name(),
                        shown(lt.round()),
                        shown(MAX_SIM_SECONDS)
                    ));
                }
            }
            match l.source {
                LayerSource::Text | LayerSource::Shape => {
                    let Some([x0, y0, x1, y1]) = effectcraft_engine::render::content_bounds(&ctx, l) else { continue };
                    let side = |a: f64, b: f64| ((b.min(20_000.0) - a.max(-20_000.0)).max(0.0) * scale).ceil() + 4.0;
                    let px = side(x0, x1).min(16_384.0) * side(y0, y1).min(16_384.0);
                    if px > MAX_FRAME_PIXELS {
                        return Err(format!(
                            "effect.{method}: layer `{}` in `{}` draws into a {} pixel buffer at this size, more than the {} the door allows",
                            l.name,
                            name(),
                            shown(px),
                            shown(MAX_FRAME_PIXELS)
                        ));
                    }
                }
                LayerSource::Comp { item } if depth < 16 => {
                    let full = scale < 1.0 && s.project.comp(item).is_some_and(|n| n.preserve_resolution);
                    todo.push((item, ctx.nested_time(l), if full { 1.0 } else { scale }, depth + 1));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Before `run` writes a Lottie file: the exporter embeds every still image
/// the composition and its precomps draw as base64, reading each whole, so
/// together they stay within the [`MAX_FILE_BYTES`] the write could hold
/// (base64 adds a third). Only files inside the call's area count: the
/// exporter reads through the [`Guard`], which refuses the rest.
fn check_lottie(b: &mut Backend, area: &Area, comp: Option<&Json>, method: &str) -> Result<(), String> {
    use effectcraft_engine::project::{FootageKind, ItemKind, LayerSource};
    let Some(s) = b.session() else { return Ok(()) };
    let cid = s.resolve_comp(comp).map_err(|e| format!("effect.{method}: {e}"))?;
    let mut images = std::collections::BTreeSet::new();
    for id in nested_comps(&s.project, cid) {
        for l in s.project.comp(id).map(|c| c.layers.as_slice()).unwrap_or_default() {
            if let LayerSource::Footage { item } = l.source {
                if let Some(ItemKind::Footage(f)) = s.project.item(item).map(|i| &i.kind) {
                    if f.kind == FootageKind::Still {
                        images.insert(f.path.clone());
                    }
                }
            }
        }
    }
    let root = area.root.canonicalize().map_err(|e| format!("effect.{method}: {e}"))?;
    let bytes: u64 = images.iter().filter(|p| allowed_in(&root, p).is_ok()).filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    if bytes > MAX_FILE_BYTES / 4 * 3 {
        return Err(format!("effect.{method}: the images this Lottie file would embed total {bytes} bytes, more than the {} the door writes", MAX_FILE_BYTES / 4 * 3));
    }
    Ok(())
}

/// The command door's session: when it ends (a refusal part way included),
/// it closes its project, which cancels every analysis and task a command
/// started on its own thread and waits for each to stop, so none outlives
/// the call.
struct DoorSession(Backend);

impl Drop for DoorSession {
    fn drop(&mut self) {
        let _ = self.0.exec("file.closeProject", json!({}));
    }
}

impl std::ops::Deref for DoorSession {
    type Target = Backend;
    fn deref(&self) -> &Backend {
        &self.0
    }
}

impl std::ops::DerefMut for DoorSession {
    fn deref_mut(&mut self) -> &mut Backend {
        &mut self.0
    }
}

/// The longest side of the frame `args` ask for (`max_side`, default 1024,
/// 0 or more than [`MAX_RENDER_SIDE`] being that).
fn frame_side(args: &Json) -> u32 {
    let requested = args["max_side"].as_u64().unwrap_or(1024);
    if requested == 0 { MAX_RENDER_SIDE } else { requested.min(MAX_RENDER_SIDE as u64) as u32 }
}

/// What a write to `out` makes, by `out`'s extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutKind {
    /// `.ecproj`: the project, as `file.saveAs` writes it.
    Project,
    /// `.json` / `.lottie`: a composition as Lottie, as `export_lottie` writes it.
    Lottie,
    /// `.png`: one frame of a composition, as `render` writes it.
    Frame,
}

impl OutKind {
    fn of(rel: &str) -> Option<OutKind> {
        match extension(rel).as_str() {
            "ecproj" => Some(OutKind::Project),
            "json" | "lottie" => Some(OutKind::Lottie),
            "png" => Some(OutKind::Frame),
            _ => None,
        }
    }
}

/// `rel`'s extension, lowercase.
fn extension(rel: &str) -> String {
    Path::new(rel).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Write the session's project to `out` (contained, and admitted by the
/// area's rules) as `kind`, fenced once more first: the project itself
/// through the [`Guard`], a composition as Lottie (`comp`,
/// `include_expressions`) through the [`Guard`], or one frame (`comp`,
/// `time`, `max_side`, `transparent`) as PNG under the area's rules. Returns
/// the writer's own answer fields.
fn write_out(b: &mut Backend, area: &Area, out: &Path, kind: OutKind, args: &Json, method: &str) -> Result<Json, String> {
    fence_effect_files(b, method)?;
    let comp = args.get("comp").filter(|c| !c.is_null());
    match kind {
        OutKind::Project => {
            let r = b.exec("file.saveAs", json!({"path": out.to_string_lossy()})).map_err(|e| format!("effect.{method}: {e}"))?;
            Ok(json!({"bytes": r["bytes"]}))
        }
        OutKind::Lottie => {
            let mut p = json!({"path": out.to_string_lossy(), "includeExpressions": args["include_expressions"].as_bool().unwrap_or(false)});
            if let Some(comp) = comp {
                p["comp"] = comp.clone();
            }
            let r = b.exec("file.exportLottie", p).map_err(|e| format!("effect.{method}: {e}"))?;
            Ok(json!({"bytes": r["bytes"], "warnings": r["warnings"]}))
        }
        OutKind::Frame => {
            let max_side = frame_side(args);
            let transparent = args["transparent"].as_bool().unwrap_or(false);
            let frame = b.render_with(comp, args["time"].as_f64(), max_side, transparent).map_err(|e| format!("effect.{method}: {e}"))?;
            area.write(out, &frame.png).map_err(|e| format!("effect.{method}: {e}"))?;
            Ok(json!({
                "bytes": frame.png.len(),
                "width": frame.width, "height": frame.height,
                "comp": frame.comp, "time": frame.time,
            }))
        }
    }
}

fn info(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let mut b = session(area)?;
    let rel = open(&mut b, area, args, "info")?;
    let mut sum = b.exec("project.summary", json!({})).map_err(|e| format!("effect.info: {e}"))?;
    sum["path"] = json!(rel);
    Ok(sum)
}

fn render(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let mut b = session(area)?;
    open(&mut b, area, args, "render")?;
    let mut r = write_out(&mut b, area, &out, OutKind::Frame, args, "render")?;
    r["out"] = json!(out_rel);
    Ok(r)
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over the project or Lottie file at `path`, or
/// a new empty project, the project fenced after each; then `out` written by
/// its extension under the area's rules ([`write_out`]).
///
/// Engine work runs on the shell's UI thread, so the call is bounded as it
/// goes: the gate's limits before anything runs; the project measured
/// against the caps once it is open and after every command ([`measure`]);
/// each command's own work on the project as it stands checked before it
/// runs ([`precheck`]); a frame or Lottie file of a composition that holds
/// an enabled expression refused, and a frame's layer buffers and
/// simulations checked before it is drawn ([`check_expressions`],
/// [`check_frame`], [`check_lottie`]). The session evaluates no expression,
/// and nothing a command starts on another thread outlives the call
/// ([`DoorSession`]).
fn run(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let door = door()?;
    let admitted = door.admit_all(&args["cmds"], area)?;
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => {
            let kind = OutKind::of(rel)
                .ok_or_else(|| format!("effect.run: `out` is a project (.ecproj), a Lottie file (.json or .lottie) or a frame (.png), not `{rel}`"))?;
            Some((rel, out_path(area, rel)?, kind))
        }
        None => None,
    };
    let mut b = DoorSession(session_for(area, true)?);
    // Without `path`, commands build on a fresh empty project (comp.new …).
    let imported = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(rel) if matches!(extension(rel).as_str(), "json" | "lottie") => {
            let lottie = contained(area, "path", rel)?;
            Some(import(&mut b, &lottie, "run")?)
        }
        Some(_) => {
            open(&mut b, area, args, "run")?;
            None
        }
        None => None,
    };
    let mut measured = measure(&project_of(&mut b), "run")?;
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        precheck(&mut b, door, &id, &params, &measured)?;
        // Commands that draw a layer or the comp at full size as they run
        // (an effect's input, a puppet mesh, the scopes, every frame of an
        // analysis) draw what a written frame would: checked the same way,
        // at the current time.
        if DRAWING_COMMANDS.contains(&id.as_str()) {
            check_frame(&mut b, params.get("comp").filter(|c| !c.is_null()), None, 0, "run")?;
        }
        let r = b.exec(&id, params).map_err(|e| format!("effect.run {id}: {e}"))?;
        // What the command planted, before a later command or the write
        // could draw it.
        fence_effect_files(&mut b, "run")?;
        measured = measure(&project_of(&mut b), "run")?;
        results.push(json!({"id": id, "result": r}));
    }
    let mut answer = json!({"results": results, "out": Json::Null, "format": Json::Null});
    if let Some(imported) = imported {
        answer["imported"] = imported;
    }
    if let Some((rel, abs, kind)) = out {
        let comp = args.get("comp").filter(|c| !c.is_null());
        match kind {
            OutKind::Project => {}
            OutKind::Lottie => {
                check_expressions(&mut b, &measured, comp, "run")?;
                check_lottie(&mut b, area, comp, "run")?;
            }
            OutKind::Frame => {
                check_expressions(&mut b, &measured, comp, "run")?;
                check_frame(&mut b, comp, args["time"].as_f64(), frame_side(args), "run")?;
            }
        }
        if let Json::Object(fields) = write_out(&mut b, area, &abs, kind, args, "run")? {
            for (key, value) in fields {
                answer[key.as_str()] = value;
            }
        }
        answer["out"] = json!(rel);
        answer["format"] = json!(extension(rel));
    }
    Ok(answer)
}

/// The engine's catalog entries of the commands the door runs.
fn commands(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let door = door()?;
    let mut b = session(area)?;
    let list = b.exec("command.list", json!({"filter": args["filter"]})).map_err(|e| format!("effect.commands: {e}"))?;
    let runs = |c: &&Json| c["id"].as_str().is_some_and(|id| door.runs(id));
    Ok(Json::Array(list.as_array().map(|all| all.iter().filter(runs).cloned().collect()).unwrap_or_default()))
}

fn export_lottie(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let mut b = session(area)?;
    open(&mut b, area, args, "export_lottie")?;
    let mut r = write_out(&mut b, area, &out, OutKind::Lottie, args, "export_lottie")?;
    r["out"] = json!(out_rel);
    Ok(r)
}

fn import_lottie(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let rel = args["path"].as_str().unwrap_or("");
    let lottie = contained(area, "path", rel)?;
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let mut b = session(area)?;
    let mut r = import(&mut b, &lottie, "import_lottie")?;
    write_out(&mut b, area, &out, OutKind::Project, args, "import_lottie")?;
    r["out"] = json!(out_rel);
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a real project in the area — a 64x36 24 fps comp with an
    /// opaque solid — through the engine's own commands, saved as
    /// `main.ecproj`.
    fn fixture(host: &Path) -> Json {
        dispatch(
            "run",
            &json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
            ], "out": "main.ecproj"}),
            host,
        )
        .unwrap()
    }

    #[test]
    fn the_area_is_the_familys_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = Slot::new().area(&service_call("info", json!({}), host, false), "effect").unwrap();
        assert_eq!(a.root, host.join("effect"));
        assert!(a.root.is_dir() && a.may_replace && a.quota_left.is_none(), "without a resolver, as before");
        fixture(host);
        fixture(host);
        assert!(host.join("effect/main.ecproj").is_file(), "files land inside the area, replacing as before");
        assert!(!host.join("main.ecproj").exists(), "and not beside Mail's and Calendar's data");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("effect.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    /// The command door as the shell calls it, in the area `areas` gives
    /// the call.
    fn run_in(areas: &Slot, args: Json, host_dir: &Path, may_prompt: bool) -> Result<Json, String> {
        serve(areas, &service_call("run", args, host_dir, may_prompt))
    }

    /// The fixture project, built straight into a caller's folder `root`.
    fn fixture_in(areas: &Slot, root: &Path) -> Json {
        run_in(
            areas,
            json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
            ], "out": "main.ecproj"}),
            root,
            true,
        )
        .unwrap()
    }

    /// The centre pixel of a PNG.
    fn centre(png: &[u8]) -> [u8; 4] {
        let img = image::load_from_memory(png).unwrap().to_rgba8();
        img.get_pixel(img.width() / 2, img.height() / 2).0
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        fixture_in(&areas, &host);
        let sum = serve(&areas, &service_call("info", json!({"path": "main.ecproj"}), &host, false)).unwrap();
        assert_eq!(sum["path"], json!("main.ecproj"), "{sum}");
        serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": "frames/f.png", "max_side": 16}), &host, false)).unwrap();
        serve(&areas, &service_call("export_lottie", json!({"path": "main.ecproj", "comp": "Main", "out": "main.json"}), &host, false)).unwrap();
        serve(&areas, &service_call("import_lottie", json!({"path": "main.json", "out": "again.ecproj"}), &host, false)).unwrap();
        for made in ["main.ecproj", "frames/f.png", "main.json", "again.ecproj"] {
            assert!(root.join(made).is_file(), "{made}");
        }
        assert!(!host.exists() && !root.join("effect").exists());
        std::fs::write(dir.path().join("beside.ecproj"), b"{}").unwrap();
        for bad in ["../beside.ecproj", "/etc/hosts", "frames/../../beside.ecproj"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.ecproj"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": "up/f.png"}), &host, true)).is_err());
            assert!(!dir.path().join("f.png").exists());
        }
    }

    /// An agent's call never replaces a file — the engine's own saves
    /// included — before the engine works; an app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        fixture_in(&areas, dir.path());
        for name in ["taken.png", "taken.json", "taken.ecproj"] {
            std::fs::write(dir.path().join(name), b"keep me").unwrap();
        }
        for (method, args) in [
            ("render", json!({"path": "main.ecproj", "out": "taken.png"})),
            ("export_lottie", json!({"path": "main.ecproj", "comp": "Main", "out": "taken.json"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        for out in ["taken.ecproj", "main.ecproj"] {
            let refused = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": out}), dir.path(), false).unwrap_err();
            assert!(refused.contains("already exists"), "run: {refused}");
        }
        for name in ["taken.png", "taken.json", "taken.ecproj"] {
            assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"keep me", "{name}");
        }
        // Nor a command that saves on its own: the door never runs one.
        let saves = run_in(&areas, json!({"path": "main.ecproj", "cmds": [{"id": "file.saveAs", "params": {"path": dir.path().join("taken.ecproj").to_string_lossy()}}]}), dir.path(), false);
        assert!(saves.as_ref().is_err_and(|e| e.contains("not reviewed to run through it")), "{saves:?}");
        assert_eq!(std::fs::read(dir.path().join("taken.ecproj")).unwrap(), b"keep me");
        serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": "taken.png", "max_side": 8}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.png")).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    }

    /// What a call writes, the engine's own saves included, must fit what
    /// is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        fixture_in(&resolver(dir.path(), None), dir.path());
        let tight = resolver(dir.path(), Some(8));
        for (method, args) in [
            ("render", json!({"path": "main.ecproj", "out": "f.png"})),
            ("export_lottie", json!({"path": "main.ecproj", "comp": "Main", "out": "m.json"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        let refused = run_in(&tight, json!({"path": "main.ecproj", "cmds": [], "out": "copy.ecproj"}), dir.path(), true).unwrap_err();
        assert!(refused.contains("bytes left"), "run: {refused}");
        assert!(!dir.path().join("f.png").exists() && !dir.path().join("copy.ecproj").exists() && !dir.path().join("m.json").exists());
    }

    /// Footage outside the folder is never decoded: an image sequence
    /// whose frames lead outside it (through a link on import, or listed by
    /// a project) decodes nothing, while the same sequence inside decodes;
    /// a 3D model (whose sibling files the engine would read unguarded) is
    /// refused and renders nothing.
    #[test]
    fn footage_outside_the_folder_is_never_read() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let rules = Arc::new(Area::new(&root, None, true));
        let guard = Arc::new(Guard::new(&rules).unwrap());
        let png = |rgb: [u8; 3]| {
            let mut out = std::io::Cursor::new(Vec::new());
            image::RgbaImage::from_pixel(4, 4, image::Rgba([rgb[0], rgb[1], rgb[2], 255])).write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        std::fs::write(root.join("shot_0001.png"), png([0, 200, 0])).unwrap();
        std::fs::write(root.join("shot_0002.png"), png([0, 0, 200])).unwrap();
        let secret = dir.path().join("secret.png");
        std::fs::write(&secret, png([200, 0, 0])).unwrap();
        let importer = ContainedImporter { guard: guard.clone() };
        let source = ContainedFootage { pool: effectcraft_media::MediaPool::new(), guard: guard.clone() };
        // Inside the folder: a two-frame sequence that decodes.
        let sequence = importer.probe(&root.join("shot_0001.png").to_string_lossy()).unwrap();
        assert_eq!(sequence.sequence.len(), 2, "{sequence:?}");
        assert!(source.frame(ItemId(1), &sequence, Tick::ZERO).is_some(), "a sequence inside the folder decodes");
        // A project listing a frame outside: nothing of it decodes.
        let mut listed = sequence.clone();
        listed.sequence[1] = secret.to_string_lossy().into();
        for t in [Tick::ZERO, Tick::from_seconds_f64(1.0 / 30.0)] {
            assert!(source.frame(ItemId(2), &listed, t).is_none(), "a frame outside is never decoded");
        }
        #[cfg(unix)]
        {
            // A sibling that is a link out of the folder: refused on import.
            std::fs::remove_file(root.join("shot_0002.png")).unwrap();
            std::os::unix::fs::symlink(&secret, root.join("shot_0002.png")).unwrap();
            let refused = importer.probe(&root.join("shot_0001.png").to_string_lossy()).unwrap_err();
            assert!(refused.contains("outside"), "{refused}");
        }
        // Models: refused on import, and never loaded.
        let gltf = root.join("scene.gltf");
        std::fs::write(&gltf, br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"../secret.png","byteLength":4}]}"#).unwrap();
        assert!(importer.probe(&gltf.to_string_lossy()).unwrap_err().contains("3D models"));
        let model = Footage { path: gltf.to_string_lossy().into(), kind: effectcraft_engine::project::FootageKind::Model, ..Default::default() };
        assert!(source.model(ItemId(3), &model).is_none());
    }

    /// The engine's writes never climb out of the area through a folder the
    /// write itself would create: `missing/../../x` resolves inside until
    /// `missing` exists, so the Guard refuses any `..` in a write path.
    #[test]
    fn the_guard_never_writes_through_a_parent_step() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let guard = Guard::new(&Arc::new(Area::new(&root, None, true))).unwrap();
        let climb = root.join("missing/../../escape.txt");
        assert!(guard.write_file(&climb.to_string_lossy(), b"x").is_err());
        assert!(!dir.path().join("escape.txt").exists(), "nothing lands beside the area");
        guard.write_file(&root.join("made/ok.txt").to_string_lossy(), b"x").unwrap();
        assert_eq!(std::fs::read(root.join("made/ok.txt")).unwrap(), b"x");
    }

    /// A 2x2x2 `.cube` LUT that turns every colour pure green.
    const GREEN_LUT: &str = "LUT_3D_SIZE 2\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n";
    /// The same, pure blue.
    const BLUE_LUT: &str = "LUT_3D_SIZE 2\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n";

    /// Effect parameters that the engine reads as a file by their own path,
    /// with `std::fs` past every gate, refuse the project. The hostile
    /// fixture is a real project whose Apply Color LUT names a `.cube`
    /// outside the caller's folder: the engine's own session reads it (its
    /// render turns green), the service refuses it for every method that
    /// opens it. The LUT's text inline is drawn; a command door call that
    /// sets a path is refused before a later command could draw it, and so
    /// is a project that holds an expression that could make one; so are
    /// OCIO configs and mocha shape files.
    #[test]
    fn effect_parameters_that_name_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let secret = dir.path().join("secret.cube");
        std::fs::write(&secret, GREEN_LUT).unwrap();
        let areas = resolver(&root, None);
        let solid = || vec![
            json!({"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
            json!({"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}}),
        ];
        let with = |extra: Vec<Json>| solid().into_iter().chain(extra).collect::<Vec<Json>>();
        let lut = |value: &str| vec![
            json!({"id": "effect.apply", "params": {"effect": "ec.utility.applylut"}}),
            json!({"id": "prop.set", "params": {"path": "effects/#1/lut", "value": value}}),
        ];
        run_in(&areas, json!({"cmds": solid(), "out": "plain.ecproj"}), &root, true).unwrap();
        run_in(&areas, json!({"cmds": with(lut(GREEN_LUT)), "out": "inline.ecproj"}), &root, true).unwrap();
        // The LUT inline is drawn through the service.
        serve(&areas, &service_call("render", json!({"path": "inline.ecproj", "out": "inline.png", "max_side": 16}), &root, false)).unwrap();
        // The same project, its LUT naming the file outside instead.
        let text = std::fs::read_to_string(root.join("inline.ecproj")).unwrap();
        let quoted = |v: &str| serde_json::to_string(v).unwrap();
        let hostile = text.replace(&quoted(GREEN_LUT), &quoted(&secret.to_string_lossy()));
        assert_ne!(hostile, text, "the fixture names the outside file");
        std::fs::write(root.join("hostile.ecproj"), hostile).unwrap();
        // The engine's own session reads it: its render differs from the
        // plain solid's.
        let engine_render = |name: &str| {
            let mut b = Backend::headless(Session::default());
            b.exec("file.open", json!({"path": root.join(name).to_string_lossy()})).unwrap();
            b.render_with(None, Some(0.0), 16, false).unwrap().png
        };
        assert_ne!(engine_render("hostile.ecproj"), engine_render("plain.ecproj"), "the fixture is live: the engine reads the outside LUT");
        for (method, args) in [
            ("info", json!({"path": "hostile.ecproj"})),
            ("render", json!({"path": "hostile.ecproj", "out": "hostile.png", "max_side": 16})),
            ("export_lottie", json!({"path": "hostile.ecproj", "comp": "Main", "out": "hostile.json"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
            assert!(refused.contains("names a file") && refused.contains("`lut`"), "{method}: {refused}");
        }
        assert!(!root.join("hostile.png").exists() && !root.join("hostile.json").exists());
        // The command door refuses a path as it is set, and never sets an
        // expression (the command is classed code); a project that holds
        // one on a file parameter is refused when it opens.
        let refused = run_in(&areas, json!({"cmds": with(lut(&secret.to_string_lossy()))}), &root, true).unwrap_err();
        assert!(refused.contains("names a file"), "{refused}");
        let expression = with(vec![
            json!({"id": "effect.apply", "params": {"effect": "ec.utility.applylut"}}),
            json!({"id": "prop.setExpression", "params": {"path": "effects/#1/lut", "expression": "'/etc/x.cube'"}}),
        ]);
        let refused = run_in(&areas, json!({"cmds": expression}), &root, true).unwrap_err();
        assert!(refused.contains("`prop.setExpression` is classed code"), "{refused}");
        let mut b = Backend::headless(Session::default());
        b.exec("file.open", json!({"path": root.join("inline.ecproj").to_string_lossy()})).unwrap();
        b.exec("prop.setExpression", json!({"comp": "Main", "layer": "Red", "path": "effects/#1/lut", "expression": "'/etc/x.cube'"})).unwrap();
        b.exec("file.saveAs", json!({"path": root.join("expression.ecproj").to_string_lossy()})).unwrap();
        let refused = run_in(&areas, json!({"path": "expression.ecproj", "cmds": []}), &root, true).unwrap_err();
        assert!(refused.contains("has an expression"), "{refused}");
        for (effect, key, value) in [
            ("ec.color.ociocolorspace", "configFile", "studio.ocio"),
            ("ec.color.ociofile", "file", "/etc/look.cube"),
            ("ec.obsolete.mochashape", "shapeData", "shapes.json"),
            ("ec.color.lumetri", "basicCorrection/inputLutFile", "/etc/in.cube"),
        ] {
            let cmds = with(vec![
                json!({"id": "effect.apply", "params": {"effect": effect}}),
                json!({"id": "prop.set", "params": {"path": format!("effects/#1/{key}"), "value": value}}),
            ]);
            let refused = run_in(&areas, json!({"cmds": cmds}), &root, true).unwrap_err();
            assert!(refused.contains("names a file") && refused.contains(key), "{effect}: {refused}");
        }
    }

    /// The commands that reach the file system outside the session's gated
    /// services (the media browser, watch folders, Collect Files, logging)
    /// are classed `file` or `host`, so the door refuses them.
    #[test]
    fn run_refuses_ambient_file_commands() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["mediaBrowser.list", "mediaBrowser.go", "mediaBrowser.fileInfo", "file.watchFolder", "file.watchFolder.poll", "file.collectFiles", "help.enableLogging"] {
            let e = dispatch("run", &json!({"cmds": [{"id": id, "params": {"path": "/"}}]}), dir.path()).unwrap_err();
            assert!(e.contains("not reviewed to run through it") || e.contains("is classed host"), "{id}: {e}");
        }
    }

    #[test]
    fn run_builds_a_project_with_engine_commands() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let ran = fixture(host);
        let results = ran["results"].as_array().unwrap();
        assert_eq!(results.len(), 2, "{ran}");
        assert_eq!(results[0]["id"], json!("comp.new"));
        assert_eq!(ran["out"], json!("main.ecproj"));
        assert!(host.join("effect/main.ecproj").metadata().unwrap().len() > 0);
    }

    #[test]
    fn info_reads_the_composition_back() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        let sum = dispatch("info", &json!({"path": "main.ecproj"}), host).unwrap();
        assert_eq!(sum["path"], json!("main.ecproj"), "{sum}");
        let items = sum["items"].as_array().unwrap();
        let comp = items.iter().find(|i| i["name"] == json!("Main")).unwrap();
        assert_eq!(comp["size"], json!([64, 36]), "{comp}");
        assert_eq!(comp["frameRate"], json!(24.0));
        assert_eq!(comp["layers"], json!(1));
    }

    #[test]
    fn render_writes_the_comps_frame_as_png() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        let r = dispatch("render", &json!({"path": "main.ecproj", "out": "frames/first.png", "time": 0.0, "max_side": 32}), host)
            .unwrap();
        assert_eq!(r["out"], json!("frames/first.png"), "{r}");
        assert!(r["bytes"].as_u64().unwrap() > 0);
        assert_eq!((r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap()), (32, 18));
        let png = std::fs::read(host.join("effect/frames/first.png")).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (32, 18));
        let p = img.get_pixel(16, 9);
        let want = [0xcc_i32, 0x33, 0x44];
        for (got, want) in p.0.iter().take(3).zip(want) {
            assert!((*got as i32 - want).abs() <= 4, "the solid's colour comes back ({p:?})");
        }
    }

    /// `effect.commands` lists the catalog entries of exactly the commands
    /// the door runs.
    #[test]
    fn commands_lists_what_the_door_runs() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let cat = dispatch("commands", &json!({}), host).unwrap();
        let list = cat.as_array().unwrap();
        assert_eq!(list.len(), door().unwrap().runnable().len(), "every command the door runs, and only those");
        assert!(list.len() > 300, "a real catalog, not a stub ({})", list.len());
        let has = |list: &[Json], id: &str| list.iter().any(|c| c["id"] == json!(id));
        assert!(has(list, "comp.new") && has(list, "effect.apply") && has(list, "effect.plugins.list"));
        for refused in ["engine.batch", "file.exportLottie", "file.saveAs", "prefs.set", "effect.plugins.load", "playback.toggle", "help.website"] {
            assert!(!has(list, refused), "{refused}");
        }
        let filtered = dispatch("commands", &json!({"filter": "effect.p"}), host).unwrap();
        let filtered = filtered.as_array().unwrap();
        assert!(has(filtered, "effect.plugins.list") && has(filtered, "effect.pickColor") && !has(filtered, "effect.plugins.load"), "{filtered:?}");
    }

    #[test]
    fn export_lottie_writes_the_comp_as_lottie_json() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        let r = dispatch("export_lottie", &json!({"path": "main.ecproj", "comp": "Main", "out": "main.json"}), host).unwrap();
        assert_eq!(r["out"], json!("main.json"), "{r}");
        assert!(r["bytes"].as_u64().unwrap() > 0);
        assert!(r["warnings"].is_array());
        let lottie: Json = serde_json::from_slice(&std::fs::read(host.join("effect/main.json")).unwrap()).unwrap();
        assert!(lottie["layers"].is_array(), "{lottie}");
    }

    #[test]
    fn import_lottie_opens_it_as_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        dispatch("export_lottie", &json!({"path": "main.ecproj", "comp": "Main", "out": "main.json"}), host).unwrap();
        let r = dispatch("import_lottie", &json!({"path": "main.json", "out": "roundtrip.ecproj"}), host).unwrap();
        assert!(r["comp"].is_number(), "{r}");
        assert!(host.join("effect/roundtrip.ecproj").metadata().unwrap().len() > 0);
        let sum = dispatch("info", &json!({"path": "roundtrip.ecproj"}), host).unwrap();
        let items = sum["items"].as_array().unwrap();
        assert!(items.iter().any(|i| i["size"] == json!([64, 36])), "the comp survives the roundtrip: {sum}");
    }

    #[test]
    fn paths_stay_inside_the_effect_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for bad in ["../up.ecproj", "/etc/x.ecproj", "a/../../up.ecproj", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("render", &json!({"path": "main.ecproj", "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("import_lottie", &json!({"path": bad, "out": "a.ecproj"}), host).is_err(), "{bad}");
        }
        // Commands that name a path of their own never run through the
        // door (and the session's guarded file services would refuse one
        // outside the area anyway).
        let escape = dispatch(
            "run",
            &json!({"cmds": [
                {"id": "comp.new", "params": {"name": "C"}},
                {"id": "file.saveAs", "params": {"path": "/tmp/effect-escape.ecproj"}},
            ]}),
            host,
        );
        assert!(escape.is_err(), "{escape:?}");
        assert!(!Path::new("/tmp/effect-escape.ecproj").exists());
        let read = dispatch("run", &json!({"cmds": [{"id": "file.open", "params": {"path": "/etc/hosts"}}]}), host);
        assert!(read.is_err(), "{read:?}");
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.photos"));
        assert!(may_call("os.app-studio"));
        assert!(!may_call("org.example.app"));
        assert!(!may_call(""));
    }

    /// The door runs allowlisted commands in a temporary area with the
    /// shell's resolver installed, as an agent's call: a comp, a solid and a
    /// built-in effect make a new project inside the area, which
    /// `effect.info` and a query read back.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let made = run_in(
            &areas,
            json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
                {"id": "effect.apply", "params": {"effect": "Gaussian Blur"}},
                {"id": "prop.set", "params": {"path": "effects/#1/blurriness", "value": 12.0}},
            ], "out": "blurred.ecproj"}),
            &root,
            false,
        )
        .unwrap();
        assert_eq!(made["out"], json!("blurred.ecproj"), "{made}");
        assert_eq!(made["format"], json!("ecproj"));
        assert!(made["bytes"].as_u64().is_some_and(|n| n > 0), "{made}");
        assert_eq!(made["results"].as_array().unwrap().len(), 4);
        assert_eq!(made["results"][2]["result"]["effect"], json!("ec.blur.gaussian"));
        assert!(root.join("blurred.ecproj").is_file() && !dir.path().join("blurred.ecproj").exists());
        let sum = serve(&areas, &service_call("info", json!({"path": "blurred.ecproj"}), &root, false)).unwrap();
        let comp = sum["items"].as_array().unwrap().iter().find(|i| i["name"] == json!("Main")).cloned().unwrap();
        assert_eq!((comp["size"].clone(), comp["layers"].clone()), (json!([64, 36]), json!(1)), "{sum}");
        // A query writes nothing, and reads the effect back.
        let query = run_in(
            &areas,
            json!({"path": "blurred.ecproj", "cmds": [{"id": "prop.get", "params": {"comp": "Main", "layer": "Red", "path": "effects/#1/blurriness"}}]}),
            &root,
            false,
        )
        .unwrap();
        assert!(query["out"].is_null() && query["format"].is_null(), "{query}");
        assert_eq!(query["results"][0]["result"]["value"], json!(12.0), "{query}");
    }

    /// Every class but `safe` is refused, and so is an id the
    /// classification does not know, before any command runs: a refused id
    /// anywhere in the list writes nothing. Checked over the whole
    /// classification, then through the service for each class.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let door = door().unwrap();
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let area = Area::new(&root, None, false);
        let mut refused_classes = std::collections::BTreeSet::new();
        for (id, class) in safety["commands"].as_object().unwrap() {
            let admitted = door.admit(id, &json!({}), &area);
            match class.as_str().unwrap() {
                "safe" => assert!(door.runs(id) && admitted.is_ok(), "{id}: {admitted:?}"),
                other => {
                    assert!(!door.runs(id) && admitted.is_err(), "{id} is classed {other}");
                    refused_classes.insert(other.to_string());
                }
            }
        }
        assert_eq!(refused_classes.into_iter().collect::<Vec<_>>(), ["code", "device", "file", "host", "network"]);
        let start = json!({"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16}});
        for (id, params, class) in [
            ("engine.batch", json!({"steps": []}), "code"),
            ("file.runScript", json!({"path": "evil.jsx"}), "code"),
            ("script.run", json!({"code": "1"}), "code"),
            ("scriptui.click", json!({}), "code"),
            ("file.executeFile", json!({"path": "/bin/sh"}), "code"),
            ("help.website", json!({}), "network"),
            ("roto.model.download", json!({}), "network"),
            ("playback.toggle", json!({}), "device"),
            ("prefs.set", json!({"key": "pluginsFolder", "value": "/tmp/evil"}), "host"),
            ("view.zoomIn", json!({}), "host"),
            ("edit.purge", json!({}), "host"),
            ("roto.model.select", json!({"id": "x"}), "host"),
            ("mediaBrowser.addFavorite", json!({}), "host"),
        ] {
            let e = run_in(&areas, json!({"cmds": [start, {"id": id, "params": params}], "out": "x.ecproj"}), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed {class}")) && e.contains("never runs it"), "{id}: {e}");
        }
        for (id, params) in [
            ("file.saveAs", json!({"path": "elsewhere.ecproj"})),
            ("file.open", json!({"path": "/etc/hosts"})),
            ("file.import", json!({"path": "/etc/hosts"})),
            ("mediaBrowser.list", json!({"path": "/"})),
            ("renderQueue.render", json!({})),
            ("comp.saveFrameAs", json!({"path": "f.png"})),
            ("templates.create", json!({"id": "lower-third"})),
            ("roto.model.install", json!({"path": "/etc/hosts"})),
        ] {
            let e = run_in(&areas, json!({"cmds": [start, {"id": id, "params": params}], "out": "x.ecproj"}), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        let e = run_in(&areas, json!({"cmds": [start, {"id": "effect.secret"}], "out": "x.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("`effect.secret` is not a reviewed effect command"), "{e}");
        assert!(run_in(&areas, json!({"cmds": [start, {"id": ""}]}), &root, false).unwrap_err().contains("has an `id`"));
        assert!(run_in(&areas, json!({"cmds": [{"id": "comp.new", "params": [1]}]}), &root, false).unwrap_err().contains("`params` is an object"));
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "time.start"})).collect();
        assert!(run_in(&areas, json!({"cmds": too_many}), &root, false).unwrap_err().contains("at most 64"));
        assert!(!root.join("x.ecproj").exists(), "nothing written");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "nothing at all");
    }

    /// Composite commands run other commands that no check on their own id
    /// sees: a batch is refused even when it wraps only an allowed command,
    /// and when it wraps a plug-in install; so are scripts and the tutorial
    /// step that runs its own command list.
    #[test]
    fn the_door_refuses_composites() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        for c in [
            json!({"id": "engine.batch", "params": {"steps": [{"command": "comp.new", "params": {"name": "C"}}]}}),
            json!({"id": "engine.batch", "params": {"steps": [{"command": "effect.plugins.load", "params": {"path": "evil.wasm"}}]}}),
            json!({"id": "file.runScript", "params": {"path": "evil.jsx"}}),
            json!({"id": "learn.step", "params": {"action": "showMe"}}),
        ] {
            let id = c["id"].as_str().unwrap().to_string();
            let cmds = json!([{"id": "learn.start", "params": {"id": "nope"}}, {"id": "comp.new", "params": {"name": "C"}}, c]);
            let e = run_in(&areas, json!({"cmds": cmds, "out": "c.ecproj"}), dir.path(), false).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed code")), "{id}: {e}");
            assert!(!e.contains("unknown tutorial"), "refused before the first command ran: {e}");
        }
        assert!(!dir.path().join("c.ecproj").exists());
    }

    /// Plug-in loading changes a process-wide registry and runs
    /// WebAssembly: `effect.plugins.load` is classed code and the other
    /// would-be mutators are ids the classification does not know, so the
    /// door refuses them all; listing the registry is `safe` and runs.
    #[test]
    fn the_door_refuses_plugin_mutators_and_lists_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let e = run_in(&areas, json!({"cmds": [{"id": "effect.plugins.load", "params": {"path": "x.wasm"}}]}), dir.path(), false).unwrap_err();
        assert!(e.contains("`effect.plugins.load` is classed code"), "{e}");
        for id in ["effect.plugins.install", "effect.plugins.reload", "effect.plugins.remove", "effect.plugins.unload"] {
            let e = run_in(&areas, json!({"cmds": [{"id": id, "params": {"path": "x.wasm"}}]}), dir.path(), false).unwrap_err();
            assert!(e.contains(&format!("`{id}` is not a reviewed effect command")), "{id}: {e}");
        }
        let listed = run_in(&areas, json!({"cmds": [{"id": "effect.plugins.list"}]}), dir.path(), false).unwrap();
        let r = &listed["results"][0]["result"];
        assert!(r["plugins"].is_array() && r["wasm"] == json!(false), "no plug-in loader in the service's session: {listed}");
        assert!(listed["out"].is_null());
    }

    /// A plug-in for the tests: it shadows a built-in in the engine's
    /// `lookup` and never draws.
    struct Shadow(effectcraft_engine::effects::plugin::PluginManifest);

    impl effectcraft_engine::effects::plugin::EffectPlugin for Shadow {
        fn manifest(&self) -> &effectcraft_engine::effects::plugin::PluginManifest {
            &self.0
        }

        fn render(
            &self,
            _: &mut effectcraft_engine::effects::plugin::PluginFrame,
            _: &effectcraft_engine::effects::plugin::PluginParams,
            _: f64,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    /// Register, once per test process, two plug-ins that shadow built-ins
    /// in the engine's `lookup`: one under Mosaic's display name in a
    /// category sorted before Stylize, one whose id is Emboss's display
    /// name. No other test names Mosaic or Emboss.
    fn shadow_builtins() {
        use effectcraft_engine::effects::plugin::{register_plugin, PLUGIN_API_VERSION};
        static DONE: OnceLock<()> = OnceLock::new();
        DONE.get_or_init(|| {
            for (id, name) in [("octosense.test.mosaic", "Mosaic"), ("Emboss", "OctoSense Test Emboss")] {
                let manifest = serde_json::from_value(json!({"api": PLUGIN_API_VERSION, "id": id, "name": name, "category": "AAA OctoSense Test"})).unwrap();
                register_plugin(Arc::new(Shadow(manifest))).unwrap();
            }
        });
    }

    /// `effect.apply` runs only an effect the engine builds in, by id,
    /// display name (any case) or alias; a name that is no built-in, a
    /// plug-in's id, or a built-in's name a registered plug-in shadows in
    /// the engine's own lookup is refused before any command runs. A project
    /// whose effect instance names a plug-in is refused by the fence.
    #[test]
    fn the_door_applies_only_built_in_effects() {
        use effectcraft_engine::effects::lookup;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        for name in ["Gaussian Blur", "gaussian BLUR", "ec.blur.gaussian", "Apply Color LUT", "Keylight (1.2)", "mocha shape"] {
            assert!(builtin_effect(name), "{name}");
        }
        for name in ["", "Totally Not An Effect", "org.example.evil", "plugin.evil", "ec.blur"] {
            assert!(!builtin_effect(name), "{name}");
        }
        let apply = |effect: Json| {
            json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
                {"id": "effect.apply", "params": {"effect": effect}},
            ], "out": "fx.ecproj"})
        };
        for name in ["Totally Not An Effect", "plugin.evil", ""] {
            let e = run_in(&areas, apply(json!(name)), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{name}` is not an effect the engine builds in")), "{name}: {e}");
        }
        assert!(run_in(&areas, apply(json!(7)), &root, false).unwrap_err().contains("is not an effect the engine builds in"));
        assert!(run_in(&areas, apply(json!(["Gaussian Blur", "plugin.evil"])), &root, false).unwrap_err().contains("`plugin.evil`"));
        assert!(!root.join("fx.ecproj").exists());
        // Registered plug-ins that shadow built-ins: the engine's lookup
        // lands on them, so the door refuses those names.
        shadow_builtins();
        assert_eq!(lookup("Mosaic").map(|s| s.id), Some("octosense.test.mosaic"), "the shadow is live");
        assert_eq!(lookup("Emboss").map(|s| s.id), Some("Emboss"), "the shadow is live");
        for name in ["Mosaic", "MOSAIC", "Emboss", "octosense.test.mosaic", "OctoSense Test Emboss"] {
            assert!(!builtin_effect(name), "{name}");
            let e = run_in(&areas, apply(json!(name)), &root, false).unwrap_err();
            assert!(e.contains("is not an effect the engine builds in"), "{name}: {e}");
        }
        assert!(!root.join("fx.ecproj").exists());
        // The built-ins themselves, by id, still apply.
        let mut cmds = apply(json!("ec.stylize.mosaic"));
        cmds["cmds"].as_array_mut().unwrap().push(json!({"id": "effect.apply", "params": {"effect": "ec.stylize.emboss"}}));
        cmds["cmds"].as_array_mut().unwrap().push(json!({"id": "effect.plugins.list"}));
        let made = run_in(&areas, cmds, &root, false).unwrap();
        assert_eq!((made["results"][2]["result"]["effect"].clone(), made["results"][3]["result"]["effect"].clone()), (json!("ec.stylize.mosaic"), json!("ec.stylize.emboss")));
        let ids: Vec<&str> = made["results"][4]["result"]["plugins"].as_array().unwrap().iter().filter_map(|p| p["id"].as_str()).collect();
        assert!(ids.contains(&"octosense.test.mosaic") && ids.contains(&"Emboss"), "{ids:?}");
        // A project whose effect instance names the plug-in: refused by every
        // method that opens it.
        let text = std::fs::read_to_string(root.join("fx.ecproj")).unwrap();
        let hostile = text.replace("\"ec.stylize.mosaic\"", "\"octosense.test.mosaic\"");
        assert_ne!(hostile, text);
        std::fs::write(root.join("plugin.ecproj"), hostile).unwrap();
        for (method, args) in [
            ("info", json!({"path": "plugin.ecproj"})),
            ("run", json!({"path": "plugin.ecproj", "cmds": [], "out": "plugin.png"})),
            ("render", json!({"path": "plugin.ecproj", "out": "plugin2.png"})),
        ] {
            let e = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
            assert!(e.contains("`octosense.test.mosaic`") && e.contains("is an effect plug-in"), "{method}: {e}");
        }
        assert!(!root.join("plugin.png").exists() && !root.join("plugin2.png").exists());
    }

    /// #419's LUT fence stays closed through the door, as the shell calls
    /// it: Apply Color LUT's `lut` set to a `.cube` outside the area is
    /// refused right after the command that sets it, for every kind of
    /// `out`, with nothing written; the LUT's text inline is drawn. The same
    /// holds when the path arrives through Essential Graphics: an OCIO File
    /// Transform's `file` exposed as a control and given the path as a
    /// precomp layer's value (the renderer puts it into the parameter), in
    /// one precomp or through two. Each hostile fixture is live: the engine's
    /// own session reads the outside file.
    #[test]
    fn the_door_keeps_the_lut_fence_closed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let secret = dir.path().join("secret.cube");
        std::fs::write(&secret, GREEN_LUT).unwrap();
        let secret = secret.to_string_lossy().into_owned();
        let areas = resolver(&root, None);
        let lut = |value: &str| {
            json!([
                {"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
                {"id": "effect.apply", "params": {"effect": "Apply Color LUT"}},
                {"id": "prop.set", "params": {"path": "effects/#1/lut", "value": value}},
                {"id": "effect.pickColor", "params": {"param": "missing", "x": 1, "y": 1}},
            ])
        };
        for out in ["lut.ecproj", "lut.png", "lut.json"] {
            let e = run_in(&areas, json!({"cmds": lut(&secret), "out": out}), &root, false).unwrap_err();
            assert!(e.contains("names a file") && e.contains("`lut`") && !e.contains("pickColor"), "{out}: {e}");
            assert!(!root.join(out).exists(), "{out}");
        }
        let mut inline = lut(GREEN_LUT);
        inline.as_array_mut().unwrap().pop();
        let drawn = run_in(&areas, json!({"cmds": inline, "out": "inline.png", "max_side": 16}), &root, false).unwrap();
        assert_eq!(drawn["format"], json!("png"), "{drawn}");
        assert_eq!(centre(&std::fs::read(root.join("inline.png")).unwrap())[..3], [0, 255, 0], "the inline LUT is drawn");

        // Through Essential Graphics: Inner's OCIO `file` is a control, and
        // Main's layer of Inner gives it a value.
        let exposed = |value: &str| {
            vec![
                json!({"id": "comp.new", "params": {"name": "Inner", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
                json!({"id": "layer.newSolid", "params": {"name": "Lit", "color": "#cc3344"}}),
                json!({"id": "effect.apply", "params": {"effect": "OCIO File Transform"}}),
                json!({"id": "essential.addProperty", "params": {"comp": "Inner", "layer": "Lit", "path": "effects/#1/file"}}),
                json!({"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
                json!({"id": "layer.addItem", "params": {"comp": "Main", "item": "Inner"}}),
                json!({"id": "essential.set", "params": {"comp": "Main", "layer": "Inner", "control": "File", "value": value}}),
            ]
        };
        // Then Top's layer of Main gives Main's control of that value
        // another one (without one, Main's own value shows).
        let nested = |control: u64, value: Option<&str>| {
            let mut cmds = vec![
                json!({"id": "essential.addProperty", "params": {"comp": "Main", "layer": "Inner", "path": format!("essential/eg{control}"), "name": "Look"}}),
                json!({"id": "comp.new", "params": {"name": "Top", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
                json!({"id": "layer.addItem", "params": {"comp": "Top", "item": "Main"}}),
            ];
            if let Some(value) = value {
                cmds.push(json!({"id": "essential.set", "params": {"comp": "Top", "layer": "Main", "control": "Look", "value": value}}));
            }
            cmds
        };
        // The engine's own session, unfenced, reads the outside file both
        // ways.
        let engine = |cmds: Vec<Json>, comp: &str| {
            let mut b = Backend::headless(Session::default());
            let mut control = 0;
            for c in cmds {
                let r = b.exec(c["id"].as_str().unwrap(), c["params"].clone()).unwrap();
                control = r["controls"][0].as_u64().unwrap_or(control);
            }
            (centre(&b.render_with(Some(&json!(comp)), Some(0.0), 16, false).unwrap().png), control)
        };
        assert_eq!(engine(exposed(""), "Main").0[..3], [0xcc, 0x33, 0x44], "no value: the solid as it is");
        assert_eq!(engine(exposed(&secret), "Main").0[..3], [0, 255, 0], "the fixture is live: the engine reads the outside LUT");
        let (blue, control) = engine(exposed(BLUE_LUT), "Main");
        assert_eq!(blue[..3], [0, 0, 255], "a value inline is drawn");
        let chain = |value: Option<&str>| exposed(BLUE_LUT).into_iter().chain(nested(control, value)).collect::<Vec<Json>>();
        assert_eq!(engine(chain(None), "Top").0[..3], [0, 0, 255], "Main's own value");
        assert_eq!(engine(chain(Some(&secret)), "Top").0[..3], [0, 255, 0], "the nested fixture is live too");
        // The door refuses both, right after the value is set.
        let e = run_in(&areas, json!({"cmds": exposed(&secret), "out": "exposed.png", "max_side": 16}), &root, false).unwrap_err();
        assert!(e.contains("Essential Property `File`") && e.contains("names a file") && e.contains("`file`"), "{e}");
        let first = run_in(&areas, json!({"cmds": exposed(BLUE_LUT), "out": "chain.ecproj"}), &root, false).unwrap();
        assert_eq!(first["results"][3]["result"]["controls"][0].as_u64(), Some(control), "{first}");
        let e = run_in(&areas, json!({"path": "chain.ecproj", "cmds": nested(control, Some(&secret)), "out": "chain.png", "max_side": 16}), &root, false).unwrap_err();
        assert!(e.contains("Essential Property `Look`") && e.contains("names a file") && e.contains("`file`"), "{e}");
        assert!(!root.join("exposed.png").exists() && !root.join("chain.png").exists());
        // The file's text inline, as a value, is drawn.
        run_in(&areas, json!({"path": "chain.ecproj", "cmds": nested(control, Some(GREEN_LUT)), "out": "chain-inline.png", "comp": "Top", "max_side": 16}), &root, false).unwrap();
        assert_eq!(centre(&std::fs::read(root.join("chain-inline.png")).unwrap())[..3], [0, 255, 0]);
        // A project file carrying such a value is refused when opened.
        let text = std::fs::read_to_string(root.join("chain.ecproj")).unwrap();
        let quoted = |v: &str| serde_json::to_string(v).unwrap();
        let hostile = text.replace(&quoted(BLUE_LUT), &quoted(&secret));
        assert_ne!(hostile, text);
        std::fs::write(root.join("hostile.ecproj"), hostile).unwrap();
        let e = serve(&areas, &service_call("info", json!({"path": "hostile.ecproj"}), &root, false)).unwrap_err();
        assert!(e.contains("Essential Property `File`") && e.contains("names a file"), "{e}");
    }

    /// `run` writes each kind of `out` by its extension: the project
    /// (.ecproj), a composition as Lottie (.json, .lottie) with its
    /// warnings, one frame (.png) with its size; a Lottie `path` opens as a
    /// composition the commands then edit. Another extension is refused
    /// before the engine works.
    #[test]
    fn the_door_writes_each_kind_of_out() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let project = fixture_in(&areas, &root);
        assert_eq!((project["format"].clone(), project["out"].clone()), (json!("ecproj"), json!("main.ecproj")), "{project}");
        assert!(project["bytes"].as_u64().is_some_and(|n| n > 0), "{project}");
        let lottie = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "main.json", "comp": "Main"}), &root, false).unwrap();
        assert_eq!(lottie["format"], json!("json"), "{lottie}");
        assert!(lottie["warnings"].is_array() && lottie["bytes"].as_u64().is_some_and(|n| n > 0), "{lottie}");
        let parsed: Json = serde_json::from_slice(&std::fs::read(root.join("main.json")).unwrap()).unwrap();
        assert!(parsed["layers"].is_array(), "{parsed}");
        let dot = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "main.lottie"}), &root, false).unwrap();
        assert_eq!(dot["format"], json!("lottie"), "{dot}");
        assert!(std::fs::read(root.join("main.lottie")).unwrap().starts_with(b"PK"), "a dotLottie archive");
        let frame = run_in(
            &areas,
            json!({"path": "main.ecproj", "cmds": [{"id": "layer.newSolid", "params": {"name": "Blue", "color": "#2244cc"}}], "out": "frame.png", "time": 0.0, "max_side": 32}),
            &root,
            false,
        )
        .unwrap();
        assert_eq!((frame["format"].clone(), frame["width"].clone(), frame["height"].clone()), (json!("png"), json!(32), json!(18)), "{frame}");
        let png = std::fs::read(root.join("frame.png")).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert_eq!(centre(&png)[..3], [0x22, 0x44, 0xcc], "the command ran before the frame was drawn");
        let back = run_in(
            &areas,
            json!({"path": "main.json", "cmds": [{"id": "layer.newSolid", "params": {"name": "Blue", "color": "#2244cc"}}], "out": "from-lottie.ecproj"}),
            &root,
            false,
        )
        .unwrap();
        assert!(back["imported"]["comp"].is_number() && back["imported"]["warnings"].is_array(), "{back}");
        let sum = serve(&areas, &service_call("info", json!({"path": "from-lottie.ecproj"}), &root, false)).unwrap();
        let comp = sum["items"].as_array().unwrap().iter().find(|i| i["type"] == json!("Composition") && i["size"] == json!([64, 36])).cloned();
        assert_eq!(comp.map(|c| c["layers"].clone()), Some(json!(2)), "the Lottie comp, with the new solid: {sum}");
        let e = run_in(&areas, json!({"cmds": [{"id": "comp.new"}], "out": "movie.mp4"}), &root, false).unwrap_err();
        assert!(e.contains("`out` is a project (.ecproj)"), "{e}");
        assert!(!root.join("movie.mp4").exists());
    }

    /// Whatever `run` writes keeps the area's rules: an agent's `out` never
    /// replaces a file, for any kind; the result fits the quota; `out` and
    /// `path` stay inside the area; an app's own call may replace.
    #[test]
    fn the_doors_writes_keep_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        fixture_in(&areas, &root);
        for name in ["taken.ecproj", "taken.json", "taken.png"] {
            std::fs::write(root.join(name), b"keep me").unwrap();
            let e = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": name}), &root, false).unwrap_err();
            assert!(e.contains("already exists"), "{name}: {e}");
            assert_eq!(std::fs::read(root.join(name)).unwrap(), b"keep me", "{name}");
        }
        let tight = resolver(&root, Some(8));
        for name in ["q.ecproj", "q.json", "q.png"] {
            let e = run_in(&tight, json!({"path": "main.ecproj", "cmds": [], "out": name}), &root, true).unwrap_err();
            assert!(e.contains("bytes left"), "{name}: {e}");
            assert!(!root.join(name).exists(), "{name}");
        }
        std::fs::write(dir.path().join("up.json"), b"{}").unwrap();
        for bad in ["../up.ecproj", "/etc/x.png", "a/../../up.json"] {
            assert!(run_in(&areas, json!({"cmds": [], "out": bad}), &root, true).is_err(), "out {bad}");
            assert!(run_in(&areas, json!({"cmds": [], "path": bad}), &root, true).is_err(), "path {bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(run_in(&areas, json!({"cmds": [], "path": "up/up.json"}), &root, true).is_err());
            assert!(run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "up/f.png"}), &root, true).is_err());
            assert!(!dir.path().join("f.png").exists());
        }
        run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "taken.png", "max_side": 8}), &root, true).unwrap();
        assert!(std::fs::read(root.join("taken.png")).unwrap().starts_with(&[0x89, b'P', b'N', b'G']), "an app's own call may replace");
    }

    /// The door's gate is built from the generated classification: it runs
    /// exactly the `safe` ids.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let safe = safety["commands"].as_object().unwrap().values().filter(|c| *c == "safe").count();
        assert_eq!(door.runnable().len(), safe);
        assert!(door.runs("comp.new") && door.runs("effect.apply") && door.runs("prop.set") && door.runs("effect.plugins.list") && door.runs("essential.set"));
        for refused in ["engine.batch", "file.saveAs", "prefs.set", "effect.plugins.load", "mediaBrowser.addFavorite", "learn.step", "nope"] {
            assert!(!door.runs(refused), "{refused}");
        }
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "effect", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("effect.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }

    /// Every `effect.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        // The Lottie animation the examples open: one red solid layer.
        std::fs::write(dir.path().join("intro.json"), br##"{"v":"5.7.0","fr":24,"ip":0,"op":24,"w":32,"h":18,"nm":"Main","ddd":0,"assets":[],"layers":[{"ddd":0,"ind":1,"ty":1,"nm":"Red","sr":1,"ks":{"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[16,9,0]},"a":{"a":0,"k":[16,9,0]},"s":{"a":0,"k":[100,100,100]}},"ao":0,"sw":32,"sh":18,"sc":"#cc3344","ip":0,"op":24,"st":0,"bm":0}]}"##).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("effect.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 4, "{ran} examples");
        let info = serve(&areas, &service_call("info", json!({"path": "title.ecproj"}), dir.path(), false)).unwrap();
        assert!(info.to_string().contains("Title"), "{info}");
        let blurred = String::from_utf8_lossy(&std::fs::read(dir.path().join("intro.ecproj")).unwrap()).to_lowercase();
        assert!(blurred.contains("gaussian"), "the blur is in the saved project");
    }

    /// A workspace folder and the shell-shaped resolver of an agent's
    /// calls in it.
    fn workspace() -> (tempfile::TempDir, PathBuf, Slot) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        (dir, root, areas)
    }

    /// Save, from the engine's own session (no door, no fence), the project
    /// that `cmds` build, as `name` in `root`: a hostile fixture written as
    /// a project file the door then opens.
    fn engine_project(root: &Path, name: &str, cmds: &[Json]) {
        let mut b = Backend::headless(Session::default());
        for c in cmds {
            b.exec(c["id"].as_str().unwrap(), c["params"].clone()).unwrap_or_else(|e| panic!("{c}: {e}"));
        }
        b.exec("file.saveAs", json!({"path": root.join(name).to_string_lossy()})).unwrap();
    }

    /// The first object in `v` (depth first) with `key` = `value`, changed by
    /// `f`: how the hostile fixtures edit a saved project.
    fn edit_json(v: &mut Json, key: &str, value: &Json, f: &dyn Fn(&mut serde_json::Map<String, Json>)) -> bool {
        match v {
            Json::Object(map) => {
                if map.get(key) == Some(value) {
                    f(map);
                    return true;
                }
                map.values_mut().any(|c| edit_json(c, key, value, f))
            }
            Json::Array(items) => items.iter_mut().any(|c| edit_json(c, key, value, f)),
            _ => false,
        }
    }

    /// A comp and a solid, as a call's first commands.
    fn comp_and_solid(w: u64, h: u64) -> Vec<Json> {
        vec![
            json!({"id": "comp.new", "params": {"name": "Main", "width": w, "height": h, "frameRate": 24, "duration": 1.0}}),
            json!({"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}}),
        ]
    }

    /// Every limit of the door at its cap and one past it: the gate admits
    /// the first and refuses the second, naming what it counts, before
    /// anything runs. The table covers every limit in `LIMITS`.
    #[test]
    fn the_doors_limits_pass_at_their_cap_and_refuse_one_over() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let door = door().unwrap();
        let px = |w: u64, h: u64| json!({"width": w, "height": h});
        let mut cases: Vec<(&str, &str, Json, Json)> = vec![
            ("comp.new", "pixels", px(4096, 2160), px(4096, 2161)),
            ("comp.new", "frames", json!({"duration": 1200, "frameRate": 30}), json!({"duration": 1200.1, "frameRate": 30})),
            ("comp.new", "frames per second", json!({"fps": 240, "duration": 1}), json!({"fps": 241, "duration": 1})),
            ("comp.new", "motion-blur samples", json!({"motionBlurSamples": 32}), json!({"motionBlurSamples": 33})),
            ("comp.new", "adaptive motion-blur samples", json!({"adaptiveSampleLimit": 128}), json!({"adaptiveSampleLimit": 129})),
            ("comp.settings", "pixels", px(4096, 2160), px(4096, 2161)),
            ("comp.settings", "frames", json!({"duration": 600, "frameRate": 60}), json!({"duration": 600.05, "frameRate": 60})),
            ("comp.settings", "frames per second", json!({"frameRate": 240}), json!({"frameRate": 241})),
            ("comp.settings", "motion-blur samples", json!({"motionBlurSamples": 32}), json!({"motionBlurSamples": 33})),
            ("comp.settings", "adaptive motion-blur samples", json!({"adaptiveSampleLimit": 128}), json!({"adaptiveSampleLimit": 129})),
            ("layer.newSolid", "pixels", px(4096, 2160), px(4096, 2161)),
            ("file.importSolid", "pixels", px(2160, 4096), px(2161, 4096)),
            ("file.replaceWithSolid", "pixels", px(4096, 2160), px(4097, 2160)),
            ("layer.settings", "pixels", px(4096, 2160), px(4096, 2161)),
            ("file.importPlaceholder", "pixels", px(4096, 2160), px(4096, 2161)),
            ("file.importPlaceholder", "frames", json!({"duration": 1200, "frameRate": 30}), json!({"duration": 1201, "frameRate": 30})),
            ("file.importPlaceholder", "frames per second", json!({"frameRate": 240, "duration": 1}), json!({"frameRate": 241, "duration": 1})),
            ("file.replaceWithPlaceholder", "pixels", px(4096, 2160), px(4096, 2161)),
            ("file.replaceWithPlaceholder", "frames", json!({"duration": 1200, "frameRate": 30}), json!({"duration": 1201, "frameRate": 30})),
            ("file.replaceWithPlaceholder", "frames per second", json!({"frameRate": 240}), json!({"frameRate": 241})),
            ("comp.vr.createEnvironment", "cube-map pixels", json!({"size": 1214}), json!({"size": 1215})),
            ("comp.vr.extractCubemap", "cube-map pixels", json!({"faceSize": 1214}), json!({"faceSize": 1215})),
            ("layer.newText", "px of font size", json!({"size": 1296}), json!({"size": 1297})),
            ("layer.setText", "px of font size", json!({"size": 1296}), json!({"size": 1296.5})),
            ("text.addSelector", "expression selectors", json!({"kind": "wiggly"}), json!({"kind": "Expression"})),
            ("layer.addTextSelector", "expression selectors", json!({"kind": "range"}), json!({"kind": "expression"})),
            ("puppet.mesh", "pixels of mesh expansion", json!({"expansion": 200}), json!({"expansion": 201})),
            ("puppet.addPin", "pixels of mesh expansion", json!({"expansion": -200}), json!({"expansion": -201})),
            ("track.setPoint", "pixels of feature region", json!({"featureSize": [512, 40]}), json!({"featureSize": [40, 513]})),
            ("track.setPoint", "pixels of search region", json!({"searchSize": [1024, 80]}), json!({"searchSize": [1025, 80]})),
            ("liquify.stroke", "dabs", json!({"points": [[0, 0], [160000, 0]], "size": 64}), json!({"points": [[0, 0], [160001, 0]], "size": 64})),
            ("mask.interpolate", "keyframes per second", json!({"keyframeRate": 240}), json!({"keyframeRate": 241})),
            ("mask.interpolationOptions", "keyframes per second", json!({"keyframeRate": 240}), json!({"keyframeRate": 241})),
        ];
        for id in [
            "edit.duplicate",
            "effect.apply",
            "effect.applyLast",
            "effect.copy",
            "effect.paste",
            "keys.paste",
            "edit.pasteReversedKeyframes",
            "project.duplicate",
            "camera.createFromSolve",
            "camera.fromModel",
            "light.fromModel",
            "file.interpretFootage",
            "file.interpretProxy",
        ] {
            cases.push((id, "list entries", json!({"layers": vec![1; 1000]}), json!({"layers": vec![1; 1001]})));
        }
        for setter in ["prop.set", "prop.addKey", "keys.set"] {
            cases.push((setter, "copies", json!({"path": "contents/repeater/copies", "value": 1000}), json!({"path": "contents/Repeater 1/Copies", "value": 1001})));
            cases.push((setter, "star points", json!({"path": "contents/star/points", "value": 100}), json!({"path": "contents/#1/points", "value": "101"})));
            cases.push((
                setter,
                "pixels of feature region",
                json!({"path": "motionTrackers/#1/#1/featureSize", "value": [512, 512]}),
                json!({"path": "motionTrackers/#1/#1/featureSize", "value": [513, 4]}),
            ));
            cases.push((
                setter,
                "pixels of search region",
                json!({"path": "motionTrackers/#1/#1/searchSize", "value": [1024, 1024]}),
                json!({"path": "motionTrackers/#1/#1/Search Size", "value": [4, 1025]}),
            ));
        }
        for limit in LIMITS {
            assert!(cases.iter().any(|(id, what, _, _)| *id == limit.id && *what == limit.what), "no case for {} ({})", limit.id, limit.what);
        }
        for (id, what, at, over) in &cases {
            door.admit(id, at, &area).unwrap_or_else(|e| panic!("{id} at its cap ({what}): {e}"));
            let e = door.admit(id, over, &area).unwrap_err();
            let named = if *what == "expression selectors" { "Expression Selector".to_string() } else { format!("{what} the door allows") };
            assert!(e.contains(&format!("`{id}`")) && (e.contains(&named) || e.contains(&format!("{what},"))), "{id} one over ({what}): {e}");
        }
        // A frame rate under one frame a second, a zero or a negative one,
        // is refused wherever a command sets one.
        for (id, p) in [("comp.new", json!({"frameRate": 0.5})), ("comp.settings", json!({"fps": 0})), ("file.importPlaceholder", json!({"frameRate": -30}))] {
            let e = door.admit(id, &p, &area).unwrap_err();
            assert!(e.contains("slower than the 1 fps the door allows"), "{id}: {e}");
        }
        // A numeric string counts by its number; anything else is refused.
        assert!(door.admit("comp.new", &json!({"width": "100000"}), &area).unwrap_err().contains("pixels"));
        assert!(door.admit("comp.new", &json!({"width": [1]}), &area).unwrap_err().contains("`width` is a number the door bounds"));
        // A path the gate cannot read (a uid, an index) is left to the
        // project fence, which `run` applies after the command.
        assert!(door.admit("prop.set", &json!({"path": "contents/#2/#1", "value": 1e9}), &area).is_ok());
        assert!(door.admit("prop.set", &json!({"prop": 55, "value": 1e9}), &area).is_ok());
        // Copies multiply across a call.
        let two = json!([
            {"id": "prop.set", "params": {"path": "contents/repeater/copies", "value": 1000}},
            {"id": "prop.set", "params": {"path": "contents/repeater#2/copies", "value": 11}},
        ]);
        let e = door.admit_all(&two, &area).unwrap_err();
        assert!(e.contains("the copies this call makes multiply to 11000, more than the 10000"), "{e}");
    }

    /// Hostile: a huge comp is refused, however it would arrive: as
    /// `comp.new`'s size (the gate), widened by Composition Settings past
    /// what the gate sees (after the command, before anything draws), or
    /// held by a project or Lottie file (when it opens). Nothing is written.
    #[test]
    fn a_huge_comp_is_refused() {
        let (_dir, root, areas) = workspace();
        let e = run_in(&areas, json!({"cmds": [{"id": "comp.new", "params": {"width": 100000, "height": 100000}}], "out": "huge.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("`comp.new` asks for 10000000000 pixels, more than the 8847360 the door allows in one command"), "{e}");
        let widened = json!([{"id": "comp.new", "params": {"name": "Main"}}, {"id": "comp.settings", "params": {"width": 30000}}]);
        let e = run_in(&areas, json!({"cmds": widened, "out": "wide.png"}), &root, false).unwrap_err();
        assert!(e.contains("the composition `Main` is 30000 × 1080 = 32400000 pixels, more than the 8847360 the door allows"), "{e}");
        // A project file.
        engine_project(&root, "small.ecproj", &comp_and_solid(64, 36));
        let mut project: Json = serde_json::from_slice(&std::fs::read(root.join("small.ecproj")).unwrap()).unwrap();
        assert!(edit_json(&mut project, "width", &json!(64), &|m| {
            m.insert("width".into(), json!(100000));
            m.insert("height".into(), json!(100000));
        }));
        std::fs::write(root.join("huge.ecproj"), serde_json::to_vec(&project).unwrap()).unwrap();
        let e = run_in(&areas, json!({"path": "huge.ecproj", "cmds": [], "out": "huge.png"}), &root, false).unwrap_err();
        assert!(e.contains("is 100000 × 100000 = 10000000000 pixels"), "{e}");
        // A Lottie file.
        std::fs::write(root.join("huge.json"), br##"{"v":"5.7.0","fr":24,"ip":0,"op":24,"w":100000,"h":100000,"nm":"Huge","ddd":0,"assets":[],"layers":[]}"##).unwrap();
        let e = run_in(&areas, json!({"path": "huge.json", "cmds": [], "out": "huge2.png"}), &root, false).unwrap_err();
        assert!(e.contains("is 100000 × 100000"), "{e}");
        for made in ["huge.png", "huge2.png", "wide.png"] {
            assert!(!root.join(made).exists(), "{made}");
        }
    }

    /// Hostile: a huge frame range (duration × frame rate) is refused: as
    /// `comp.new`'s or a placeholder's parameters (the gate), as a duration
    /// Composition Settings stretches at the comp's own rate (after the
    /// command), and as a rate under one frame a second.
    #[test]
    fn a_huge_frame_range_is_refused() {
        let (_dir, root, areas) = workspace();
        let e = run_in(&areas, json!({"cmds": [{"id": "comp.new", "params": {"duration": 1e6, "frameRate": 60}}]}), &root, false).unwrap_err();
        assert!(e.contains("`comp.new` asks for 60000000 frames, more than the 36000 the door allows in one command"), "{e}");
        let e = run_in(&areas, json!({"cmds": [{"id": "file.importPlaceholder", "params": {"duration": 3600}}]}), &root, false).unwrap_err();
        assert!(e.contains("`file.importPlaceholder` asks for 107892 frames"), "{e}");
        let stretched = json!([{"id": "comp.new", "params": {"name": "Main", "frameRate": 30}}, {"id": "comp.settings", "params": {"duration": 3600}}]);
        let e = run_in(&areas, json!({"cmds": stretched, "out": "long.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("the composition `Main` is 108000 frames long (3600 s at 30 fps), more than the 36000 the door allows"), "{e}");
        let e = run_in(&areas, json!({"cmds": [{"id": "comp.new", "params": {"frameRate": 0.01, "duration": 10}}]}), &root, false).unwrap_err();
        assert!(e.contains("slower than the 1 fps"), "{e}");
        assert!(!root.join("long.ecproj").exists());
        // At the cap: ten minutes at 60 fps.
        run_in(&areas, json!({"cmds": [{"id": "comp.new", "params": {"name": "Long", "width": 64, "height": 36, "duration": 600, "frameRate": 60}}], "out": "ok.ecproj"}), &root, false).unwrap();
    }

    /// Hostile: a shape Repeater of a billion copies is refused before
    /// anything draws: by the gate when a path names `copies`; by the
    /// project fence right after the command when the path is an index the
    /// gate cannot read, or the value arrives as a keyframe; nested
    /// repeaters that multiply past the cap; a project file holding one.
    #[test]
    fn a_billion_copy_repeater_is_refused() {
        let (_dir, root, areas) = workspace();
        let shape = |extra: Vec<Json>| -> Json {
            let mut cmds = vec![
                json!({"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "frameRate": 24, "duration": 1.0}}),
                json!({"id": "layer.newShape", "params": {"kind": "rect", "size": [10, 10]}}),
                json!({"id": "layer.addShapeItem", "params": {"kind": "repeater"}}),
            ];
            cmds.extend(extra);
            Json::Array(cmds)
        };
        let e = run_in(&areas, json!({"cmds": shape(vec![json!({"id": "prop.set", "params": {"path": "contents/repeater/copies", "value": 1e9}})])}), &root, false).unwrap_err();
        assert!(e.contains("`prop.set` asks for 1000000000 copies, more than the 1000 the door allows in one command"), "{e}");
        for (id, p) in [("prop.set", json!({"path": "contents/#2/#1", "value": 1e9})), ("prop.addKey", json!({"path": "contents/#2/#1", "value": 1e9, "time": 0.5}))] {
            let e = run_in(&areas, json!({"cmds": shape(vec![json!({"id": id, "params": p})]), "out": "rep.png"}), &root, false).unwrap_err();
            assert!(e.contains("asks for 1000000000 repeater copies, more than the 1000 the door allows"), "{id}: {e}");
        }
        // Two repeaters in one group multiply: 100 × 101.
        let nested = shape(vec![
            json!({"id": "layer.addShapeItem", "params": {"kind": "repeater"}}),
            json!({"id": "prop.set", "params": {"path": "contents/#2/#1", "value": 100}}),
            json!({"id": "prop.set", "params": {"path": "contents/#3/#1", "value": 101}}),
        ]);
        let e = run_in(&areas, json!({"cmds": nested, "out": "rep.png"}), &root, false).unwrap_err();
        assert!(e.contains("multiply to 10100 copies, more than the 10000 the door allows"), "{e}");
        // At the cap, it draws.
        let at = shape(vec![json!({"id": "prop.set", "params": {"path": "contents/repeater/copies", "value": 1000}})]);
        run_in(&areas, json!({"cmds": at, "out": "rep.png", "max_side": 16}), &root, false).unwrap();
        // A project file holding a billion copies is refused when it opens.
        let mut cmds: Vec<Json> = shape(vec![]).as_array().unwrap().clone();
        cmds.push(json!({"id": "prop.set", "params": {"path": "contents/repeater/copies", "value": 1e9}}));
        engine_project(&root, "billion.ecproj", &cmds);
        let e = run_in(&areas, json!({"path": "billion.ecproj", "cmds": [], "out": "billion.png"}), &root, false).unwrap_err();
        assert!(e.contains("asks for 1000000000 repeater copies"), "{e}");
        assert!(!root.join("billion.png").exists());
    }

    /// Hostile: an expression that never ends never runs through the door.
    /// Setting one is refused (the setters are classed code, an Expression
    /// Selector is refused by its limit); a project or Lottie file that
    /// holds one opens, is queried and saved without evaluating it, and no
    /// frame or Lottie file of it is written.
    #[test]
    fn an_endless_expression_never_runs() {
        let (_dir, root, areas) = workspace();
        let set = comp_and_solid(64, 36).into_iter().chain([json!({"id": "prop.setExpression", "params": {"path": "transform/rotation", "expression": "while (true) {}"}})]);
        let e = run_in(&areas, json!({"cmds": set.collect::<Vec<_>>()}), &root, false).unwrap_err();
        assert!(e.contains("`prop.setExpression` is classed code"), "{e}");
        for (id, params) in [
            ("prop.pickWhip", json!({"path": "transform/rotation", "target": {"layer": 1, "path": "transform/opacity"}})),
            ("layer.expressions", json!({"enabled": true})),
            ("edit.copyExpressionOnly", json!({})),
            ("edit.copyWithPropertyLinks", json!({})),
            ("prop.convertExpressionToKeyframes", json!({"path": "transform/rotation"})),
            ("camera.linkFocusToPoi", json!({})),
            ("camera.stereoRig", json!({})),
            ("puppet.follow", json!({})),
            ("paths.nullsFollowPoints", json!({})),
        ] {
            let e = run_in(&areas, json!({"cmds": [{"id": id, "params": params}]}), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed code")), "{id}: {e}");
        }
        let text = json!([
            {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36}},
            {"id": "layer.newText", "params": {"text": "Hi"}},
            {"id": "layer.addTextAnimator", "params": {"properties": ["opacity"]}},
            {"id": "text.addSelector", "params": {"kind": "expression"}},
        ]);
        let e = run_in(&areas, json!({"cmds": text}), &root, false).unwrap_err();
        assert!(e.contains("an Expression Selector runs an expression"), "{e}");
        // A project file holding an endless expression.
        let mut cmds = comp_and_solid(64, 36);
        cmds.push(json!({"id": "prop.setExpression", "params": {"path": "transform/rotation", "expression": "while (true) {}"}}));
        engine_project(&root, "endless.ecproj", &cmds);
        let started = std::time::Instant::now();
        let q = run_in(
            &areas,
            json!({"path": "endless.ecproj", "cmds": [{"id": "prop.get", "params": {"comp": "Main", "layer": "Red", "path": "transform/rotation"}}], "out": "copy.ecproj"}),
            &root,
            false,
        )
        .unwrap();
        let got = &q["results"][0]["result"];
        assert_eq!(got["expression"], json!("while (true) {}"), "{q}");
        assert!(got.get("evaluated").is_none(), "the door's session evaluates no expression: {got}");
        assert!(root.join("copy.ecproj").is_file(), "saved as it is");
        for out in ["endless.png", "endless.json", "endless.lottie"] {
            let e = run_in(&areas, json!({"path": "endless.ecproj", "cmds": [], "out": out}), &root, false).unwrap_err();
            assert!(e.contains("the composition `Main` holds an enabled expression"), "{out}: {e}");
            assert!(!root.join(out).exists(), "{out}");
        }
        // A Lottie file whose layer rotation carries an expression (`x`).
        std::fs::write(root.join("endless.json"), br##"{"v":"5.7.0","fr":24,"ip":0,"op":24,"w":32,"h":18,"nm":"Loop","ddd":0,"assets":[],"layers":[{"ddd":0,"ind":1,"ty":1,"nm":"Red","sr":1,"ks":{"o":{"a":0,"k":100},"r":{"a":0,"k":0,"x":"while (true) {}"},"p":{"a":0,"k":[16,9,0]},"a":{"a":0,"k":[16,9,0]},"s":{"a":0,"k":[100,100,100]}},"ao":0,"sw":32,"sh":18,"sc":"#cc3344","ip":0,"op":24,"st":0,"bm":0}]}"##).unwrap();
        let e = run_in(&areas, json!({"path": "endless.json", "cmds": [], "out": "lottie.png"}), &root, false).unwrap_err();
        assert!(e.contains("holds an enabled expression"), "{e}");
        assert!(started.elapsed().as_secs() < 60, "nothing ran the loop");
    }

    /// Hostile: a duplicate loop stops at the size ceiling. Duplicating a
    /// 1,200-layer composition over and over grows the project by 1,201 a
    /// time: the copy that would pass 5,000 is refused before it runs, and
    /// nothing is written. Doubling one comp's layers stops sooner: at the
    /// layers one frame may draw (2,000), or for copies that share a name,
    /// at the comparisons the engine makes naming them.
    #[test]
    fn a_duplicate_loop_stops_at_the_size_ceiling() {
        let (_dir, root, areas) = workspace();
        let name = |i: usize| -> String { (0..3).map(|k| (b'a' + (i / 26usize.pow(k) % 26) as u8) as char).collect() };
        let mut cmds = vec![json!({"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16, "duration": 1.0}})];
        cmds.extend((0..1200).map(|i| json!({"id": "layer.newSolid", "params": {"name": name(i), "width": 16, "height": 16}})));
        engine_project(&root, "many.ecproj", &cmds);
        let copies: Vec<Json> = (0..6).map(|_| json!({"id": "project.duplicate", "params": {"items": ["Main"]}})).collect();
        let e = run_in(&areas, json!({"path": "many.ecproj", "cmds": copies, "out": "more.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("`project.duplicate` would grow the project to 6005 items, layers and effects, more than the 5000 the door allows"), "{e}");
        assert!(!root.join("more.ecproj").exists());
        // Doubling the 1,200 distinct layers: 2,400 in one frame.
        let doubling = json!([{"id": "comp.open", "params": {"comp": "Main"}}, {"id": "edit.selectAll"}, {"id": "edit.duplicate"}]);
        let e = run_in(&areas, json!({"path": "many.ecproj", "cmds": doubling, "out": "more.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("would draw 2400 layers"), "{e}");
        // Doubling copies of one name.
        let mut same = comp_and_solid(16, 16);
        for _ in 0..12 {
            same.push(json!({"id": "edit.selectAll"}));
            same.push(json!({"id": "edit.duplicate"}));
        }
        let e = run_in(&areas, json!({"cmds": same, "out": "same.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("names giving its copies unique names") || e.contains("layers with its precompositions expanded"), "{e}");
        assert!(!root.join("same.ecproj").exists() && !root.join("more.ecproj").exists());
    }

    /// Hostile: precompositions that nest many instances of each other are
    /// refused once a frame of one would draw more than the cap with them
    /// expanded (32 × (1 + 32 × 2) layers here), and a cycle of
    /// compositions in a project file is refused when it opens.
    #[test]
    fn nesting_that_multiplies_and_cycles_are_refused() {
        let (_dir, root, areas) = workspace();
        let mut cmds = vec![
            json!({"id": "comp.new", "params": {"name": "C", "width": 16, "height": 16, "duration": 1.0}}),
            json!({"id": "layer.newSolid", "params": {"name": "S", "width": 16, "height": 16}}),
            json!({"id": "comp.new", "params": {"name": "B", "width": 16, "height": 16, "duration": 1.0}}),
            json!({"id": "layer.addItem", "params": {"comp": "B", "item": "C"}}),
        ];
        for _ in 0..5 {
            cmds.push(json!({"id": "edit.selectAll"}));
            cmds.push(json!({"id": "edit.duplicate"}));
        }
        cmds.push(json!({"id": "comp.new", "params": {"name": "A", "width": 16, "height": 16, "duration": 1.0}}));
        cmds.push(json!({"id": "layer.addItem", "params": {"comp": "A", "item": "B"}}));
        for _ in 0..5 {
            cmds.push(json!({"id": "edit.selectAll"}));
            cmds.push(json!({"id": "edit.duplicate"}));
        }
        let e = run_in(&areas, json!({"cmds": cmds, "out": "nest.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("a frame of the composition `A` would draw 2080 layers with its precompositions expanded, more than the 2000"), "{e}");
        // A cycle: B's layer of C made to point at B itself.
        engine_project(
            &root,
            "acyclic.ecproj",
            &[
                json!({"id": "comp.new", "params": {"name": "C", "width": 16, "height": 16, "duration": 1.0}}),
                json!({"id": "comp.new", "params": {"name": "B", "width": 16, "height": 16, "duration": 1.0}}),
                json!({"id": "layer.addItem", "params": {"comp": "B", "item": "C"}}),
                json!({"id": "comp.new", "params": {"name": "A", "width": 16, "height": 16, "duration": 1.0}}),
                json!({"id": "layer.addItem", "params": {"comp": "A", "item": "B"}}),
            ],
        );
        let text = std::fs::read_to_string(root.join("acyclic.ecproj")).unwrap();
        let mut project: Json = serde_json::from_str(&text).unwrap();
        let items = project["items"].as_object_mut().unwrap();
        let id_of = |items: &serde_json::Map<String, Json>, n: &str| items.values().find(|i| i["name"] == json!(n)).and_then(|i| i["id"].as_u64()).unwrap();
        let b = id_of(items, "B");
        // C gets a layer of B: A → B → C → B.
        let mut layer = items.values().find(|i| i["name"] == json!("B")).unwrap()["kind"]["layers"][0].clone();
        layer["id"] = json!(9_000_001);
        layer["source"] = json!({"type": "Comp", "item": b});
        let c_item = items.values_mut().find(|i| i["name"] == json!("C")).unwrap();
        c_item["kind"]["layers"].as_array_mut().unwrap().push(layer);
        std::fs::write(root.join("cycle.ecproj"), serde_json::to_vec(&project).unwrap()).unwrap();
        let e = run_in(&areas, json!({"path": "cycle.ecproj", "cmds": [], "out": "cycle.png"}), &root, false).unwrap_err();
        assert!(e.contains("contains itself"), "{e}");
        assert!(!root.join("cycle.png").exists());
    }

    /// A frame's text and shape layers are drawn into buffers the size of
    /// their content at the frame's scale: one past the cap is refused
    /// before it is drawn; the same layer drawn smaller is not.
    #[test]
    fn a_frame_refuses_a_layer_buffer_past_the_cap() {
        let (_dir, root, areas) = workspace();
        let cmds = json!([
            {"id": "comp.new", "params": {"name": "Main", "width": 1920, "height": 1080, "duration": 1.0}},
            {"id": "layer.newText", "params": {"text": "WWWWWWWWWWWW", "size": 1296, "hScale": 1000}},
        ]);
        let e = run_in(&areas, json!({"cmds": cmds, "out": "big.png", "max_side": 4096}), &root, false).unwrap_err();
        assert!(e.contains("draws into a") && e.contains("pixel buffer at this size, more than the 8847360"), "{e}");
        assert!(!root.join("big.png").exists());
        run_in(&areas, json!({"cmds": cmds, "out": "small.png", "max_side": 64}), &root, false).unwrap();
    }

    /// An analysis walks every frame of its layer within the comp, drawing
    /// each at full size: past the cap it is refused before it starts.
    #[test]
    fn an_analysis_of_too_many_frames_is_refused() {
        let (_dir, root, areas) = workspace();
        let mut cmds = comp_and_solid(1920, 1080);
        cmds[0]["params"]["duration"] = json!(10.0);
        for id in ["camera.analyze", "warp.analyze", "track.camera", "layer.sceneEditDetection", "roto.propagate"] {
            let mut c = cmds.clone();
            c.push(json!({"id": id, "params": {"layer": 1, "wait": true}}));
            let e = run_in(&areas, json!({"cmds": c}), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{id}` would analyse 241 frames of 2073600 pixels")), "{id}: {e}");
        }
        let mut c = cmds.clone();
        c.push(json!({"id": "layer.autoTrace", "params": {"layer": 1, "timeSpan": "workArea"}}));
        assert!(run_in(&areas, json!({"cmds": c}), &root, false).unwrap_err().contains("would analyse"));
    }

    /// The keyframe generators are bounded by the keys and frames they would
    /// make from the selection: a wiggle or smoothing of a twenty-minute
    /// span is refused before it runs.
    #[test]
    fn keyframe_generators_are_bounded() {
        let (_dir, root, areas) = workspace();
        let keys = |extra: Json| {
            json!([
                {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "duration": 1200, "frameRate": 30}},
                {"id": "layer.newSolid", "params": {"name": "Red"}},
                {"id": "prop.addKey", "params": {"layer": "Red", "path": "transform/rotation", "time": 0, "value": 0}},
                {"id": "prop.addKey", "params": {"layer": "Red", "path": "transform/rotation", "time": 1199, "value": 90}},
                {"id": "prop.addKey", "params": {"layer": "Red", "path": "transform/rotation", "time": 600, "value": 45}},
                {"id": "keys.selectAll", "params": {"layers": ["Red"]}},
                extra,
            ])
        };
        let e = run_in(&areas, json!({"cmds": keys(json!({"id": "keys.wiggle", "params": {"frequency": 30}}))}), &root, false).unwrap_err();
        assert!(e.contains("`keys.wiggle` would add 35970 keyframes, more than the 10000"), "{e}");
        let e = run_in(&areas, json!({"cmds": keys(json!({"id": "keys.smooth"}))}), &root, false).unwrap_err();
        assert!(e.contains("`keys.smooth` would smooth") && e.contains("more than the 2000"), "{e}");
        // Every few seconds is fine.
        run_in(&areas, json!({"cmds": keys(json!({"id": "keys.wiggle", "params": {"frequency": 0.25}}))}), &root, false).unwrap();
    }

    /// Every built-in effect, applied at its defaults, passes the project
    /// fence: no cap is set below an effect's own default, but Time
    /// Displacement's, whose defaults read 64 frames at once (see
    /// `PARAM_CAPS`).
    #[test]
    fn every_built_in_effect_passes_at_its_defaults() {
        let mut b = Backend::headless(Session::default());
        b.exec("comp.new", json!({"name": "Main", "width": 1920, "height": 1080})).unwrap();
        let mut refused = vec![];
        for spec in effectcraft_engine::effects::registry().iter().filter(|s| s.id != "ec.time.timedisplacement") {
            b.exec("layer.newSolid", json!({"name": spec.id})).unwrap();
            if let Err(e) = b.exec("effect.apply", json!({"effect": spec.id})) {
                refused.push(format!("{}: apply: {e}", spec.id));
                continue;
            }
            if let Err(e) = measure(&project_of(&mut b), "run") {
                refused.push(e);
            }
            b.exec("edit.undo", json!({})).unwrap();
        }
        assert!(refused.is_empty(), "{refused:#?}");
        b.exec("layer.newSolid", json!({"name": "Displaced"})).unwrap();
        b.exec("effect.apply", json!({"effect": "ec.time.timedisplacement"})).unwrap();
        let e = measure(&project_of(&mut b), "run").unwrap_err();
        assert!(e.contains("asks for 1 s of maximum displacement, more than the 0.25"), "{e}");
    }

    /// Every capped effect parameter is one the effect declares (a typo
    /// would cap nothing).
    #[test]
    fn the_param_caps_name_real_parameters() {
        for c in PARAM_CAPS.iter().filter(|c| c.owner.starts_with("ec.") && !c.param.starts_with("*/")) {
            let spec = effectcraft_engine::effects::registry().iter().find(|s| s.id == c.owner).unwrap_or_else(|| panic!("no effect {}", c.owner));
            assert!(spec.params.iter().any(|p| p.id == c.param), "{} has no parameter {}", c.owner, c.param);
        }
        for c in PARAM_CAPS.iter().filter(|c| c.owner.starts_with("ec.")) {
            assert!(effectcraft_engine::effects::registry().iter().any(|s| s.id == c.owner), "no effect {}", c.owner);
        }
    }

    /// A value past its slider's valid range, or past a reviewed cap, is
    /// refused however it is set (`prop.set` itself clamps to the range; a
    /// keyframe does not); a point that reaches far outside its layer, and a
    /// LUT inline that declares a vast table, are refused too.
    #[test]
    fn values_past_their_range_are_refused_however_they_are_set() {
        let (_dir, root, areas) = workspace();
        let with = |effect: &str, extra: Vec<Json>| {
            let mut cmds = comp_and_solid(64, 36);
            cmds.push(json!({"id": "effect.apply", "params": {"effect": effect}}));
            cmds.extend(extra);
            Json::Array(cmds)
        };
        // Unsharp Mask's radius: valid 0.1..500.
        let set = with("ec.blur.unsharp", vec![json!({"id": "prop.set", "params": {"path": "effects/#1/radius", "value": 1e9}})]);
        let made = run_in(&areas, json!({"cmds": set}), &root, false).unwrap();
        assert_eq!(made["results"][3]["result"], json!(500.0), "prop.set clamps: {made}");
        let key = with("ec.blur.unsharp", vec![json!({"id": "prop.addKey", "params": {"path": "effects/#1/radius", "value": 1e9}})]);
        let e = run_in(&areas, json!({"cmds": key}), &root, false).unwrap_err();
        assert!(e.contains("is 1000000000, outside its valid range 0.1..500"), "{e}");
        // Gaussian Blur's blurriness: valid 0..3000, capped at 200.
        let e = run_in(&areas, json!({"cmds": with("Gaussian Blur", vec![json!({"id": "prop.set", "params": {"path": "effects/#1/blurriness", "value": 201}})])}), &root, false).unwrap_err();
        assert!(e.contains("asks for 201 px of blurriness, more than the 200"), "{e}");
        run_in(&areas, json!({"cmds": with("Gaussian Blur", vec![json!({"id": "prop.set", "params": {"path": "effects/#1/blurriness", "value": 200}})])}), &root, false).unwrap();
        // A floor: CC Star Burst's grid spacing.
        let e = run_in(&areas, json!({"cmds": with("ec.sim.ccstarburst", vec![json!({"id": "prop.set", "params": {"path": "effects/#1/gridSpacing", "value": 1}})])}), &root, false).unwrap_err();
        assert!(e.contains("asks for 1 px of grid spacing, less than the 2"), "{e}");
        // CC Power Pin's corner far outside the layer.
        let e = run_in(&areas, json!({"cmds": with("ec.distort.ccpowerpin", vec![json!({"id": "prop.set", "params": {"path": "effects/#1/topLeft", "value": [-100000, 0]}})])}), &root, false).unwrap_err();
        assert!(e.contains("reaches 100000 px outside the layer, more than the 500"), "{e}");
        // An inline .csp that declares a 100000³ table (a few bytes).
        let csp = "CSPLUTV100\n3D\n2\n0 1\n0 1\n2\n0 1\n0 1\n2\n0 1\n0 1\n100000 100000 100000\n0 0 0\n";
        let e = run_in(&areas, json!({"cmds": with("ec.color.ociofile", vec![json!({"id": "prop.set", "params": {"path": "effects/#1/file", "value": csp}})])}), &root, false).unwrap_err();
        assert!(e.contains("holds a LUT for `file`") && e.contains("larger table"), "{e}");
        let cube = "LUT_3D_SIZE 4096\n0 0 0\n";
        let e = run_in(&areas, json!({"cmds": with("ec.utility.applylut", vec![json!({"id": "prop.set", "params": {"path": "effects/#1/lut", "value": cube}})])}), &root, false).unwrap_err();
        assert!(e.contains("holds a LUT for `lut`"), "{e}");
    }
}
