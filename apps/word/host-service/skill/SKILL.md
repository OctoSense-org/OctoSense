---
name: word-engine
description: Word documents (docx, md, html, rtf, odt, txt): create from text, read, inspect structure, convert, also to pdf or png. Read before using word.* tools.
---

# Word engine

wordcraft is a headless word processor. It reads docx, Markdown, HTML, RTF,
ODT, plain text and its own JSON, and writes all of those plus PDF and PNG.
It understands paragraphs and their styles, runs, tables, sections, comments
and document properties.

## Tools

- `word.info {path}`: pages, words, paragraphs, sections, comments and document properties.
- `word.text {path}`: the plain text, with word and paragraph counts.
- `word.inspect {path, text?}`: the blocks (paragraphs with style and runs, tables with cells); `text: false` leaves the text out.
- `word.convert {path, out, format?}`: the document written as docx, md, html, rtf, odt, txt, json, pdf or png; `format` wins over the extension of `out`.
- `word.new {out, text?, title?}`: a new document from plain text, one paragraph per line, with an optional title property; usually `.docx`.

The first three only read; `word.convert` and `word.new` write `out`.

## Files

Every path is relative to the word engine's folder, a private workspace that
only `word.*` tools read and write. Absolute paths, `..` and links out of it
are refused. Your own workspace (`read_file`, `write_file`, `view_image`),
the person's files and the other engines' folders are outside it, and what
these tools write stays in it: a PDF that `word.convert` makes cannot be
opened by `pdf.*` or shown from your workspace. So work on documents that
`word.new` or an earlier call made, and pick a new name for each `out`. If
the person names a file of their own, say that the word engine cannot reach
it yet.

## Examples

1. Draft a memo and make a PDF of it:
   `word.new {"out": "memo.docx", "title": "Q3 memo", "text": "Revenue grew 12 percent.\nCosts fell."}`,
   then `word.convert {"path": "memo.docx", "out": "memo.pdf"}`.
2. Turn that memo into Markdown and read it back:
   `word.convert {"path": "memo.docx", "out": "memo.md"}`, then `word.text {"path": "memo.md"}`.
3. Check a long document's outline without its text:
   `word.inspect {"path": "memo.docx", "text": false}`.

## The engine's commands

`commands.md` in this skill's folder lists every command of wordcraft's
registry, one line each (id, label, parameters), with a tag on those that
reach past the open document. Grep it (`grep -i table commands.md`) when the
person asks what the engine can do. No tool on your list runs these ids:
they show the engine's reach, not what you can call.
