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
| Status line | Saved or Edited, and the storage the PDFs use ("14.6 of 64 MB used", "37 KB of 64 MB used") |

The designs are drawn at 1536 x 1024, and the manifest asks the desktop to
open the window at that size: `"window": {"width": 1536, "height": 1024}`,
App Hub's optional window hint, which needs `"schema_minor": 1`. The desk
clamps it to its own size, less the margins every new window keeps (42
points left and right, 32 top and bottom; `crates/shell/src/desktop_layout.rs`).
In the hidden shell's 1400 x 899 window, PDF Tools opens at 1296 x 783
points with an app area of 1292 x 747
([tests/evidence/window-20261010](tests/evidence/window-20261010/README.md)).
Before the hint it got the default window, 72% of the desk's width and 76%
of its height, at most 1000 x 720 points: an app area of 990 x 603. A
shell that predates the hint ignores it and opens the default. The person
can resize or maximize the window either way. The OctoSense style's dock
overlays the desk, so a window this tall has its lowest points behind the
dock. Below 1180 points wide the mode tabs and Save trim their padding, both
right panels take 320 points, a right panel takes the left panel's place,
and until the person zooms, the page fits the canvas, up to 100%.

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
- One handler, timer or callback has a 64 ms budget of wall-clock time; page
  images are read with `fs.read_bytes` and shown with `binary_resource`, one
  stored handle per image. `library.json` is written by the heartbeat (at most
  every 2 s) when a page turn or an open changed it, so no tap waits on the
  disk; a new title, a removal or the "Comment as" name is written at once.

## Testing

The library's rules are pure functions in `main.splash`, tested from Rust by
evaluating the bundle's functions in a script VM
(`crates/shell/src/pdftools_model_tests.rs`): an imported PDF's title, a split
part's title, what the library index keeps across a restart, the pages a
range names, a find snippet's match in bold, an avatar's initials and a
comment's date.

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

The runtime gives one handler 64 ms of wall-clock time, so on a busy machine
a run can fail on a handler that waited for the CPU (`script time budget
exceeded` in its log); `--budget-ms 1000` raises the hosts' budget
(`MAKEPAD_SPLASH_BUDGET_MS`) for such a machine.

| Run | What it covers |
| --- | --- |
| `shell` | Every mode on the samples in the shell's own window, light, then dark through the shell's own style menu (the dark pass works on what the light pass left): Home and its covers, "Open a PDF from this device" refused, reading and thumbnails, find ("9 matches on 4 pages"), a highlight and its reply, a rotation and its undo, combine with a page range ("One PDF of 5 pages"), initials placed by Fill & Sign, a text edit, save, and the damaged file |
| `restart` | The same home again: the library, its titles, the page a PDF was left at, and `Imported PDF.pdf` placed as an import leaves one |
| `full` | Storage filled to 8 KB short of its 64 MiB: the refusal on Home, then a removal, which asks once more |
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

- **Verified on 10 Oct 2026** (macOS, Apple silicon; the frames, side-by-sides
  and receipt are in [tests/evidence/v2-20261010](tests/evidence/v2-20261010/README.md)):
  the model tests; the `shell`, `restart`, `full` and `empty` runs in a hidden
  desktop shell with the real engine, at the runtime's 64 ms script budget
  (and again with `--budget-ms 1000` while other builds loaded the machine);
  the `missing` and `fixture` runs in card-host, light and dark. `fixture`
  ran with `--budget-ms 1000`: its stand-in engine answers in script, and at
  64 ms one of its answers overran in the dark pass. Those runs had the
  default window, an app area of 990 x 603 points.
- **Verified on 10 Oct 2026, the window's size** (macOS, Apple silicon;
  [tests/evidence/window-20261010](tests/evidence/window-20261010/README.md)):
  in a hidden desktop shell, the manifest's 1536 x 1024 opens a 1296 x 783
  window on the 1380 x 847 desk.
- **Not verified**: the host's file dialog and a real import (a hidden window
  cannot open it); with the real engine, form fields, outlines, Redo, the
  other comment and page operations, password-protected PDFs, the eight-PDF
  cap and the unsaved-changes question (the dev fixture covers their screens);
  the modes at the window's new size (the runs above checked them at
  990 x 603 points and, in card-host, at 1536 x 1024); other screen sizes;
  Linux, Windows, phones.
- **Known**: a comment's date is the engine's UTC time read as local time, so
  a comment made in the evening west of UTC shows the next day's date, until
  the pdf service converts it. The status line counts only the PDFs in the
  library: the files service does not report the storage's use. On a heavily
  loaded machine a tap can overrun the 64 ms budget and be lost (seen at load
  averages of 60 to 150 before `library.json` moved off the tap path).
- **Not part of this version**: export (no approved design), opening
  password-protected PDFs, keyboard shortcuts (the runtime gives script apps no
  key events), the Edit panel's Alignment and Colour (`pdf.edit_text` takes
  only text).
