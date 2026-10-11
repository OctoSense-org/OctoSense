---
name: effect-engine
description: Motion graphics (After Effects-class .ecproj projects, Lottie): build and edit compositions with the engine's commands, render a frame, export and import Lottie. Read before using effect.* tools.
---

# Effect engine

effectcraft is a headless motion-graphics compositor in the After Effects
tradition: compositions, layers, keyframes, masks, 300 and more effects,
expressions, text and shape layers. It reads and writes its own `.ecproj`
projects and Lottie (`.json` and dotLottie `.lottie`).

## Tools

- `effect.info {path}`: the project's items (names, types, sizes, durations) and each composition's layer count and frame rate. Reads only.
- `effect.run {path?, cmds, out?, comp?, time?, max_side?, transparent?}`: up to 64 of the engine's commands, run in order on the project at `path` (an `.ecproj`, or a Lottie `.json`/`.lottie` opened as a new project) or on a new empty project, then written to `out` when given, by its extension: the project as `.ecproj`; a composition (`comp`: its name or id, default the active one) as Lottie `.json` or `.lottie`, with the engine's warnings about what Lottie cannot carry; or one frame of it at `time` seconds as a `.png` at most `max_side` pixels long (default 1024; `transparent` keeps the alpha). Each command is `{"id": ..., "params": {...}}`. The answer has each command's result in `results`, so query commands read without writing anything; `"cmds": []` with an `out` converts or renders.

## How `effect.run` works

The door checks every command of a call before it runs any. A command that
`commands.md` lists untagged works on the open project only, and runs.
`effect.apply` runs only an effect the engine builds in (`effect.list`
lists them), never an effect plug-in. Every tagged id ([file], [code],
[network], [device], [host]) and any id that is not in `commands.md` refuse
the whole call, and nothing is written. After every command the project is
checked again: footage outside your workspace, 3D models, effect plug-ins,
and a LUT, OCIO or mocha parameter that names a file rather than holding
its text fail the call. Expressions are code: the commands that set them
are tagged [code], and a project whose compositions hold an enabled
expression is saved as `.ecproj` but never rendered or exported.

Each call is capped so it cannot stall the device: a composition holds at
most 8.8 million pixels (4096 × 2160) and 36,000 frames, at 1 to 240 fps;
a project holds at most 5,000 items, layers and effects; and effect
parameters stay within their reviewed ranges (a Gaussian blur of at most
200, an Echo of at most 8 echoes). A command over a cap refuses the whole
call and says which cap.

The commands act like the app's own menus on one open project: `comp.new`
makes a composition and opens it, new layers go into the open one, and
most layer edits apply to the selected layers or the `layer` (or `layers`)
they are given (by name, id or `#n`). Useful queries: `comp.info {comp?}`,
`prop.get {layer, path}` and `effect.list {filter?}`. The file at `path`
is never changed: write the result to a new `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the effect engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the effect engine
cannot reach it. A Lottie file's embedded images are written beside it when
it is opened.

## Examples

1. What a project holds: `effect.info {"path": "intro.ecproj"}`.
2. A three-second title card, saved as a project:
   `effect.run {"cmds": [{"id": "comp.new", "params": {"name": "Title", "width": 1280, "height": 720, "frameRate": 30, "duration": 3}}, {"id": "layer.newSolid", "params": {"name": "Background", "color": "#1d2b53"}}, {"id": "layer.newText", "params": {"text": "Launch day", "size": 96, "fill": "#ffffff", "position": [640, 380]}}], "out": "title.ecproj"}`.
3. A still of it at 1.5 seconds, then the same composition for the web:
   `effect.run {"path": "title.ecproj", "cmds": [], "out": "title.png", "comp": "Title", "time": 1.5}`, then
   `effect.run {"path": "title.ecproj", "cmds": [], "out": "title.json", "comp": "Title"}`,
   and read its warnings before saying it is complete.
4. Open a Lottie animation, blur its first layer and keep it as a project:
   `effect.run {"path": "intro.json", "cmds": [{"id": "layer.select", "params": {"layers": ["#1"]}}, {"id": "effect.apply", "params": {"effect": "Gaussian Blur"}}], "out": "intro.ecproj"}`.

## The engine's commands

`commands.md` in this skill's folder lists every effectcraft command, one
line each (id, label, parameters), with a tag on those that reach past the
open project. Grep it (`grep -i glow commands.md`) for the ids and
parameters a request needs: `effect.run` runs the untagged ones, and
refuses the rest.
