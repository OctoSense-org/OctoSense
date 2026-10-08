//! `octosense-sound-service` — the `sound` host service (ADR 0013).
//!
//! soundcraft's engine, offline only. The service consumes just the
//! engine's device-free crates — `soundcraft-audio-io` (bytes in, bytes
//! out: WAV/BWF/RF64 and AIFF native readers and writers, a FLAC encoder,
//! symphonia-backed decode of MP3/Ogg/AAC/ALAC/CAF, waveform peaks) and
//! `soundcraft-dsp`'s offline processors (gain, resample) — and never
//! opens an audio or MIDI device: no playback, no recording, no cpal, no
//! plugin hosts. Every call reads files, computes, writes files, returns.
//!
//! Files live in the `sound/` area of the caller's host directory (the
//! shared `.host` also holds Mail's and Calendar's data, which `sound.*`
//! never touches); every path is relative and contained there.
//!
//! Methods (all under the `sound` family; paths relative to the area):
//! - `info {path}` → format, sample format, sample rate, channels,
//!   frames, duration (and BWF metadata when present)
//! - `convert {path, out, format?, bit_depth?}` → decode anything the
//!   engine reads, encode WAV, AIFF or FLAC
//! - `trim {path, out, start_ms, end_ms}` → cut a span to a new file
//! - `mix {tracks: [{path, gain_db?}], out, format?}` → sum tracks
//!   offline, resampling to the highest rate, mono broadcast to wider
//! - `peaks {path, cols?}` → waveform min/max columns as JSON
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

use std::path::{Component, Path, PathBuf};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value as Json};
use soundcraft_audio_io::{self as audio, AudioBuffer, BitDepth, EncodeOptions, FileFormat};

/// Hard caps: an offline helper for app-sized files, not a bulk pipeline.
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
/// The longest audio any one call decodes or writes, in seconds.
const MAX_SECS: f64 = 600.0;
/// The most tracks one `mix` sums.
const MAX_TRACKS: usize = 16;
/// The most waveform columns `peaks` returns.
const MAX_PEAK_COLS: u64 = 4096;

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct SoundService;

/// Register the `sound` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(SoundService));
}

impl HostService for SoundService {
    fn family(&self) -> &'static str {
        "sound"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The sound service serves system apps only.".into()));
            return;
        }
        let method = call.method().to_string();
        let args = call.args.clone();
        let host_dir = call.host_dir.clone();
        reply.send(dispatch(&method, &args, &host_dir));
    }
}

/// The family's area under the host directory: everything the service
/// reads or writes stays in `<host_dir>/sound`.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let dir = host_dir.join("sound");
    std::fs::create_dir_all(&dir).map_err(|e| format!("sound: cannot prepare the sound area: {e}"))?;
    Ok(dir)
}

/// A caller path resolved inside the area: relative, normal components
/// only — separators are fine, but `..`, `.`, roots and prefixes are not.
fn contained(area: &Path, path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let normal = p.components().all(|c| matches!(c, Component::Normal(_)));
    if path.is_empty() || p.is_absolute() || !normal {
        return Err(format!("sound: `{path}` must be a relative path inside the sound area"));
    }
    Ok(area.join(p))
}

fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = area(host_dir)?;
    match method {
        "info" => info(args, &area),
        "convert" => convert(args, &area),
        "trim" => trim(args, &area),
        "mix" => mix(args, &area),
        "peaks" => peaks(args, &area),
        other => Err(format!("sound.{other} is not a method of the sound service")),
    }
}

fn arg_str<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("sound: `{key}` is required"))
}

/// Read one input file inside the area, size-capped, with its extension
/// as the engine's format hint.
fn read_input(area: &Path, path: &str, method: &str) -> Result<(Vec<u8>, Option<String>), String> {
    let full = contained(area, path)?;
    let len = std::fs::metadata(&full).map_err(|e| format!("sound.{method}: {path}: {e}")).map(|m| m.len())?;
    if len > MAX_INPUT_BYTES {
        return Err(format!("sound.{method}: {path} is {len} bytes; the cap is {MAX_INPUT_BYTES}"));
    }
    let bytes = std::fs::read(&full).map_err(|e| format!("sound.{method}: {path}: {e}"))?;
    let hint = Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_string);
    Ok((bytes, hint))
}

