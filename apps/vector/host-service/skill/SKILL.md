---
name: vector-engine
description: Vector graphics (SVG, .vectorcraft, PDF, EPS, DXF, EMF): draw and edit with the engine's commands, inspect artboards and layers, convert, render to PNG. Read before using vector.* tools.
---

# Vector engine

vectorcraft is a headless vector illustrator in the Illustrator tradition:
artboards, layers and groups, paths and compound shapes, strokes, fills,
gradients, symbols and text. It opens SVG and its own `.vectorcraft`
format, among others, and exports SVG/SVGZ, PDF, EPS, DXF, EMF/WMF and the
raster formats PNG, JPEG, WebP, GIF, TIFF, BMP, TGA and PSD.

## Tools

- `vector.info {path, depth?}`: artboards, the layer tree to `depth` levels (default 2, at most 8), object count, colour mode, units and import warnings. Reads only.
- `vector.run {path?, cmds, out?, format?, scale?, artboard?}`: up to 64 of the engine's commands, run in order on the document at `path` (or on a new default one), then exported to `out` when given, in the format `format` or else the extension of `out` names. A raster export is one artboard (`artboard`, 0-based, default 0) at `scale` pixels per point (0.01 to 16, default 1); a multi-artboard SVG export also writes numbered siblings, listed in `files`. Each command is `{"id": ..., "params": {...}}`. The answer has each command's result in `results`, so query commands read without writing anything; `"cmds": []` with an `out` converts.

## How `vector.run` works

The door checks every command of a call before it runs any. A command that
`commands.md` lists untagged works on the open document only, and runs.
`effect.apply` and `appearance.addEffect` run only an effect the engine
builds in (the ids `effect.list` gives, such as `stylize.dropShadow`), never
an effect plug-in. Every tagged id ([file], [code], [host]) and any id that
is not in `commands.md` refuse the whole call, and nothing is written. A
command that would link an image from outside your workspace fails the
call too: embed images instead.

The commands act like the app's own menus on one open document with a
selection: a shape or text a command draws is selected, and most edits
(paint, effects, arrange, align) apply to the selection or to the `ids`
they are given. Useful queries: `document.inspect` (the layer tree,
artboards and selection, with object ids) and `document.info` (counts,
fonts, images and swatches). The file at `path` is never changed: write
the result to a new `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the vector engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the vector engine
cannot reach it. A document that links a file outside your workspace is
refused.

## Examples

1. What a file holds: `vector.info {"path": "logo.svg", "depth": 3}`.
2. A 4x PNG of it for a slide: `vector.run {"path": "logo.svg", "cmds": [], "out": "logo@4x.png", "scale": 4}`.
3. The same logo for print and for a cutter: `vector.run {"path": "logo.svg", "cmds": [], "out": "logo.pdf"}`,
   then `vector.run {"path": "logo.svg", "cmds": [], "out": "logo.dxf"}`.
4. A rounded badge with a label and a drop shadow, as SVG:
   `vector.run {"cmds": [{"id": "shape.rectangle", "params": {"x": 20, "y": 20, "width": 240, "height": 96, "radius": 16}}, {"id": "paint.setFill", "params": {"color": "#3366cc"}}, {"id": "effect.apply", "params": {"effect": "stylize.dropShadow"}}, {"id": "text.create", "params": {"x": 48, "y": 80, "text": "Beta", "size": 40, "color": "#ffffff"}}], "out": "badge.svg"}`.

## The engine's commands

`commands.md` in this skill's folder lists every vectorcraft command, one
line each (id, label, parameters), with a tag on those that reach past the
open document. Grep it (`grep -i pathfinder commands.md`) for the ids and
parameters a request needs: `vector.run` runs the untagged ones, and
refuses the rest.
