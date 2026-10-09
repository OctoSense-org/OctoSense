---
name: sound-engine
description: Audio files (WAV, AIFF, FLAC, MP3, Ogg, AAC, ALAC): inspect, waveform peaks, convert, trim, mix, all offline with no audio device. Read before using sound.* tools.
---

# Sound engine

soundcraft's offline core: native WAV/BWF/RF64, AIFF and FLAC readers and
writers, decoding of MP3, Ogg, AAC, ALAC and CAF, gain and resampling, and
waveform peaks. It never opens an audio or MIDI device: it cannot play,
record or listen, and it hosts no plug-ins. Every call reads files,
computes, writes files and returns.

## Tools

- `sound.info {path}`: format, sample format, sample rate, channels, frames, duration and any BWF metadata.
- `sound.peaks {path, cols?}`: the waveform of a mono mixdown as `cols` columns (default 512, at most 4096) of [min, max] samples; files up to 10 minutes.
- `sound.convert {path, out, format?, bit_depth?}`: decode (up to 10 minutes) and write WAV, AIFF or FLAC, chosen by `format` or the extension of `out`, at `bit_depth` (int16, int24, int32 or float32; default int24).
- `sound.trim {path, out, start_ms, end_ms, format?, bit_depth?}`: the span from `start_ms` to `end_ms` as a new file.
- `sound.mix {tracks, out, format?, bit_depth?}`: 1 to 16 `{path, gain_db?}` tracks (gain -96 to 24 dB) summed into one file, resampled to the highest rate, mono spread to the widest layout; the answer says whether the sum clipped.

`sound.info` and `sound.peaks` only read; the others write `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`) see: the sound engine works in it
for you. Absolute paths, `..` and links out of it are refused. Every engine
works in the same folder, so what one writes the next can open, and a file
the person puts there is yours to use. No call ever replaces an existing
file: pick a new name for each `out`, or the call is refused. If the person
names a file outside your workspace, say that the sound engine cannot reach
it.

## Examples

1. How long and in what format: `sound.info {"path": "interview.wav"}`.
2. The first 30 seconds as a 16-bit FLAC:
   `sound.trim {"path": "interview.wav", "out": "intro.flac", "start_ms": 0, "end_ms": 30000, "bit_depth": "int16"}`.
3. Voice over music, the music 12 dB down:
   `sound.mix {"tracks": [{"path": "voice.wav"}, {"path": "music.wav", "gain_db": -12}], "out": "mix.wav"}`.

The sound engine has no command catalog of its own here: its device-free
crates offer only what the five tools above do.
