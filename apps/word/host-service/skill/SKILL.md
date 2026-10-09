---
name: word-engine
description: Word documents (docx, md, html, rtf, odt, txt): create and edit with the engine's commands, read, convert, also to pdf or png. Read before using word.* tools.
---

# Word engine

wordcraft is a headless word processor. It reads docx, Markdown, HTML, RTF,
ODT, plain text and its own JSON, and writes all of those plus PDF and PNG.
It understands paragraphs and their styles, runs, tables, sections, comments
and document properties.

## Tools

- `word.info {path}`: pages, words, paragraphs, sections, comments and document properties. Reads only.
- `word.run {path?, cmds, out?, format?}`: up to 64 of the engine's commands, run in order on the document at `path` (or on a new empty document), then the document written to `out` when given, as docx, md, html, rtf, odt, txt, json, pdf or png (`format` wins over the extension of `out`). Each command is `{"id": ..., "params": {...}}`. The answer has each command's result in `results`, so query commands read without writing anything; `"cmds": []` with an `out` converts.

## How `word.run` works

The door checks every command of a call before it runs any. A command that
`commands.md` lists untagged works on the open document only, and runs. So
do `insert.picture` and `picture.change`, whose `path` must name an image
in your workspace. Every other tagged id ([file], [code], [network],
[device], [host]) and any id that is not in `commands.md` refuse the whole
call, and nothing is written.

The commands act like the app's own menus on one open document with a
caret: typed text goes where the caret is, a new document starts empty with
the caret at its start, `caret.docStart` and `caret.docEnd` move it, and a
paragraph style applies to the caret's paragraph. Useful queries:
`document.text` (the plain text with counts) and `document.inspect` (the
blocks: paragraphs with style and runs, tables with cells; `"text": false`
leaves the text out). The file at `path` is never changed: write the result
to a new `out`.

Each call is capped so it cannot stall the device: a table has at most
10,000 cells, a page is 72 to 1584 points on a side, a replacement may
grow the text at most 1,000-fold (and replacements chained in one call
10,000-fold), and a document holds at most 500,000 characters, 50,000
paragraphs and 128 MiB of pictures; a PDF is at most 10,000 pages. A
command over a cap refuses the whole call and says which cap.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the word engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the word engine
cannot reach it. A PDF that `word.run` writes opens with `pdf.*`, and a PNG
with `view_image`.

## Examples

1. Draft a memo with a heading and make a PDF of it:
   `word.run {"cmds": [{"id": "file.properties", "params": {"title": "Q3 memo"}}, {"id": "text.insert", "params": {"text": "Q3 memo"}}, {"id": "para.style", "params": {"style": "Heading 1"}}, {"id": "text.newParagraph"}, {"id": "para.style", "params": {"style": "Normal"}}, {"id": "text.insert", "params": {"text": "Revenue grew 12 percent. Costs fell."}}], "out": "memo.docx"}`,
   then `word.run {"path": "memo.docx", "cmds": [], "out": "memo.pdf"}`.
2. Read a document's text and its outline without writing anything:
   `word.run {"path": "memo.docx", "cmds": [{"id": "document.text"}, {"id": "document.inspect", "params": {"text": false}}]}`.
3. Replace a word everywhere and save the result as Markdown:
   `word.run {"path": "memo.docx", "cmds": [{"id": "edit.replaceAll", "params": {"text": "Revenue", "with": "Turnover"}}], "out": "memo-v2.md"}`.
4. Append a table and a picture from your workspace:
   `word.run {"path": "memo.docx", "cmds": [{"id": "caret.docEnd"}, {"id": "text.newParagraph"}, {"id": "insert.table", "params": {"rows": 2, "cols": 3}}, {"id": "caret.docEnd"}, {"id": "insert.picture", "params": {"path": "chart.png", "width": 200}}], "out": "memo-v3.docx"}`.

## The engine's commands

`commands.md` in this skill's folder lists every command of wordcraft's
registry, one line each (id, label, parameters), with a tag on those that
reach past the open document. Grep it (`grep -i table commands.md`) for the
ids and parameters a request needs: `word.run` runs the untagged ones and
the two picture commands above, and refuses the rest.
