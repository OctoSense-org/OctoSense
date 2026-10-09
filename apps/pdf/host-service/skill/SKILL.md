---
name: pdf-engine
description: PDF files: inspect pages, metadata, fonts and security, extract text in reading order, render a page to PNG, merge, split. Read before using pdf.* tools.
---

# PDF engine

pdfcraft is a headless PDF engine: it parses and repairs PDFs, extracts text
in reading order, renders pages, and edits page structure. Its full tool
table goes further (annotations, forms, redaction, signatures, OCR, compare,
optimise), but you reach only the five tools below.

## Tools

- `pdf.info {path}`: page count and page sizes, metadata, outline, fonts, security and repair notes.
- `pdf.text {path, pages?}`: the text of each page in reading order: every page of a document up to 512 pages, or the 1-based `pages` listed (at most 512).
- `pdf.render {path, page, out, max_side?}`: one page (1-based) as a PNG, its longest edge about `max_side` pixels (default 1024).
- `pdf.merge {paths, out}`: 2 to 16 PDFs combined in order into one, with one bookmark per file.
- `pdf.split {path, out_dir, every?, before?}`: files in `out_dir`, giving exactly one of `every` (pages per file) or `before` (the 1-based pages that start a new file); at most 256 files.

`pdf.info` and `pdf.text` only read; the others write.

## Files

Every path is relative to the pdf engine's folder, a private workspace that
only `pdf.*` tools read and write. Absolute paths, `..` and links out of it
are refused. Your own workspace (`read_file`, `write_file`, `view_image`),
the person's files and the other engines' folders are outside it, and what
these tools write stays in it: a PDF that `word.convert` or `deck.convert`
made is in its own engine's folder, not here. Nothing you have puts a PDF
into this folder: these tools open only files an earlier `pdf.*` call wrote
there, so a PDF the person has elsewhere cannot be opened yet. Say so rather
than guessing names, and pick a new name for each `out`.

## Examples

1. What a document is, then the text of its first three pages:
   `pdf.info {"path": "contract.pdf"}`, then `pdf.text {"path": "contract.pdf", "pages": [1, 2, 3]}`.
2. One file per chapter, chapters starting on pages 1, 9 and 23:
   `pdf.split {"path": "book.pdf", "out_dir": "chapters", "before": [9, 23]}`.
3. A cover sheet in front of a report: `pdf.merge {"paths": ["cover.pdf", "report.pdf"], "out": "report-with-cover.pdf"}`.

## The engine's tools

`commands.md` in this skill's folder lists every tool of pdfcraft's own
automation table, one line each (name, title and description, parameters),
with a tag on those that reach past the open document. Grep it
(`grep -i redact commands.md`) when the person asks what the engine can do.
No tool on your list runs these names: they show the engine's reach, not
what you can call.
