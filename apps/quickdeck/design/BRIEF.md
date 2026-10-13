# Quick Deck v2: product requirements

Status: draft for review, 11 Oct 2026. This brief starts the OctoSense App
Flow for a new design of Quick Deck (`os.quickdeck`), as PDF Tools v2 did
(`apps/pdftools/design/`). The order is: product requirements (this file),
UX design by image generation (`source/prompts.md`; generating the images is
a separate, paid step the maintainer approves), pixel mapping to Splash
widgets, then the logic wired to the deck engine through the service contract
drafted in `SERVICE-DRAFT.md`. It replaces v1's design; v1 stays on `main`
until v2 is accepted.

## Why a new design

v1 (#420) is a phone-style wizard on a desktop: a deck list, then four steps
(Outline, Generate, Review, Export) in an 840-point column. It calls four of
the deck service's file methods (`deck.new`, `deck.info`, `deck.render`,
`deck.convert`), each on a fresh engine session, and edits nothing. A deck
can't be edited at all: to change a word you rewrite the outline and generate
the whole deck again. There is no theme or layout choice, no picture, shape,
table or chart, no undo, no speaker notes and no way to present. Someone used
to a desktop presentation app finds nothing they expect.

The engine underneath, deckcraft at d0e57d7e, is a full presentation editor
that OctoSense links without its own UI. It has 222 commands covering slide
masters and eleven layouts, eight themes, about 150 preset shapes, pictures,
tables, charts, transitions, builds, speaker notes, sections, an undo history,
and PowerPoint import and export. v2 gives it a desktop editor in the same
family as PDF Tools v2.

v1's "Generate" step is not AI. The engine lays out an outline the person
typed: `deck.new` turns titles and points into slides. v2 keeps that path as
"New from an outline". It may add drafting the outline with the host's model,
as Photos' Memories does (open question 2).

## Who it is for, and what they do

People who make and give short presentations on an OctoSense desktop: a
quarterly update, a lesson, a pitch, a field-trip briefing. Their jobs, in
order of frequency:

1. **Edit**: change a slide's text in place, format it (font, size, bold,
   colour, alignment, bullets), and change a slide's layout.
2. **Organize slides**: add, duplicate, delete, hide and reorder slides, in the
   navigator or in a slide sorter, and group them into sections.
3. **Insert**: a text box, a picture from the app's storage (or brought in from
   this device), a shape, a table and a chart, then format and arrange them.
4. **Present**: show the deck with the app's chrome hidden, go forward and
   back (builds step by step), and leave with a visible Exit control.
5. **Write speaker notes** for each slide, and read them while presenting.
6. **Start a deck** from a theme or template, or from an outline, which the
   person types or (open question 2) has the model draft.
7. **Change the look**: theme, colour variant, font pair, background, slide
   size, footer and slide numbers, transitions.
8. **Export** PowerPoint (`.pptx`), PDF (slides, notes pages or handouts) or
   one picture per slide, inside the app's storage.
9. **Keep a library** of decks: open, duplicate, remove, and open a `.pptx`
   from this device.

## Platform and constraints

- **OctoSense desktop only.** The deck engine is behind the desktop-only
  `craft-engines` feature. It's a contained Splash system app, id
  `os.quickdeck`. It declares `storage`, `files` and `deck`, plus `model` if
  open question 2 is yes.
- **The window.** The manifest asks for the designs' size, as PDF Tools does:
  `"schema_minor": 1, "window": {"width": 1536, "height": 1024}`. The shell
  clamps it to the desk and keeps it above the dock. On the hidden 1400 × 898
  test shell, the app area is about 1292 × 662 points: the wide layout
  (1180 points and up), with navigator, canvas and inspector side by side.
  Below 1180 points (the 990 × 603 default window of a shell without the
  hint), the inspector opens over the canvas from its toolbar button, and the
  navigator narrows.