/// Decode one input through the engine, refusing anything longer than
/// [`MAX_SECS`] (checked on the probe when the container declares a
/// length, and again on the decoded buffer).
fn decode_input(area: &Path, path: &str, method: &str) -> Result<(audio::AudioInfo, AudioBuffer), String> {
    let (bytes, hint) = read_input(area, path, method)?;
    let probed = audio::probe(&bytes, hint.as_deref()).map_err(|e| format!("sound.{method}: {path}: {e}"))?;
    if probed.duration_secs() > MAX_SECS {
        return Err(format!("sound.{method}: {path} is {:.1}s long; the cap is {MAX_SECS}s", probed.duration_secs()));
    }
    let (info, buf) = audio::decode(&bytes, hint.as_deref()).map_err(|e| format!("sound.{method}: {path}: {e}"))?;
    if buf.duration_secs() > MAX_SECS {
        return Err(format!("sound.{method}: {path} is {:.1}s long; the cap is {MAX_SECS}s", buf.duration_secs()));
    }
    Ok((info, buf))
}

/// The encode options for an output: `format` names `wav`, `aiff` or
/// `flac` (defaulting to the `out` extension), `bit_depth` one of
/// `int16`, `int24` (the default), `int32` or `float32`.
fn encode_options(args: &Json, out: &str, method: &str) -> Result<EncodeOptions, String> {
    let named = args["format"].as_str();
    let ext = Path::new(out).extension().and_then(|e| e.to_str());
    let format = match named.or(ext).map(str::to_ascii_lowercase).as_deref() {
        Some("wav" | "wave" | "bwf") => FileFormat::Wav,
        Some("aiff" | "aif" | "aifc") => FileFormat::Aiff,
        Some("flac") => FileFormat::Flac,
        other => {
            let name = other.unwrap_or("none").to_string();
            return Err(format!("sound.{method}: the engine encodes wav, aiff or flac, not `{name}`"));
        }
    };
    let bit_depth = match args["bit_depth"].as_str() {
        None => BitDepth::Int24,
        Some("int16") => BitDepth::Int16,
        Some("int24") => BitDepth::Int24,
        Some("int32") => BitDepth::Int32,
        Some("float32") => BitDepth::Float32,
        Some(other) => return Err(format!("sound.{method}: `bit_depth` is int16, int24, int32 or float32, not `{other}`")),
    };
    Ok(EncodeOptions { format, bit_depth, ..EncodeOptions::default() })
}

/// Encode through the engine and write inside the area.
fn write_output(area: &Path, out: &str, buf: &AudioBuffer, opts: &EncodeOptions, method: &str) -> Result<usize, String> {
    if buf.duration_secs() > MAX_SECS {
        return Err(format!("sound.{method}: the output would be {:.1}s long; the cap is {MAX_SECS}s", buf.duration_secs()));
    }
    let bytes = audio::encode(buf, opts).map_err(|e| format!("sound.{method}: {e}"))?;
    let full = contained(area, out)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("sound.{method}: {out}: {e}"))?;
    }
    std::fs::write(&full, &bytes).map_err(|e| format!("sound.{method}: {out}: {e}"))?;
    Ok(bytes.len())
}

fn info(args: &Json, area: &Path) -> Result<Json, String> {
    let path = arg_str(args, "path")?;
    let (bytes, hint) = read_input(area, path, "info")?;
    let probed = audio::probe(&bytes, hint.as_deref()).map_err(|e| format!("sound.info: {path}: {e}"))?;
    let bwf = match &probed.bwf {
        Some(b) => serde_json::to_value(b).map_err(|e| format!("sound.info: {e}"))?,
        None => Json::Null,
    };
    Ok(json!({
        "path": path,
        "format": format!("{:?}", probed.format).to_ascii_lowercase(),
        "sample_format": format!("{:?}", probed.sample_format).to_ascii_lowercase(),
        "sample_rate": probed.sample_rate,
        "channels": probed.channels,
        // 0 when the container does not declare a length.
        "frames": probed.frames,
        "duration_secs": probed.duration_secs(),
        "bwf": bwf,
    }))
}

