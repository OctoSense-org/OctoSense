---
name: vector-engine
description: Vector graphics (SVG, .vectorcraft, PDF, EPS, DXF, EMF): inspect artboards and layers, convert between formats, render an artboard to PNG. Read before using vector.* tools.
---

# Vector engine

vectorcraft is a headless vector illustrator in the Illustrator tradition:
artboards, layers and groups, paths and compound shapes, strokes, fills,
gradients, symbols and text. It opens SVG and its own `.vectorcraft`
format, among others, and exports SVG/SVGZ, PDF, EPS, DXF, EMF/WMF and the
raster formats PNG, JPEG, WebP, GIF, TIFF, BMP, TGA and PSD.

## Tools

- `vector.info {path, depth?}`: artboards, the layer tree to `depth` levels (default 2, at most 8), object count, colour mode, units and import warnings.
- `vector.convert {path, out, format?, scale?}`: the document in the format `format` or the extension of `out` names; `scale` (0.01 to 16) sizes raster output. A multi-artboard SVG export also writes numbered siblings, listed in `files`.
- `vector.render {path, out, max_side?, artboard?}`: one artboard (0-based, default 0) as a PNG, its longest edge `max_side` pixels (default 1024).

`vector.info` only reads; the others write `out`.

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
2. A 4x PNG of it for a slide: `vector.convert {"path": "logo.svg", "out": "logo@4x.png", "scale": 4}`.
3. The same logo for print and for a cutter: `vector.convert {"path": "logo.svg", "out": "logo.pdf"}`,
   then `vector.convert {"path": "logo.svg", "out": "logo.dxf"}`.

## The engine's commands

`commands.md` in this skill's folder lists every vectorcraft command, one
line each (id, label, parameters), with a tag on those that reach past the
open document. Grep it (`grep -i pathfinder commands.md`) when the person
asks what the engine can do. `vector.run` would run these ids, but it is
held for its own review and is not one of your tools: they show the
engine's reach, not what you can call.
