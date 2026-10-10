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
//!   `prores` MOV, `wav`, `gif`), at most [`MAX_EXPORT_MS`] per call, within
//!   the frame, sample and GIF caps of [`export_work`] and at most
//!   [`MAX_RENDER_SIDE`] on the long edge ([`export_size`])
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
//!   other id is refused before any command runs, and so is a parameter
//!   past its reviewed limit ([`LIMITS`]). After every command the project
//!   and the session are fenced ([`fence`]): nothing in them may point the
//!   engine at a file or folder outside the area. Engine work runs on the
//!   shell's UI thread, so what one call may ask for in all is capped too
//!   ([`Caps`]): the analyses it runs, what it copies, how large the project
//!   may grow and what it may hold.
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

use filmcraft_engine::project::{
    resolve_auto_points, ClipId, EffectInstance, ItemId, ItemKind, Param, ParamKind, ParamValue, Project, Sequence, SequenceSettings, Track, TrackItem, TrackKind,
};
use filmcraft_engine::time::{FrameRate, Tick, TimeRange, TICKS_PER_SECOND};
use filmcraft_engine::{Services, Session};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, FileRead, Inner, InnerRule, Limit, Measure, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The largest media file the service reads (bytes).
const MAX_MEDIA_BYTES: u64 = 512 << 20;
/// The largest project file `project.info` and `run` open, and `run` writes
/// (bytes).
const MAX_PROJECT_BYTES: u64 = 64 << 20;
/// The longest range one `export` call encodes (milliseconds).
const MAX_EXPORT_MS: f64 = 300_000.0;
/// The most frames one export encodes: [`MAX_EXPORT_MS`] at 60 fps. A
/// faster sequence exports a shorter range (the engine accepts 1000 fps,
/// which would be 300,000 frames in 5 minutes).
const MAX_EXPORT_FRAMES: f64 = 18_000.0;
/// The most audio sample frames one export mixes: [`MAX_EXPORT_MS`] at
/// 48 kHz. A WAV is held whole in memory (planar, interleaved and encoded:
/// about 290 MB of stereo at this size); a higher sample rate exports a
/// shorter range.
const MAX_EXPORT_SAMPLES: f64 = 300.0 * 48_000.0;
/// The most pixels × frames one GIF export encodes: the GIF encoder keeps
/// the whole file in memory until it is written, up to about a byte per
/// pixel (2^28 is 5 s of 1080p, or 27 s at 854 × 480).
const MAX_GIF_PIXELS: f64 = (1u64 << 28) as f64;
/// The longest edge `frame` produces, and its default.
const MAX_RENDER_SIDE: u32 = 4096;
const DEFAULT_RENDER_SIDE: u32 = 1024;
/// The most pixels one frame the door renders or encodes may have: 4096 ×
/// 2304. The renderer composites in f32 RGBA (151 MB a working image at
/// this size); a square 4096 frame would be 268 MB.
const MAX_RENDER_PIXELS: f64 = 4096.0 * 2304.0;
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
/// - **Limits** ([`LIMITS`]): the parameters that multiply work or memory,
///   capped per command before anything runs. What no parameter shows (the
///   clips an analysis walks, what a paste copies, a project file's
///   content) `run` bounds from the session itself ([`Caps`]).
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
    limits: LIMITS,
    copies_per_call: COPIES_PER_CALL,
    held: &[],
};

// ---------------------------------------------------------------------------
// the door's limits (reviewed from filmcraft at the pinned revision)
// ---------------------------------------------------------------------------
//
// Engine work runs on the shell's UI thread (until #399), so every cap is
// set well above everyday editing and far below what would stall that thread
// or exhaust memory: about one 5-minute export's worth of work, and frames
// the size the door already renders and encodes (`MAX_RENDER_SIDE`).

/// The longest side of a frame a command may ask for (pixels): the door's
/// render and export side, so a sequence or generator it makes never needs
/// scaling down to render.
const MAX_SIDE: f64 = MAX_RENDER_SIDE as f64;
/// The most pixels one frame may hold: 4096 × 2304 (DCI 4K, UHD and their
/// portrait forms fit). The renderer composites in f32 RGBA, so this is
/// 151 MB per working image; the engine's own ceiling is 16384 × 16384
/// (4 GiB per image).
const MAX_PIXELS: f64 = MAX_RENDER_PIXELS;
/// The highest frame rate a sequence or offline file may run at: the
/// engine's fastest standard rate (it accepts up to 1000 fps). Every
/// per-frame cost (export, scene detection, tracking) scales with it.
const MAX_FPS: f64 = 120.0;
/// The highest audio sample rate (Hz): twice the everyday 48 kHz (the engine
/// accepts 384 kHz). Audio analysis, the mix and a WAV export, which the
/// engine holds whole in memory, scale with it.
const MAX_SAMPLE_RATE: f64 = 96_000.0;
/// The most audio channels an offline file may declare (its silence is
/// allocated per channel; the engine takes any `u32`), and the most audio
/// clips a channel map may give one item: Premiere's adaptive-track maximum.
const MAX_CHANNELS: f64 = 32.0;
/// The longest duration a command may give, in seconds: 24 hours. The
/// engine takes any length up to its tick range (about 105 days); a slowed
/// clip, a long still or marker makes timelines analyses then walk.
const MAX_SECONDS: f64 = 86_400.0;
/// [`MAX_SECONDS`] in the engine's ticks.
const MAX_TICKS: f64 = MAX_SECONDS * TICKS_PER_SECOND as f64;
/// [`MAX_SECONDS`] in frames at [`MAX_FPS`].
const MAX_FRAMES: f64 = MAX_SECONDS * MAX_FPS;
/// The fastest clip speed, in percent: 100×, Premiere's own limit. An export
/// decodes up to that many source frames per frame it writes, and a clip's
/// peak scan reads its source at that rate (the engine takes any positive
/// speed).
const MAX_SPEED: f64 = 10_000.0;
/// The most a speed change may lengthen a clip: 100× (1 %).
const MAX_STRETCH: f64 = 100.0;
/// The largest type a command may ask for (pixels). The text engine
/// rasterises each glyph whole at its final size (`glyph_mask`), so a
/// glyph's mask grows with the square of the size: 1000 px is about 3 MB a
/// glyph. The engine clamps a set size to 2000 but takes any size when a
/// text layer or character style is made.
const MAX_TEXT_PX: f64 = 1_000.0;
/// The most pen points a mask path may have: every point is an edge (up to
/// 256 when curved) the coverage of every pixel of every frame walks.
const MAX_MASK_POINTS: f64 = 512.0;
/// The widest mask feather or expansion (pixels). Coverage measures each
/// pixel against every edge within that band, and the engine takes any
/// width.
const MAX_MASK_BAND: f64 = 1_000.0;
/// The most transcript words a command may import or walk: about three
/// hours of speech. The transcript commands map every word of every
/// transcribed clip to the timeline (checking each against the clips above
/// it), and Remove Pauses or Fillers cuts the sequence once per word range.
const MAX_WORDS: f64 = 30_000.0;
/// The most frames the analyses of one call decode: scene detection walks
/// every frame of its clips, mask tracking every frame it tracks. As many
/// as a 5-minute export at 60 fps writes ([`MAX_EXPORT_FRAMES`]).
const MAX_ANALYSIS_FRAMES: f64 = MAX_EXPORT_FRAMES;
/// Copies of the project's content one call may multiply by. No film
/// command takes a count of copies: paste, duplicate and the like copy the
/// selection, whose growth `run` bounds with its size ceiling ([`Caps`]).
const COPIES_PER_CALL: f64 = 1.0;

/// A limit on the product of numeric parameters (the door's own measure).
const fn product(id: &'static str, what: &'static str, paths: &'static [&'static str], max: f64) -> Limit {
    Limit { id, what, measure: Measure::Product(paths), max, copies: false }
}

/// A limit the reviewer measures from the parameters.
const fn custom(id: &'static str, what: &'static str, f: fn(&Json) -> Result<Option<f64>, String>, max: f64) -> Limit {
    Limit { id, what, measure: Measure::Custom(f), max, copies: false }
}