- **Storage.** The app's storage holds 64 MiB and at most 256 entries, files
  and folders together.
  - A picture or a `.pptx` comes in through `files.import`: one document of up
    to 64 MiB, titled with its own name. `files.pick_photo` takes only PNG,
    JPEG or WebP up to 1 MiB, so it is the second choice.
  - Exports stay in the app's storage for v2's first release, because
    `files.export` reads at most 1 MiB, and most decks with pictures are
    bigger. A deck keeps its pictures inside its own file, so an imported
    picture can be removed after it is placed.
  - Slide renders live in the service's cache, with a byte and file budget
    (`SERVICE-DRAFT.md`).
- **The UI thread.** Engine calls run on the shell's UI thread until #399
  lands. The design must stay usable with that:
  - the canvas re-renders only the slide that changed, after the change, at
    screen resolution;
  - thumbnails fill in one at a time;
  - the navigator and the sorter show a window of slides around the current
    one;
  - every service call is bounded by caps, and export is one bounded call
    with an "Exporting…" state.
- **The 64 ms script budget is wall-clock time.** One handler reads at most
  one picture and makes at most one engine call. Anything longer is chained
  through timers, as v1 and PDF Tools do.
- **No key events reach scripts.** Every shortcut a desktop deck app has is a
  visible button: Undo, Redo, Duplicate, Delete, move earlier and later,
  next and previous in Present, and Exit. There is no Esc, no arrow-key
  nudging and no ⌘-anything. Text is typed into the runtime's own text fields.
- **No scroll position.** Scripts can't read or set it. The navigator and the
  sorter therefore show a run of slides starting near the current one, moved
  by the canvas pill's ‹ and › and by taps, as PDF Tools' Pages panel does.
- **No popovers and no drop-down callbacks.** Every menu is inline: an insert
  strip under the toolbar, a list that expands in the inspector, and choices
  laid out as chips or swatches. Confirmations are inline bars, not dialogs.
- **TextInput gives a script its text, never its selection.** Formatting
  applies to the whole text box or to chosen paragraphs (the inspector's
  paragraph picker), not to a word selected with the mouse. Typing replaces
  only the span that changed, so formatting elsewhere in the box stays
  (`deck.set_text`).
- **Present hides the app's own chrome**: tabs, toolbar, navigator, inspector
  and status line. An app can't make the OS window full screen, and there is
  no second-display API, so the person maximizes the window from the shell.
- **Gestures.** `GestureView` gives taps, double taps, long presses, swipes
  and pans (`on_pan` with the delta from the press, at most once a frame). The
  canvas selects with a tap through the engine's own hit test (`deck.hit`).
  It drags a selected object by its frame or its handles, and commits the move
  once, at the end of the pan (`deck.shape {op: "bounds"}`). Present advances
  on a tap or a swipe.
- **No passwords in apps (OctoSense rule 3).** A password-protected `.pptx`
  shows "This presentation is protected with a password" and doesn't open.
