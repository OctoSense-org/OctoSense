# Quick Deck v2: deck service contract

The app-facing methods Quick Deck v2 calls over `host.request`. The service
(`apps/deck/host-service`) implements them, and the app
(`apps/quickdeck/bundle`) calls nothing else. Both sides build against this
file, and a change to it goes to both, as with PDF Tools'
`apps/pdftools/design/SERVICE.md`, which it follows. The decisions at the end
settle what the first draft left open. They are derived from
`BRIEF.md` ("Actions and the engine"). Each method lists the deckcraft
commands it runs (engine d0e57d7e; classes from `skill/safety.json`).

## Ground rules

- **Callers.** System apps only (`may_call`: `os.*`), as today. The system
  agent's two tools (`deck.info`, `deck.run`, `tools.json`) don't change. The
  v1 file methods (`info`, `text`, `render`, `new`, `convert`) stay until v1
  leaves `main`.
- **Where.** Every path is relative to the caller's area, which for an app's
  own request is its storage (`octosense_engine_area::Slot::area`), resolved
  and contained as today. `.cache/` belongs to the service. A write never
  replaces a file the call didn't open or name for saving.
- **Open documents.**
  - `deck.open` and `deck.create` return a document handle, and later calls
    name it as `doc`. Handles are strings of lowercase hex.
  - The service keeps the engine's open document between calls: one
    `deckcraft_engine::Session` per document, holding its edits, undo history,
    current slide and selection.
  - A document belongs to the caller app, its storage scope (the area's root)
    and its handle. A handle from another app or scope is refused as
    `unknown_doc:`.
  - A caller has at most 8 open documents, and the service at most 32 across
    callers. The next open is refused with `too_many_open:`.
  - A document is released by `deck.close` or when its app's isolate closes:
    a listener on App Hub's `services::on_isolate_closed`, as the pdf service
    has (`apps/pdf/host-service/src/docs.rs`). It is also released when its
    storage scope changes or goes, or, for a caller with no isolate, after
    15 minutes without a call.
  - The table is a `thread_local`, because every call arrives on the UI
    thread.
- **One undo step per call.** The engine makes each command its own undo
  step. After a change method runs, the service folds the steps it pushed
  into one, labelled by the action (for example "Format text"). The engine's
  `History` is public. Undo history is limited to 100 steps per document;
  the engine's default is 500 shared-slide snapshots.
- **One gate.** Every engine command a method runs passes the same allowlist
  as `deck.run`: the door built from `safety.json` and `REVIEWED`. So the
  door's limits (table cells, chart points, extents, line breaks, slide area)
  and its held commands (#448) apply to the app as to the agent. The door's
  64 commands a call are `deck.run`'s; each app method has its own most
  commands a call, with its reason beside it (decision 2), and every command
  is admitted before any runs. The `RunBudget` ceilings then hold after every
  command. The
  service builds every engine argument itself, and each method refuses a key
  it doesn't take. Nothing reaches `file.*` commands that touch paths,
  `show.*` start commands, `media.*` playback or the held ids.
- **Exact text.** An app session turns off the engine's `prefs.autocorrect`
  and `prefs.smart_quotes`, so text is stored as typed.
- **Speed.** Engine work still runs on the shell's UI thread (#399). A
  typical method (an inspect, a text edit or a format change) must answer in
  well under 100 ms. A slide render is one render, and the caps below bound
  open, save and export.
- **Errors.** `Err` strings start with a stable code, then a colon and plain
  words for a person. `invalid:` also covers a path outside storage, a taken
  output name, nothing to undo or redo, and a key the method doesn't take.

  | Code | Meaning |
  | --- | --- |
  | `not_found:` | no such file |
  | `damaged:` | the engine couldn't read it; the engine's reason follows |
  | `unsupported:` | not a presentation the engine reads: `.ppt`, `.key`, `.odp`, PDF |
  | `protected:` | an encrypted `.pptx`, which needs a password; v2 doesn't open these |
  | `held:` | its zip is beyond the import guard (#448) |
  | `too_large:` | over a cap below |
  | `storage_full:` | the storage has no room |
  | `too_many_open:` | the caller already has 8 open documents |
  | `unknown_doc:` | no such handle for this caller and scope |
  | `unknown_id:` | a slide or shape id the document doesn't have (an undo may have removed it) |
  | `unsaved:` | the document has unsaved changes |
  | `invalid:` | anything else the call got wrong |
- **Geometry.**
  - Units are points, with the origin at the top left of the slide and y
    down. Every box is `[x, y, w, h]`, and rotation is in degrees, clockwise.
  - The default slide is 960 × 540 (16:9); 4:3 is 720 × 540.
  - A canvas at zoom z shows one point as z layout points, so a tap at
    (x, y) on the slide's view is at (x/z, y/z) points.
- **Ids.**
  - Slides and shapes are named by the engine's ids (integers): `id` in
    `document.inspect` and `slide.inspect`. Ids stay stable while a document
    is open, and through a save and reopen in the native format.
  - Positions (`index`, `at`, `to`) are 0-based and change as slides move.
  - Methods name slides by id (`slide`, `slides`), except where a position is
    meant.
- **Revisions.**
  - Every document has a `rev`, bumped by each change, undo and redo.
  - Every change, undo and redo answers `changed`, the ids of the slides
    whose content changed, found by comparing the engine's copy-on-write
    slide pointers. A deck-wide change (theme, slide size, footer) lists every
    slide. The app re-renders only those slides.
- **Common answer.** Every change method answers
  `{rev, changed, edited, can_undo, can_redo}`, plus what the method lists.
- **Storage use** is the shell's: Quick Deck's status line reads
  `files.status`, which counts the service's cache too.

## Documents

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.open` | `{path}` | `{doc, path, title, size: [w, h], slides, theme, layouts: [{id, name, kind}], sections: [{name, first, count}], rev, edited: false, can_undo: false, can_redo: false, missing_fonts}` | `open_presentation` on bytes the service read (`.deckcraft`, `.pptx`, outline `.txt`/`.md`, by content); `document.inspect {}` |
| `deck.create` | `{out, theme?, size?, slides?: [{layout?, title?, body?, notes?}], outline?}` | as `deck.open` | `file.new {theme, size, blank: true}`; per slide `slide.new {layout, title, body}`, `slide.notes {text}`; or `slide.fromOutline {text}` |
| `deck.close` | `{doc, discard?}` | `{closed: true}`; refuses `unsaved:` unless `discard: true` | drops the session |
| `deck.state` | `{doc}` | `{rev, edited, can_undo, can_redo, undo, redo, slides}`, where `undo` and `redo` are the next step's labels | `document.inspect {}` |
| `deck.save` | `{doc, path?}` | `{path, bytes, rev}` | `save_bytes(doc, "deckcraft")`; the service writes the file (staged, then renamed) |

- `deck.open`:
  - `missing_fonts` lists the font families the deck names (in its theme and
    its text runs) that the engine draws with a substitute.
  - A deck opened from a `.pptx` or an outline has no native file yet, so its
    first `deck.save` must name `path`.
- `deck.create`:
  - `out` is a new `.deckcraft` file; the deck is written there as made.
  - Give at most one of `slides` (1 to 200) and `outline` (at most 200
    slides, the door's `OUTLINE_SLIDES`). With neither, the deck has one
    title slide.
  - `theme` is an engine theme name (Default is `"DeckCraft"`). `size` is
    `"16:9"`, `"4:3"` or `[w, h]`, within the door's slide area.
- `deck.save` writes the native format to the document's own file, or to a
  new `path`, which then becomes its file (never over another file).
  `.pptx` goes through `deck.export`.
- **Caps.** The file is at most 64 MiB (`MAX_DECK_BYTES`). An opened deck
  must be within the door's session ceilings: 500 slides, 20,000 shapes,
  2,000 on one slide, 50,000 table cells, 50,000 chart points, 1,000,000
  characters and 128 MiB of media. Otherwise the answer is `too_large:`.
- **Import guard** (decision 4). Before the engine sees a zip (`.pptx` or
  `.deckcraft`), the service reads its central directory, on every path
  that opens a file: `deck.open`, `deck.cover`, the v1 file methods and the
  agent's `deck.info`. It refuses with `held:` more than 10,000 entries, any
  entry declaring over 64 MiB uncompressed, a declared total over 256 MiB, a
  compression ratio over 100 to 1 (parts up to 100 KiB are exempt), a name
  over 1,024 bytes, and a zip inside the zip outside `ppt/embeddings/`. In a
  `.pptx`, an embedded Office object under `ppt/embeddings/` (a chart's
  workbook) is a zip too; it stays within every other bound, and neither the
  service nor the engine ever unzips it. A native deck has no embedded
  objects, so a nested zip in one is `held:`. A directory that doesn't parse, an entry that
  inflates past its declared size, an encrypted entry, an unknown
  compression method or a multi-disk zip is `damaged:`. Large (ZIP64)
  directories are read. A zip that is neither a deckcraft deck nor an Office
  package is `unsupported:`. An OLE file (`D0 CF 11 E0`) is
  `protected:` when it holds an `EncryptedPackage` stream, and
  `unsupported:` otherwise (a legacy `.ppt`).
- **Audio and video** open. The service carries a deck's clips as bytes: it
  draws a clip's poster (or a film or speaker glyph), and saves and exports
  them unchanged. Nothing decodes or plays them, and `media.info`,
  `media.posterFrame`, `insert.audio` and `insert.video` stay held. A deck
  holding more than 128 MiB of media is `too_large:`.

## Reading and drawing

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.slides` | `{doc}` | `{slides: [{id, index, title, layout, hidden, section, notes, transition, builds, comments}], sections, size}` | `document.inspect {}` |
| `deck.slide` | `{doc, slide}` | `{id, index, layout, background, hidden, notes, transition, builds, shapes: [{id, name, kind, box, rotation, placeholder?, preset?, text?, paragraphs?, fill?, line?, hidden?, locked?, alt_text?, table?, chart?, picture?}]}` | `slide.inspect {index}`, `text.get {id}` |
| `deck.hit` | `{doc, slide, at: [x, y], tol?, selected?: [ids]}` | `{hit: null}` or `{hit: {kind, id, handle?, index?, end?}}`, where `kind` is `shape`, `text`, `resize`, `rotate`, `adjust` or `line_end` | `Session::hit_test` on that slide, with `selected` as the selection, so handles hit |
| `deck.view` | `{doc, slide, width, edit?, editing?, step?}` | `{path, width, height, rev}`: the slide as a PNG in `.cache/slides/<doc>/` | `render_slide`, `RenderOpts {scale, edit, skip_text: editing, state}` |
| `deck.layouts` | `{doc, width?}` | `{layouts: [{id, name, kind, path}]}` | `render_layout` for each layout of the first master |
| `deck.themes` | `{}` | `{themes: [{name, label, colors, major, minor}], color_schemes: [name], font_schemes: [{name, major, minor}]}` | `design.themes {}` |
| `deck.fonts` | `{}` | `{families: [name]}`: what the engine can draw on this device | `FontDb::global().families()` |
| `deck.shapes` | `{}` | `{presets: [{name, label, category}]}` | `shape.presets {}` |
| `deck.text` | `{doc}` | `{outline}`: titles unindented, points tab-indented by level | `slides_to_outline` |
| `deck.format_state` | `{doc, slide, id, paragraph?}` | `{font, size, bold, italic, underline, strike, color, align, bullets, numbering, line_spacing}` | `edit.select {ids}` or `text.edit` and `text.selectParagraph`, then `format.state {}` |
| `deck.cover` | `{path, width?}` | `{path, width, height}`: slide 1 of the deck at `path` as a PNG in `.cache/covers/`, kept across closes | `open_presentation`, `render_slide` (no handle; not counted toward the 8) |

- `deck.slide`: `paragraphs` gives each paragraph's text and level, so the
  inspector can offer a paragraph picker. `text` is the box's text, with
  `\n` between paragraphs and `\u000b` for a line break.
- `deck.view`:
  - `width` is in pixels, 48 to 3840, with the longer side at most 4096 and
    at most 16 megapixels.
  - `edit: true` draws the editor view: placeholder prompts ("Click to add
    title") and dashed placeholder outlines.
  - `editing` names a shape whose text is left out, because the app draws a
    text field over it while the person types.
  - `step` draws the show view after that many builds: no editor marks, and
    hidden shapes hidden (see Present).
  - A render of an edited document shows its unsaved edits.
  - A render of an older `rev` of the same slide, size and mode is removed
    when the new one is written.
- **Cache.** Renders, layout pictures and covers together use at most 16 MiB
  and 64 files per app, at most 32 of them covers. App storage allows 256
  entries in all, and the library, images and exports need room too.
  - Slide renders go before covers, each oldest first.
  - The whole cache is cleared before a write would fail with
    `storage_full:`.
  - A cover's key is a hash of the file's storage-relative name, its size and
    its modification time, as `pdf.cover` does.

## Slides

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.slides_op` | `{doc, op: "new", at?, layout?, title?, body?}` | `{slide}` | `slide.new {at, layout, title, body}` |
| `deck.slides_op` | `{doc, op: "duplicate", slides}` | `{slides}` (each copy follows its original) | `slide.duplicate {index}` per slide |
| `deck.slides_op` | `{doc, op: "delete", slides}` | `{}`; deleting every slide is refused | `slide.delete {ids}` |
| `deck.slides_op` | `{doc, op: "move", slides, to}` | `{}` | `slide.move {from, to}`, in order |
| `deck.slides_op` | `{doc, op: "hide", slides, hidden}` | `{}` | `slide.hide {index, hidden}` |
| `deck.slides_op` | `{doc, op: "layout", slides, layout}` | `{}` | `slide.layout {layout, index}` |
| `deck.slides_op` | `{doc, op: "reset", slides}` | `{}` | `slide.reset {index}` |
| `deck.slides_op` | `{doc, op: "section_add", at, name}` / `"section_rename", section, name` / `"section_remove", section` / `"section_move", section, to` | `{sections}` | `section.add`, `section.rename`, `section.remove {index}` (keeps the slides), `section.move` |
| `deck.notes` | `{doc, slide, text}` | `{}` | `slide.notes {text, index}` |
| `deck.transition` | `{doc, slides \| all: true, kind, option?, duration?}` | `{}` | `transition.set {kind, option, duration, index}` per slide, or `transition.set` then `transition.applyAll {}` |

`layout` is a layout id from `deck.open` or a layout name. `kind` is an id
from the engine's transition catalog (`none`, `fade`, `push`, `wipe`, `split`,
`reveal`, `cover`, `morph`…). **Caps:** at most 500 slides named in one call;
notes at most 20,000 characters.

## Objects

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.insert` | `{doc, slide, kind: "text", rect?, text?}` | `{id}` | `insert.textBox {rect, text}` |
| `deck.insert` | `{doc, slide, kind: "shape", preset, rect?, text?}` | `{id}` | `shape.insert {preset, rect, text}` (the service centres a default `rect`) |
| `deck.insert` | `{doc, slide, kind: "picture", path, rect?}` | `{id, size: [w, h]}` | `insert.picture {path, rect}`, a reviewed read inside storage |
| `deck.insert` | `{doc, slide, kind: "table", rows, cols, rect?, cells?, style?}` | `{id}` | `insert.table {rows, cols, rect, data}` (the cells' text in one command), `table.style {style, id}` |
| `deck.insert` | `{doc, slide, kind: "chart", type, categories, series: [{name, values}], rect?, title?}` | `{id}` | `insert.chart {type, rect, categories, series}`, `chart.options {title, id}` |
| `deck.set_text` | `{doc, slide, id, text, cell?: [r, c]}` | `{id}` | `edit.select`, `text.edit {id, at, cell}`, `text.select {anchor, caret}`, `text.insert {text}`, `text.exit {}`; `text.set {id, text, cell}` when the box is empty |
| `deck.format` | `{doc, slide, ids, paragraphs?: [first, last], font?, size?, bold?, italic?, underline?, strike?, color?, highlight?, align?, bullets?, numbering?, level?, line_spacing?, space_before?, space_after?, anchor?, autofit?, columns?, margins?}` | `{}` | `edit.select {ids}` (or `text.edit` and `text.select` over the paragraphs), then `format.font`, `format.size`, `format.bold`, `format.italic`, `format.underline`, `format.strikethrough`, `format.color`, `format.highlight`, `format.align`, `format.bullets`, `format.numbering`, `format.indent` / `format.outdent`, `format.lineSpacing`, `format.paragraph`, `format.anchor`, `format.autofit`, `format.columns`, `format.margins` |
| `deck.shape` | `{doc, slide, ids, op, …}` (below) | `{ids}` | below |
| `deck.picture` | `{doc, slide, id, op: "replace", path}` / `"crop", left, top, right, bottom` / `"adjust", brightness?, contrast?, saturation?, grayscale?, transparency?` / `"reset"` / `"alt_text", text, decorative?` | `{}` | `picture.change {path, ids}` (reviewed read), `picture.crop`, `picture.adjust`, `picture.reset`, `shape.altText` |
| `deck.table` | `{doc, slide, id, op, …}`: `insert_row {row, below}`, `insert_col {col, right}`, `delete_row {row}`, `delete_col {col}`, `merge {from, to}`, `split {cell}`, `style {style}`, `options {header_row, total_row, banded_rows, first_column, last_column, banded_columns}`, `fill {cells, color}`, `col_width {col, width}`, `row_height {row, height}` | `{rows, cols}` | `table.insertRowAbove/insertRowBelow`, `table.insertColumnLeft/insertColumnRight`, `table.deleteRow`, `table.deleteColumn`, `table.merge`, `table.split`, `table.style`, `table.options`, `table.cellFill`, `table.columnWidth`, `table.rowHeight` |
| `deck.chart` | `{doc, slide, id, type?, categories?, series?, title?, legend?, data_labels?, gridlines?, palette?}` | `{}` | `chart.type {type, id}`, `chart.data {categories, series, id}`, `chart.options {title, legend, dataLabels, gridlines, palette, id}` |

`deck.shape` ops:

| `op` | Arguments | Engine |
| --- | --- | --- |
| `bounds` | `x, y, w, h` (one shape) | `shape.setBounds {x, y, w, h, ids}` |
| `move` | `dx, dy` | `shape.move {dx, dy, ids}` |
| `rotate` | `deg` | `shape.rotate {deg, ids}` |
| `flip` | `axis` | `shape.flip {axis, ids}` |
| `order` | `to`: `front`, `back`, `forward` or `backward` | `arrange.bringToFront`, `arrange.sendToBack`, `arrange.bringForward`, `arrange.sendBackward` |
| `align` | `edge`, `to`: `slide` or `selection` | `arrange.align {edge, to, ids}` |
| `distribute` | `dir` | `arrange.distribute {dir, ids}` |
| `group`, `ungroup` | | `arrange.group {ids}`, `arrange.ungroup {ids}` |
| `duplicate`, `delete` | | `edit.duplicate {ids}`, `edit.delete {ids}` |
| `fill` | `color`, `gradient` or `none`; `transparency?` | `shape.fill {…, ids}`, never with `picture` |
| `line` | `color?, width?, dash?, none?` | `shape.line {…, ids}` |
| `effects` | `shadow?, glow?, soft_edges?, reflection?` | `shape.effects {…, ids}` |
| `style` | `index` (0–41) | `shape.quickStyle {index, ids}` |
| `lock`, `hide` | `locked` / `hidden` | `shape.lock`, `shape.visible` |

- `deck.set_text` keeps formatting:
  - The service compares the box's text with `text` and replaces only the
    span that changed. The rest of the box keeps its runs, and the new text
    takes the formatting at the start of the span.
  - `\n` separates paragraphs, and `\u000b` is a line break. A slide's notes
    use `deck.notes` instead.
  - TextInput gives a script its text but never its selection, which is why
    the service works out the span.
- **Pictures** are PNG, JPEG, WebP, GIF, TIFF or BMP; the engine has no SVG.
  The deck keeps the picture's bytes, so the storage file can be removed
  afterwards.
- **Caps.** These are the door's: a table has at most 5,625 cells; a chart
  has at most 10,000 data points; a text has at most 1,000 line breaks; no
  position or size goes past 100,000 pt; and one call's picture reads total
  at most 64 MiB. Beyond the door, a call names at most 500 shapes, and a
  `cells` fill holds at most 1,024 cells, passed to `insert.table` as its
  `data`; a larger table gets the rest of its text cell by cell through
  `deck.set_text`.

## Design

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.design` | `{doc, theme?, colors?, fonts?, size?, scale?, background?, footer?}` | `{size}` | `design.theme {name}`, `design.colors {name}`, `design.fonts {name}`, `design.slideSize {preset \| w, h, scale}`, `design.background {color \| gradient \| style \| reset, all, index}`, `design.headerFooter {slideNumber, footer, footerText, date, hideOnTitle, all}` |

- `background` is `{color \| gradient \| reset, slides? \| all?}`, and never
  a picture. `design.background` is classed file because a non-string
  `picture` reads a path.
- `footer` is `{slide_numbers?, footer?, footer_text?, date?,
  hide_on_title?}`.
- `size` is `"16:9"`, `"4:3"` or `[w, h]`, within the door's 1920 × 1080 pt
  of area; `scale` is `fit` or `maximize`.

## History and saving

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.undo` | `{doc}` | the common answer, plus `{undo, redo}` labels | `edit.undo {}` (one folded step) |
| `deck.redo` | `{doc}` | the same | `edit.redo {}` |
| `deck.save` | (under Documents) | | |

## Present

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.show` | `{doc, from?}` | `{order: [{slide, steps, advance_after?}], loop}` | deckcraft-anim's `show_order`, `ShowState::steps`, and the show settings from `show.setup` |
| `deck.view` | `{doc, slide, width, step}` | as above | `render_slide` with `state` from `Timeline::state(shape, para, step, end)` |

- **What Present shows.** The show order leaves out hidden slides and keeps a
  custom show or range, as the engine's own show does. `steps` is the number
  of click builds on a slide. The app draws a slide at step 0 and goes
  through its steps before the next slide. Transitions and animations are not
  played: each step shows its end state. That is the honest limit of a
  Splash host.
- **Dependency.** `deckcraft-anim` is a new service dependency, at the same
  pinned deckcraft rev (decision 5). The engine crate doesn't depend on it.

## Export

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.export` | `{doc, kind: "pptx", out}` | `{path, bytes}` | `save_bytes(doc, "pptx")` |
| `deck.export` | `{doc, kind: "pdf", out, layout?, per_page?, slides?, include_hidden?, dpi?}` | `{path, bytes, pages}` | `pdf_bytes(doc, {layout, perPage, slides, includeHidden, dpi})` |
| `deck.export` | `{doc, kind: "images", out_dir, width?, slides?}` | `{paths}` | `render_png` per slide |
| `deck.export` | `{doc, kind: "outline", out}` | `{path, bytes}` | `save_bytes(doc, "outline")` |

- **Outputs** are new files only, inside storage and within the quota, and
  each export writes its outputs in full or not at all.
- **PDF.** `layout` is `slides`, `notes` or `handouts`. `per_page` (for
  handouts) is 1, 2, 3, 4, 6 or 9, and `dpi` is 72 to 200 (default 150).
- **Caps.** One call rasters at most 160 megapixels: the door's
  `MAX_RUN_RASTER`, about 70 default slides at 150 dpi. Images are at most
  64 slides, each at most 4096 px on its long side, and at most 256
  megapixels in all. A call over a cap answers `too_large:` before anything
  is written.

## Decisions (coordinator, 11 Oct 2026)

The maintainer approved the ten designs on 11 Oct 2026. The open questions of
the draft are settled as follows; both the service and the app build to this
file, and neither changes it without the coordinator.

1. **Shape of the API: grouped by `op`**, as `pdf.pages` and `pdf.comment`
   are, wherever several actions act on the same target (slides, an object's
   insert kinds, formatting); one method per distinct concern otherwise.
2. **The gate's count: a count of the method's own.** App methods build their
   engine commands from reviewed values; each method declares the most
   commands one call may make, with its reason beside it (a table fill sets
   at most 1,024 cells a call, in one `insert.table`; the app fills a larger
   table over several calls). Every command still passes the door's
   allowlist and its `Limit`s; chunking a call to dodge a cap is not allowed.
3. **Undo folding: the engine's public `History`, now.** One call is one undo
   step, proven by a test. A grouping API in deckcraft is a follow-up, not a
   blocker.
4. **The import guard: the service's own, before the engine sees a byte.**
   "Open a presentation from this device" goes through it: the zip's central
   directory read with bounds (entries, names, declared and actual sizes,
   compression ratio, nesting outside `ppt/embeddings/`), embedded audio and
   video refused, and every
   refusal answered as the Import guard above says (`held:`, `damaged:`,
   `protected:` or `unsupported:`) with plain words. Hostile
   fixtures (a zip bomb, a lying declared size, ten thousand entries, media)
   prove each refusal happens before the engine runs. Engine-side bounds stay
   tracked in #448.
5. **Present: `deckcraft-anim`, at the same pinned revision** (one revision
   per dependency). Build steps show their end states; nothing animates.
6. **The cache budget: PDF Tools' numbers** (16 MiB and 64 files, at most 32
   covers, page renders evicted first, the whole cache cleared before
   `storage_full:`). Quick Deck's storage is its own 64 MiB jail.
7. **Fonts: no bundling** (the maintainer decided for PDF Tools on 11 Oct
   2026, and it holds here). `deck.fonts` lists what the engine finds:
   system fonts and its last-resort face; themes fall back.
8. **Tests: the pdf service's standard.** Hostile fixtures for every reviewed
   read, `held:` and the import guard; one undo step per call; a render after
   a save and a reopen; a timing test (ignored by default). Engine crate
   tests are not in CI (#412), so they run by hand on every change.

BRIEF.md's open questions are settled too:

1. **Working format:** the library keeps native `.deckcraft` files and
   exports `.pptx`.
2. **Draft with AI** is in, through the host's `model.complete` (the `model`
   capability, Settings → AI providers) with the person's own provider; the
   app never sees a key and works without a provider.
3. **Opening `.pptx` from this device** ships behind the service's import
   guard (decision 4).
4. **Theme fonts:** system fallbacks (decision 7).
5. **Notes in Present:** the notes strip and timer, as screen 08 shows. A
   presenter layout (current and next slide) is a later design.

Quick Deck's manifest asks for a 1536 x 1024 window (`"schema_minor": 1`),
as PDF Tools does.

## As built in #503 (coordinator, 11 Oct 2026)

The deck service (OctoSense #503) follows this file with these settled
changes; where they differ from the sections above, they win.

- **Method-only commands.** `shape.fill` and `design.background` are classed
  `file` (a `picture` or `path` reads a file), so `deck.run` refuses them.
  An app method runs them through the door's `admit_method` with reviewed
  values only: a colour, a gradient, `none` or `reset`. The door refuses a
  `picture` or `path` from a method too.
- **How some commands run.** Hide, layout and transition go through
  `slide.selectSlides` and one command. Reset and a per-slide background
  each take a `slide.go` first. Theme, colours and fonts go to each master
  in use (at most 16). The engine copies the current slide on every edit,
  so this keeps `changed` right and copies few.
- **`deck.set_text`** runs `edit.select`, `text.edit`, `text.select` over
  the changed span, `text.delete`, `text.select` again and `text.insert`,
  so the new text takes the format of the span's first character, then
  `text.exit`. An empty box takes `text.set` only when no paragraph starts
  with a tab, because `text.set` reads a leading tab as an outline level.
- **More caps.**
  - A picture is at most 50 megapixels and PNG, JPEG, GIF, BMP, WebP or TIFF.
  - `deck.cover`: `width` 48 to 3840 px, default 320.
  - `deck.layouts`: at most 64 layouts and 16 megapixels a call.
  - `deck.create`: a title at most 2,000 characters, a body or notes at
    most 20,000, an outline at most 200,000.
  - `deck.set_text`: at most 100,000 characters, a cell 10,000.
  - A section name: at most 256 characters.
- **Answers.**
  - `deck.slides`: `notes` is a preview of at most 200 characters, and
    `layout` is the layout's id.
  - `deck.format`: `level` is relative (-8 to 8), and `highlight: "none"`
    clears it.
  - `deck.transition`: `duration` is in milliseconds.
  - `deck.show`: `steps` counts clicks only; a build that starts on its own
    is part of step 0. `advance_after` is in seconds, and only shows when
    the show uses timings.
- **Fonts at startup.** A deck call made while the engine's font warm-up is
  still running waits for it (`src/warm.rs`): the engine's font database
  can't load one family on two threads at once, which drew a title in the
  wrong fallback font. The wait is never longer than the warm-up, and work
  stays on the UI thread (#399).
- **Dependencies.** `deckcraft-anim` and `deckcraft-fonts`, at the pinned
  deckcraft revision.
- **Reading alike.** The import guard reads a zip only in a shape that it and
  the engine's own `zip` reader read the same way (matching entry counts, the
  directory filling exactly its declared span, the last end record, no data
  before the zip, extra fields by the reader's own rules), then reads the
  directory again with that reader and compares; any difference is
  `damaged:`. A ZIP64 end record carries at most 1 KiB beyond its 44 bytes.
- **The guard picks the reader.** A native deck is known by the engine's own
  marker and an Office package by a `[Content_Types].xml` entry in the
  directory the guard verified. That verdict picks the engine's reader, with
  the same steps `open_presentation` takes after its own check, so a `.pptx`
  whose content types come late (as LibreOffice writes them) opens.
- **`deck.hit`** treats `selected: []` as no selection.
- **Pictures** whose size can't be read are `invalid:`.
- **Audio and video (#511)** open, carried as bytes and never decoded, as
  "Audio and video" above says; that lifts decision 4's media refusal.

## Inspector states (#507)

The deck service also reports what the inspector shows, read from the open
document's model (read-only, no engine command):

- `deck.themes`: `color_schemes` is `[{name, colors}]`, each scheme's twelve
  `#rrggbb` colours in the engine's order (dk1, lt1, dk2, lt2, accent1 to
  accent6, hlink, folHlink).
- `deck.open` and `deck.state`: `color_scheme` and `font_scheme` (the first
  master's, whose theme `theme` names), and `footer` as
  `{slide_numbers, footer, footer_text, date, hide_on_title}`, the keys
  `deck.design {footer}` takes. `deck.state` gives `theme` too, so the
  inspector follows an undo or a redo without reopening.
- `deck.format_state`: `anchor` (`top`, `middle` or `bottom`; an imported
  deck may also say `justified` or `distributed`) and `autofit` (`none`,
  `shrink` or `resize`), as the box draws them: its own setting, else its
  layout's and master's, else the engine's default.
- `deck.slide`: a chart's `legend` (a position code, or `"none"`),
  `data_labels`, `gridlines` and `palette`; a table's `options`, as
  `{header_row, total_row, banded_rows, first_column, last_column,
  banded_columns}`, the keys `deck.table {op: "options"}` takes.
- An Office document that isn't a presentation (a `.docx` or `.xlsx`) is
  `unsupported:`.

## Playback (maintainer approved, 13 Oct 2026)

Quick Deck plays a deck's video and audio with the runtime's `Video` widget,
which hands the file to the operating system's own player (AVFoundation on
macOS and iOS, the platform player on Android). The deck engine's decoders
stay unused: the service never decodes, probes or transcodes a clip, and
`media.info`, `media.posterFrame`, `insert.audio` and `insert.video` stay
held (#448).

| Method | Arguments | Answer | Engine |
| --- | --- | --- | --- |
| `deck.media` | `{doc, slide, id}` | `{path, kind, ext, bytes, autoplay, loop, poster}` | none: the clip's bytes from the open document's model, written as they are |

- `id` is the shape id of a video or audio object; `deck.slide` reports such
  a shape with `media: {id, video, autoplay, loop}`. Any other shape is
  `unknown_id:`.
- `path` is a storage-relative file under `.cache/media/`, named by a hash of
  the clip's bytes, holding exactly the bytes the deck stores. `ext` is the
  deck's own extension for the clip, else one inferred from its declared
  content type (`mp4`, `m4v`, `mov`, `mp3`, `m4a`, `aac`, `wav`); a clip
  with neither answers `unsupported:`. `kind` is `video` or `audio`;
  `autoplay` and `loop` are the clip's own settings; `poster` says whether
  the clip has a poster image.
- A clip already written is reused (the same hash). The media cache holds at
  most 32 MiB and 8 files per app, oldest first; a document's clips are
  removed when it closes. If a clip doesn't fit, the render cache and then the
  media cache are cleared before the write, and a clip that still doesn't fit
  is `storage_full:` ("There isn't room in Quick Deck's storage to play this
  clip.").
- Writes are staged and never replace a file the call didn't create, with
  one exception: a stale entry at a clip's own name (a file that doesn't
  hold the clip's bytes, or a link, which is never followed) is removed and
  the clip written there anew.
- **Where it plays:** the app overlays a `Video` widget on the clip's box,
  with the file as its storage source, in the editor (a play control on a
  chosen clip) and in Present (autoplay clips start when their slide shows;
  a tap plays or pauses; moving on or Exit stops). Where the `Video` widget
  has no player (today: Linux and Windows desktops), the app keeps the poster
  and says "Playback isn't available on this system".

As built in #515 (coordinator, 13 Oct 2026); where these differ from the
bullets above, they win:

- `deck.slide`'s `media.id` is the clip's media item id in the deck (two
  shapes that play one item share it); `deck.media` takes the shape's own
  `id`.
- A clip larger than the whole media cache (32 MiB) is `storage_full:` at
  once, with nothing cleared.
- A clip the deck holds no bytes for (linked to a file elsewhere, which the
  service never follows, or with a missing or empty item) is
  `unsupported:`.
- "Oldest first" means used longest ago: a clip handed out again counts as
  just used (its file's modification time is set to now), so a slide's
  other clip can't push it out before it plays. Clips no open document uses
  go before any that one does.
- Anything else at a clip's file name (a file cut short, or a link) is a
  stale entry: it goes, and the clip is written anew, create-only. A folder
  there is `invalid:`.
- Clips no open document uses (an earlier run's, or a released document's)
  go when a document next opens in that storage.
- `deck.media` finds a clip inside a group too; `deck.slide` lists only
  top-level shapes.
- From a `.pptx`: deckcraft at `d0e57d7e` doesn't read a clip's autoplay or
  loop from a package, so such a clip answers `autoplay: false` and
  `loop: false` (a native deck keeps both). And deckcraft's `.pptx` writer
  gives a clip without a poster a placeholder picture, so after a round
  trip through a `.pptx` every clip answers `poster: true`.
