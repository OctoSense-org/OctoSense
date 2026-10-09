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

Every path is relative to the vector engine's folder, a private workspace
that only `vector.*` tools read and write. Absolute paths, `..` and links
out of it are refused. Your own workspace (`read_file`, `write_file`,
`view_image`), the person's files and the other engines' folders are outside
it, and what these tools write stays in it. Nothing you have puts artwork
into this folder: these tools open only files an earlier `vector.*` call
wrote there, so a file the person has elsewhere cannot be opened yet. Say so
rather than guessing names, and pick a new name for each `out`.

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
