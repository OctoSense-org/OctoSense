---
name: effect-engine
description: Motion graphics (After Effects-class .ecproj projects, Lottie): summarise projects, render a frame, export and import Lottie. Read before using effect.* tools.
---

# Effect engine

effectcraft is a headless motion-graphics compositor in the After Effects
tradition: compositions, layers, keyframes, masks, 300 and more effects,
expressions, text and shape layers. It reads and writes its own `.ecproj`
projects and Lottie (`.json` and dotLottie `.lottie`).

## Tools

- `effect.info {path}`: the project's items (names, types, sizes, durations) and each composition's layer count and frame rate.
- `effect.render {path, out, comp?, time?, max_side?, transparent?}`: one frame of a composition (`comp`: its name or id, default the active one) at `time` seconds, as a PNG at most `max_side` pixels long (default 1024); `transparent` keeps the alpha. Footage outside your workspace renders as a placeholder.
- `effect.export_lottie {path, out, comp?, include_expressions?}`: a composition as Lottie (`.json`, or `.lottie`), with the engine's warnings about what Lottie cannot carry.
- `effect.import_lottie {path, out}`: a Lottie file opened as a composition and saved as a project (`.ecproj`); embedded images are written beside the Lottie file.

`effect.info` only reads; the others write.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the effect engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the effect engine
cannot reach it. Footage outside your workspace renders as a placeholder, 3D
models are refused, and so is a project whose effects name a LUT, OCIO or
mocha file by its path rather than holding its text.

## Examples

1. What a project holds: `effect.info {"path": "intro.ecproj"}`.
2. A still of its title card at 1.5 seconds, with transparency:
   `effect.render {"path": "intro.ecproj", "comp": "Title", "time": 1.5, "out": "title.png", "transparent": true}`.
3. The same composition for the web: `effect.export_lottie {"path": "intro.ecproj", "comp": "Title", "out": "title.json"}`,
   and read its warnings before saying it is complete.

## The engine's commands

`commands.md` in this skill's folder lists every effectcraft command, one
line each (id, label, parameters), with a tag on those that reach past the
open project. Grep it (`grep -i glow commands.md`) when the person asks what
the engine can do. `effect.run` would run these ids, but it is held for its
own review and is not one of your tools: they show the engine's reach, not
what you can call.