- **Hostile files (#448).** deckcraft can abort the shell process on hostile
  video, audio or zip data. Audio and video stay out of v2: nothing inserts
  them, and the service refuses a deck that holds them (`held:`). Opening a
  `.pptx` from this device ships only once the service checks a file's zip
  directory before the engine sees it, or once deckcraft bounds media and zip
  reads itself (open question 3).
- No network, apart from the host's model call for open question 2. No
  device features.

## Window and layout

The slide is the hero; tools appear where they act. Regions, top to bottom and
left to right, as measured on PDF Tools v2 where they are shared:

| Region | What it holds |
| --- | --- |
| Document tabs (53 pt) | "Home", then one tab per open deck with its name and a close cross; "+ New" at the end |
| Toolbar (54 pt) | View tabs "Slides" and "Sorter"; insert buttons Text, Picture, Shape, Table, Chart; Undo, Redo, Save, Export and **Present** (the one filled accent button) |
| Insert strip (optional, ~44 pt) | Under the toolbar while an insert tool is chosen: shape presets, the pictures in storage and "From this device…", table size, chart types |
| Slide navigator (220 pt) | "+ New slide", then numbered slide thumbnails, section names between them; the current slide outlined; a hidden slide dimmed with an eye-off glyph |
| Canvas | The current slide on the light grey desk, at Fit by default. A selection frame with eight handles and a rotate handle. A text box being edited shows a text field over it, while the engine draws the slide without that box's text (`deck.view {editing}`) |
| Canvas pill | Floating, bottom centre: "‹", "4 / 12", "›", zoom "−", "92%", "+", and Fit |
| Inspector (320 pt) | Segments **Format · Layout · Theme · Notes** (below) |
| Status line (44 pt) | Save state ("Saved", or "Edited" with an amber dot), "Slide 4 of 12 · 16:9", storage used ("24.8 of 64 MB used") |

The inspector's segments:

- **Format** follows the selection.
  - **Text:** font (an inline list of the families the engine can draw,
    `deck.fonts`), size stepper, B I U S, colour swatches from the theme plus
    custom, alignment, bullets and numbering, indent and outdent, line
    spacing, and the paragraphs the change applies to.
  - **Box:** autofit and vertical alignment.
  - **Shape:** fill (theme swatches, none, gradient), outline (colour, width,
    dash), shadow, and quick styles.
  - **Picture:** replace, crop, brightness, contrast, saturation, greyscale,
    transparency, reset, and alt text.
  - **Table:** style gallery, header row, banded rows, first column, insert
    and delete rows and columns, merge and split, and cell fill.
  - **Chart:** type, a small data grid (categories by series, "+ Row",
    "+ Series"), title, legend, data labels, gridlines, and palette.
  - **Arrange**, under every kind: X, Y, W, H in points, rotation, front and
    back, align, group and ungroup, duplicate and delete.
  - **Nothing selected:** the slide's background and a hint to tap something.
- **Layout** is the current slide's settings:
  - the eleven layouts as thumbnails (Title Slide, Title and Content, Section
    Header, Two Content, Comparison, Title Only, Blank, Content with Caption,
    Picture with Caption, Title and Vertical Text, Vertical Title and Text);
  - background (theme, colour, gradient);
  - transition (None, Fade, Push, Wipe, Split, Reveal, Cover, Morph…), its
    duration and "Apply to all slides";
  - "Skip this slide in the show" and "Reset slide".
- **Theme** is the whole deck:
  - the eight themes (shown as Default, Harbor, Ember, Meadow, Nocturne,
    Paper, Coral Reef and Slate; the engine calls Default "DeckCraft");
  - colour variants (the themes' schemes plus Grayscale, Ocean, Sunset,
    Forest and Berry) and font pairs (Inter, Classic, Geometric, Editorial,
    Humanist, Friendly, Neutral, Office Compatible, Times / Arial);
  - slide size (16:9, 4:3), and slide numbers, date and footer text.
- **Notes**: the current slide's speaker notes in a large text field, their
  length ("28 words · about 10 seconds"), and "Show notes while presenting".

Present replaces the whole window with the slide on a near-black stage. A
floating control pill shows "‹", "4 / 12", "›", "Notes", a timer and
"Exit". A quiet Exit control stays in the top right corner. A tap on the
slide goes forward, and a swipe goes either way. "Notes" opens a strip with
the current slide's notes under the slide.

## Screens

These are the images `source/prompts.md` describes, one per screen:

1. **Home**: the library's six decks as cards with their first slide, slide
   count, size and when each was edited. A "Start a new deck" row of the eight
   themes, "New from an outline", "Open a presentation from this device
   (.pptx, up to 64 MB)", and storage used.
2. **Edit text**: the board update's slide 5, its bullet box being edited in
   place (a frame, handles and a caret). The inspector's Format segment shows
   Text, Box and Arrange.
3. **Insert a chart**: the Chart insert strip with chart types. Slide 4's
   column chart is selected, and the inspector shows Chart with its data grid
   and elements.
4. **Layout**: slide 7, a Picture with Caption slide with a photo. The Layout
   segment shows the eleven layout thumbnails, background and transition.
5. **Theme**: the title slide in Harbor. The Theme segment shows the eight
   theme cards, colour variants, font pairs, slide size and footer.
6. **Slide sorter**: the twelve slides in sections. Two are selected, and the
   action strip reads "2 selected", Duplicate, Delete, Hide, Move earlier,
   Move later. A drag insertion marker is shown, and the hidden slide is
   dimmed.
7. **New from an outline**: outline cards (a title and its points each),
   "Draft with AI" with a topic field, the theme choice, slide size, and
   "Make deck".
8. **Present**: slide 4 alone on the stage, with the control pill and the
   corner Exit.
9. **Export**: Presentation, PDF or Images; for a PDF, slides, notes pages or
   handouts; the slide range; "Include hidden slides"; the name; "Export PDF";
   and the deck's earlier exports.
10. **Dark appearance** of editing, with the Notes segment open on slide 6 and
    its table.

An optional eleventh screen, the Shape and Picture insert strips with the
Shape and Picture variants of Format, is generated only if the pixel mapping
can't derive them from screens 2 and 3.

States each screen must design for:

- **Empty library** (first run): the theme row and both start buttons, with no
  Recent grid.
- **A slide still rendering**: its thumbnail is a numbered grey card. The
  canvas keeps the last picture with a small spinner, and a new deck says
  "Opening…".
- **A damaged deck**: "Couldn't open this deck", the engine's reason, and
  Remove.
- **An unsupported file**: a `.ppt`, `.key`, `.odp` or PDF shows "Quick Deck
  opens .pptx presentations".
- **A protected file**: "This presentation is protected with a password".
- **A held file**: "This presentation has video or audio, which Quick Deck
  can't open yet".
- **Storage full**: an inline bar, "Quick Deck's storage is full: remove a
  deck or an export to make room", with the status bar red. Save is refused,
  and edits stay in the open deck.
- **Unsaved changes when closing a tab**: an inline bar in the tab row, "Save
  changes to “Q3 2026 Board Update”?" with Save, Don't save and Cancel.
- **No engine** (card-host, a phone): "The deck engine isn't available here",
  as v1.
- **No AI provider**: "Set up an AI provider in Settings to draft outlines".
  The outline cards still work by hand.

## Actions and the engine

Every action runs reviewed deckcraft commands through the app's own service
methods. The class column is `skill/safety.json`'s classification of each
command. The service builds every engine argument itself. It admits each
method's commands through the same allowlist door as the system agent's
`deck.run` (`crates/engine-area/src/door.rs`), so the door's caps and its held
commands apply to the app too. Methods are those of `SERVICE-DRAFT.md`.

| Action | Engine commands (parameters) | Class | Service method |
| --- | --- | --- | --- |
| Open a deck: slides, layouts, theme, size | the engine's `open_presentation` on bytes the service read itself (`.deckcraft`, `.pptx`, outline `.txt`/`.md`); `document.inspect {}` | safe (`file.open` is file and never runs) | `deck.open` |
| New deck from a theme or template | `file.new {theme, size, blank: true}`; per slide `slide.new {layout, title, body}` and `slide.notes {text}` | safe | `deck.create` |
| New deck from an outline | `file.new {theme, size, blank: true}`, `slide.fromOutline {text}` (≤ 200 slides, the door's `OUTLINE_SLIDES`) | safe | `deck.create {outline}` |
| Draft the outline with AI | no engine call: the host's `model.complete` with a JSON schema of slides (title, points, notes) | `model` capability | none |
| Slide list; one slide's shapes | `document.inspect {}`; `slide.inspect {index}`; `text.get {id}` | safe | `deck.slides`, `deck.slide` |
| Tap to select on the canvas | `Session::hit_test` (not a command): shape, text, resize, rotate or adjust handle | read | `deck.hit` |
| Draw a slide, a thumbnail, a build step | `render_slide` with `RenderOpts {scale, edit, skip_text, state}` (the function behind `file.render`) | safe | `deck.view` |
| Layout gallery | `render_layout` per layout of the first master | safe | `deck.layouts` |
| Themes, colour and font schemes | `design.themes {}` | safe | `deck.themes` |
| Fonts the engine can draw | `FontDb::global().families()` (not a command) | read | `deck.fonts` |
| Edit text in a box or a table cell | `edit.select {ids}`, `text.edit {id, at, cell?}`, `text.select {anchor, caret}`, `text.insert {text}`, `text.exit {}`; `text.set {id, text, cell?}` for an empty box | safe (`text.insert` ≤ 1,000 line breaks) | `deck.set_text` |
| Character and paragraph format | `edit.select {ids}` or `text.selectParagraph {at}`, then `format.font {family}`, `format.size {size}`, `format.bold/italic/underline/strikethrough {on}`, `format.color {color}`, `format.highlight {color}`, `format.align {align}`, `format.bullets {on, char}`, `format.numbering {on, scheme}`, `format.indent {}`, `format.outdent {}`, `format.lineSpacing {lines}`, `format.paragraph {spaceBefore, spaceAfter}`; `format.state {}` reads it back | safe (`format.bullets` size ≤ 400 %) | `deck.format`, `deck.format_state` |
| Text box options | `format.anchor {anchor}`, `format.autofit {mode}`, `format.margins {…}`, `format.columns {count}` | safe | `deck.format {box}` |
| Insert a text box | `insert.textBox {rect, text}` | safe (positions and sizes ≤ 100,000 pt) | `deck.insert {kind: "text"}` |
| Insert a shape | `shape.presets {}` for the strip; `shape.insert {preset, rect, text?}` | safe | `deck.shapes`, `deck.insert {kind: "shape"}` |
| Insert a picture from storage | `insert.picture {path, rect?}` | file: a reviewed read of a file inside storage (`REVIEWED.file_reads`), ≤ 64 MiB a call | `deck.insert {kind: "picture"}` |
| Replace, crop, correct a picture; alt text | `picture.change {path, ids}` (reviewed read), `picture.crop {left, top, right, bottom, ids}`, `picture.adjust {brightness, contrast, saturation, grayscale, transparency, ids}`, `picture.reset {ids}`, `shape.altText {text, decorative, ids}` | file (`picture.change`) / safe | `deck.picture` |
| Insert a table | `insert.table {rows, cols, rect}`; `text.set {id, text, cell}` per filled cell; `table.style {style, id}` | safe (≤ 5,625 cells) | `deck.insert {kind: "table"}` |
| Edit a table | `table.insertRowAbove/insertRowBelow {row, id}`, `table.insertColumnLeft/insertColumnRight {col, id}`, `table.deleteRow {row, id}`, `table.deleteColumn {col, id}`, `table.merge {from, to, id}`, `table.split {cell, id}`, `table.style {style, id}`, `table.options {headerRow, totalRow, bandedRows, firstColumn, lastColumn, bandedColumns, id}`, `table.cellFill {color, cells, id}`, `table.columnWidth {col, width, id}`, `table.rowHeight {row, height, id}` | safe (≤ 5,625 cells) | `deck.table` |
| Insert a chart | `insert.chart {type, rect, categories, series}` | safe (≤ 10,000 data points) | `deck.insert {kind: "chart"}` |
| Edit a chart | `chart.type {type, id}`, `chart.data {categories, series, id}`, `chart.options {title, legend, dataLabels, gridlines, palette, id}` | safe (≤ 10,000 data points) | `deck.chart` |
| Move, size, rotate, flip | `shape.setBounds {x, y, w, h, ids}`, `shape.move {dx, dy, ids}`, `shape.rotate {deg, ids}`, `shape.flip {axis, ids}` | safe (≤ 100,000 pt) | `deck.shape` |
| Order, align, distribute, group | `arrange.bringToFront/sendToBack/bringForward/sendBackward {ids}`, `arrange.align {edge, to, ids}`, `arrange.distribute {dir, ids}`, `arrange.group {ids}`, `arrange.ungroup {ids}` | safe | `deck.shape` |
| Duplicate, delete objects | `edit.duplicate {ids}`, `edit.delete {ids}` | safe | `deck.shape` |
| Fill, outline, effects, quick style | `shape.fill {color \| gradient \| none, transparency, ids}`, `shape.line {color, width, dash, none, ids}`, `shape.effects {shadow, glow, softEdges, reflection, ids}`, `shape.quickStyle {index, ids}` | `shape.fill` is file, because a non-string `picture` makes it read a path: the service never passes `picture` | `deck.shape` |
| New, duplicate, delete, move, hide slides | `slide.new {layout, at, title?, body?}`, `slide.duplicate {index}`, `slide.delete {ids}`, `slide.move {from, to}`, `slide.hide {index, hidden}` | safe | `deck.slides_op` |
| Change or reset a slide's layout | `slide.layout {layout, index}`, `slide.reset {index}` | safe | `deck.slides_op` |
| Sections | `section.add {name, at}`, `section.rename {index, name}`, `section.remove {index}`, `section.move {index, to}` | safe | `deck.slides_op` |
| Speaker notes | `slide.notes {text, index}` | safe | `deck.notes` |
| Theme, colours, fonts | `design.theme {name}`, `design.colors {name}`, `design.fonts {name}` | safe | `deck.design` |
| Slide size | `design.slideSize {preset \| w, h, scale}` | safe (≤ 1920 × 1080 pt of area) | `deck.design` |
| Background | `design.background {color \| gradient \| style \| reset, all?, index?}` | file, for the same `picture` reason as `shape.fill`: never passed | `deck.design` |
| Footer, slide numbers, date | `design.headerFooter {slideNumber, footer, footerText, date, hideOnTitle, all}` | safe | `deck.design` |
| Transitions | `transition.list {}`, `transition.set {kind, option, duration, index}`, `transition.applyAll {}` | safe | `deck.transition` |
| Undo, redo | `edit.undo {}`, `edit.redo {}` | safe | `deck.undo`, `deck.redo` |
| Save | `save_bytes(doc, "deckcraft")` (behind `file.saveBytes`); the service writes the file itself | safe (`file.save`/`file.saveAs` are host and never run) | `deck.save` |
| Present: order, builds, a slide at a build step | deckcraft-anim's `show_order`, `ShowState::steps` and `Timeline::state`, passed to `render_slide`'s `state` | functions (`show.fromStart`/`fromCurrent` are device and never run) | `deck.show`, `deck.view {step}` |
| Export `.pptx`, outline | `save_bytes(doc, "pptx" \| "outline")` | safe | `deck.export` |
| Export PDF: slides, notes pages, handouts | `pdf_bytes(doc, {layout, perPage, slides, includeHidden})` (behind `file.export`, which is file and never runs) | function, capped | `deck.export {kind: "pdf"}` |
| Export a picture per slide | `render_png` per slide | function, capped | `deck.export {kind: "images"}` |
| Library covers | `render_slide`, slide 1 of a deck file, without an open document | safe | `deck.cover` |

Never used: `insert.audio`, `insert.video`, `media.info`, `media.posterFrame`
and `file.openBytes` (held, #448); the `media.*` playback and `show.*` start
commands (device); `file.save`, `file.saveAs` and `file.saveTemplate` (host);
and `file.open`, `file.export`, `file.close` and `file.recovery.*` (file; the
service does its own file work).

## What the deck service lacks today

`apps/deck/host-service` serves the agent's tools (`deck.info`, `deck.run`)
and v1's file methods (`info`, `text`, `render`, `new`, `convert`). What v2
needs on top, specified in `SERVICE-DRAFT.md`:

1. **Open-document sessions.** Every call today is a fresh, stateless engine
   session: `read_deck` reads the file into `Session::new()`, and the session
   is dropped at the end. The engine's undo history (500 snapshots of shared
   slides), its selection and its dirty state never outlive a call. So there is
   no undo, and every edit re-reads and re-writes the whole file. PDF Tools v2
   needed the same thing: a handle per open deck, per caller app and storage
   scope, at most 8 per caller, released on close and when the app's isolate
   closes (App Hub's `services::on_isolate_closed`).
2. **Typed editing methods for apps.** The only way to edit today is
   `deck.run`, the agent's door. Each call reads the whole deck from its
   file, runs the commands on a fresh session, writes the whole deck out
   again, and answers raw engine JSON. Shape ids and the selection have to be
   found again on every call, and nothing can be undone.
3. **One undo step per action.** The engine makes each command its own undo
   step. The service folds a method's commands into one step labelled by the
   method (the engine's `History` is public).
4. **Renders into a managed cache.** Today `render` re-reads the deck file
   for every slide, writes wherever the app names, and draws without editor
   marks. v2 needs a cache keyed by slide and revision, with a byte and file
   budget. It also needs the editor view (placeholder prompts, dashed
   outlines), a box's text left out while it is edited (`skip_text`), and the
   build state for Present.
5. **What the inspector lists**: layout thumbnails (`render_layout`), the
   theme catalog, the installed font families, the shape presets and the
   engine's hit test. None of these is reachable today.
6. **Present.** Show order and build steps need deckcraft-anim, which is not
   one of the service's dependencies today. It would be the same deckcraft
   rev, so it is still one source.
7. **Export options.** `convert` writes a PDF with the engine's defaults
   only. v2 needs notes pages, handouts, slide ranges and hidden slides, and
   pictures per slide, each capped for the UI thread.
8. **Covers for the library**, kept across closes, as `pdf.cover` does.
9. **Stable error codes** (`damaged:`, `held:`, `storage_full:`,
   `unknown_doc:`…). Today errors are prose (`deck.render: …`), so v1 matches
   substrings.
10. **Import guards for files from this device (#448)**: check a `.pptx`
    file's zip directory (entries, declared sizes, total) and refuse audio and
    video before the engine sees it.
11. **Exact text.** The engine's AutoCorrect and smart quotes rewrite typed
    text (`text.insert` runs them). An app session turns both off, so a box
    says what the person typed.

## Engine limits the design accepts

What a Keynote- or PowerPoint-class editor would want that deckcraft at
d0e57d7e does not do, from reading its code and its own `docs/parity.md`:

- **No PDF import.** deckcraft reads `.pptx`, its own `.deckcraft` and outline
  text. A PDF could become picture slides through the pdf engine's page
  renders, but that would not be editable. It is out of scope for v2.
- **No legacy formats**: no `.ppt`, `.potx` or `.ppsx` variants, and no Keynote
  or OpenDocument files.
- **No SVG pictures.** Pictures are PNG, JPEG, WebP, GIF, TIFF or BMP.
- **No find and replace.** No such command exists in the registry. A
  service-side search over slide text is possible later.
- **Spelling** is a small built-in word list (`review.spelling`). There's no
  dictionary, thesaurus or language support.
- **PDF export rasters slide artwork** at 200 dpi, with a selectable text
  layer; vector artwork is pending upstream. PDFs are large, and each page
  costs about 73 ms of UI thread at opt-level 1 (the service's own measure).
- **Animations and transitions don't play in OctoSense.** The engine stores,
  times and exports them, and its show model steps builds. Moving pictures
  need a host that composites frames, which a Splash app can't do at frame
  rate on the UI thread. Present cuts between slides and shows each build
  step's end state. Editing builds is left for later.
- **Audio and video** are supported by the engine but held back (#448). The
  show couldn't play them anyway: `media.play` is a device command.
- **No multi-monitor presenting** (missing upstream) and no presenter display
  in OctoSense. Notes in Present are a strip in the same window.
- **Theme fonts are not embedded.** The eight themes name Inter, Merriweather,
  Open Sans, Montserrat, Lato, Playfair Display, Poppins, Nunito Sans and
  Roboto. deckcraft embeds them only when built with `CRAFT_FONTS_DIR` (the
  craft-fonts checkout), which OctoSense's build doesn't set. Without them
  those families, and Office fonts such as Calibri (whose Carlito substitute
  is a craft font too), draw with system fallbacks, which changes a theme's
  look and a `.pptx`'s line breaks. CJK text is partial upstream.
- **Advanced charts are approximate.** Column, bar, line, area, pie,
  doughnut and scatter (and their stacked forms) draw properly. Radar,
  bubble, waterfall, funnel, treemap, sunburst, box and whisker, stock,
  surface and combo charts are stored and exported but drawn as plain bars or
  lines.
- **Missing upstream**: edit points, 3-D and bevel, text warp, artistic
  effects, background removal, equations, the SmartArt text pane, reusing
  slides from another file (copying slides between decks is partial),
  password protection and printing.
- **PPTX fidelity** is verified upstream on generated decks only; hardening
  on a corpus of real-world decks is upstream's next item.
- **Hostile media and zip data can abort the process (#448)**, so opening
  decks from this device waits for a guard.

## Data

The app's storage, at most 64 MiB and 256 entries:

- `library.json`: the library index. Each entry has its file, title, slide
  count, size, theme, when it was created, edited and opened, and its last
  slide and zoom.
- `decks/<name>.deckcraft`: each deck, in the engine's native format (a zip of
  JSON and media). It keeps everything the engine models, which a `.pptx`
  round trip may not. This is open question 1.
- `imports/<name>.pptx`: a presentation brought in from this device. It opens
  as a deck, and its first save writes `decks/<name>.deckcraft`; the import is
  then removed.
- `images/`: pictures brought in for slides, kept for reuse until removed.
- `exports/`: `.pptx`, `.pdf`, outline `.txt` and per-slide `.png` exports.
  A per-slide export uses one entry per slide, so the app checks the entry
  budget first.
- `.cache/slides/` and `.cache/covers/`: the service's renders, within its
  budget.

Unsaved edits live in the engine's open document, and Undo and Redo use its
history. v1's decks (`decks.json`, `decks/<id>/g<n>/deck.deckcraft`) appear in
the library on v2's first run. Each one's latest generation opens as a deck,
and its v1 outline stays readable through `deck.text`.

## Visual direction for the image prompts

The same family as PDF Tools v2. It is a precise, calm desktop tool that looks
like it belongs to OctoSense:

- **The slide is the brightest thing on screen.** Chrome is quiet and thin,
  and one accent, the blue #116cdc, marks the current view, the selection,
  focus and the primary action.
- **Colour and type.** Neutral greys with a slight cool bias, and the light
  grey desk (#eceded). Crisp, small interface type in the style of IBM Plex
  Sans.
- **Shared chrome**: the same tab row, toolbar height, status line and
  floating canvas pill as PDF Tools.
- **Slides carry their own deck theme**: Harbor, with navy text, teal-blue and
  teal accents, and a serif for titles. It never uses the interface accent.
- **Content**: a believable sample deck, the Northwind Cooperative's Q3 2026
  board update, which matches PDF Tools' board report, with real charts and a
  table.
- **Dark appearance**: the same structure.
- **No imitation.** It must not copy Keynote's, PowerPoint's or Google
  Slides' look or icons, and it must show no product logos.

## Out of scope for v2's first release

- Audio and video (#448);
- editing builds (animations);
- comments;
- find and replace;
- an outline view;
- master and layout editing;
- SmartArt;
- ink;
- PDF import;
- saving outside the app (`files.export` is limited to 1 MiB);
- password-protected files;
- printing;
- co-editing and sharing;
- an app agent (`tools.json`).

The engine already supports comments, SmartArt, master editing and builds.
They need design work, not engine work.

## Done when

- Every job above works end to end with the real engine in the hidden desktop
  shell, in light and dark, with the states above.
- A 30-slide deck opens to its first slide in under a second. An edit
  re-renders only the slide it changed, and no editing call stalls the shell
  for more than one slide render. A smooth canvas waits for #399.
- An exported `.pptx` opens back in Quick Deck with the same slides, and a
  PDF of the sample deck shows every slide.
- Side-by-side comparisons with the approved generated designs pass a person's
  visual review.
- It is rebuilt from the approved images through the App Flow pixel mapping,
  not hand-drawn in another style.

## Open questions

1. **Working format.** Should the library keep native `.deckcraft` files
   (recommended: lossless for everything the engine models) and export
   `.pptx`? Or should it work in `.pptx` directly?
2. **AI drafting.** Should "New from an outline" include "Draft with AI" in
   the first release (`model.complete`, the `model` capability, Settings → AI
   providers)? Or should the outline only be typed by hand, as in v1?
3. **Opening `.pptx` from this device.** Should it ship with the service's
   zip and media guard, or wait for deckcraft to bound media and zip reads
   (#448)?
4. **Theme fonts.** Should OctoSense's desktop build embed craft-fonts
   (`CRAFT_FONTS_DIR`), so themes and PowerPoint's usual fonts draw as
   designed? Its size cost hasn't been measured. Or are system fallbacks
   acceptable?
5. **Notes in Present.** Is the notes strip in Present enough, or should a
   presenter layout (current slide, next slide, notes, timer) be designed
   too?
