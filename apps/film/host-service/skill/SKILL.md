---
name: film-engine
description: Video and media files (MP4, MOV, MKV, WebM, MXF, TS, Ogg) and FilmCraft projects: probe, edit with the engine's commands, grab a frame, transcode offline. Read before using film.* tools.
---

# Film engine

filmcraft is a headless video editor in pure Rust, with no ffmpeg, system
codecs, GPU or network: its own demuxers (MP4/MOV, Matroska/WebM, MXF, MPEG
TS/PS, Ogg), decoders (H.264, HEVC, VP9, AV1, ProRes, DNx, AAC, Opus, ...)
and encoders (H.264 with AAC, ProRes, PCM, GIF). It also opens FilmCraft
projects (`.fcproj`): bins, sequences, clips, effects and captions.

## Tools

- `film.info {path}`: container, video and audio streams, duration in ms, timecode and size. Reads only.
- `film.run {path, cmds, out?, format?, at_ms?, start_ms?, end_ms?, audio?, max_side?}`: up to 64 of the engine's commands, run in order on the project at `path` (a `.fcproj`) or on a new sequence holding the media file at `path`, then written to `out` when given: the project as `.fcproj`; the active sequence transcoded (at most 5 minutes per call, `start_ms` to `end_ms`, default all of it) to H.264+AAC `.mp4`, ProRes `.mov`, `.wav` or `.gif` by `format` (`h264`, `prores`, `wav`, `gif`) or the extension of `out`, `audio: false` dropping the sound; or the frame at `at_ms` (default 0) as a `.png` at most `max_side` pixels long (default 1024). Each command is `{"id": ..., "params": {...}}`. The answer has each command's result in `results`, so query commands read without writing anything; `"cmds": []` with an `out` transcodes or grabs a frame.

## How `film.run` works

The door checks every command of a call before it runs any. A command that
`commands.md` lists untagged works on the open project only, and runs; so
does `captions.import {path}`, whose `path` must name a caption file (SRT,
VTT, SCC, MCC, STL or TTML) in your workspace. Commands that name an
effect, transition or preset run only the engine's built-in ones. Every
other tagged id ([file], [network], [device], [host]) and any id that is
not in `commands.md` refuse the whole call, and nothing is written. After
every command the project is checked again: an effect or setting that
names a file outside your workspace fails the call.

Useful queries: `project.inspect` (the bins and items with their ids, and
the active sequence) and `sequence.inspect {item?}` (its tracks with clip
ids, timing, effects and transitions). Effects apply to the clips you name
(`effects.apply {clips?, effect}`), or to the selected ones. The file at
`path` is never changed: write the result to a new `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_video`) see: the film engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the film engine
cannot reach it. Media a project points at outside your workspace stay
offline, and a project that sets a scratch disk or an ingest folder is
refused.

## Examples

1. What a clip holds: `film.info {"path": "talk.mp4"}`.
2. A thumbnail two seconds in: `film.run {"path": "talk.mp4", "cmds": [], "out": "thumb.png", "at_ms": 2000, "max_side": 640}`.
3. A silent GIF of seconds 1 to 3:
   `film.run {"path": "talk.mp4", "cmds": [], "out": "clip.gif", "start_ms": 1000, "end_ms": 3000, "audio": false}`.
4. Add its captions from an SRT file and keep it as a project:
   `film.run {"path": "talk.mp4", "cmds": [{"id": "captions.import", "params": {"path": "talk.srt"}}], "out": "talk.fcproj"}`,
   then read the project back: `film.run {"path": "talk.fcproj", "cmds": [{"id": "project.inspect"}, {"id": "sequence.inspect"}, {"id": "captions.list"}]}`.

## The engine's commands

`commands.md` in this skill's folder lists every filmcraft command, one line
each (id, label, parameters), with a tag on those that reach past the open
project. Grep it (`grep -i caption commands.md`) for the ids and parameters
a request needs: `film.run` runs the untagged ones and `captions.import`,
and refuses the rest.
