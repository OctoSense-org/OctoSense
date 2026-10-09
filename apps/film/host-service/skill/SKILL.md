---
name: film-engine
description: Video and media files (MP4, MOV, MKV, WebM, MXF, TS, Ogg): probe streams, grab a frame, transcode to H.264, ProRes, WAV or GIF, offline. Read before using film.* tools.
---

# Film engine

filmcraft is a headless video editor in pure Rust, with no ffmpeg, system
codecs, GPU or network: its own demuxers (MP4/MOV, Matroska/WebM, MXF, MPEG
TS/PS, Ogg), decoders (H.264, HEVC, VP9, AV1, ProRes, DNx, AAC, Opus, ...)
and encoders (H.264 with AAC, ProRes, PCM, GIF). It also opens FilmCraft
projects (`.fcproj`): bins, sequences, clips and effects.

## Tools

- `film.info {path}`: container, video and audio streams, duration in ms, timecode and size.
- `film.project.info {path}`: a `.fcproj` opened headlessly: its bins and items, and the active sequence when it has one. Media it points at outside the folder stay offline.
- `film.frame {path, out, at_ms?, max_side?}`: the frame at `at_ms` (default 0) through the program renderer, as a PNG at most `max_side` pixels long (default 1024).
- `film.export {path, out, format?, start_ms?, end_ms?, audio?}`: the range transcoded (default the whole clip, at most 5 minutes per call) to H.264+AAC MP4 (`h264`), ProRes MOV (`prores`), `wav` or `gif`, by `format` or the extension of `out`; `audio: false` drops the sound.

`film.info` and `film.project.info` only read; the others write `out`.

## Files

Every path is relative to the film engine's folder, a private workspace that
only `film.*` tools read and write. Absolute paths, `..` and links out of it
are refused. Your own workspace (`read_file`, `write_file`, `view_video`),
the person's files and the other engines' folders are outside it, and what
these tools write stays in it. Nothing you have puts a clip into this
folder: these tools open only files an earlier `film.*` call wrote there, so
a video the person has elsewhere cannot be opened yet. Say so rather than
guessing names, and pick a new name for each `out`.

## Examples

1. What a clip holds: `film.info {"path": "talk.mp4"}`.
2. A thumbnail 10 seconds in: `film.frame {"path": "talk.mp4", "out": "thumb.png", "at_ms": 10000, "max_side": 640}`.
3. A silent GIF of seconds 5 to 8:
   `film.export {"path": "talk.mp4", "out": "clip.gif", "start_ms": 5000, "end_ms": 8000, "audio": false}`.

## The engine's commands

`commands.md` in this skill's folder lists every filmcraft command, one line
each (id, label, parameters), with a tag on those that reach past the open
project. Grep it (`grep -i caption commands.md`) when the person asks what
the engine can do. No tool on your list runs these ids: they show the
engine's reach, not what you can call.
