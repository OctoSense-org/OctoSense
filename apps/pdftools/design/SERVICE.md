# PDF Tools v2: pdf service contract

The app-facing methods PDF Tools v2 calls over `host.request`. The service
(`apps/pdf/host-service`) implements them; the app (`apps/pdftools/bundle`)
calls nothing else. Both sides build against this file, and a change to it
goes to both. Derived from `BRIEF.md` (Actions and the engine).

## Ground rules

- **Callers:** system apps only (`may_call`: `os.*`), as today. The system
  agent's five `pdf.*` tools (`tools.json`) do not change.
- **Where:** every path is relative to the caller's area, which for an app's
  own request is its storage (`octosense_engine_area::Slot::area`), resolved
  and contained as today. A write never replaces a file the call didn't open
  for editing.
- **Open documents:** `pdf.open` returns a document handle; later calls name it
  as `doc`. The service keeps the engine's open document (its edits and undo
  history) between calls, keyed by the caller app, its storage scope and the
  handle. A handle from another app or scope is refused as unknown. A caller has
  at most 8 open documents; the 9th open is refused with `too_many_open`.
  `pdf.close` and the app closing (its storage scope going away) release them.
- **Scripts:** a document's own JavaScript never runs: every session turns the
  engine's JavaScript off before opening, as `session()` does today.
- **Speed:** engine work still runs on the shell's UI thread (#399). Every
  method must answer a typical page in well under 100 ms; caps below keep the
  worst case bounded.
