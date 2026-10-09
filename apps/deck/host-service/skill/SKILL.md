---
name: deck-engine
description: Presentations (pptx, .deckcraft, outline text): create from titles and bullets, read as outline, render a slide to PNG, convert, also to pdf. Read before using deck.* tools.
---

# Deck engine

deckcraft is a headless presentation engine. It reads and writes PowerPoint
`.pptx`, its own `.deckcraft` and outline text, renders slides, and writes
PDF. It knows slides, layouts, masters, sections, themes, shapes, tables and
charts.

## Tools

- `deck.info {path}`: the slides with their titles, layouts and shape counts, the sections and the theme.
- `deck.text {path}`: the deck as outline text (titles unindented, bullets tab-indented by level) and the slide count.
- `deck.render {path, out, slide?, max_side?}`: one slide (0-based, default 0) as a PNG, its longest edge `max_side` pixels (default 1024).
- `deck.new {out, slides}`: a new deck from `[{title, bullets?}]` (1 to 200 slides, at most 64 bullets each); `out` ends in `.pptx` or `.deckcraft`.
- `deck.convert {path, out}`: the deck written as `.pptx`, `.deckcraft`, outline `.txt` or `.pdf`, by the extension of `out`.

`deck.info` and `deck.text` only read; the others write `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the deck engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the deck engine
cannot reach it. A PDF that `deck.convert` makes opens with `pdf.*`, and a
slide `deck.render` draws shows with `view_image`.

## Examples

1. A three-slide deck, then a PDF of it:
   `deck.new {"out": "launch.pptx", "slides": [{"title": "Launch plan"}, {"title": "Goals", "bullets": ["Ship in May", "Two pilots"]}, {"title": "Next steps", "bullets": ["Budget", "Hiring"]}]}`,
   then `deck.convert {"path": "launch.pptx", "out": "launch.pdf"}`.
2. Check what a deck says, slide by slide: `deck.text {"path": "launch.pptx"}`.
3. Render its second slide: `deck.render {"path": "launch.pptx", "out": "slide2.png", "slide": 1}`.

## The engine's commands

`commands.md` in this skill's folder lists every deckcraft command, one line
each (id, label, parameters), with a tag on those that reach past the open
deck. Grep it (`grep -i chart commands.md`) when the person asks what the
engine can do. No tool on your list runs these ids: they show the engine's
reach, not what you can call.