/// The parameters of the door's commands that multiply work or memory, each
/// with its reason (read from the command's implementation at the pinned
/// filmcraft revision). Positions (`time`, `frame`, `timecode`, marks) are
/// not limited: where something lands cannot multiply work, and how long a
/// timeline grows is bounded after every command ([`Caps::after`]).
static LIMITS: &[Limit] = &[
    // Sequence frame size: every render of the sequence composites working
    // images of width × height f32 pixels; `export`, `.png` and scopes all
    // render it. Engine: 32768 a side, 16384² in all.
    product("file.newSequence", "pixels", &["width", "height"], MAX_PIXELS),
    custom("file.newSequence", "pixels on a side", long_side, MAX_SIDE),
    product("sequence.settings", "pixels", &["width", "height"], MAX_PIXELS),
    custom("sequence.settings", "pixels on a side", long_side, MAX_SIDE),
    // Sequence frame rate: every per-frame cost (an export writes duration ×
    // fps frames) scales with it. Engine: 1000 fps.
    product("file.newSequence", "frames per second", &["fps"], MAX_FPS),
    product("sequence.settings", "frames per second", &["fps"], MAX_FPS),
    // Sequence sample rate: the mix, every audio analysis and a WAV export
    // (held whole in memory) scale with it. Engine: 384 kHz.
    product("file.newSequence", "samples per second", &["sampleRate"], MAX_SAMPLE_RATE),
    product("sequence.settings", "samples per second", &["sampleRate"], MAX_SAMPLE_RATE),
    // Offline file: its slate is drawn and its silence allocated at the size,
    // rate and channels it declares, none of which the engine checks
    // (`offline_file` truncates them to u32); its duration is
    // `seconds` (default 10).
    product("file.newOfflineFile", "pixels", &["width", "height"], MAX_PIXELS),
    custom("file.newOfflineFile", "pixels on a side", long_side, MAX_SIDE),
    product("file.newOfflineFile", "frames per second", &["fps"], MAX_FPS),
    product("file.newOfflineFile", "samples per second", &["sampleRate"], MAX_SAMPLE_RATE),
    product("file.newOfflineFile", "channels", &["channels"], MAX_CHANNELS),
    product("file.newOfflineFile", "seconds", &["seconds"], MAX_SECONDS),
    // Generated media: `new_generator` takes undocumented `width` and
    // `height` (default the sequence's) unchecked, and draws every frame at
    // that size; `seconds` (default 5) is its length, unchecked.
    product("file.newBarsAndTone", "pixels", &["width", "height"], MAX_PIXELS),
    custom("file.newBarsAndTone", "pixels on a side", long_side, MAX_SIDE),
    product("file.newBarsAndTone", "seconds", &["seconds"], MAX_SECONDS),
    product("file.newBlackVideo", "pixels", &["width", "height"], MAX_PIXELS),
    custom("file.newBlackVideo", "pixels on a side", long_side, MAX_SIDE),
    product("file.newBlackVideo", "seconds", &["seconds"], MAX_SECONDS),
    product("file.newColorMatte", "pixels", &["width", "height"], MAX_PIXELS),
    custom("file.newColorMatte", "pixels on a side", long_side, MAX_SIDE),
    product("file.newColorMatte", "seconds", &["seconds"], MAX_SECONDS),
    product("file.newCountingLeader", "pixels", &["width", "height"], MAX_PIXELS),
    custom("file.newCountingLeader", "pixels on a side", long_side, MAX_SIDE),
    product("file.newCountingLeader", "seconds", &["seconds"], MAX_SECONDS),
    product("file.newTransparentVideo", "pixels", &["width", "height"], MAX_PIXELS),
    custom("file.newTransparentVideo", "pixels on a side", long_side, MAX_SIDE),
    product("file.newTransparentVideo", "seconds", &["seconds"], MAX_SECONDS),
    // An adjustment layer's length (default 5 s; the size is the sequence's).
    product("file.newAdjustmentLayer", "seconds", &["seconds"], MAX_SECONDS),
    // Speed (percent, default 100): a fast clip decodes `speed` source frames
    // per frame shown and its peak scan reads its whole source at that rate
    // (a vast speed loops it practically forever); a slow one lengthens the
    // timeline by 100 / `speed`. Engine: any positive speed, the clip's end
    // only kept within the tick range.
    product("clip.speedDuration", "percent", &["speed"], MAX_SPEED),
    custom("clip.speedDuration", "times the clip's length (a speed under 1 %)", stretch, MAX_STRETCH),
    // Rate Stretch sets the speed from a new length: `delta` (ticks) or
    // `deltaFrames` stretch the clip, clamped only by the next clip.
    product("timeline.rateStretch", "ticks", &["delta"], MAX_TICKS),
    product("timeline.rateStretch", "frames", &["deltaFrames"], MAX_FRAMES),
    // Marker lengths in frames (`tick_of` wraps past the tick range) or
    // ticks: nothing iterates over them, but a billion-frame marker is a
    // hostile value, refused like every other length.
    product("markers.add", "frames", &["durationFrames"], MAX_FRAMES),
    product("markers.edit", "frames", &["durationFrames"], MAX_FRAMES),
    product("markers.addFlashCue", "frames", &["durationFrames"], MAX_FRAMES),
    product("markers.addRange", "frames", &["durationFrames"], MAX_FRAMES),
    product("markers.addRange", "ticks", &["duration"], MAX_TICKS),
    // Lengths of what a command adds to the timeline, which later analyses
    // walk and every overflow check of the engine guards: a caption (default
    // 3 s), a frame hold segment (2 s, rippling the rest), a graphic (5 s;
    // the shape commands read `seconds` too), a transition (`frames`, default
    // from preferences), a placed clip (`duration` ticks; a still has no end).
    product("captions.add", "seconds", &["durationSeconds"], MAX_SECONDS),
    product("clip.insertFrameHoldSegment", "seconds", &["seconds"], MAX_SECONDS),
    product("graphics.newText", "seconds", &["seconds"], MAX_SECONDS),
    product("graphics.newVerticalText", "seconds", &["seconds"], MAX_SECONDS),
    product("graphics.newShape", "seconds", &["seconds"], MAX_SECONDS),
    product("graphics.newRectangle", "seconds", &["seconds"], MAX_SECONDS),
    product("graphics.newEllipse", "seconds", &["seconds"], MAX_SECONDS),
    product("graphics.newPolygon", "seconds", &["seconds"], MAX_SECONDS),
    product("graphics.resetDuration", "seconds", &["seconds"], MAX_SECONDS),
    product("sequence.applyVideoTransition", "frames", &["frames"], MAX_FRAMES),
    product("sequence.applyAudioTransition", "frames", &["frames"], MAX_FRAMES),
    product("timeline.place", "ticks", &["duration"], MAX_TICKS),
    // Automate to Sequence: each still's length and each overlap (a
    // transition), in frames.
    product("clip.automateToSequence", "frames", &["stillFrames"], MAX_FRAMES),
    product("clip.automateToSequence", "frames", &["overlapFrames"], MAX_FRAMES),
    // Remix target: planning makes about target / clip-length joints (each a
    // window search) and the clip becomes that long. Given as `duration`
    // (ticks), `seconds`, `frame`/`frames` or a `timecode` duration.
    product("clip.remix", "ticks", &["duration"], MAX_TICKS),
    product("clip.remix", "seconds", &["seconds"], MAX_SECONDS),
    product("clip.remix", "frames", &["frame"], MAX_FRAMES),
    product("clip.remix", "frames", &["frames"], MAX_FRAMES),
    custom("clip.remix", "seconds of timecode", timecode_seconds, MAX_SECONDS),
    product("clip.remix.properties", "ticks", &["duration"], MAX_TICKS),
    product("clip.remix.properties", "seconds", &["seconds"], MAX_SECONDS),
    product("clip.remix.properties", "frames", &["frame"], MAX_FRAMES),
    product("clip.remix.properties", "frames", &["frames"], MAX_FRAMES),
    custom("clip.remix.properties", "seconds of timecode", timecode_seconds, MAX_SECONDS),
    // Trims and moves by a length: a ripple trim of a still, a caption trim
    // or move, lengthens the timeline by it; frame steps by a count
    // (`frame_at + frames` overflows the engine's i64 first).
    product("timeline.trim", "ticks", &["delta"], MAX_TICKS),
    product("timeline.trim", "frames", &["deltaFrames"], MAX_FRAMES),
    product("captions.trim", "ticks", &["delta"], MAX_TICKS),
    product("captions.trim", "frames", &["deltaFrames"], MAX_FRAMES),
    product("captions.move", "ticks", &["delta"], MAX_TICKS),
    product("captions.move", "frames", &["deltaFrames"], MAX_FRAMES),
    product("trim.nudge", "frames", &["frames"], MAX_FRAMES),
    product("playhead.step", "frames", &["frames"], MAX_FRAMES),
    // A known sync delay, in frames, shifts every clip but the reference.
    product("clip.synchronize", "frames", &["offset"], MAX_FRAMES),
    product("clip.createMulticam", "frames", &["offset"], MAX_FRAMES),
    product("clip.mergeClips", "frames", &["offset"], MAX_FRAMES),
    // Mask tracking decodes and tracks one frame per step (at up to 960 px)
    // and keeps a path keyframe for each; without `frames` it tracks to the
    // clip's end, which `run` bounds from the clip ([`Caps::before`]).
    product("masks.track", "frames", &["frames"], MAX_ANALYSIS_FRAMES),
    // Type size: a text layer or character style is made at any size (the
    // effect's 2000 px clamp applies only to `graphics.set`), and its glyphs
    // are rasterised whole at that size. `run` also bounds the size the
    // layer's scale and the clip's Motion make of it ([`Caps::after`]).
    product("graphics.newText", "pixels of type", &["size"], MAX_TEXT_PX),
    product("graphics.newVerticalText", "pixels of type", &["size"], MAX_TEXT_PX),
    product("graphics.set", "pixels of type", &["props.size"], MAX_TEXT_PX),
    custom("graphics.setCharStyle", "pixels of type", char_size, MAX_TEXT_PX),
    // Masks: the pen points of a path, and the feather and expansion band
    // every pixel's coverage searches (`masks.set` clamps neither; the
    // keyframe commands reach them with `mask`).
    custom("masks.add", "pen points", path_points, MAX_MASK_POINTS),
    custom("masks.set", "pen points", path_points, MAX_MASK_POINTS),
    product("masks.set", "pixels of feather", &["feather"], MAX_MASK_BAND),
    product("masks.set", "pixels of expansion", &["expansion"], MAX_MASK_BAND),
    custom("effects.setParam", "pixels of mask feather or expansion", mask_band, MAX_MASK_BAND),
    custom("effects.setParam", "pen points", mask_points, MAX_MASK_POINTS),
    custom("effects.setKeyframe", "pixels of mask feather or expansion", mask_band, MAX_MASK_BAND),
    custom("effects.setKeyframe", "pen points", mask_points, MAX_MASK_POINTS),
    // Modify ▸ Audio Channels: an item's channel map lists the audio clips
    // every later placement of it makes (one per track below), and Sequence
    // From Clip one audio track per entry; the engine takes any number.
    custom("clip.audioChannels", "audio clips per placement", channel_map_clips, MAX_CHANNELS),
    // A transcript's words (the engine takes any number): every transcript
    // command then walks them ([`Caps::before`] bounds what a sequence's
    // clips show of all its transcripts).
    custom("transcript.set", "words", transcript_words, MAX_WORDS),
    // Captions made from a transcript: their lengths (`minSeconds`,
    // `maxSeconds`, the `gapFrames` between them) and the characters one may
    // hold, `maxChars` × `lines`, which the engine multiplies unchecked.
    product("transcript.createCaptions", "seconds", &["minSeconds"], MAX_SECONDS),
    product("transcript.createCaptions", "seconds", &["maxSeconds"], MAX_SECONDS),
    product("transcript.createCaptions", "frames", &["gapFrames"], MAX_FRAMES),
    product("transcript.createCaptions", "characters in a caption", &["maxChars", "lines"], 10_000.0),
];

/// A numeric parameter as the door's products read it: absent or null is
/// nothing asked, a number or a numeric string counts by its size, anything
/// else is refused.
fn number(p: &Json, key: &str) -> Result<Option<f64>, String> {
    let n = match p.get(key) {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Number(n)) => n.as_f64(),
        Some(Json::String(s)) => s.trim().parse::<f64>().ok(),
        Some(_) => None,
    };
    n.filter(|n| n.is_finite()).map(|n| Some(n.abs())).ok_or_else(|| format!("`{key}` is a number the door bounds"))
}

/// The longer of `width` and `height`, when either is given.
fn long_side(p: &Json) -> Result<Option<f64>, String> {
    Ok(number(p, "width")?.into_iter().chain(number(p, "height")?).reduce(f64::max))
}

/// How many times longer `speed` (percent) makes a clip: 100 / `speed`.
fn stretch(p: &Json) -> Result<Option<f64>, String> {
    Ok(number(p, "speed")?.map(|speed| 100.0 / speed))
}

/// A `timecode` duration in seconds, counting each frame as a second (no
/// frame rate is known here, and every rate is at least one frame a second).
fn timecode_seconds(p: &Json) -> Result<Option<f64>, String> {
    let Some(tc) = p.get("timecode").and_then(Json::as_str) else { return Ok(None) };
    // Fields the engine cannot parse it refuses itself.
    Ok(filmcraft_engine::time::parse_timecode(tc, FrameRate::new(1, 1), false, 0).ok().map(|frames| frames.unsigned_abs() as f64))
}

/// The type size `graphics.setCharStyle` gives (`style.size` or `style.fontSize`).
fn char_size(p: &Json) -> Result<Option<f64>, String> {
    let style = &p["style"];
    Ok(number(style, "size")?.into_iter().chain(number(style, "fontSize")?).reduce(f64::max))
}

/// The pen points of a mask path value: a list of points, or `{vertices}`.
fn points_of(path: &Json) -> Option<f64> {
    path.as_array().or_else(|| path.get("vertices").and_then(Json::as_array)).map(|points| points.len() as f64)
}

/// The pen points of `masks.add` / `masks.set`'s `path`.
fn path_points(p: &Json) -> Result<Option<f64>, String> {
    Ok(points_of(&p["path"]))
}

/// Whether a keyframe command addresses a mask's parameter `param`.
fn mask_param(p: &Json, params: &[&str]) -> bool {
    p.get("mask").is_some_and(|m| !m.is_null()) && p["param"].as_str().is_some_and(|id| params.contains(&id))
}

/// The feather or expansion a keyframe command gives a mask.
fn mask_band(p: &Json) -> Result<Option<f64>, String> {
    if mask_param(p, &["feather", "expansion"]) { number(p, "value") } else { Ok(None) }
}

/// The pen points of the path a keyframe command gives a mask.
fn mask_points(p: &Json) -> Result<Option<f64>, String> {
    Ok(if mask_param(p, &["path"]) { points_of(&p["value"]) } else { None })
}

/// The audio clips `clip.audioChannels` maps an item to (`clips` as lists
/// of source channels; a list of clip ids sets timeline clips' channels).
fn channel_map_clips(p: &Json) -> Result<Option<f64>, String> {
    Ok(p["clips"].as_array().filter(|a| !a.iter().all(Json::is_u64)).map(|a| a.len() as f64))
}

