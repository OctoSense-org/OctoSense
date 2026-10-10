---
name: cad-engine
description: CAD drawings (DXF, DWG): draw and edit with the engine's commands, inspect layers and entities, measure, render or convert to SVG, PNG or PDF. Read before using cad.* tools.
---

# CAD engine

cadcraft is a headless 2D drafting engine in the AutoCAD tradition. It reads
and writes DXF and DWG, and knows model space and layouts, layers, blocks,
dimensions, text, hatches, polylines and splines.

## Tools

- `cad.info {path}`: entity counts by type, layers, blocks, layouts and the model-space extents. Reads only.
- `cad.run {path?, cmds, out?, format?, max_side?}`: up to 64 of the engine's commands, run in order on the drawing at `path` (or on a new empty one), then written to `out` when given as DXF, DWG, SVG, PDF or PNG, by `format` or else the extension of `out` (a PNG is fitted to the drawing, its longest edge `max_side` pixels, default 1024). Each command is `{"id": ..., "params": {...}}`. The answer has each command's result in `results`, so query commands read without writing anything; `"cmds": []` with an `out` converts or renders.

## How `cad.run` works

The door checks every command of a call before it runs any. A command that
`commands.md` lists untagged works on the open drawing only, and runs; every
`[file]` command (`open`, `qsave`, `saveas`, `wblock`, `plot`,
`exportpdf`) refuses the whole call, and so does `setvar` (it sets
variables by name) and any id that is not in `commands.md`. Spell ids as
`commands.md` does: lower case.

The commands are AutoCAD's, with points as `[x, y]` and entities named by
their hex handles. A new drawing is imperial; `new {"metric": true}` as the
first command makes it metric. Useful queries: `entities {type?, layer?,
limit?, offset?}` (type, layer and handle of each model-space entity),
`dist {p1, p2}` (distance and angle), `area {points}` or `area {handle}`
(area and perimeter) and `drawing.inspect {entities: false}`. The file at
`path` is never changed: write the result to a new `out`.

Each call is capped so it cannot stall the device: an array or copy makes
at most 10,000 copies, and the copies of one call multiply to at most
10,000 (an array of an array counts as both); a drawing holds at most
200,000 objects; a hatch or linetype scale is at least 0.0001; and a
render whose dashes, hatch lines and block copies would take more than
about a second is refused before it draws. A command over a cap refuses
the whole call and says which cap.

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

1. Draw a 4 by 3 m room with a column on its own layer, as DXF:
   `cad.run {"cmds": [{"id": "new", "params": {"metric": true}}, {"id": "layer.new", "params": {"name": "WALLS", "current": true}}, {"id": "rectang", "params": {"p1": [0, 0], "p2": [4000, 3000]}}, {"id": "layer.new", "params": {"name": "COLUMNS", "current": true}}, {"id": "circle", "params": {"center": [2000, 1500], "radius": 150}}], "out": "room.dxf"}`.
2. What is on the walls layer, a distance and an area, without writing anything:
   `cad.run {"path": "room.dxf", "cmds": [{"id": "entities", "params": {"layer": "WALLS", "limit": 100}}, {"id": "dist", "params": {"p1": [0, 0], "p2": [4000, 3000]}}, {"id": "area", "params": {"points": [[0, 0], [4000, 0], [4000, 3000], [0, 3000]]}}]}`.
3. A PDF and a 1600-pixel PNG of it:
   `cad.run {"path": "room.dxf", "cmds": [], "out": "room.pdf"}`, then
   `cad.run {"path": "room.dxf", "cmds": [], "out": "room.png", "max_side": 1600}`.
4. Dimension the long wall and save as DWG:
   `cad.run {"path": "room.dxf", "cmds": [{"id": "dimlinear", "params": {"p1": [0, 0], "p2": [4000, 0], "at": [2000, -500]}}], "out": "room-dim.dwg"}`.

## The engine's commands

`commands.md` in this skill's folder lists every cadcraft command (the
AutoCAD-style names: `line`, `offset`, `dimlinear`, ...), one line each,
with a tag on those that reach past the open drawing. Grep it
(`grep -i hatch commands.md`) for the ids and parameters a request needs:
`cad.run` runs the untagged ones but `setvar`, and refuses the rest.
