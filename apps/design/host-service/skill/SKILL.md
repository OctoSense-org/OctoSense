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

Every path is relative to the design engine's folder, a private workspace
that only `design.*` tools read and write. Absolute paths, `..` and links
out of it are refused. Your own workspace (`read_file`, `write_file`,
`view_image`), the person's files and the other engines' folders are outside
it, and what these tools write stays in it. Nothing you have puts a document
into this folder: these tools open only files an earlier `design.*` call
wrote there, so a layout the person has elsewhere cannot be opened yet. Say
so rather than guessing names, and pick a new name for each `out`.

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