/// The words of the transcript `transcript.set` imports.
fn transcript_words(p: &Json) -> Result<Option<f64>, String> {
    Ok(p["transcript"]["words"].as_array().map(|w| w.len() as f64))
}

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
/// most `max_side`. The engine renders at the scale `max_side` gives, so the
/// working images it composites are that size: at most
/// [`MAX_RENDER_PIXELS`] (a square `max_side` 4096 would be 268 MB each).
fn program_png(s: &Session, (w, h): (u32, u32), rate: FrameRate, duration_ms: f64, at_ms: f64, max_side: u32) -> Result<Vec<u8>, String> {
    if at_ms > duration_ms {
        return Err(format!("`at_ms` {at_ms} is past the end ({duration_ms:.0} ms)"));
    }
    // The display time of the last frame, not one past it.
    let frame_ms = rate.tick_of(1).seconds() * 1000.0;
    let t = Tick::from_seconds_f64(at_ms.min((duration_ms - frame_ms).max(0.0)) / 1000.0);
    let long = w.max(h);
    let scale = if long > max_side { max_side as f32 / long as f32 } else { 1.0 };
    // As `filmcraft_render::output_size` sizes the render.
    let (rw, rh) = (((w as f32 * scale).round() as f64).max(1.0), ((h as f32 * scale).round() as f64).max(1.0));
    if rw * rh > MAX_RENDER_PIXELS {
        return Err(format!(
            "a {w}×{h} frame at `max_side` {max_side} renders {rw}×{rh} pixels, more than the {} the door renders at once; give a smaller `max_side`",
            MAX_RENDER_PIXELS as u64
        ));
    }
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
    let (bytes, result) = export_staged(&mut s, rules, &canon_to_area(rules, &area, &out_abs), &format, (start_ms, end_ms), audio).map_err(err)?;
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
/// engine's own encoders, at the sequence's size ([`export_size`] scales a
/// larger one down), and place it at `out` (the area's spelling). What the
/// export asks for is checked first ([`export_work`]). The engine streams an
/// export to a path itself (`file.exportMedia`, the exporter's
/// `std::fs::File::create`): a staging folder inside the area, then into
/// place under the call's rules ([`octosense_engine_area::Stage::commit`]).
/// The settings come from these parameters alone: no preset, no `settings`
/// patch, so no image overlay or caption sidecar path can enter. Answers the
/// bytes and the encoder's report.
fn export_staged(s: &mut Session, rules: &Area, out: &Path, format: &str, (start_ms, end_ms): (f64, f64), audio: bool) -> Result<(u64, Json), String> {
    let st = s.active_sequence().map(|q| q.settings.clone()).ok_or("no sequence is active")?;
    let size = export_size(st.width, st.height);
    export_work(format, (start_ms, end_ms), audio, st.frame_rate, st.sample_rate, size.unwrap_or((st.width, st.height)))?;
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

/// What an export of `start_ms..end_ms` asks the engine for, refused over
/// the door's caps before it starts: the frames a picture format encodes at
/// the sequence's `rate` ([`MAX_EXPORT_FRAMES`]), the audio sample frames a
/// format with sound mixes at its `sample_rate` ([`MAX_EXPORT_SAMPLES`]),
/// and for a GIF, which the encoder keeps whole in memory, its pixels
/// ([`MAX_GIF_PIXELS`]).
fn export_work(format: &str, (start_ms, end_ms): (f64, f64), audio: bool, rate: FrameRate, sample_rate: u32, (w, h): (u32, u32)) -> Result<(), String> {
    let seconds = (end_ms - start_ms) / 1000.0;
    let frames = (seconds * rate.sane().as_f64()).ceil();
    if format != "wav" && frames > MAX_EXPORT_FRAMES {
        return Err(format!(
            "{seconds:.1} s at {} fps is {frames} frames, more than the {MAX_EXPORT_FRAMES} one export encodes; give a shorter `start_ms`/`end_ms`",
            rate.sane().label()
        ));
    }
    let samples = (seconds * f64::from(sample_rate)).ceil();
    if (format == "wav" || (audio && format != "gif")) && samples > MAX_EXPORT_SAMPLES {
        return Err(format!(
            "{seconds:.1} s of sound at {sample_rate} Hz is {samples} samples, more than the {MAX_EXPORT_SAMPLES} one export mixes; give a shorter range (or `audio: false`)"
        ));
    }
    let pixels = frames * f64::from(w) * f64::from(h);
    if format == "gif" && pixels > MAX_GIF_PIXELS {
        return Err(format!(
            "a {w}×{h} GIF of {frames} frames is {pixels} pixels, more than the {MAX_GIF_PIXELS} one GIF holds in memory; give a shorter range"
        ));
    }
    Ok(())
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

/// The frame size an export encodes at: the sequence's, scaled down (even
/// sides) so its longest edge is at most [`MAX_RENDER_SIDE`] and it holds at
/// most [`MAX_RENDER_PIXELS`]. Media or a project may be as large as 16K, and
/// the exporter renders whole frames, a batch of them at once.
fn export_size(w: u32, h: u32) -> Option<(u32, u32)> {
    let (wf, hf) = (f64::from(w.max(1)), f64::from(h.max(1)));
    let k = (f64::from(MAX_RENDER_SIDE) / wf.max(hf)).min((MAX_RENDER_PIXELS / (wf * hf)).sqrt());
    if k >= 1.0 {
        return None;
    }
    let even = |v: f64| (((v * k) / 2.0).floor() as u32 * 2).max(2);
    Some((even(wf), even(hf)))
}

/// What a `run` call writes.
enum Out {
    Project,
    Frame { at_ms: f64, max_side: u32 },
    Media { format: String },
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over `path`, each checked against what the
/// call may still ask for before it runs and the project fenced ([`fence`])
/// and bounded ([`Caps`]) after it; then `out` written under the area's
/// rules.
fn run(args: &Json, rules: &Arc<Area>) -> Result<Json, String> {
    let err = |e: String| format!("film.run: {e}");
    let area = canonical(rules).map_err(err)?;
    let door = door()?;
    let admitted = door.admit_all(&args["cmds"], rules)?;
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
    let mut caps = Caps::open(&mut s).map_err(|e| err(format!("`{rel}`: {e}")))?;
    let mut results = Vec::with_capacity(admitted.len());
    for (id, mut params) in admitted {
        // A job the engine would leave running on a thread of its own runs
        // here, within this call's budgets: the session is gone when the
        // call returns, so its results would never be applied anyway.
        if BACKGROUND_JOBS.contains(&id.as_str()) {
            if params.is_null() {
                params = json!({});
            }
            let Some(named) = params.as_object_mut() else { return Err(format!("film.run {id}: `params` is an object")) };
            named.insert("wait".into(), json!(true));
        }
        caps.before(&s, door, &id, &params).map_err(|e| format!("film.run {id}: {e}"))?;
        let r = s.execute(&id, params).map_err(|e| format!("film.run {id}: {e}"))?;
        fence(&s, &area).map_err(|e| format!("film.run {id}: {e}"))?;
        caps.after(&mut s).map_err(|e| format!("film.run {id}: {e}"))?;
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
            let (_, _, duration_ms) = active_shape(&s).map_err(err)?;
            let (start_ms, end_ms) = range_ms(args, duration_ms, "sequence").map_err(err)?;
            let audio = args["audio"].as_bool().unwrap_or(true);
            let (bytes, _) = export_staged(&mut s, rules, &dest, &format, (start_ms, end_ms), audio).map_err(err)?;
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

// ---------------------------------------------------------------------------
// the caps on what one call may ask for
// ---------------------------------------------------------------------------
//
// The door's limits ([`LIMITS`]) bound what a command's parameters ask for.
// What no parameter shows is bounded here, from the session: the clips an
// analysis walks, what a paste copies, the edits a command makes one after
// another, and what the project may hold at all (a project file is as
// hostile as a parameter).

/// Commands the engine runs on a thread of its own unless asked to `wait`
/// (`scene_detect.rs`, `masks.rs` `track`): the door always waits for them.
const BACKGROUND_JOBS: [&str; 2] = ["clip.sceneEditDetection", "masks.track"];
/// Undo steps a door session keeps. Every engine edit clones the whole
/// project and keeps the old copy for undo (200 by default), so a call of 64
/// edits held 65 copies; eight still let a call undo what it just tried.
const DOOR_HISTORY: usize = 8;
/// Items, tracks, clips, transitions, captions and markers one call may add
/// ([`elements`]); each clip is a few KB, kept up to [`DOOR_HISTORY`] times
/// over. A copy loop (select all, copy, paste, again) doubles them every
/// round and stops here; one command copies at most what the project holds.
const MAX_ADDED_ELEMENTS: u64 = 5_000;
/// Keyframes and pen points one call may add ([`points`]): a tracked mask
/// keeps its whole path every frame it tracks.
const MAX_ADDED_POINTS: u64 = 250_000;
/// The longest a sequence or project item may run: [`MAX_SECONDS`]. The
/// engine's tick range is about 105 days.
const MAX_TIMELINE: Tick = Tick(86_400 * TICKS_PER_SECOND);
/// The most source renders one frame may make, and the most streams one
/// audio sample may mix ([`fanout`]): a nest renders every track of the
/// sequence it holds, an Echo renders `count` more frames of its clip (the
/// engine renders at most 30), a fast clip decodes up to `speed` frames, a
/// Write-on stamps its brush up to 20,000 times. A nest of nests multiplies
/// these; the engine stops only at 8 levels.
const MAX_FANOUT: f64 = 256.0;
/// The most pixels a Warp Stabilizer may analyse: on first render it decodes
/// every frame of its clip (up to 20,000; at 480 or, detailed, 960 pixels
/// wide) and keeps them all as f32 pyramids at once. 300 frames at 480 × 270
/// is about 200 MB.
const MAX_STABILIZER_PIXELS: f64 = 300.0 * 480.0 * 270.0;
/// The most edits a command may make one after another on its own, each a
/// copy of the whole project (Apply Default Transitions, Automate to
/// Sequence, Sequence From Clip).
const MAX_INNER_EDITS: usize = 256;
/// The most audio sample frames the analyses of one call walk: an hour at
/// 48 kHz (decoded, mixed and measured on this thread). A peak scan reads
/// its clips' sources, loudness matching and ducking their signals, Normalize
/// Mix Track the whole mix up to four times.
const MAX_ANALYSIS_SAMPLES: f64 = 3_600.0 * 48_000.0;
/// The most audio samples one analysis holds at once: Remix reads its clip
/// whole, audio sync up to 30 minutes of each clip. Five minutes of stereo at
/// 48 kHz with its mono mix (about 170 MB of f32).
const MAX_HELD_SAMPLES: f64 = 300.0 * 48_000.0 * 3.0;
/// The most clips the render bar may hash in one command
/// ([`preview_work`]): a few seconds of serialising, where an everyday
/// timeline is thousands.
const MAX_PREVIEW_HASHES: f64 = 500_000.0;
/// The largest caption file the door reads (the door's own read cap is
/// 64 MiB): a feature film's subtitles are a few hundred KB, and every
/// caption is a project element kept in every undo copy.
const MAX_CAPTION_BYTES: u64 = 16 << 20;

/// What one door call has asked the engine for so far, and its ceilings:
/// checked before each command runs ([`Caps::before`]) and on the project
/// as the call opens it and after every command ([`Caps::after`]).
struct Caps {
    /// The project's [`elements`] and [`points`] as the call opened it.
    elements: u64,
    points: u64,
    /// Every sequence's frame size, rate and sample rate before the command
    /// that runs next.
    shapes: BTreeMap<ItemId, Shape>,
    /// Frames and audio sample frames the call's analyses have walked.
    frames: f64,
    samples: f64,
}

/// A sequence's width, height, frame rate and sample rate.
type Shape = (u32, u32, FrameRate, u32);

impl Caps {
    /// The ceilings of a call over the project `s` opened, which must itself
    /// be within what the door allows a project to hold ([`bounded`]).
    fn open(s: &mut Session) -> Result<Caps, String> {
        trim_history(s);
        bounded(&s.project)?;
        Ok(Caps { elements: elements(&s.project), points: points(&s.project), shapes: shapes(&s.project), frames: 0.0, samples: 0.0 })
    }

    /// What command `id` would ask for, before it runs: what it copies of the
    /// project within the call's size ceiling, the edits it makes one after
    /// another, the frames and samples its analysis walks or holds within the
    /// call's budgets, the clips the render bar hashes, the transcript words
    /// it walks, the frame scopes render and the caption file it reads.
    fn before(&mut self, s: &Session, door: &Door, id: &str, params: &Json) -> Result<(), String> {
        let p = &*s.project;
        self.shapes = shapes(p);
        let ceiling = self.elements + MAX_ADDED_ELEMENTS;
        let grown = elements(p) as f64 * door.copies(id, params) + copied(s, id) as f64;
        if over(grown, ceiling as f64) {
            return Err(format!(
                "it would bring the project to {grown:.0} clips, items and markers, more than the {ceiling} this call may reach ({} as opened, and {MAX_ADDED_ELEMENTS} more)",
                self.elements
            ));
        }
        inner_edits(s, id, params)?;
        let spend = analysis(s, id, params);
        if over(spend.held, MAX_HELD_SAMPLES) {
            return Err(format!(
                "its analysis would hold {:.0} audio samples at once, more than the {MAX_HELD_SAMPLES} the door allows (five minutes of stereo at 48 kHz)",
                spend.held
            ));
        }
        if over(self.frames + spend.frames, MAX_ANALYSIS_FRAMES) {
            return Err(format!(
                "its analysis would decode {:.0} frames, and this call's analyses decode at most {MAX_ANALYSIS_FRAMES} ({:.0} so far)",
                spend.frames, self.frames
            ));
        }
        if over(self.samples + spend.samples, MAX_ANALYSIS_SAMPLES) {
            return Err(format!(
                "its analysis would walk {:.0} audio samples, and this call's analyses walk at most {MAX_ANALYSIS_SAMPLES} (an hour at 48 kHz; {:.0} so far)",
                spend.samples, self.samples
            ));
        }
        if spend.points != 0.0 {
            let (now, ceiling) = (points(p) as f64, (self.points + MAX_ADDED_POINTS) as f64);
            if over(now + spend.points, ceiling) {
                return Err(format!(
                    "it would add up to {:.0} keyframes and pen points, and this call may reach {ceiling} ({} as opened, and {MAX_ADDED_POINTS} more)",
                    spend.points, self.points
                ));
            }
        }
        self.frames += spend.frames;
        self.samples += spend.samples;
        preview_work(s, id)?;
        words_walked(s, id)?;
        scope_size(s, id, params)?;
        caption_file(id, params)
    }

    /// The project after a command: its history trimmed, no job left running,
    /// within the call's size ceilings, every sequence the command made or
    /// changed within the door's frame, rate and sample-rate caps, and the
    /// whole of it within what the door allows a project to hold
    /// ([`bounded`]).
    fn after(&mut self, s: &mut Session) -> Result<(), String> {
        trim_history(s);
        no_jobs(s)?;
        let p = &*s.project;
        let (n, ceiling) = (elements(p), self.elements + MAX_ADDED_ELEMENTS);
        if n > ceiling {
            return Err(format!(
                "the project now holds {n} clips, items and markers, more than the {ceiling} this call may reach ({} as opened, and {MAX_ADDED_ELEMENTS} more)",
                self.elements
            ));
        }
        let (n, ceiling) = (points(p), self.points + MAX_ADDED_POINTS);
        if n > ceiling {
            return Err(format!(
                "the project now holds {n} keyframes and pen points, more than the {ceiling} this call may reach ({} as opened, and {MAX_ADDED_POINTS} more)",
                self.points
            ));
        }
        for (id, q) in sequences(p) {
            let st = &q.settings;
            if self.shapes.get(&id) == Some(&(st.width, st.height, st.frame_rate, st.sample_rate)) {
                continue;
            }
            let (w, h, fps) = (f64::from(st.width), f64::from(st.height), st.frame_rate.as_f64());
            if w.max(h) > MAX_SIDE || w * h > MAX_PIXELS || over(fps, MAX_FPS) || f64::from(st.sample_rate) > MAX_SAMPLE_RATE {
                return Err(format!(
                    "sequence `{}` is {}×{} at {} fps and {} Hz; a sequence the door makes or changes is at most {MAX_SIDE} pixels a side and {MAX_PIXELS} in all, {MAX_FPS} fps and {MAX_SAMPLE_RATE} Hz",
                    p.item(id).map_or("", |i| i.name.as_str()),
                    st.width,
                    st.height,
                    st.frame_rate.label(),
                    st.sample_rate
                ));
            }
        }
        bounded(p)
    }
}

/// Keep the session's undo history to [`DOOR_HISTORY`] steps (the engine's
/// project commands set its own 200 again).
fn trim_history(s: &mut Session) {
    s.history.limit = DOOR_HISTORY;
    for steps in [&mut s.history.undo, &mut s.history.redo] {
        let extra = steps.len().saturating_sub(DOOR_HISTORY);
        steps.drain(..extra);
    }
}

/// No job left running past the command (the door waits for the ones it
/// knows, [`BACKGROUND_JOBS`]): any other is cancelled and refuses the call.
fn no_jobs(s: &Session) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let running = s.jobs.iter().any(|j| !j.progress.finished.load(Ordering::Relaxed));
    if running || !s.mask_jobs.is_empty() || !s.scene_jobs.is_empty() {
        for j in &s.jobs {
            j.progress.cancel.store(true, Ordering::Relaxed);
        }
        return Err("a background job is still running, and the door runs no work past its call".into());
    }
    Ok(())
}

/// Whether `n` is over `max`, or not a number at all.
fn over(n: f64, max: f64) -> bool {
    n.is_nan() || n > max
}

/// Every sequence of the project, with its item id.
fn sequences(p: &Project) -> impl Iterator<Item = (ItemId, &Sequence)> {
    p.items.iter().filter_map(|(id, item)| match &item.kind {
        ItemKind::Sequence(q) => Some((*id, &**q)),
        _ => None,
    })
}

/// Every sequence's [`Shape`].
fn shapes(p: &Project) -> BTreeMap<ItemId, Shape> {
    sequences(p).map(|(id, q)| (id, (q.settings.width, q.settings.height, q.settings.frame_rate, q.settings.sample_rate))).collect()
}

/// The project's size in what commands add and copy: its items, and every
/// track, clip, transition, caption and marker of its sequences and media.
fn elements(p: &Project) -> u64 {
    p.items.keys().map(|id| item_elements(p, *id)).sum()
}

/// One project item's [`elements`]: a sequence with everything it holds.
fn item_elements(p: &Project, id: ItemId) -> u64 {
    let Some(item) = p.item(id) else { return 0 };
    1 + match &item.kind {
        ItemKind::Sequence(q) => {
            let tracks: u64 = q
                .video_tracks
                .iter()
                .chain(&q.audio_tracks)
                .chain(&q.submix_tracks)
                .map(|t| 1 + t.items.len() as u64 + t.transitions.len() as u64 + t.items.iter().map(|i| i.markers.len() as u64).sum::<u64>())
                .sum();
            let captions: u64 = q.caption_tracks.iter().map(|t| 1 + t.captions.len() as u64).sum();
            tracks + captions + q.markers.len() as u64
        }
        ItemKind::Media(m) => m.markers.len() as u64,
        _ => 0,
    }
}

/// What a command copies of the project as it stands, which no door limit
/// measures: a paste its clipboard, a duplicate the selected items (a
/// sequence with everything in it), Simplify Sequence the active sequence.
fn copied(s: &Session, id: &str) -> u64 {
    let p = &*s.project;
    match id {
        "edit.paste" | "edit.pasteInsert" => (s.state.clipboard.len() + s.state.clipboard_markers.len()) as u64,
        "edit.duplicate" => s.state.project_selection.iter().map(|i| item_elements(p, *i)).sum(),
        "sequence.simplify" => s.state.active_sequence.map_or(0, |i| item_elements(p, i)),
        _ => 0,
    }
}

/// Every effect instance of the project: on clips, transitions, tracks, the
/// mix and source graphics.
fn effects(p: &Project) -> impl Iterator<Item = &EffectInstance> {
    let tracks = sequences(p).flat_map(|(_, q)| {
        q.video_tracks.iter().chain(&q.audio_tracks).chain(&q.submix_tracks).flat_map(|t| {
            t.items.iter().flat_map(|i| &i.effects).chain(t.transitions.iter().map(|x| &x.effect)).chain(&t.effects)
        })
    });
    let master = sequences(p).flat_map(|(_, q)| &q.master_effects);
    tracks.chain(master).chain(p.source_graphics.values().flat_map(|g| &g.layers))
}

/// A parameter's keyframes and the pen points (or curve points) of its
/// values: what tracking and keyframing add.
fn param_points(prm: &Param) -> u64 {
    let of = |v: &ParamValue| match v {
        ParamValue::Path(path) => path.vertices.len() as u64,
        ParamValue::Curve(c) => c.len() as u64,
        _ => 0,
    };
    prm.keyframes.len() as u64 + of(&prm.value) + prm.keyframes.iter().map(|k| of(&k.value)).sum::<u64>()
}

/// The project's keyframes and pen points: of every effect and mask
/// parameter, and of the mixers' automation.
fn points(p: &Project) -> u64 {
    let masks = |e: &EffectInstance| -> u64 { e.masks.iter().flat_map(|m| [&m.path, &m.feather, &m.opacity, &m.expansion]).map(param_points).sum() };
    let effect: u64 = effects(p).map(|e| e.params.values().map(param_points).sum::<u64>() + masks(e)).sum();
    let lanes: u64 = sequences(p)
        .flat_map(|(_, q)| q.video_tracks.iter().chain(&q.audio_tracks).chain(&q.submix_tracks).map(|t| &t.mixer).chain([&q.master_mixer]))
        .flat_map(|m| m.lanes.values())
        .map(param_points)
        .sum();
    effect + lanes
}

/// The largest size of a float parameter over its value and keyframes (a
/// value that is not finite counts as infinite).
fn largest(prm: Option<&Param>) -> Option<f64> {
    let prm = prm?;
    std::iter::once(&prm.value)
        .chain(prm.keyframes.iter().map(|k| &k.value))
        .filter_map(|v| match v {
            ParamValue::Float(x) if x.is_finite() => Some(x.abs()),
            ParamValue::Float(_) => Some(f64::INFINITY),
            _ => None,
        })
        .reduce(f64::max)
}

/// The smallest value of a float parameter over its value and keyframes.
fn smallest(prm: Option<&Param>) -> Option<f64> {
    let prm = prm?;
    std::iter::once(&prm.value)
        .chain(prm.keyframes.iter().map(|k| &k.value))
        .filter_map(|v| match v {
            ParamValue::Float(x) => Some(*x),
            _ => None,
        })
        .reduce(f64::min)
}

/// The most pen points a path parameter has, over its value and keyframes.
fn most_pen_points(prm: &Param) -> usize {
    std::iter::once(&prm.value)
        .chain(prm.keyframes.iter().map(|k| &k.value))
        .map(|v| match v {
            ParamValue::Path(path) => path.vertices.len(),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

/// A scale an effect gives (percent, over `scale` and `scale_width` at their
/// largest), as a factor: 1 when unset.
fn scale_of(e: &EffectInstance) -> f64 {
    ["scale", "scale_width"].iter().filter_map(|id| largest(e.param(id))).reduce(f64::max).unwrap_or(100.0) / 100.0
}

/// The largest type a clip's graphic draws, in pixels: each text layer's
/// size (and its characters' own sizes) times the layer's scale and the
/// clip's Motion scale, at their largest keyframes. The engine rasterises a
/// glyph whole at that size, whatever of it the frame shows.
fn type_size(p: &Project, it: &TrackItem) -> f64 {
    let motion = it.effect("motion").map_or(1.0, scale_of);
    let shared = p.source_graphics.get(&it.item).map(|g| g.layers.as_slice()).unwrap_or(&[]);
    it.effects
        .iter()
        .chain(shared)
        .filter(|e| e.effect == filmcraft_engine::project::graphic::TEXT_LAYER)
        .map(|e| {
            let runs = e.layer.iter().flat_map(|x| &x.runs).filter_map(|r| r.style.size).map(|s| f64::from(s.abs())).fold(0.0, f64::max);
            largest(e.param("size")).unwrap_or(100.0).max(runs) * scale_of(e) * motion
        })
        .fold(0.0, f64::max)
}

/// The pixels a clip's Warp Stabilizers analyse at once: every frame of the
/// clip at the sequence's rate (at most 20,000), each at 480 pixels wide (960
/// detailed) and the source's aspect.
fn stabilizer_pixels(p: &Project, q: &Sequence, it: &TrackItem) -> f64 {
    let (w, h) = filmcraft_engine::render::source_size(p, it.item).unwrap_or((q.settings.width, q.settings.height));
    let frames = (it.duration.seconds() * q.settings.frame_rate.sane().as_f64()).round().clamp(1.0, 20_000.0);
    it.effects
        .iter()
        .filter(|e| e.enabled && e.effect == "warp_stabilizer")
        .map(|e| {
            let target = if matches!(e.param("detailed").map(|p| &p.value), Some(ParamValue::Bool(true))) { 960.0 } else { 480.0 };
            let k = (target / f64::from(w.max(1))).min(1.0);
            frames * (f64::from(w) * k) * (f64::from(h) * k)
        })
        .sum()
}

/// Every float parameter of `e` (value and keyframes) within one range of
/// its definition's. The engine clamps a typed value to the range, but
/// `effects.setParam` takes any number, and an effect may loop by it (Long
/// Shadow doubles its reach until it covers its length, so an infinite one
/// never ends).
fn in_range(e: &EffectInstance) -> Result<(), String> {
    let Some(def) = e.def() else { return Ok(()) };
    for (id, prm) in &e.params {
        let Some(ParamKind::Float { min, max, .. }) = def.param(id).map(|d| &d.kind) else { continue };
        let slack = max - min;
        for v in std::iter::once(&prm.value).chain(prm.keyframes.iter().map(|k| &k.value)) {
            if let ParamValue::Float(x) = v {
                if !(min - slack..=max + slack).contains(x) {
                    return Err(format!(
                        "`{}` `{id}` is {x}, past the {min}..{max} the effect takes (the door allows one range beyond either end)",
                        e.effect
                    ));
                }
            }
        }
    }
    Ok(())
}

/// What the door allows a project to hold at all, checked as a call opens
/// it and after every command: no sequence or item longer than
/// [`MAX_TIMELINE`]; no frame rendering more than [`MAX_FANOUT`] sources, nor
/// a sample mixing more streams; no type larger than [`MAX_TEXT_PX`]; no
/// Warp Stabilizer analysis past [`MAX_STABILIZER_PIXELS`]; masks within
/// [`MAX_MASK_POINTS`] and [`MAX_MASK_BAND`]; every effect parameter within
/// reach of its range ([`in_range`]).
fn bounded(p: &Project) -> Result<(), String> {
    for item in p.items.values() {
        // A graphic's source is an hour long by definition.
        if matches!(item.kind, ItemKind::Graphic { .. }) {
            continue;
        }
        let d = item.duration();
        if d > MAX_TIMELINE {
            return Err(format!("`{}` runs {:.1} hours, more than the 24 the door edits", item.name, d.seconds() / 3600.0));
        }
    }
    let mut memo = Memo::new();
    for (id, _) in sequences(p) {
        for video in [true, false] {
            let n = fanout(p, id, video, 0, &mut memo);
            if over(n, MAX_FANOUT) {
                let (unit, does, what) = if video { ("frame", "renders", "sources") } else { ("sample", "mixes", "streams") };
                return Err(format!(
                    "a {unit} of `{}` {does} up to {n} {what} (through nests, time effects, Write-on and fast clips), more than the {MAX_FANOUT} the door allows",
                    p.item(id).map_or("", |i| i.name.as_str()),
                ));
            }
        }
    }
    for (_, q) in sequences(p) {
        for it in q.video_tracks.iter().flat_map(|t| &t.items) {
            let size = type_size(p, it);
            if over(size, MAX_TEXT_PX) {
                return Err(format!(
                    "`{}` draws type {size:.0} pixels tall (the layer's size × its scale × the clip's Motion scale), more than the {MAX_TEXT_PX} the door renders",
                    it.name
                ));
            }
            let pixels = stabilizer_pixels(p, q, it);
            if over(pixels, MAX_STABILIZER_PIXELS) {
                return Err(format!(
                    "the Warp Stabilizer of `{}` would analyse {pixels:.0} pixels at once (every frame of the clip), more than the {MAX_STABILIZER_PIXELS} the door allows; stabilize a shorter clip",
                    it.name
                ));
            }
            for m in it.effects.iter().flat_map(|e| &e.masks) {
                let points = most_pen_points(&m.path) as f64;
                let band = largest(Some(&m.feather)).unwrap_or(0.0).max(largest(Some(&m.expansion)).unwrap_or(0.0));
                if points > MAX_MASK_POINTS || over(band, MAX_MASK_BAND) {
                    return Err(format!(
                        "a mask of `{}` has {points} pen points and a {band}-pixel feather or expansion; the door allows {MAX_MASK_POINTS} and {MAX_MASK_BAND}",
                        it.name
                    ));
                }
            }
        }
    }
    effects(p).try_for_each(in_range)
}

/// Sources computed so far: (sequence, video, nesting depth) → [`fanout`].
type Memo = BTreeMap<(ItemId, bool, u32), f64>;

/// The most sources one frame of sequence `id` renders (`video`), or the
/// most streams one sample of it mixes, at nesting `depth`: per track the
/// heaviest clip (or the two clips of a transition), summed over the tracks.
/// Past the engine's nesting depth a nest renders nothing.
fn fanout(p: &Project, id: ItemId, video: bool, depth: u32, memo: &mut Memo) -> f64 {
    if depth > filmcraft_engine::render::MAX_NEST_DEPTH {
        return 0.0;
    }
    if let Some(n) = memo.get(&(id, video, depth)) {
        return *n;
    }
    let Some(ItemKind::Sequence(q)) = p.item(id).map(|i| &i.kind) else { return 1.0 };
    let mut n = if video { 0.0 } else { q.submix_tracks.len() as f64 };
    for t in if video { &q.video_tracks } else { &q.audio_tracks } {
        n += track_fanout(p, t, video, depth, memo);
    }
    memo.insert((id, video, depth), n);
    n
}

/// One track's share of [`fanout`]: its heaviest clip, or transition.
fn track_fanout(p: &Project, t: &Track, video: bool, depth: u32, memo: &mut Memo) -> f64 {
    let mut most = 0.0f64;
    for it in &t.items {
        most = most.max(clip_fanout(p, it, video, depth, memo));
    }
    for x in &t.transitions {
        let mut both = 0.0;
        for c in [x.from, x.to].into_iter().flatten() {
            if let Some(it) = t.item(c) {
                both += clip_fanout(p, it, video, depth, memo);
            }
        }
        most = most.max(both);
    }
    most
}

/// One clip's share of [`fanout`]: a media clip decodes up to `speed` source
/// frames per frame (and reads its sound that much faster), a nest renders
/// its sequence (a multi-camera clip only its angle's track), and its time
/// effects and Write-on add their work ([`extra_renders`]).
fn clip_fanout(p: &Project, it: &TrackItem, video: bool, depth: u32, memo: &mut Memo) -> f64 {
    let speed = if it.speed.is_finite() { it.speed.abs().max(1.0) } else { f64::INFINITY };
    let base = match p.item(it.item).map(|i| &i.kind) {
        Some(ItemKind::Sequence(nested)) => {
            let angle = if video { it.multicam_angle(nested) } else { None };
            let inner = match angle {
                Some(a) if depth < filmcraft_engine::render::MAX_NEST_DEPTH => {
                    nested.angle_video_track_index(a).and_then(|i| nested.video_tracks.get(i)).map_or(0.0, |t| track_fanout(p, t, video, depth + 1, memo))
                }
                Some(_) => 0.0,
                None => fanout(p, it.item, video, depth + 1, memo),
            };
            if video { inner } else { inner * speed }
        }
        _ => speed,
    };
    let renders = if video {
        let (w, h) = filmcraft_engine::render::source_size(p, it.item).unwrap_or((1920, 1080));
        let frame = f64::from(w.max(1)) * f64::from(h.max(1));
        1.0 + it.effects.iter().filter(|e| e.enabled).map(|e| extra_renders(e, it.duration.seconds(), frame)).sum::<f64>()
    } else {
        1.0
    };
    renders * base
}

/// The frames' worth of work an effect of a clip `seconds` long, whose
/// picture is `frame` pixels, adds to each frame: Echo renders `count` more
/// frames of the clip (at most 30, without its other time effects),
/// Posterize Time and Auto Reframe another; Write-on stamps its brush once
/// per `spacing` of the clip so far (at most 20,000 times), each over the
/// brush's square within the picture, counted in frames of the door's
/// largest size.
fn extra_renders(e: &EffectInstance, seconds: f64, frame: f64) -> f64 {
    let most = |id: &str| largest(e.param(id)).unwrap_or_else(|| e.f64_at(id, Tick::ZERO).abs());
    let n = match e.effect.as_str() {
        "echo" => most("count").clamp(0.0, 30.0),
        "posterize_time" | "auto_reframe" => 1.0,
        "write_on" => {
            let spacing = smallest(e.param("spacing")).unwrap_or_else(|| e.f64_at("spacing", Tick::ZERO)).max(0.001);
            let stamps = (seconds.max(0.0) / spacing + 1.0).min(20_000.0);
            stamps * (most("size") + 2.0).powi(2).min(frame) / MAX_RENDER_PIXELS
        }
        _ => 0.0,
    };
    if n.is_nan() { f64::INFINITY } else { n }
}

/// Commands that make one edit after another, each a copy of the whole
/// project: Apply Default Transitions two per selected clip (or one per edit
/// point), Automate to Sequence a placement and up to two transitions per
/// item, Sequence From Clip a placement per item.
fn inner_edits(s: &Session, id: &str, p: &Json) -> Result<(), String> {
    let listed = |key: &str, selected: usize| p.get(key).and_then(Json::as_array).map_or(selected, Vec::len);
    let n = match id {
        "trim.applyDefaultTransition" => match listed("clips", s.state.selection.len()) {
            0 => s.state.edit_points.len(),
            clips => 2 * clips,
        },
        "clip.automateToSequence" => 3 * listed("items", s.state.project_selection.len()),
        "file.newSequenceFromClip" => listed("items", s.state.project_selection.len()),
        _ => return Ok(()),
    };
    if n > MAX_INNER_EDITS {
        return Err(format!(
            "it would make {n} edits one after another (each copies the whole project), more than the {MAX_INNER_EDITS} the door allows in one command; give fewer clips or items"
        ));
    }
    Ok(())
}

/// What an analysis command would walk or hold, worked out from the session
/// before it runs.
#[derive(Default)]
struct Spend {
    /// Video frames it decodes.
    frames: f64,
    /// Audio sample frames it walks, at the rate it reads them.
    samples: f64,
    /// Audio samples (all channels) it holds at once.
    held: f64,
    /// Keyframes and pen points it adds.
    points: f64,
}

/// A number as the engine's `u64_p` reads it (floats truncated).
fn engine_u64(p: &Json, key: &str) -> Option<u64> {
    p.get(key).and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
}

/// Timeline clips a command names: `clips` (a list), else `clip` when it
/// takes one, else the selection (`clips_p`, `targets`).
fn named(s: &Session, p: &Json, clip: bool) -> Vec<ClipId> {
    match p.get("clips").and_then(Json::as_array) {
        Some(a) => a.iter().filter_map(Json::as_u64).map(ClipId).collect(),
        None => match engine_u64(p, "clip").filter(|_| clip) {
            Some(c) => vec![ClipId(c)],
            None => s.state.selection.clone(),
        },
    }
}

/// `clips` with every clip linked to one of them (what Linked Selection
/// adds; counted whether it is on or not).
fn with_partners(q: &Sequence, mut clips: Vec<ClipId>) -> Vec<ClipId> {
    let links: Vec<u64> = clips.iter().filter_map(|c| q.find_item(*c).and_then(|(_, i)| i.link)).collect();
    for i in q.all_tracks().flat_map(|t| &t.items) {
        if i.link.is_some_and(|l| links.contains(&l)) && !clips.contains(&i.id) {
            clips.push(i.id);
        }
    }
    clips
}

/// The clips of `tracks` among `clips`.
fn among<'a>(tracks: &'a [Track], clips: &'a [ClipId]) -> impl Iterator<Item = &'a TrackItem> {
    tracks.iter().flat_map(|t| &t.items).filter(move |i| clips.contains(&i.id))
}

/// Audio channels of the media an item shows (2 when unknown).
fn channels(p: &Project, item: ItemId) -> f64 {
    p.resolve_media(item).and_then(|(_, m, _)| m.info.audio.as_ref().map(|a| f64::from(a.channels.max(1)))).unwrap_or(2.0)
}

/// What analysis command `id` would walk or hold (nothing for any other
/// command), from the clips it would act on as the engine picks them:
///
/// - Scene Edit Detection decodes every frame of its video clips at the
///   sequence's rate;
/// - mask tracking decodes one source frame per step (to the clip's end
///   without `frames`) and keeps a path keyframe for each;
/// - a peak scan (`clip.audioPeak`, Normalize in `clip.audioGain`) reads its
///   clips' sources at their speed; Auto-Match renders its typed clips' signal
///   and ducking each music clip's overlapping triggers; Normalize Mix Track
///   mixes the whole sequence up to four times;
/// - Remix reads its clip's whole analysed range into memory at once;
/// - audio sync (Synchronize, Merge Clips, Multi-Camera) reads up to 30
///   minutes of every clip at 48 kHz, the reference's and one other's at once.
fn analysis(s: &Session, id: &str, p: &Json) -> Spend {
    let mut out = Spend::default();
    let (Some(q), pr) = (s.active_sequence(), &*s.project) else { return out };
    let rate = f64::from(q.settings.sample_rate.max(1));
    let samples = |d: Tick| d.seconds().max(0.0) * rate;
    let audio_sync = || p["method"].as_str().is_some_and(|m| matches!(m.to_ascii_lowercase().as_str(), "audio" | "sound" | "waveform"));
    match id {
        "clip.sceneEditDetection" => {
            let frame = q.settings.frame_rate.frame_duration().0.max(1) as f64;
            for it in among(&q.video_tracks, &named(s, p, true)) {
                out.frames += (it.duration.0 as f64 / frame).floor().max(1.0);
            }
        }
        "masks.track" => {
            let video = |c: &ClipId| q.video_tracks.iter().find_map(|t| t.item(*c));
            let clip = engine_u64(p, "clip").map(ClipId).or(s.state.selected_mask.map(|m| m.clip)).or_else(|| s.state.selection.iter().copied().find(|c| video(c).is_some()));
            if let Some(it) = clip.as_ref().and_then(video) {
                let frame = pr.item(it.item).map_or(q.settings.frame_rate, |i| i.frame_rate()).frame_duration().0.max(1) as f64;
                let span = (it.duration.0 as f64 * it.speed.abs() / frame).max(0.0);
                let steps = engine_u64(p, "frames").map_or(span, |n| span.min(n as f64));
                let pen = it.effects.iter().flat_map(|e| &e.masks).map(|m| most_pen_points(&m.path)).max().unwrap_or(0);
                out.frames += steps;
                out.points += steps * (1.0 + pen as f64);
            }
        }
        "clip.audioPeak" | "clip.audioGain" => {
            if id == "clip.audioGain" && !matches!(p["mode"].as_str(), Some("normalizeMax" | "normalizeAll")) {
                return out;
            }
            for it in among(&q.audio_tracks, &with_partners(q, named(s, p, false))) {
                out.samples += samples(it.duration) * it.speed.abs();
            }
        }
        "essentialSound.autoMatch" | "essentialSound.generateDucking" => {
            let all: Vec<&TrackItem> = q.audio_tracks.iter().flat_map(|t| &t.items).collect();
            for it in among(&q.audio_tracks, &with_partners(q, named(s, p, true))).filter(|i| i.essential.is_some()) {
                let triggers = match id {
                    "essentialSound.generateDucking" => all.iter().filter(|o| o.id != it.id && o.enabled && o.range().overlaps(&it.range())).count(),
                    _ => 0,
                };
                out.samples += samples(it.duration) * it.speed.abs().max(1.0) * (1 + triggers) as f64;
            }
        }
        "sequence.normalizeMixTrack" => {
            if let Some(seq) = s.state.active_sequence {
                out.samples += 4.0 * samples(q.duration()) * fanout(pr, seq, false, 0, &mut Memo::new()).max(1.0);
            }
        }
        "clip.remix" | "clip.remix.enable" | "clip.remix.properties" => {
            let changes = ["duration", "seconds", "frame", "frames", "timecode", "segments", "variations"].iter().any(|k| p.get(*k).is_some());
            let audio = |c: &ClipId| q.audio_tracks.iter().find_map(|t| t.item(*c));
            let clip = engine_u64(p, "clip").map(ClipId).or_else(|| s.state.selection.iter().copied().find(|c| audio(c).is_some()));
            if let Some(it) = clip.as_ref().and_then(audio) {
                let remix = filmcraft_engine::render::remix::Remix::of(it);
                let analyses = match id {
                    "clip.remix.enable" => remix.is_none(),
                    "clip.remix.properties" => changes,
                    _ => true,
                };
                if analyses {
                    let read = samples(remix.map_or(it.duration, |r| r.original).max(it.duration));
                    out.samples += read;
                    out.held = read * (channels(pr, it.item) + 1.0);
                }
            }
        }
        "clip.synchronize" | "clip.createMulticam" | "clip.mergeClips" if audio_sync() => {
            let longest = filmcraft_engine::sync::MAX_ANALYSIS;
            let at48 = |d: Tick| d.min(longest).seconds().max(0.0) * f64::from(filmcraft_engine::sync::ANALYSIS_RATE);
            let reads: Vec<(f64, f64)> = match id {
                "clip.synchronize" => {
                    let clips = with_partners(q, named(s, p, true));
                    q.all_tracks().flat_map(|t| &t.items).filter(|i| clips.contains(&i.id)).map(|i| (at48(i.source_out() - i.source_in), channels(pr, i.item))).collect()
                }
                _ => {
                    let items: Vec<ItemId> = match p.get("items").and_then(Json::as_array) {
                        Some(a) => a.iter().filter_map(Json::as_u64).map(ItemId).collect(),
                        None => s.state.project_selection.clone(),
                    };
                    items
                        .iter()
                        .filter_map(|i| pr.resolve_media(*i).map(|(_, m, sub)| (at48(sub.map_or(m.duration(), |r| r.duration)), channels(pr, *i))))
                        .collect()
                }
            };
            out.samples += reads.iter().map(|r| r.0).sum::<f64>();
            let mut held: Vec<f64> = reads.iter().map(|(n, ch)| n * (ch + 1.0)).collect();
            held.sort_by(|a, b| b.total_cmp(a));
            out.held = held.iter().take(2).sum();
        }
        _ => {}
    }
    out
}

/// Clips one nest of sequence `id` makes the render bar hash at nesting
/// `depth`: every clip and transition of every video track, through nests
/// (`hash_source` and `decode_cost_ms` visit them all, without memory, to
/// eight levels).
fn nested_clips(p: &Project, id: ItemId, depth: u32, memo: &mut BTreeMap<(ItemId, u32), f64>) -> f64 {
    if depth > 8 {
        return 0.0;
    }
    if let Some(n) = memo.get(&(id, depth)) {
        return *n;
    }
    let Some(ItemKind::Sequence(q)) = p.item(id).map(|i| &i.kind) else { return 0.0 };
    let mut n = 0.0;
    for t in &q.video_tracks {
        n += t.transitions.len() as f64;
        for it in &t.items {
            n += 1.0 + nested_clips(p, it.item, depth + 1, memo);
        }
    }
    memo.insert((id, depth), n);
    n
}

/// `sequence.renderBar` cuts the active sequence's video into segments (at
/// every clip and transition edge) and hashes every layer of each, a nest
/// with everything it holds: segments × the heaviest layer of every track,
/// at most [`MAX_PREVIEW_HASHES`].
fn preview_work(s: &Session, id: &str) -> Result<(), String> {
    let (Some(seq), Some(q)) = (s.state.active_sequence.filter(|_| id == "sequence.renderBar"), s.active_sequence()) else { return Ok(()) };
    let p = &*s.project;
    let mut memo = BTreeMap::new();
    let tracks: Vec<&Track> = q.video_tracks.iter().filter(|t| t.enabled).collect();
    let cuts: f64 = tracks.iter().map(|t| 2.0 * (t.items.len() + t.transitions.len()) as f64).sum();
    let per_segment: f64 = tracks.iter().map(|t| 2.0 * t.items.iter().map(|it| 1.0 + nested_clips(p, it.item, 1, &mut memo)).fold(0.0, f64::max)).sum();
    let work = cuts * per_segment;
    if over(work, MAX_PREVIEW_HASHES) {
        let name = p.item(seq).map_or("", |i| i.name.as_str());
        return Err(format!("its render bar of `{name}` would hash up to {work} clips (every segment's layers, nests with everything in them), more than the {MAX_PREVIEW_HASHES} the door allows"));
    }
    Ok(())
}

/// The transcript commands that map the active sequence's words to the
/// timeline (`sequence_words`) walk every word of every transcribed audio
/// clip's range: at most [`MAX_WORDS`].
fn words_walked(s: &Session, id: &str) -> Result<(), String> {
    const WALKS: [&str; 8] = [
        "transcript.createCaptions",
        "transcript.extract",
        "transcript.inspect",
        "transcript.lift",
        "transcript.removeFillers",
        "transcript.removePauses",
        "transcript.search",
        "transcript.select",
    ];
    let Some(q) = s.active_sequence().filter(|_| WALKS.contains(&id)) else { return Ok(()) };
    let words: f64 = q
        .audio_tracks
        .iter()
        .flat_map(|t| &t.items)
        .filter_map(|it| {
            let range = TimeRange::from_bounds(it.source_in, it.source_out().max(it.source_in));
            s.project.transcripts.get(&it.item).map(|tr| tr.words_in(range).len() as f64)
        })
        .sum();
    if over(words, MAX_WORDS) {
        return Err(format!("it would walk {words} transcript words of the sequence's clips, more than the {MAX_WORDS} the door allows"));
    }
    Ok(())
}

/// `scopes.read` renders the active sequence at `scale` (default ½, at most
/// 1): at most [`MAX_RENDER_PIXELS`].
fn scope_size(s: &Session, id: &str, p: &Json) -> Result<(), String> {
    let Some(q) = s.active_sequence().filter(|_| id == "scopes.read") else { return Ok(()) };
    let scale = p.get("scale").and_then(Json::as_f64).unwrap_or(0.5).clamp(1.0 / 32.0, 1.0);
    let (w, h) = ((f64::from(q.settings.width) * scale).round(), (f64::from(q.settings.height) * scale).round());
    if over(w * h, MAX_RENDER_PIXELS) {
        return Err(format!("it would render a {w}×{h} frame, more than the {MAX_RENDER_PIXELS} pixels the door renders at once; give a smaller `scale`"));
    }
    Ok(())
}

/// The caption file `captions.import` reads (its `path`, made absolute by
/// the door): at most [`MAX_CAPTION_BYTES`].
fn caption_file(id: &str, p: &Json) -> Result<(), String> {
    let Some(path) = p["path"].as_str().filter(|_| id == "captions.import") else { return Ok(()) };
    let len = std::fs::metadata(path).map_or(0, |m| m.len());
    if len > MAX_CAPTION_BYTES {
        return Err(format!("the caption file is {len} bytes, and the door reads at most {MAX_CAPTION_BYTES} bytes of captions"));
    }
    Ok(())
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

    // -----------------------------------------------------------------------
    // the door's caps
    // -----------------------------------------------------------------------

    /// `n` pen points, as a mask `path` lists them.
    fn pen(n: usize) -> Json {
        Json::Array(vec![json!([10.0, 10.0]); n])
    }

    /// Parameters with the one `key`.
    fn one(key: &str, value: impl Into<Json>) -> Json {
        Json::Object(serde_json::Map::from_iter([(key.to_string(), value.into())]))
    }

    /// Every limit of the door: the call at its cap is admitted, one past it
    /// is refused before anything runs, naming the command and what it
    /// counts. The table covers every `(id, what)` of [`LIMITS`].
    #[test]
    fn every_limit_holds_at_its_cap_and_refuses_one_past_it() {
        let ticks = MAX_TICKS as i64;
        let tick_over = ticks + TICKS_PER_SECOND;
        let frames = MAX_FRAMES as i64;
        let mut rows: Vec<(&str, Json, Json, &str)> = Vec::new();
        for id in ["file.newSequence", "sequence.settings", "file.newOfflineFile"] {
            rows.push((id, json!({"width": 4096, "height": 2304}), json!({"width": 4096, "height": 2305}), "pixels"));
            rows.push((id, json!({"width": 16, "height": 4096}), json!({"width": 16, "height": 4097}), "pixels on a side"));
            rows.push((id, json!({"fps": 120}), json!({"fps": 120.01}), "frames per second"));
            rows.push((id, json!({"sampleRate": 96_000}), json!({"sampleRate": 96_001}), "samples per second"));
        }
        rows.push(("file.newOfflineFile", json!({"channels": 32}), json!({"channels": 33}), "channels"));
        for id in ["file.newBarsAndTone", "file.newBlackVideo", "file.newColorMatte", "file.newCountingLeader", "file.newTransparentVideo"] {
            rows.push((id, json!({"width": 2304, "height": 4096}), json!({"width": 2305, "height": 4096}), "pixels"));
            rows.push((id, json!({"width": 4096, "height": 16}), json!({"width": 4097, "height": 16}), "pixels on a side"));
        }
        for id in [
            "file.newOfflineFile",
            "file.newBarsAndTone",
            "file.newBlackVideo",
            "file.newColorMatte",
            "file.newCountingLeader",
            "file.newTransparentVideo",
            "file.newAdjustmentLayer",
            "clip.insertFrameHoldSegment",
            "graphics.newText",
            "graphics.newVerticalText",
            "graphics.newShape",
            "graphics.newRectangle",
            "graphics.newEllipse",
            "graphics.newPolygon",
            "graphics.resetDuration",
            "clip.remix",
            "clip.remix.properties",
        ] {
            rows.push((id, json!({"seconds": 86_400}), json!({"seconds": 86_400.5}), "seconds"));
        }
        rows.push(("captions.add", json!({"durationSeconds": 86_400}), json!({"durationSeconds": 86_401}), "seconds"));
        rows.push(("clip.speedDuration", json!({"speed": 10_000}), json!({"speed": 10_001}), "percent"));
        rows.push(("clip.speedDuration", json!({"speed": 1}), json!({"speed": 0.99}), "times the clip's length (a speed under 1 %)"));
        for (id, key) in [
            ("timeline.rateStretch", "delta"),
            ("markers.addRange", "duration"),
            ("timeline.place", "duration"),
            ("clip.remix", "duration"),
            ("clip.remix.properties", "duration"),
            ("timeline.trim", "delta"),
            ("captions.trim", "delta"),
            ("captions.move", "delta"),
        ] {
            rows.push((id, one(key, ticks), one(key, -tick_over), "ticks"));
        }
        for (id, key) in [
            ("timeline.rateStretch", "deltaFrames"),
            ("markers.add", "durationFrames"),
            ("markers.edit", "durationFrames"),
            ("markers.addFlashCue", "durationFrames"),
            ("markers.addRange", "durationFrames"),
            ("sequence.applyVideoTransition", "frames"),
            ("sequence.applyAudioTransition", "frames"),
            ("clip.automateToSequence", "stillFrames"),
            ("clip.automateToSequence", "overlapFrames"),
            ("clip.remix", "frame"),
            ("clip.remix", "frames"),
            ("clip.remix.properties", "frame"),
            ("clip.remix.properties", "frames"),
            ("timeline.trim", "deltaFrames"),
            ("captions.trim", "deltaFrames"),
            ("captions.move", "deltaFrames"),
            ("trim.nudge", "frames"),
            ("playhead.step", "frames"),
            ("clip.synchronize", "offset"),
            ("clip.createMulticam", "offset"),
            ("clip.mergeClips", "offset"),
        ] {
            rows.push((id, one(key, frames), one(key, frames + 1), "frames"));
        }
        for id in ["clip.remix", "clip.remix.properties"] {
            // A timecode duration counts each frame as a second.
            rows.push((id, json!({"timecode": "24:00:00:00"}), json!({"timecode": "24:00:00:01"}), "seconds of timecode"));
            rows.push((id, json!({"timecode": "+86400"}), json!({"timecode": "-86401"}), "seconds of timecode"));
        }
        rows.push(("masks.track", json!({"frames": 18_000}), json!({"frames": 18_001}), "frames"));
        rows.push(("graphics.newText", json!({"size": 1000}), json!({"size": 1000.5}), "pixels of type"));
        rows.push(("graphics.newVerticalText", json!({"size": 1000}), json!({"size": 1001}), "pixels of type"));
        rows.push(("graphics.set", json!({"props": {"size": 1000}}), json!({"props": {"size": 1001}}), "pixels of type"));
        rows.push(("graphics.setCharStyle", json!({"style": {"size": 1000}}), json!({"style": {"fontSize": 1001}}), "pixels of type"));
        for id in ["masks.add", "masks.set"] {
            rows.push((id, json!({"path": pen(512)}), json!({"path": {"vertices": pen(513)}}), "pen points"));
        }
        let words = |n: usize| json!({"transcript": {"language": "en", "speakers": [], "words": vec![json!({"text": "w", "start": 0, "end": 1}); n]}});
        rows.push(("transcript.set", words(30_000), words(30_001), "words"));
        rows.push(("clip.audioChannels", json!({"clips": vec![json!([0]); 32]}), json!({"clips": vec![json!([0]); 33]}), "audio clips per placement"));
        rows.push(("transcript.createCaptions", json!({"minSeconds": 86_400}), json!({"minSeconds": 86_401}), "seconds"));
        rows.push(("transcript.createCaptions", json!({"maxSeconds": 86_400}), json!({"maxSeconds": 86_401}), "seconds"));
        rows.push(("transcript.createCaptions", json!({"gapFrames": frames}), json!({"gapFrames": frames + 1}), "frames"));
        rows.push(("transcript.createCaptions", json!({"maxChars": 5_000, "lines": 2}), json!({"maxChars": 5_001, "lines": 2}), "characters in a caption"));
        rows.push(("masks.set", json!({"feather": 1000}), json!({"feather": 1001}), "pixels of feather"));
        rows.push(("masks.set", json!({"expansion": -1000}), json!({"expansion": -1001}), "pixels of expansion"));
        for id in ["effects.setParam", "effects.setKeyframe"] {
            rows.push((id, json!({"mask": 0, "param": "feather", "value": 1000}), json!({"mask": 0, "param": "expansion", "value": -1001}), "pixels of mask feather or expansion"));
            rows.push((id, json!({"mask": 0, "param": "path", "value": pen(512)}), json!({"mask": 0, "param": "path", "value": pen(513)}), "pen points"));
        }
        let door = door().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        for (id, at, over, what) in &rows {
            assert!(door.admit(id, at, &area).is_ok(), "{id} {at}: {:?}", door.admit(id, at, &area));
            let e = door.admit(id, over, &area).unwrap_err();
            assert!(e.starts_with(&format!("film.run: `{id}`")) && e.contains(what), "{id} {over}: {e}");
        }
        for l in LIMITS {
            assert!(rows.iter().any(|(id, _, _, what)| *id == l.id && *what == l.what), "no row for {} ({})", l.id, l.what);
        }
        // Nothing asked is the engine's default (within every cap); a value that
        // is not a number is refused rather than read as nothing.
        assert!(door.admit("clip.speedDuration", &json!({}), &area).is_ok());
        assert!(door.admit("clip.speedDuration", &json!({"speed": "1e9"}), &area).unwrap_err().contains("percent"));
        assert!(door.admit("file.newSequence", &json!({"width": [4096]}), &area).unwrap_err().contains("is a number the door bounds"));
        // A mask parameter is limited only when the command addresses a mask.
        assert!(door.admit("effects.setParam", &json!({"param": "feather", "value": 1e9}), &area).is_ok());
        // `u64_p` truncates a generator's width to u32 (2^32 + 16 would draw
        // 16 pixels): the door counts what was asked.
        let e = door.admit("file.newBlackVideo", &json!({"width": 4_294_967_312u64, "height": 1}), &area).unwrap_err();
        assert!(e.contains("is 4294967312, more than the 9437184 pixels"), "{e}");
    }

    /// A sequence asked to be huge, or to run at a huge rate, is refused by the
    /// door; one that a command makes too large in all is refused after it.
    #[test]
    fn a_huge_frame_size_or_rate_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let run = |cmds: Json| door_run(&areas, json!({"path": clip, "cmds": cmds, "out": "x.fcproj"}), dir.path(), true);
        for (cmd, needle) in [
            (json!({"id": "sequence.settings", "params": {"width": 16384, "height": 16384}}), "is 268435456, more than the 9437184 pixels"),
            (json!({"id": "file.newSequence", "params": {"width": 32768, "height": 16}}), "asks for 32768 pixels on a side"),
            (json!({"id": "file.newSequence", "params": {"fps": 1000}}), "`fps` is 1000, more than the 120 frames per second"),
            (json!({"id": "sequence.settings", "params": {"sampleRate": 384_000}}), "more than the 96000 samples per second"),
            (json!({"id": "file.newColorMatte", "params": {"width": 4_294_967_295u64, "height": 4_294_967_295u64}}), "pixels"),
            (json!({"id": "file.newOfflineFile", "params": {"channels": 1_000_000_000}}), "more than the 32 channels"),
        ] {
            let e = run(json!([cmd])).unwrap_err();
            assert!(e.contains(needle), "{cmd}: {e}");
        }
        assert!(!dir.path().join("x.fcproj").exists(), "nothing written");
        // Within every cap on its own, but larger in all than a frame the door
        // renders once applied to the sequence's other side.
        let e = run(json!([
            {"id": "sequence.settings", "params": {"width": 4096, "height": 2304}},
            {"id": "sequence.settings", "params": {"height": 2400}},
        ]))
        .unwrap_err();
        assert!(e.starts_with("film.run sequence.settings: ") && e.contains("is 4096×2400") && e.contains("at most 4096 pixels a side and 9437184 in all"), "{e}");
        // At the caps it runs.
        let v = run(json!([{"id": "sequence.settings", "params": {"width": 4096, "height": 2304, "fps": 120, "sampleRate": 96_000}}])).unwrap();
        assert_eq!(v["out"], json!("x.fcproj"), "{v}");
    }

    /// Bars and tone `seconds` long (picture and sound, `w`×`h`) laid after
    /// the suite's clip on its sequence: the commands that make it, the item,
    /// and its picture and sound clips (a probe call learns the ids, which
    /// later calls repeat: sessions are deterministic).
    fn bars(areas: &Slot, root: &Path, clip: &str, seconds: f64, (w, h): (u32, u32)) -> (Vec<Json>, u64, u64, u64) {
        let make = json!({"id": "file.newBarsAndTone", "params": {"seconds": seconds, "width": w, "height": h}});
        let v = door_run(areas, json!({"path": clip, "cmds": [make]}), root, false).unwrap();
        let item = v["results"][0]["result"]["item"].as_u64().unwrap_or_else(|| panic!("{v}"));
        let place = json!({"id": "timeline.place", "params": {"item": item, "seconds": 1.0}});
        let v = door_run(areas, json!({"path": clip, "cmds": [make, place]}), root, false).unwrap();
        let clips = &v["results"][1]["result"]["clips"];
        let (video, audio) = (clips[0].as_u64().unwrap_or_else(|| panic!("{v}")), clips[1].as_u64().unwrap_or_else(|| panic!("{v}")));
        (vec![make, place], item, video, audio)
    }

    /// A huge frame range is refused: a marker or a subclip of billions of
    /// frames, a clip slowed to 1 % past what one export encodes, a timeline
    /// placed past a day, and a fast sequence's export past its frame cap.
    #[test]
    fn a_huge_frame_range_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let c = first_clip(&areas, dir.path(), clip);
        let run = |args: Json| door_run(&areas, args, dir.path(), true);
        let e = run(json!({"path": clip, "cmds": [{"id": "markers.add", "params": {"durationFrames": 9_000_000_000i64}}]})).unwrap_err();
        assert!(e.contains("`durationFrames` is 9000000000, more than the 10368000 frames"), "{e}");
        let e = run(json!({"path": clip, "cmds": [{"id": "clip.speedDuration", "params": {"clips": [c], "speed": 0.01}}]})).unwrap_err();
        assert!(e.contains("asks for 10000 times the clip's length"), "{e}");
        // A subclip whose end is days past its media: the item is refused.
        let e = run(json!({"path": clip, "cmds": [{"id": "clip.makeSubclip", "params": {"clip": c, "start": 0, "end": 100_000_000_000_000_000i64}}]})).unwrap_err();
        assert!(e.starts_with("film.run clip.makeSubclip: ") && e.contains("more than the 24 the door edits"), "{e}");
        // A position is not limited, but the timeline it makes is.
        let (prefix, _, video, _) = bars(&areas, dir.path(), clip, 10.0, (64, 36));
        let mut cmds = prefix.clone();
        cmds.push(json!({"id": "timeline.move", "params": {"moves": [{"clip": video, "track": "V1", "time": 90_000 * TICKS_PER_SECOND}]}}));
        let e = run(json!({"path": clip, "cmds": cmds})).unwrap_err();
        assert!(e.starts_with("film.run timeline.move: ") && e.contains("hours, more than the 24 the door edits"), "{e}");
        // 1 % of ten seconds of bars is 1000 s: past one export's 300 s.
        let mut cmds = prefix.clone();
        cmds.push(json!({"id": "clip.speedDuration", "params": {"clips": [video], "speed": 1, "ripple": true}}));
        let e = run(json!({"path": clip, "cmds": cmds, "out": "slow.mp4"})).unwrap_err();
        assert!(e.contains("at most 300000 ms per export"), "{e}");
        // Within 300 s, a 120 fps sequence still writes 36,000 frames: refused
        // before the encoder starts.
        let mut cmds = prefix;
        cmds.push(json!({"id": "clip.speedDuration", "params": {"clips": [video], "speed": 1, "ripple": true}}));
        cmds.push(json!({"id": "sequence.settings", "params": {"fps": 120}}));
        let e = run(json!({"path": clip, "cmds": cmds, "out": "fast.mp4", "end_ms": 300_000})).unwrap_err();
        assert!(e.contains("is 36000 frames, more than the 18000 one export encodes"), "{e}");
        assert!(!dir.path().join("slow.mp4").exists() && !dir.path().join("fast.mp4").exists() && no_staging_left(dir.path()));
    }

    /// What an export asks for, at and past each cap; and the frame size it
    /// encodes at.
    #[test]
    fn an_export_keeps_to_its_frame_sample_and_gif_caps() {
        let at = |fps: i64, ms: f64| (FrameRate::new(fps, 1), (0.0, ms));
        let (r, range) = at(60, 300_000.0);
        assert!(export_work("h264", range, true, r, 48_000, (4096, 2304)).is_ok(), "18,000 frames and 14.4 M samples");
        let (r, range) = at(60, 300_001.0);
        assert!(export_work("h264", range, false, r, 48_000, (16, 16)).unwrap_err().contains("frames, more than the 18000"));
        assert!(export_work("wav", range, false, r, 48_000, (16, 16)).unwrap_err().contains("samples, more than the 14400000"), "a WAV has sound, `audio` or not");
        let (r, range) = at(60, 150_000.0);
        assert!(export_work("prores", range, true, r, 96_000, (16, 16)).is_ok());
        assert!(export_work("prores", range, true, r, 96_001, (16, 16)).unwrap_err().contains("samples"));
        assert!(export_work("prores", range, false, r, 384_000, (16, 16)).is_ok(), "no sound asked: the sample rate does not matter");
        // 2^28 pixels: 4096 frames of 256 × 256; a GIF's frames are capped too.
        let (r, range) = at(60, 300_001.0);
        assert!(export_work("gif", range, false, r, 48_000, (16, 16)).unwrap_err().contains("frames, more than the 18000"));
        let (r, range) = at(16, 256_000.0);
        assert!(export_work("gif", range, true, r, 384_000, (256, 256)).is_ok(), "a GIF has no sound");
        assert!(export_work("gif", range, true, r, 384_000, (256, 257)).unwrap_err().contains("one GIF holds in memory"));
        // Scaled to at most 4096 a side and 4096 × 2304 in all, even sides.
        assert_eq!(export_size(3840, 2160), None);
        assert_eq!(export_size(4096, 2304), None);
        assert_eq!(export_size(8192, 4608), Some((4096, 2304)));
        assert_eq!(export_size(4096, 4096), Some((3072, 3072)));
        assert_eq!(export_size(16384, 16), Some((4096, 4)));
    }

    /// A project the door opens (a file as hostile as a parameter), with a
    /// sequence of `width`×`height` and an adjustment layer `seconds` long,
    /// saved by the engine into `root` as `name`.
    fn crafted(root: &Path, name: &str, (width, height): (u32, u32), seconds: f64) {
        let root = root.canonicalize().unwrap();
        let rules = Arc::new(Area::new(&root, None, true));
        let mut s = session(&root, &rules);
        s.execute("file.newSequence", json!({"width": width, "height": height})).unwrap();
        s.execute("file.newAdjustmentLayer", json!({"seconds": seconds})).unwrap();
        s.execute("file.saveCopy", json!({"path": root.join(name).to_string_lossy()})).unwrap();
    }

    /// A frame the door renders is at most 4096 × 2304 pixels, a frame scopes
    /// read too; a project file holding an item past a day is refused as it
    /// opens.
    #[test]
    fn a_rendered_frame_and_an_opened_project_keep_to_the_caps() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        crafted(dir.path(), "square.fcproj", (4096, 4096), 5.0);
        let e = door_run(&areas, json!({"path": "square.fcproj", "cmds": [], "out": "f.png", "max_side": 4096}), dir.path(), true).unwrap_err();
        assert!(e.contains("renders 4096×4096 pixels, more than the 9437184") && e.contains("smaller `max_side`"), "{e}");
        let e = door_run(&areas, json!({"path": "square.fcproj", "cmds": [{"id": "scopes.read", "params": {"scale": 1}}]}), dir.path(), true).unwrap_err();
        assert!(e.starts_with("film.run scopes.read: ") && e.contains("smaller `scale`"), "{e}");
        let v = door_run(&areas, json!({"path": "square.fcproj", "cmds": [], "out": "f.png", "max_side": 64}), dir.path(), true).unwrap();
        assert_eq!((v["width"].as_u64(), v["height"].as_u64()), (Some(64), Some(64)), "{v}");
        crafted(dir.path(), "long.fcproj", (64, 36), 90_000.0);
        let e = door_run(&areas, json!({"path": "long.fcproj", "cmds": []}), dir.path(), true).unwrap_err();
        assert!(e.starts_with("film.run: `long.fcproj`: ") && e.contains("runs 25.0 hours, more than the 24 the door edits"), "{e}");
    }

    /// A duplicate loop (select all, duplicate, again) and a paste loop (select
    /// all, copy, paste at the end, again) double the project every round: the
    /// size ceiling stops them before the copy that would pass it.
    #[test]
    fn a_duplicate_loop_stops_at_the_size_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let rounds = |cmds: &[&str], n: usize| -> Json { Json::Array((0..n).flat_map(|_| cmds.iter().map(|id| json!({"id": id}))).collect()) };
        let e = door_run(&areas, json!({"path": clip, "cmds": rounds(&["project.selectAll", "edit.duplicate"], 32), "out": "dup.fcproj"}), dir.path(), true).unwrap_err();
        assert!(e.starts_with("film.run edit.duplicate: it would bring the project to ") && e.contains("more than the 5006 this call may reach"), "{e}");
        let e = door_run(&areas, json!({"path": clip, "cmds": rounds(&["edit.selectAll", "edit.copy", "playhead.end", "edit.paste"], 16), "out": "paste.fcproj"}), dir.path(), true)
            .unwrap_err();
        assert!(e.starts_with("film.run edit.paste: it would bring the project to ") && e.contains("this call may reach"), "{e}");
        assert!(!dir.path().join("dup.fcproj").exists() && !dir.path().join("paste.fcproj").exists());
        // A few rounds are everyday editing.
        let v = door_run(&areas, json!({"path": clip, "cmds": rounds(&["edit.selectAll", "edit.copy", "playhead.end", "edit.paste"], 4), "out": "ok.fcproj"}), dir.path(), true).unwrap();
        assert_eq!(v["out"], json!("ok.fcproj"), "{v}");
    }

    /// The analyses' budgets, worked out from the clips each would walk before
    /// it runs: scene detection and mask tracking over two hours of frames,
    /// peak scans, loudness matching and the mix over two hours of sound,
    /// Remix and audio sync holding ten minutes at once.
    #[test]
    fn analyses_stay_within_the_calls_budgets() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let media = first_clip(&areas, dir.path(), clip);
        let refused = |prefix: &[Json], cmds: Json| {
            let mut all = prefix.to_vec();
            all.extend(cmds.as_array().cloned().unwrap_or_default());
            door_run(&areas, json!({"path": clip, "cmds": all}), dir.path(), true).unwrap_err()
        };
        let (long, _, video, audio) = bars(&areas, dir.path(), clip, 7200.0, (64, 36));
        let e = refused(&long, json!([{"id": "clip.sceneEditDetection", "params": {"clips": [video]}}]));
        assert!(e.starts_with("film.run clip.sceneEditDetection: its analysis would decode 172") && e.contains("at most 18000"), "{e}");
        let e = refused(&long, json!([{"id": "masks.add", "params": {"clip": video}}, {"id": "masks.track", "params": {"clip": video, "mask": 0}}]));
        assert!(e.starts_with("film.run masks.track: its analysis would decode 172") && e.contains("at most 18000"), "{e}");
        for cmd in [
            json!({"id": "clip.audioPeak", "params": {"clips": [audio]}}),
            json!({"id": "clip.audioGain", "params": {"clips": [audio], "mode": "normalizeMax", "db": -1}}),
            json!({"id": "sequence.normalizeMixTrack"}),
        ] {
            let e = refused(&long, json!([cmd]));
            assert!(e.contains("its analysis would walk") && e.contains("at most 172800000 (an hour at 48 kHz"), "{cmd}: {e}");
        }
        let e = refused(&long, json!([{"id": "essentialSound.setType", "params": {"clips": [audio], "type": "music"}}, {"id": "essentialSound.autoMatch", "params": {"clips": [audio]}}]));
        assert!(e.starts_with("film.run essentialSound.autoMatch: its analysis would walk 3455") && e.contains("at most 172800000"), "{e}");
        // An adjustment of gain is no analysis.
        let mut ok = long.clone();
        ok.push(json!({"id": "clip.audioGain", "params": {"clips": [audio], "mode": "adjust", "db": -1}}));
        door_run(&areas, json!({"path": clip, "cmds": ok}), dir.path(), true).unwrap();
        // Remix reads its clip whole; audio sync ten minutes of each clip.
        let (ten, item10, _, audio10) = bars(&areas, dir.path(), clip, 600.0, (64, 36));
        let e = refused(&ten, json!([{"id": "clip.remix", "params": {"clip": audio10, "seconds": 30}}]));
        assert!(e.starts_with("film.run clip.remix: its analysis would hold 864") && e.contains("audio samples at once, more than the 43200000"), "{e}");
        let e = refused(&ten, json!([{"id": "clip.synchronize", "params": {"clips": [audio10, media], "method": "audio"}}]));
        assert!(e.contains("would hold") && e.contains("more than the 43200000"), "{e}");
        let e = refused(&ten, json!([{"id": "clip.createMulticam", "params": {"items": [item10, item10], "method": "audio"}}]));
        assert!(e.starts_with("film.run clip.createMulticam: its analysis would hold"), "{e}");
        // The budgets are the call's: two detections of just over half of it
        // each, the second refused.
        let (half, _, video_half, _) = bars(&areas, dir.path(), clip, 376.0, (64, 36));
        let scan = json!({"id": "clip.sceneEditDetection", "params": {"clips": [video_half], "applyCuts": false, "generateMarkers": true}});
        let e = refused(&half, json!([scan, scan]));
        assert!(e.starts_with("film.run clip.sceneEditDetection: its analysis would decode 901") && e.contains("at most 18000 (901"), "{e}");
    }

    /// The engine's background jobs run within the call (the session is gone
    /// when it returns): scene detection without `wait` answers with its cuts,
    /// and leaves nothing running.
    #[test]
    fn the_engines_background_jobs_run_within_the_call() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let c = first_clip(&areas, dir.path(), clip);
        let v = door_run(&areas, json!({"path": clip, "cmds": [{"id": "clip.sceneEditDetection", "params": {"clips": [c], "applyCuts": false, "generateMarkers": true}}]}), dir.path(), false)
            .unwrap();
        let r = &v["results"][0]["result"];
        assert!(r["clips"].is_array() && r["frames"].as_u64().is_some_and(|n| n > 0), "the job ran in the call: {v}");
        let v = door_run(
            &areas,
            json!({"path": clip, "cmds": [{"id": "masks.add", "params": {"clip": c}}, {"id": "masks.track", "params": {"clip": c, "mask": 0, "frames": 3}}]}),
            dir.path(),
            false,
        )
        .unwrap();
        assert!(v["results"][1]["result"]["status"]["finished"] == json!(true) || v["results"][1]["result"]["frames"].as_u64().is_some(), "{v}");
        // The door hands the engine an object to wait with.
        let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "clip.sceneEditDetection", "params": [c]}]}), dir.path(), false).unwrap_err();
        assert!(e.contains("`params` is an object"), "{e}");
        // The fence of every command holds no job left running.
        let root = dir.path().canonicalize().unwrap();
        let rules = Arc::new(Area::new(&root, None, true));
        let mut s = session(&root, &rules);
        clip_sequence(&mut s, &root.join(clip)).unwrap();
        let mut caps = Caps::open(&mut s).unwrap();
        let progress: Arc<filmcraft_engine::export::Progress> = Default::default();
        s.jobs.push(filmcraft_engine::Job { id: 9, label: "stray".into(), progress: progress.clone(), result: Default::default() });
        assert!(caps.after(&mut s).unwrap_err().contains("background job is still running"));
        assert!(progress.cancel.load(std::sync::atomic::Ordering::Relaxed), "cancelled");
    }

    /// Type, effect parameters and time effects stay within reach: type
    /// scaled past 1000 pixels, a Motion scale of a billion percent, a ninth
    /// thirty-frame Echo on one clip.
    #[test]
    fn type_effects_and_echoes_stay_within_reach() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let c = first_clip(&areas, dir.path(), clip);
        let run = |cmds: Json| door_run(&areas, json!({"path": clip, "cmds": cmds}), dir.path(), true);
        run(json!([{"id": "graphics.newText", "params": {"text": "Hi", "size": 1000}}])).unwrap();
        let e = run(json!([{"id": "graphics.newText", "params": {"text": "Hi", "size": 500}}, {"id": "graphics.set", "params": {"props": {"scale": 300}}}])).unwrap_err();
        assert!(e.starts_with("film.run graphics.set: ") && e.contains("draws type 1500 pixels tall"), "{e}");
        // A character's own size counts, times the layer's scale.
        let styled = [
            json!({"id": "graphics.newText", "params": {"text": "Hi", "size": 200}}),
            json!({"id": "graphics.setCharStyle", "params": {"start": 0, "end": 1, "style": {"size": 900}}}),
        ];
        run(json!(styled)).unwrap();
        let mut scaled = styled.to_vec();
        scaled.push(json!({"id": "graphics.set", "params": {"props": {"scale": 120}}}));
        let e = run(Json::Array(scaled)).unwrap_err();
        assert!(e.starts_with("film.run graphics.set: ") && e.contains("draws type 1080 pixels tall"), "{e}");
        let e = run(json!([{"id": "effects.setParam", "params": {"clip": c, "effect": "motion", "param": "scale", "value": 1e9}}])).unwrap_err();
        assert!(e.starts_with("film.run effects.setParam: ") && e.contains("`motion` `scale` is 1000000000, past the 0..10000"), "{e}");
        run(json!([{"id": "effects.setParam", "params": {"clip": c, "effect": "motion", "param": "scale", "value": 20000}}])).unwrap();
        let mut cmds = Vec::new();
        for k in 0..9 {
            cmds.push(json!({"id": "effects.apply", "params": {"clips": [c], "effect": "echo"}}));
            cmds.push(json!({"id": "effects.setParam", "params": {"clip": c, "effect": k, "param": "count", "value": 30}}));
        }
        let e = run(Json::Array(cmds.clone())).unwrap_err();
        assert!(e.contains("renders up to 271 sources") && e.contains("more than the 256"), "{e}");
        run(Json::Array(cmds[..16].to_vec())).unwrap();
    }

    /// A Warp Stabilizer analyses every frame of its clip at once: a short
    /// clip renders, a minute of 1080p is refused when it is applied.
    #[test]
    fn a_warp_stabilizer_analyses_only_a_short_clip() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let c = first_clip(&areas, dir.path(), clip);
        let v = door_run(&areas, json!({"path": clip, "cmds": [{"id": "effects.apply", "params": {"clips": [c], "effect": "warp_stabilizer"}}], "out": "s.png", "max_side": 32}), dir.path(), true)
            .unwrap();
        assert_eq!(v["out"], json!("s.png"), "{v}");
        let (mut cmds, _, video, _) = bars(&areas, dir.path(), clip, 60.0, (1920, 1080));
        cmds.push(json!({"id": "effects.apply", "params": {"clips": [video], "effect": "warp_stabilizer"}}));
        let e = door_run(&areas, json!({"path": clip, "cmds": cmds}), dir.path(), true).unwrap_err();
        assert!(e.starts_with("film.run effects.apply: the Warp Stabilizer of ") && e.contains("stabilize a shorter clip"), "{e}");
    }

    /// A Write-on stamps its brush up to 20,000 times a frame: its defaults
    /// are a small share of a frame's work, the largest brush stamped every
    /// millisecond of a minute of 1080p is refused.
    #[test]
    fn a_write_on_counts_its_stamps_toward_the_frames_work() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let (mut cmds, _, video, _) = bars(&areas, dir.path(), clip, 60.0, (1920, 1080));
        cmds.push(json!({"id": "effects.apply", "params": {"clips": [video], "effect": "write_on"}}));
        door_run(&areas, json!({"path": clip, "cmds": cmds}), dir.path(), true).unwrap();
        cmds.push(json!({"id": "effects.setParam", "params": {"clip": video, "effect": "write_on", "param": "size", "value": 500}}));
        cmds.push(json!({"id": "effects.setParam", "params": {"clip": video, "effect": "write_on", "param": "spacing", "value": 0.001}}));
        let e = door_run(&areas, json!({"path": clip, "cmds": cmds}), dir.path(), true).unwrap_err();
        assert!(e.starts_with("film.run effects.setParam: a frame of `Film` renders up to 535") && e.contains("more than the 256"), "{e}");
    }

    /// The transcript commands walk every word their sequence's clips show:
    /// one transcript shown twice past the cap is refused before they run.
    #[test]
    fn the_transcript_words_a_command_walks_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let v = door_run(&areas, json!({"path": clip, "cmds": [{"id": "sequence.inspect"}]}), dir.path(), false).unwrap();
        let item = v["results"][0]["result"]["audio"][0]["items"][0]["item"].as_u64().unwrap_or_else(|| panic!("{v}"));
        // 16,000 words within the clip's half second.
        let step = TICKS_PER_SECOND / 2 / 16_000;
        let words: Vec<Json> = (0..16_000).map(|i| json!({"text": "w", "start": i * step, "end": (i + 1) * step})).collect();
        let set = json!({"id": "transcript.set", "params": {"item": item, "transcript": {"language": "en", "speakers": [], "words": words}}});
        let search = json!({"id": "transcript.search", "params": {"query": "w"}});
        let once = door_run(&areas, json!({"path": clip, "cmds": [set, search]}), dir.path(), false).unwrap();
        assert!(once["results"][1]["result"].is_object() || once["results"][1]["result"].is_array(), "{once}");
        let twice = json!([set, {"id": "edit.selectAll"}, {"id": "edit.copy"}, {"id": "playhead.end"}, {"id": "edit.paste"}, search]);
        let e = door_run(&areas, json!({"path": clip, "cmds": twice}), dir.path(), false).unwrap_err();
        assert!(e.starts_with("film.run transcript.search: it would walk 32000 transcript words") && e.contains("more than the 30000"), "{e}");
    }

    /// Commands that make one edit after another are bounded before they run,
    /// and so is a caption file's size.
    #[test]
    fn inner_edits_and_caption_files_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let areas = resolver(dir.path(), None);
        let ids = |n: u64| Json::Array((1..=n).map(|i| json!(i)).collect());
        let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "trim.applyDefaultTransition", "params": {"clips": ids(129)}}]}), dir.path(), true).unwrap_err();
        assert!(e.contains("it would make 258 edits one after another") && e.contains("more than the 256"), "{e}");
        // At the cap it runs (transitions at the cuts of the clips among them).
        let v = door_run(&areas, json!({"path": clip, "cmds": [{"id": "trim.applyDefaultTransition", "params": {"clips": ids(128)}}]}), dir.path(), true).unwrap();
        assert!(v["results"][0]["result"]["transitions"].is_array(), "{v}");
        let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "clip.automateToSequence", "params": {"items": ids(86)}}]}), dir.path(), true).unwrap_err();
        assert!(e.contains("it would make 258 edits"), "{e}");
        std::fs::create_dir(dir.path().join("subs")).unwrap();
        let big = std::fs::File::create(dir.path().join("subs/big.srt")).unwrap();
        big.set_len(MAX_CAPTION_BYTES + 1).unwrap();
        let e = door_run(&areas, json!({"path": clip, "cmds": [{"id": "captions.import", "params": {"path": "subs/big.srt"}}]}), dir.path(), true).unwrap_err();
        assert!(e.starts_with("film.run captions.import: the caption file is 16777217 bytes"), "{e}");
    }

    /// A door session keeps eight undo steps, whatever a command sets, and can
    /// still undo what it just did.
    #[test]
    fn a_door_session_keeps_a_short_history() {
        let dir = tempfile::tempdir().unwrap();
        let clip = clip_in(dir.path());
        let root = dir.path().canonicalize().unwrap();
        let rules = Arc::new(Area::new(&root, None, true));
        let mut s = session(&root, &rules);
        clip_sequence(&mut s, &root.join(clip)).unwrap();
        let mut caps = Caps::open(&mut s).unwrap();
        for k in 0..20 {
            s.execute("markers.add", json!({"time": k * TICKS_PER_SECOND / 100})).unwrap();
            caps.after(&mut s).unwrap();
            assert!(s.history.undo.len() <= DOOR_HISTORY, "{}", s.history.undo.len());
        }
        let areas = resolver(dir.path(), None);
        let v = door_run(&areas, json!({"path": clip, "cmds": [{"id": "markers.add"}, {"id": "markers.add", "params": {"time": 1}}, {"id": "edit.undo"}, {"id": "sequence.inspect"}]}), dir.path(), false)
            .unwrap();
        assert_eq!(v["results"][3]["result"]["markers"].as_array().map(Vec::len), Some(1), "{v}");
    }

    /// What the door allows a project to hold, on projects made in memory: a
    /// nest of nests multiplies every frame's renders, and is refused past
    /// the fan-out ceiling; masks past their caps are refused too.
    #[test]
    fn a_nest_of_nests_is_bounded_by_its_fanout() {
        let mut p = Project::default();
        let st = SequenceSettings { width: 64, height: 36, ..Default::default() };
        // Four levels of sequences, each with `n` video tracks holding the
        // level below: n^3 sources a frame at the top.
        let build = |p: &mut Project, n: usize| {
            let mut inner = p.new_sequence("leaf", st.clone(), 1, 0, None);
            let item = p.add_item("matte", filmcraft_engine::project::Label::Iris, ItemKind::AdjustmentLayer { width: 64, height: 36, rate: FrameRate::FPS_24, duration: Tick(TICKS_PER_SECOND) }, None);
            let r = TimeRange::new(Tick::ZERO, Tick(TICKS_PER_SECOND));
            let leaf = p.make_track_item(item, TrackKind::Video, Tick::ZERO, r, FrameRate::FPS_24).unwrap();
            p.sequence_mut(inner).unwrap().video_tracks[0].items.push(leaf);
            for level in 0..3 {
                let outer = p.new_sequence(&format!("level {level}"), st.clone(), n, 0, None);
                for t in 0..n {
                    let nest = p.make_track_item(inner, TrackKind::Video, Tick::ZERO, r, FrameRate::FPS_24).unwrap();
                    p.sequence_mut(outer).unwrap().video_tracks[t].items.push(nest);
                }
                inner = outer;
            }
            inner
        };
        let top = build(&mut p, 6);
        assert_eq!(fanout(&p, top, true, 0, &mut Memo::new()), 216.0);
        assert!(bounded(&p).is_ok());
        let mut p = Project::default();
        let top = build(&mut p, 7);
        assert_eq!(fanout(&p, top, true, 0, &mut Memo::new()), 343.0);
        let e = bounded(&p).unwrap_err();
        assert!(e.contains("renders up to 343 sources") && e.contains("more than the 256"), "{e}");
        // Nests laid one after another render one source a frame, but the
        // render bar hashes every clip of every nest at every segment: 12^5
        // clips through five levels of twelve.
        let chain = |levels: usize| {
            let mut p = Project::default();
            let mut inner = p.new_sequence("leaf", st.clone(), 1, 0, None);
            let r = TimeRange::new(Tick::ZERO, Tick(TICKS_PER_SECOND));
            for level in 0..levels {
                let outer = p.new_sequence(&format!("level {level}"), st.clone(), 1, 0, None);
                for k in 0..12 {
                    let mut nest = p.make_track_item(inner, TrackKind::Video, Tick::ZERO, r, FrameRate::FPS_24).unwrap();
                    nest.start = Tick(k * TICKS_PER_SECOND);
                    nest.id = ClipId(p.alloc_id());
                    p.sequence_mut(outer).unwrap().video_tracks[0].items.push(nest);
                }
                inner = outer;
            }
            (p, inner)
        };
        let (p, top) = chain(5);
        assert!(fanout(&p, top, true, 0, &mut Memo::new()) <= 1.0, "one nest a frame");
        assert!(bounded(&p).is_ok());
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let mut s = session(&root, &Arc::new(Area::new(&root, None, true)));
        s.project = Arc::new(p);
        s.state.active_sequence = Some(top);
        let e = preview_work(&s, "sequence.renderBar").unwrap_err();
        assert!(e.contains("would hash up to") && e.contains("more than the 500000"), "{e}");
        assert!(preview_work(&s, "sequence.inspect").is_ok(), "only the render bar hashes");
        let (p, top) = chain(3);
        s.project = Arc::new(p);
        s.state.active_sequence = Some(top);
        assert!(preview_work(&s, "sequence.renderBar").is_ok(), "three levels of twelve");
    }
}
