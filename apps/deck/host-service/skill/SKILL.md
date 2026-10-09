---
name: deck-engine
description: Presentations (pptx, .deckcraft, outline text): create and edit with the engine's commands, read, render a slide to PNG, convert, also to pdf. Read before using deck.* tools.
---

# Deck engine

deckcraft is a headless presentation engine. It reads and writes PowerPoint
`.pptx`, its own `.deckcraft` and outline text, renders slides, and writes
PDF. It knows slides, layouts, masters, sections, themes, shapes, tables and
charts.

## Tools

- `deck.info {path}`: the slides with their titles, layouts and shape counts, the sections and the theme. Reads only.
- `deck.run {path?, cmds, out?, slide?, max_side?}`: up to 64 of the engine's commands, run in order on the presentation at `path` (or on a new blank one, 16:9 with no slides), then written to `out` when given: `.pptx`, `.deckcraft`, outline `.txt` or `.pdf` by its extension, or one slide (`slide`, 0-based, default 0) as a `.png` whose longest edge is `max_side` pixels (default 1024). Each command is `{"id": ..., "params": {...}}`. The answer has each command's result in `results`, so query commands read without writing anything; `"cmds": []` with an `out` converts or renders.

## How `deck.run` works

The door checks every command of a call before it runs any. A command that
`commands.md` lists untagged works on the open presentation only, and runs.
So do `insert.picture`, `insert.audio`, `insert.video` and
`picture.change`, whose `path` must name a file in your workspace (at most
64 MB read in one call). Every other tagged id ([file], [device], [host])
and any id that is not in `commands.md` refuse the whole call, and nothing
is written.

The commands act like the app's own menus on one open presentation with a
current slide. `slide.new {layout?, title?, body?}` adds a slide after the
current one and makes it current (`body` lines become bullets; layouts
include `title`, `titleAndContent`, `titleOnly` and `blank`), and
`slide.last` goes to the end first. Useful queries: `document.inspect`
(every slide's title, layout and shapes) and `slide.inspect {index}` (one
slide's shapes with their ids and text, which `text.set {id, text}`
replaces). For the outline as text, write `out` as a `.txt` and read it.
The file at `path` is never changed: write the result to a new `out`.

Each call is capped so it cannot stall the device: a table has at most
5,625 cells (75 × 75), a chart at most 10,000 data points, a slide at
most 1920 × 1080 points of area, and the presentations at most 500
slides, 20,000 shapes and 1,000,000 characters. A command over a cap
refuses the whole call and says which cap.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the deck engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the deck engine
cannot reach it. A PDF that `deck.run` writes opens with `pdf.*`, and a
slide it renders shows with `view_image`.

## Examples

1. A three-slide deck, then a PDF of it:
   `deck.run {"cmds": [{"id": "slide.new", "params": {"layout": "title", "title": "Launch plan"}}, {"id": "slide.new", "params": {"title": "Goals", "body": "Ship in May\nTwo pilots"}}, {"id": "slide.new", "params": {"title": "Next steps", "body": "Budget\nHiring"}}], "out": "launch.pptx"}`,
   then `deck.run {"path": "launch.pptx", "cmds": [], "out": "launch.pdf"}`.
2. What a deck says, slide by slide, without writing anything:
   `deck.run {"path": "launch.pptx", "cmds": [{"id": "document.inspect"}, {"id": "slide.inspect", "params": {"index": 1}}]}`;
   or its outline as text: `deck.run {"path": "launch.pptx", "cmds": [], "out": "launch.txt"}`.
3. Render its second slide: `deck.run {"path": "launch.pptx", "cmds": [], "out": "slide2.png", "slide": 1}`.
4. Append a slide with a picture from your workspace:
   `deck.run {"path": "launch.pptx", "cmds": [{"id": "slide.last"}, {"id": "slide.new", "params": {"layout": "titleOnly", "title": "The chart"}}, {"id": "insert.picture", "params": {"path": "chart.png"}}], "out": "launch-2.pptx"}`.

## The engine's commands

`commands.md` in this skill's folder lists every deckcraft command, one line
each (id, label, parameters), with a tag on those that reach past the open
deck. Grep it (`grep -i chart commands.md`) for the ids and parameters a
request needs: `deck.run` runs the untagged ones and the four media reads
above, and refuses the rest.