fn convert(args: &Json, area: &Path) -> Result<Json, String> {
    let path = arg_str(args, "path")?;
    let out = arg_str(args, "out")?;
    let opts = encode_options(args, out, "convert")?;
    let (_, buf) = decode_input(area, path, "convert")?;
    let bytes = write_output(area, out, &buf, &opts, "convert")?;
    Ok(json!({
        "out": out,
        "format": format!("{:?}", opts.format).to_ascii_lowercase(),
        "frames": buf.frames(),
        "sample_rate": buf.sample_rate,
        "channels": buf.num_channels(),
        "bytes": bytes,
    }))
}

fn trim(args: &Json, area: &Path) -> Result<Json, String> {
    let path = arg_str(args, "path")?;
    let out = arg_str(args, "out")?;
    let start_ms = args["start_ms"].as_u64().ok_or("sound.trim: `start_ms` is required")?;
    let end_ms = args["end_ms"].as_u64().ok_or("sound.trim: `end_ms` is required")?;
    if end_ms <= start_ms {
        return Err("sound.trim: `end_ms` must be after `start_ms`".into());
    }
    let opts = encode_options(args, out, "trim")?;
    let (_, buf) = decode_input(area, path, "trim")?;
    let frames = buf.frames() as u64;
    let rate = u64::from(buf.sample_rate);
    let start = (start_ms.saturating_mul(rate) / 1000).min(frames) as usize;
    let end = (end_ms.saturating_mul(rate) / 1000).min(frames) as usize;
    if end <= start {
        return Err(format!("sound.trim: the span starts at or past the end of {path}"));
    }
    let cut = AudioBuffer {
        sample_rate: buf.sample_rate,
        channels: buf.channels.iter().map(|ch| ch[start..end].to_vec()).collect(),
    };
    let bytes = write_output(area, out, &cut, &opts, "trim")?;
    Ok(json!({"out": out, "frames": cut.frames(), "sample_rate": cut.sample_rate, "bytes": bytes}))
}

fn mix(args: &Json, area: &Path) -> Result<Json, String> {
    let out = arg_str(args, "out")?;
    let tracks = args["tracks"].as_array().filter(|t| !t.is_empty()).ok_or("sound.mix: `tracks` is a non-empty list of {path, gain_db?}")?;
    if tracks.len() > MAX_TRACKS {
        return Err(format!("sound.mix: at most {MAX_TRACKS} tracks per call"));
    }
    let opts = encode_options(args, out, "mix")?;
    let mut decoded: Vec<AudioBuffer> = Vec::with_capacity(tracks.len());
    for t in tracks {
        let path = arg_str(t, "path")?;
        let gain_db = t["gain_db"].as_f64().unwrap_or(0.0);
        if !(-96.0..=24.0).contains(&gain_db) {
            return Err(format!("sound.mix: {path}: `gain_db` is between -96 and 24, not {gain_db}"));
        }
        let (_, mut buf) = decode_input(area, path, "mix")?;
        if buf.num_channels() == 0 || buf.frames() == 0 {
            return Err(format!("sound.mix: {path} holds no audio"));
        }
        if gain_db != 0.0 {
            soundcraft_dsp::offline::gain(&mut buf.channels, gain_db as f32);
        }
        decoded.push(buf);
    }
    // The engine's offline resampler lines every track up on the highest
    // rate; mono (and narrower) tracks broadcast onto the widest layout.
    let out_rate = decoded.iter().map(|b| b.sample_rate).max().unwrap_or(0);
    let out_channels = decoded.iter().map(AudioBuffer::num_channels).max().unwrap_or(0);
    for buf in &mut decoded {
        if buf.sample_rate != out_rate {
            buf.channels = soundcraft_dsp::offline::resample(&buf.channels, buf.sample_rate, out_rate);
            buf.sample_rate = out_rate;
        }
    }
    let out_frames = decoded.iter().map(AudioBuffer::frames).max().unwrap_or(0);
    let mut sum = AudioBuffer::new(out_rate, out_channels, out_frames);
    if sum.duration_secs() > MAX_SECS {
        return Err(format!("sound.mix: the mix would be {:.1}s long; the cap is {MAX_SECS}s", sum.duration_secs()));
    }
    for buf in &decoded {
        for (c, channel) in sum.channels.iter_mut().enumerate() {
            let src = &buf.channels[c % buf.num_channels()];
            for (acc, s) in channel.iter_mut().zip(src) {
                *acc += s;
            }
        }
    }
    let peak = sum.peak();
    let bytes = write_output(area, out, &sum, &opts, "mix")?;
    Ok(json!({
        "out": out,
        "tracks": decoded.len(),
        "frames": sum.frames(),
        "sample_rate": out_rate,
        "channels": out_channels,
        // The pre-encode peak: above 1.0 the engine clamped on encode.
        "peak": peak,
        "clipped": peak > 1.0,
        "bytes": bytes,
    }))
}