- **Errors:** `Err` strings start with a stable code, then a colon and plain
  words for a person: `not_found:`, `damaged:` (the engine's reason after it),
  `protected:` (needs a password; v2 does not open these), `storage_full:`,
  `too_many_open:`, `unknown_doc:`, `unsaved:`, `invalid:`, `too_large:`.
  `invalid:` also covers a path outside storage, a taken output name, nothing
  to undo or redo, and editing a read-only document.
- **Geometry:** points, origin at the top-left of the displayed page, y down,
  as the engine's `text_find` and `comment_add` use. Rotation is already
  applied. Every rectangle is `[x, y, w, h]` in points: the `lines` boxes,
  the `comments` rects, the `fields` rect and the `rects` input of a comment
  add. Find's match rects can go straight into a highlight.
- **Handles:** `doc` is a string (lowercase hex).

## Documents

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.open` | `{path}` | `{doc, path, pages, sizes: [[w, h], …], title, outline: [{title, page, children}], fields, comments, can_edit}`; `fields` and `comments` are counts |
| `pdf.close` | `{doc}` | `{closed: true}`; refuses `unsaved:` unless `{discard: true}` |
| `pdf.state` | `{doc}` | `{edited, can_undo, can_redo, pages}` |
| `pdf.info` | `{path}` (as today) or `{doc}` | as today, plus `{doc}` works on the open document |

Caps: the file is at most 128 MiB (`MAX_PDF_BYTES`).

## Reading

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.page` | `{doc, page, dpi?}` | `{path, width, height, dpi}`: the page rendered as a PNG written to `.cache/pages/<doc>/<page>@<dpi>.png` in storage |
| `pdf.find` | `{doc, query, limit?}` | `{total, matches: [{page, rects: [[x, y, w, h], …], snippet}]}`, snippet ≤ 120 characters with the match marked `[[…]]` |
| `pdf.lines` | `{doc, page}` | `{lines: [{n, text, box, font, size}], paragraphs: [{n, text, box, lines, font, size}]}` |
| `pdf.text` | `{path, pages?}` (as today) or `{doc, pages?}` | as today |

Caps: `dpi` 24 to 300 (default 96), and at most 16 megapixels per render. A
fractional `dpi` is rounded; one out of range is `invalid:`, not clamped. A
single render whose PNG is over 16 MiB is `too_large:`. `limit` at most 500
(default 200); `total` saturates at 10,000. The render cache is at most 16 MiB
and 64 files per app (app storage allows 256 entries in all, and imports and
library saves need room): the oldest renders go first, and it is cleared
before a write would fail with `storage_full:`. A render of an edited
document reflects its unsaved edits.

## Comments

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.comments` | `{doc}` | `{comments: [{id, page, type, author, text, date, color, status, rects, replies: [{id, author, text, date}]}]}` |
| `pdf.comment` | `{doc, op: "add", page, type, rects?, at?, color?, text?, author?}` | `{id}`; `type` is `highlight`, `underline`, `strikeout`, `note` or `textbox` |
| `pdf.comment` | `{doc, op: "edit", id, text?, color?}` | `{id}` |
| `pdf.comment` | `{doc, op: "delete", id}` | `{deleted: true}` |
| `pdf.comment` | `{doc, op: "reply", id, text, author?}` | `{id}` (the reply's) |
| `pdf.comment` | `{doc, op: "status", id, status}` | `{id}`; `status` is `accepted`, `rejected`, `cancelled`, `completed` or `none` |

A comment's `date` is local time without a zone or seconds
(`2026-10-10T22:21`), and its `rects` is one bounding box. An unnamed comment's
id is `@page-index`. Fill & Sign marks appear in the list. Without `author`
the engine writes "PdfCraft": always pass the person's name (PDF Tools asks
once, "Comment as", and keeps it in its storage).

## Fill & Sign

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.fields` | `{doc}` | `{fields: [{name, type, value, required, read_only, page, rect, options}]}`; `options` are always `{value, label}`; `read_only` is being added (until then a missing value means false) |
| `pdf.fill` | `{doc, values: {name: value}}` | `{filled: n}` (one undo step) |
| `pdf.fill_sign` | `{doc, page, kind, at: [x, y], text?}` | `{added: true}`; `kind` is `text`, `date`, `initials`, `check` or `cross` |

`pdf.fill` uses the engine's `form_fill` with JavaScript off. The service
review must confirm that no field script runs.

## Pages

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.pages` | `{doc, op: "rotate", pages, angle}` | `{pages}` (the new count); `angle` is ±90, 180 or −180 |
| `pdf.pages` | `{doc, op: "delete", pages}` | `{pages}`; deleting every page is refused |
| `pdf.pages` | `{doc, op: "move", pages, to}` | `{pages}` |
| `pdf.pages` | `{doc, op: "duplicate", pages}` | `{pages}` |
| `pdf.pages` | `{doc, op: "insert_file", path, at, pages?}` | `{pages}`; `path` another PDF in storage |
| `pdf.pages` | `{doc, op: "extract", pages, out}` | `{path}`; `out` a new file in storage |

Caps: at most 512 pages named in one call.

## Edit

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.edit_text` | `{doc, page, paragraph?, line?, text}` | `{edited: true}`; exactly one of `paragraph` or `line`, numbers from `pdf.lines` |

## History and saving

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.undo` | `{doc}` | `{edited, can_undo, can_redo}` |
| `pdf.redo` | `{doc}` | `{edited, can_undo, can_redo}` |
| `pdf.save` | `{doc, path?}` | `{path, bytes}`: an incremental save to the document's own file, or a full save to a new `path` (never over an existing file), which then becomes the open document's file. A plain save from a background surface answers `invalid:` |

## Combine, split, export

| Method | Arguments | Answer |
| --- | --- | --- |
| `pdf.merge` | `{paths: [path or {path, pages}], out}` | as today; `pages` is a range string such as `"1-4, 9"`, and accepts `"all"`, an empty string and typed dashes |
| `pdf.split` | as today | as today |
| `pdf.export` | `{doc, kind: "images", out_dir, dpi?, pages?}` or `{doc, kind: "text", out, pages?}` | `{paths}` |

Caps: `pdf.merge` 2 to 16 inputs, as today. `pdf.export` images at most 300 dpi,
64 pages and 256 megapixels in total per call, each page at most 16 MP,
writing new files only.
