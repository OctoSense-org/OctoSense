---
name: cad-engine
description: CAD drawings (DXF, DWG): inspect layers, blocks and entities, measure distances and areas, render or convert to SVG, PNG or PDF. Read before using cad.* tools.
---

# CAD engine

cadcraft is a headless 2D drafting engine in the AutoCAD tradition. It reads
and writes DXF and DWG, and knows model space and layouts, layers, blocks,
dimensions, text, hatches, polylines and splines.

## Tools

- `cad.info {path}`: entity counts by type, layers, blocks, layouts and the model-space extents.
- `cad.entities {path, type?, layer?, limit?, offset?}`: the model-space entities (type, layer, handle), optionally one type (`line`, `circle`, ...) or one layer, at most `limit` (default 500, at most 2000) from `offset`.
- `cad.measure {path, dist?, area?}`: exactly one of `dist {p1, p2}` (distance and angle between two points) or `area {points}` / `area {handle}` (area and perimeter of a polygon, or of a closed entity by the handle `cad.entities` gave). Points are `[x, y]`.
- `cad.render {path, out, max_side?}`: the model space fitted to its extents, as a `.png` (longest edge `max_side`, default 1024) or an `.svg`.
- `cad.convert {path, out, format?}`: the drawing written as DXF, DWG, SVG, PNG or PDF; `format` wins over the extension of `out`.

The first three only read; the others write `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the cad engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the cad engine
cannot reach it.

## Examples

1. What is in a drawing: `cad.info {"path": "plan.dxf"}`, then the walls layer:
   `cad.entities {"path": "plan.dxf", "layer": "WALLS", "limit": 100}`.
2. The distance between two corners:
   `cad.measure {"path": "plan.dxf", "dist": {"p1": [0, 0], "p2": [4200, 3100]}}`.
3. The area of a room outline by its handle from `cad.entities`:
   `cad.measure {"path": "plan.dxf", "area": {"handle": "2F"}}`, then a PDF:
   `cad.convert {"path": "plan.dxf", "out": "plan.pdf"}`.

## The engine's commands

`commands.md` in this skill's folder lists every cadcraft command (the
AutoCAD-style names: `line`, `offset`, `dimlinear`, ...), one line each,
with a tag on those that reach past the open drawing. Grep it
(`grep -i hatch commands.md`) when the person asks what the engine can do.
No tool on your list runs these ids: they show the engine's reach, not what
you can call.
