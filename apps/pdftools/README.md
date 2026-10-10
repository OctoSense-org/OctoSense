# PDF Tools

English | [简体中文](README.zh-CN.md)

PDF Tools (`os.pdftools`) is OctoSense's PDF app for the desktop: read,
search, review, fill and sign, organize pages, combine and edit the PDFs in
its own storage, and open PDFs from the device. The shell's `pdf` engine
service (pdfcraft, [ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.md))
does the work in the app's own storage, through the methods in
[design/SERVICE.md](design/SERVICE.md); the app is a contained Splash program
in `bundle/` and never reads a PDF itself. It ships in the desktop shell
only, because the craft engines are desktop only (`craft-engines`).

It declares `storage`, `files` and `pdf`, and no `storage.max_bytes`, so its
storage is App Hub's system ceiling, 64 MiB.

Version 2 was rebuilt through OctoSense App Flow from nine approved generated
designs: the product requirements in [design/BRIEF.md](design/BRIEF.md), the
untouched images and their measurements in `design/source/`, and one pixel
map per screen in `design/map/` that ties every measured region to the
widget that rebuilds it, its geometry, colours, type size and the review
decision that applies.

## The window

| Region | What it holds |
| --- | --- |
| Document tabs | Home while it shows, one tab per open PDF (its name, a dot while it has unsaved changes, a close cross), and Open, which goes to Home |
| Mode bar | Read · Comment · Fill & Sign · Pages · Combine · Edit, the current one underlined; Search in document; Undo, Redo; Save |
| Rail and left panel | Pages (thumbnails), Outline (the PDF's bookmarks), Comments, Search (the results); the panel folds away |
| Canvas | The pages, white on the desk: a run of up to eight pages from the page you went to, each the engine's render at screen resolution |
| Pill | Page "3 / 24", previous and next, zoom out, zoom, zoom in, fit width, fit page |
| Right panel | Comment: the threads. Fill & Sign: the form's fields and Add text, Add date, Add initials. Edit: the paragraph being edited |
| Status line | Saved or Edited, and the storage the PDFs use ("14.6 of 64 MB used") |

## Modes

| Mode | What it does | Engine |
| --- | --- | --- |
| Home | Recent PDFs as cards with their first page, pages, size and when they were opened; Open a PDF from this device (up to 64 MB); remove a PDF (asks once more); an empty state on a first run | `pdf.open`, `pdf.info`, `pdf.page`, `pdf.close` |
| Read | Pages render one at a time at 72 dpi x zoom x 2 and are kept; thumbnails fill in one by one; the outline jumps to a section | `pdf.open`, `pdf.page` |
| Find | Search in document, then Return: "12 matches on 7 pages", the results grouped by page with the match in bold, every match highlighted on its page, the current one stronger, previous and next | `pdf.find` |
| Comment | Highlight, Underline or Strike out the line you tap (tap twice for its paragraph), Note and Text box where you tap, in one of four colours; threads with replies, a status, delete. Comments carry the name kept under "Comment as" | `pdf.lines`, `pdf.comments`, `pdf.comment` |
| Fill & Sign | The form's fields outlined (required empty ones in red) and listed with their values; choose one to type its value; Add text, Add date and Add initials place a mark where you tap; initials are kept | `pdf.fields`, `pdf.fill`, `pdf.fill_sign` |
| Pages | Every page as a large thumbnail: choose pages, Rotate left or right, Delete, Extract (a new PDF in the library), Insert from file (another PDF here), drag a page to move it | `pdf.pages` |
| Combine | This PDF and others in order (drag the grip to reorder), each with its pages ("1-4, 9" or All), "One PDF of 31 pages", a name, Combine; the new PDF opens | `pdf.merge` |
| Edit | Click a paragraph: its text becomes editable in place, the right panel shows its font and size; Apply or Cancel | `pdf.lines`, `pdf.edit_text` |

Undo and Redo use the engine's history; Save writes the PDF back
(`pdf.undo`, `pdf.redo`, `pdf.save`, `pdf.state`). Closing a tab with unsaved
changes asks: Save, Don't save, or Cancel.

States every screen handles: an empty library, a page still rendering, a
damaged PDF ("Couldn't open this PDF" and the engine's reason), a protected
PDF ("This PDF is protected with a password": OctoSense keeps passwords on
host sheets and there is none for PDFs yet, so it is not opened), storage
full ("Remove a PDF to make room"), eight PDFs already open, unsaved changes
when closing, and no engine on the device.

## Opening a PDF from this device

"Open a PDF from this device" asks the shell's files service to copy one
file the person chooses in the host's own dialog into the library. The app
names the new file first, because `files.import` takes its destination
before the dialog opens: `Imported PDF.pdf`, then `Imported PDF 2.pdf` and so
on. The host reports the chosen file's display name (`name`), which becomes
the PDF's title without `.pdf` ("Lease 2026", or "Lease 2026 (2)" when that
title is taken); the file keeps the name the app gave it and the title lives
in `library.json`. Then it opens in a tab. A refusal shows on Home in plain
words with the host's own message below it.

## How it works

Every path is relative to the app's storage, the folder `fs.*` sees; the
engine works in the same folder.

| Path | Holds |
| --- | --- |
| `accounts/device/library/*.pdf` | The PDFs, including imported, extracted and combined ones |
| `accounts/device/library.json` | Per file: title (when not its file name), pages, size, when last opened, the page and zoom it was left at, its cover's render; and `who`: the name comments carry and the initials Fill & Sign places |
| `.cache/pages/<doc>/<page>@<dpi>.png` | The engine's page renders: its cache, at most 16 MiB and 64 files |

Notes for whoever changes the script:

- **Engine work runs on the shell's UI thread (#399).** The app keeps one
  background render out at a time (`pump()`): the current page and the two
  after it, then the visible thumbnails, then the rest of the run, then the
  other thumbnails, then Home's covers. A change drops only the renders of
  the pages it touched; Undo and Redo, which don't say what changed, redraw
  the run on show.
- **No scroll position for script apps.** The runtime gives a script app no
  way to read or set where a scroll view is. So the canvas is a static
  `ScrollXYView` that keeps its place while renders land, and going to a page
  (a thumbnail, the outline, a find result, the pill) draws the canvas empty
  for one frame, which clamps its scroll to the top, then fills it with a run
  of pages that starts at that page. Pages before it come back with the
  pill's previous.
- **No keyboard events for script apps.** Every shortcut in the brief (⌘F,
  ⌘+ and ⌘−, ⌘0, the arrows, ⌘Z, ⇧⌘Z, ⌘S) is a button instead; Search in
  document runs on Return, which a text field does report.
- **State lives on `mod.pdftools`.** A light/dark switch runs the script
  again (#440); every `let` starts over, `mod` survives, and `resume()` draws
  every region with the new look.
- **Labels draw their own margin and padding twice** on this runtime
  (`widgets/src/label.rs` passes the padded walk to both its turtle and its
  text): give a Label no margin or padding and put a View around it instead.
- **A view whose `on_render` starts hidden stays hidden**: regions that hide
  are wrapped in plain Views that are shown and hidden instead.
- One handler, timer or callback has a 64 ms budget; page images are read
  with `fs.read_bytes` and shown with `binary_resource`, one stored handle per
  image.

## Testing

The library's rules are pure functions in `main.splash`, tested from Rust by
evaluating the bundle's functions in a script VM
(`crates/shell/src/pdftools_model_tests.rs`): an imported PDF's title, a split
part's title, and what the library index keeps across a restart.

```sh
cargo test --locked -p octosense-shell --lib pdftools_model
```

`tests/ui.py` runs the app end to end. With `--shell` it uses a hidden
desktop shell built from this checkout, with the real engine, on the sample
PDFs the pdf service's `pdftools_fixture` example writes (it makes a fresh
`OCTOSENSE_HOME` for each run, starts `target/debug/octosense` from a
Terminal.app tab with `MAKEPAD_HIDE_WINDOWS=1`, `MAKEPAD_REMOTE` and
`MAKEPAD_WM_TEST_APP=pdftools`, drives it over the remote bridge, saves every
grab and ends with `/quit`; a lock folder keeps it to one hidden shell on the
machine, and it waits while another OctoSense runs). With `--card-host` it
uses App Hub's `card-host`, which serves no host services.

```sh
cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/pdftools-ui
```

| Run | What it covers |
| --- | --- |
| `shell` | Every mode on the samples, light then dark through the shell's own style menu: Home and its covers, reading and thumbnails, find, a highlight and a reply, a rotation and its undo, combine with a page range, a Fill & Sign mark, a text edit, save, the damaged file, and "Open a PDF from this device" |
| `restart` | The same home again: the library, its titles, the page a PDF was left at, and `Imported PDF.pdf` placed as an import leaves one |
| `full` | Storage filled to 8 KB short of its 64 MiB: the refusal on Home, then a removal |
| `empty` | No PDFs: the empty library and its Open button, light and dark |
| `missing` | `card-host` with the shipped bundle: no engine and no files service |
| `fixture` | `card-host` with the dev fixture: every designed screen at the designs' 1536 x 1024, light and dark, each grab beside its design in `compare/`, and the damaged, protected and unsaved states |

The dev fixture never ships. `dev-fixture/engine.splash` is a stand-in for
the pdf engine that answers SERVICE.md's methods; `tests/ui.py` puts it in
place of `engine()` in a scratch copy of the bundle. `dev-fixture/make_fixture.py`
fills the app's storage with sample documents drawn from the approved
designs' own sample text, their pages as pictures, and what the stand-in
answers (outlines, text lines and paragraphs with their boxes, form fields,
comments).

```sh
python3 apps/pdftools/dev-fixture/make_fixture.py <card-host app data>/os.pdftools
```

The host's dialog cannot be driven in a hidden shell, and a hidden window
never has focus, so the files service refuses `files.import` there before the
dialog opens (`foreground_required: …`); the runs check that the app shows
that refusal.

## Status

- **Not verified yet**: see the pull request for the runs made on this
  version and their evidence.
- **Not part of this version**: export (no approved design), password-protected
  PDFs, keyboard shortcuts (the runtime gives script apps no key events),
  the Edit panel's Alignment and Colour (`pdf.edit_text` takes only text),
  Linux, Windows, phones.
