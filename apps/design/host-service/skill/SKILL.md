---
name: design-engine
description: Page layout (InDesign-class .designcraft and IDML): inspect pages, stories and styles, render pages, export PDF, IDML or EPUB. Read before using design.* tools.
---

# Design engine

designcraft is a headless page-layout engine in the InDesign tradition:
spreads and master pages, frames and threaded stories, paragraph and
character styles, a Knuth-Plass composer, tables, swatches, and IDML
interchange. It exports print-ready PDF (including PDF/X), IDML and EPUB.

## Tools

- `design.info {path}`: page count and page settings, spreads, stories (with overset text), styles and swatches.
- `design.render {path, out, page?, max_side?}`: one page (0-based, default 0) as a PNG, its longest edge `max_side` pixels (default 1024).
- `design.export {path, out, pdf?}`: the document as `.pdf`, `.idml`, `.epub` or `.designcraft`, by the extension of `out`; for a PDF, `pdf` takes the engine's options (`pages` such as `"2-3"`, `spreads`, `bleed`, `marks`, `standard` none, x4 or a2b, `title`, `author`, view settings).

`design.info` only reads; the others write `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the design engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the design engine
cannot reach it. A document whose graphics are linked rather than embedded
(an IDML's links, a placed SVG's linked image), or whose `Document Fonts`
folder leads outside your workspace, is refused: ask for a copy with its
images embedded.

## Examples

1. Find overset text before printing: `design.info {"path": "brochure.idml"}`
   and look at each story's overset.
2. A print PDF of pages 2 and 3 with bleed and crop marks:
   `design.export {"path": "brochure.idml", "out": "brochure-print.pdf", "pdf": {"pages": "2-3", "bleed": true, "marks": true, "standard": "x4"}}`.
3. A preview of the cover: `design.render {"path": "brochure.idml", "out": "cover.png", "max_side": 800}`.

## The engine's commands

`commands.md` in this skill's folder lists every designcraft command, one
line each (id, label, parameters), with a tag on those that reach past the
open document. Grep it (`grep -i footnote commands.md`) when the person asks
what the engine can do. No tool on your list runs these ids: they show the
engine's reach, not what you can call.
