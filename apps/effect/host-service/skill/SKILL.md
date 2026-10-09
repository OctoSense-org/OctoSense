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
- `effect.render {path, out, comp?, time?, max_side?, transparent?}`: one frame of a composition (`comp`: its name or id, default the active one) at `time` seconds, as a PNG at most `max_side` pixels long (default 1024); `transparent` keeps the alpha. Footage outside the folder renders as a placeholder.
- `effect.export_lottie {path, out, comp?, include_expressions?}`: a composition as Lottie (`.json`, or `.lottie`), with the engine's warnings about what Lottie cannot carry.
- `effect.import_lottie {path, out}`: a Lottie file opened as a composition and saved as a project (`.ecproj`); embedded images are written beside the Lottie file.

`effect.info` only reads; the others write.

## Files

Every path is relative to the effect engine's folder, a private workspace
that only `effect.*` tools read and write. Absolute paths, `..` and links
out of it are refused. Your own workspace (`read_file`, `write_file`,
`view_image`), the person's files and the other engines' folders are outside
it, and what these tools write stays in it. Nothing you have puts a project
into this folder: these tools open only files an earlier `effect.*` call
wrote there, so a project or animation the person has elsewhere cannot be
opened yet. Say so rather than guessing names, and pick a new name for each
`out`.

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