fn peaks(args: &Json, area: &Path) -> Result<Json, String> {
    let path = arg_str(args, "path")?;
    let cols = args["cols"].as_u64().unwrap_or(512).clamp(1, MAX_PEAK_COLS) as usize;
    let (_, buf) = decode_input(area, path, "peaks")?;
    let frames = buf.frames();
    if frames == 0 {
        return Err(format!("sound.peaks: {path} holds no audio"));
    }
    // The engine's multi-resolution overview, on a mono mixdown.
    let channels = buf.num_channels() as f32;
    let mut mono = vec![0.0f32; frames];
    for ch in &buf.channels {
        for (m, s) in mono.iter_mut().zip(ch) {
            *m += s / channels;
        }
    }
    let overview = audio::peaks::Peaks::build(&mono);
    let columns: Vec<Json> = overview
        .columns(0.0, frames as f64, cols)
        .into_iter()
        .map(|(lo, hi)| json!([lo, hi]))
        .collect();
    Ok(json!({
        "path": path,
        "cols": columns,
        "frames": frames,
        "sample_rate": buf.sample_rate,
        "channels": buf.num_channels(),
        "duration_secs": buf.duration_secs(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a small engine-encoded WAV into the area: `frames` frames of
    /// a constant `level` at `rate` Hz, float32 so the level round-trips
    /// exactly. Pure file I/O — no device anywhere near this suite.
    fn fixture(area: &Path, name: &str, rate: u32, channels: usize, frames: usize, level: f32) -> String {
        let mut buf = AudioBuffer::new(rate, channels, frames);
        for ch in &mut buf.channels {
            ch.fill(level);
        }
        let opts = EncodeOptions { bit_depth: BitDepth::Float32, ..EncodeOptions::default() };
        let bytes = audio::encode(&buf, &opts).unwrap();
        std::fs::create_dir_all(area).unwrap();
        std::fs::write(area.join(name), bytes).unwrap();
        name.into()
    }

    fn host() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let area = dir.path().join("sound");
        (dir, area)
    }

    #[test]
    fn area_is_the_sound_subdir_and_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let a = area(dir.path()).unwrap();
        assert_eq!(a, dir.path().join("sound"));
        assert!(a.is_dir());
    }

    #[test]
    fn info_reads_an_engine_written_wav() {
        let (dir, area) = host();
        let input = fixture(&area, "in.wav", 8000, 1, 800, 0.25);
        let doc = dispatch("info", &json!({"path": input}), dir.path()).unwrap();
        assert_eq!(doc["format"], json!("wav"), "{doc}");
        assert_eq!(doc["sample_rate"], json!(8000));
        assert_eq!(doc["channels"], json!(1));
        assert_eq!(doc["frames"], json!(800));
        assert!((doc["duration_secs"].as_f64().unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn convert_writes_flac_the_engine_reads_back() {
        let (dir, area) = host();
        let input = fixture(&area, "in.wav", 8000, 2, 400, 0.25);
        let conv = dispatch("convert", &json!({"path": input, "out": "out.flac"}), dir.path()).unwrap();
        assert_eq!(conv["format"], json!("flac"), "{conv}");
        let bytes = std::fs::read(area.join("out.flac")).unwrap();
        let (info, buf) = audio::decode(&bytes, Some("flac")).unwrap();
        assert_eq!(info.format, FileFormat::Flac);
        assert_eq!(buf.frames(), 400);
        assert_eq!(buf.num_channels(), 2);
        assert!((buf.channels[0][0] - 0.25).abs() < 1e-4, "int24 round-trip");
        let refused = dispatch("convert", &json!({"path": input, "out": "out.mp3"}), dir.path());
        assert!(refused.unwrap_err().contains("wav, aiff or flac"));
    }

    #[test]
    fn trim_cuts_the_requested_span() {
        let (dir, area) = host();
        let input = fixture(&area, "in.wav", 8000, 1, 800, 0.5);
        let cut = dispatch("trim", &json!({"path": input, "out": "cut.wav", "start_ms": 25, "end_ms": 75}), dir.path()).unwrap();
        assert_eq!(cut["frames"], json!(400), "{cut}");
        let bytes = std::fs::read(area.join("cut.wav")).unwrap();
        let (_, buf) = audio::decode(&bytes, Some("wav")).unwrap();
        assert_eq!(buf.frames(), 400);
        assert!((buf.channels[0][0] - 0.5).abs() < 1e-4);
        let refused = dispatch("trim", &json!({"path": input, "out": "cut.wav", "start_ms": 75, "end_ms": 25}), dir.path());
        assert!(refused.unwrap_err().contains("after"));
    }

    #[test]
    fn mix_sums_tracks_with_engine_gain() {
        let (dir, area) = host();
        let a = fixture(&area, "a.wav", 8000, 1, 400, 0.25);
        let b = fixture(&area, "b.wav", 8000, 1, 200, 0.25);
        // -6.0206 dB is a gain of 0.5: b contributes 0.125 over its half.
        let mixed = dispatch(
            "mix",
            &json!({"tracks": [{"path": a}, {"path": b, "gain_db": -6.0206}], "out": "mix.wav", "bit_depth": "float32"}),
            dir.path(),
        )
        .unwrap();
        assert_eq!(mixed["frames"], json!(400), "{mixed}");
        assert_eq!(mixed["clipped"], json!(false));
        let bytes = std::fs::read(area.join("mix.wav")).unwrap();
        let (_, buf) = audio::decode(&bytes, Some("wav")).unwrap();
        assert!((buf.channels[0][0] - 0.375).abs() < 1e-3, "summed head {}", buf.channels[0][0]);
        assert!((buf.channels[0][300] - 0.25).abs() < 1e-3, "solo tail {}", buf.channels[0][300]);
        let over = (0..MAX_TRACKS + 1).map(|_| json!({"path": a})).collect::<Vec<_>>();
        let refused = dispatch("mix", &json!({"tracks": over, "out": "mix.wav"}), dir.path());
        assert!(refused.unwrap_err().contains("at most"));
    }

    #[test]
    fn peaks_returns_bounded_columns() {
        let (dir, area) = host();
        let input = fixture(&area, "in.wav", 8000, 1, 800, 0.25);
        let doc = dispatch("peaks", &json!({"path": input, "cols": 4}), dir.path()).unwrap();
        let cols = doc["cols"].as_array().unwrap();
        assert_eq!(cols.len(), 4, "{doc}");
        for col in cols {
            let (lo, hi) = (col[0].as_f64().unwrap(), col[1].as_f64().unwrap());
            assert!(lo <= hi && (-1.0..=1.0).contains(&lo) && (-1.0..=1.0).contains(&hi));
        }
    }

    #[test]
    fn paths_may_not_leave_the_sound_area() {
        let (dir, area) = host();
        let input = fixture(&area, "in.wav", 8000, 1, 80, 0.25);
        for bad in ["../up.wav", "/etc/x.wav", "a/../../up.wav", "./in.wav", ""] {
            assert!(dispatch("info", &json!({"path": bad}), dir.path()).is_err(), "{bad}");
            assert!(dispatch("convert", &json!({"path": input, "out": bad}), dir.path()).is_err(), "out {bad}");
        }
        // Nothing escaped beside the area and the fixture inside it.
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("sound")]);
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.sound"));
        assert!(!may_call("org.example.app"));
    }
}
