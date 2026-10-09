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

Every path is relative to the deck engine's folder, a private workspace that
only `deck.*` tools read and write. Absolute paths, `..` and links out of it
are refused. Your own workspace (`read_file`, `write_file`, `view_image`),
the person's files and the other engines' folders are outside it, and what
these tools write stays in it: a PDF that `deck.convert` makes cannot be
opened by `pdf.*`, and a slide `deck.render` draws cannot be shown from your
workspace. So work on decks that `deck.new` or an earlier call made, and
pick a new name for each `out`. If the person names a file of their own, say
that the deck engine cannot reach it yet.

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
