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
| Rail and left panel | Pages (thumbnails), Outline (the PDF's bookmarks), Comments; the panel folds away. A search's results take the panel while it runs (no rail item is lit), and give it back when it ends |
| Canvas | The pages, white on the desk: a run of up to eight pages from the page you went to, each the engine's render at screen resolution |
| Pill | Page "3 / 24", previous and next, zoom out, zoom, zoom in, fit width, fit page, and full screen (Read mode); in Read mode, Exit read mode in its place |
| Right panel | Comment: the threads. Fill & Sign: the form's fields and Add text, Add date, Add initials. Edit: the paragraph being edited |
| Status line | Saved or Edited, and what the app's storage holds against what it may hold, from the shell's `files.status` ("14.6 of 64 MB used", "37 KB of 64 MB used") |

The designs are drawn at 1536 x 1024, and the manifest asks the desktop to
open the window at that size: `"window": {"width": 1536, "height": 1024}`,
App Hub's optional window hint, which needs `"schema_minor": 1`. The desk
clamps it to its own size, less the margins every new window keeps (42
points left and right, 32 top and bottom). Where the dock floats over the
desk, as OctoSense's does, the bottom margin is kept above the dock, so the
zoom bar and the status line are never behind it
(`crates/shell/src/desktop_layout.rs`). In the hidden shell's 1400 x 897
window, PDF Tools opens at 1296 x 703 points with an app area of 1292 x 667,
its wide layout
([tests/evidence/window-20261010](tests/evidence/window-20261010/README.md)).
Before the hint it got the default window, 72% of the desk's width and 76%
of its height, at most 1000 x 720 points: an app area of 990 x 603. A
shell that predates the hint ignores it and opens the default. The person
can resize or maximize the window either way. At 1180 points wide and up
(the wide layout), a right panel opens beside the left panel. Below 1180 the
mode tabs and Save trim their padding, both right panels take 320 points, a
right panel takes the left panel's place, and until the person zooms, the
page fits the canvas, up to 100%.

## Modes

| Mode | What it does | Engine |
| --- | --- | --- |
| Home | Recent PDFs as cards with their first page, pages, size and when they were opened; Open a PDF from this device (up to 64 MB); remove a PDF (asks once more); an empty state on a first run | `pdf.open`, `pdf.info`, `pdf.page`, `pdf.close` |
| Read | Pages render one at a time at 72 dpi x zoom x 2 and are kept; thumbnails fill in one by one; the outline jumps to a section | `pdf.open`, `pdf.page` |
| Read mode | The pill's full-screen button: the pages alone. The tabs, the mode bar, the rail and both panels, the tool row and the status line give the pages their room, and the pill stays with previous and next, the page, the zoom, fit width and fit page, and Exit read mode. In a desktop shell the same tap puts the window in full screen too (`window.fullscreen`): the pages fill the screen, with no bar, title bar or dock, and no screen edge brings them back. There Esc, the shell's key (a script gets no key events), leaves Read mode, and so do ⌘F and another window taking the focus (`on_app_fullscreen`). A quiet button at the top right leaves too. Where the shell can't (a tap it doesn't count, or a host with no window service, such as card-host), Read mode fills the window, without a word. The page and the zoom you chose carry across entering and leaving; a fitted or automatic zoom fits the new canvas. A tap on a page only chooses it | `pdf.page` |
| Find | Search in document, the one way to search, then Return: "12 matches on 7 pages" in the left panel, the results grouped by page with the match in bold, every match highlighted on its page, the current one stronger, previous and next. Done, the results' cross or the field's cross ends the search, and the left panel shows what it did before (Pages if it showed nothing known); a change to the document ends it the same way | `pdf.find` |
| Comment | Highlight, Underline or Strike out the line you tap (tap twice for its paragraph), Note and Text box where you tap, in one of four colours; threads with replies, a status, delete. Comments carry the name kept under "Comment as" | `pdf.lines`, `pdf.comments`, `pdf.comment` |
| Fill & Sign | The form's fields outlined (required empty ones in red) and listed with their values; choose one to type its value; Add text, Add date and Add initials place a mark where you tap; initials are kept | `pdf.fields`, `pdf.fill`, `pdf.fill_sign` |
| Pages | Every page as a large thumbnail: choose pages, Rotate left or right, Delete, Extract (a new PDF in the library), Insert from file (another PDF here), drag a page to move it | `pdf.pages` |
| Combine | This PDF and others in order (drag the grip to reorder), each with its pages ("1-4, 9" or All), "One PDF of 31 pages", a name, Combine; the new PDF opens. When the list does not fit the window, it scrolls inside the card and the total, name and Combine stay in view | `pdf.merge` |
| Edit | Click a paragraph: its text becomes editable in place; the right panel shows its font and size, its Alignment (left, centre, right or justify: the one its lines show is chosen, since the engine sets an edited paragraph flush left unless told) and its Colour (its own until you pick one of six); before Apply it says what the edit would do besides changing the words, or why it can't be made ([Editing text](#editing-text)); Apply (or Apply anyway) or Cancel | `pdf.lines`, `pdf.edit_check`, `pdf.edit_text` |

Undo and Redo use the engine's history; Save writes the PDF back
(`pdf.undo`, `pdf.redo`, `pdf.save`, `pdf.state`). Closing a tab with unsaved
changes asks: Save, Don't save, or Cancel.

States every screen handles: an empty library, a page still rendering, a
damaged PDF ("Couldn't open this PDF" and the engine's reason), a protected
PDF ("This PDF is protected with a password": OctoSense keeps passwords on
host sheets and there is none for PDFs yet, so it is not opened), storage
full ("Remove a PDF to make room"), eight PDFs already open, unsaved changes
when closing, and no engine on the device.

## Editing text

Edit redraws a paragraph in place with the engine's own editor. Real-world
PDFs limit what that can do, and the right panel says so before Apply. As
you click a paragraph, and again when your typing pauses, the app asks the
engine what the edit would do (`pdf.edit_check`, which makes the edit on a
copy of the document and drops it), so no edit garbles a page unannounced:

- **Another font.** A PDF usually embeds only the glyphs it uses (a subset
  font), and a web browser often prints text in unnamed Type 3 fonts. When
  the paragraph's own font can't show the new text, the engine sets the
  paragraph in Helvetica, Times or Courier instead, which looks different and
  takes another width. The panel names the font it will use.
- **More room.** An edited one-line paragraph grows to the right up to the
  page's margin, and a longer one grows down; nothing around it moves. When
  the new text would run into other lines of text, or fall off the page, the
  panel says so.
- **Chinese, Japanese and Korean.** The engine draws characters a
  paragraph's font lacks from a fallback face this build doesn't include, so
  it refuses them, and the panel says so plainly: "PDF Tools can't edit
  Chinese, Japanese or Korean text yet." Characters the font already has
  (the same ones in another order, say) still work. Other text no standard
  font shows, such as Cyrillic or Greek, is refused the same way, naming the
  characters.
- **A line in pieces.** A browser prints a title whose glyphs span several
  fonts as one piece per font, and the engine reads each piece as a
  paragraph of its own, so a click selects only that piece. The panel says
  how many pieces the line has.

While the panel has a warning, Apply reads "Apply anyway"; an edit that
hasn't been checked as it stands yet is checked first, and is made at once
only if there is nothing to say. A refusal shows in red and Apply only
repeats it. With notes showing, the card takes the panel's height: its
fields scroll, and Apply and Cancel stay at its foot, in view even in the
desktop's default window. A document whose security settings forbid changes, or that opens
for reading only, is refused the same way.

The pdf service's `pdftools_fixture` example writes a page as a web browser
prints it, with all four (a title in five pieces in unnamed Type 3 fonts,
Chinese text in an embedded subset font), to try Edit on:

```sh
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- --edit-sample <file>
```

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

On Linux the host's dialog is the desktop's own helper, which Makepad runs:
the first of zenity, qarma, matedialog and kdialog on `PATH`. A machine with
none of them gets no `files.import` (`files.status` says
`import_supported: false`), so Home says "Not available here" beside the
button, and the button says that this OctoSense can't open files from the
device for PDF Tools.

## How it works

Every path is relative to the app's storage, the folder `fs.*` sees; the
engine works in the same folder.

| Path | Holds |
| --- | --- |
| `accounts/device/library/*.pdf` | The PDFs, including imported, extracted and combined ones |
| `accounts/device/library.json` | Per file: title (when not its file name), pages, size, when last opened, the page and zoom it was left at, its cover; and `who`: the name comments carry and the initials Fill & Sign places |
| `.cache/pages/<doc>/<page>@<dpi>.png` | The engine's page renders: its cache, at most 16 MiB and 64 files with the covers |
| `.cache/covers/<name>-<size>-<modified>@48.png` | Each PDF's cover, page 1 (`pdf.cover`): kept across closes and restarts, drawn again when the PDF changes; at most 32 of the cache's 64 files, and page renders go first |

Notes for whoever changes the script:

- **Engine work runs on the shell's UI thread (#399).** The app keeps one
  background render out at a time (`pump()`): the current page and the two
  after it, then the visible thumbnails, then the rest of the run, then the
  other thumbnails, then Home's covers. Once a start, Home asks `pdf.cover`
  for each of the first 32 PDFs it shows (the service keeps at most 32): a
  kept cover answers from its file alone, a stat and a 24-byte read, and the
  picture named in `library.json` shows meanwhile. A cover at a new path is a
  PDF that changed (a save), so its pages and size are read again from
  `pdf.info`, as they are for a PDF beyond the first 32 that has none. A save
  also records the open document's page count, and Remove deletes the
  cover. A change drops only the renders of
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
  ⌘+ and ⌘−, ⌘0, the arrows, ⌘Z, ⇧⌘Z, ⌘S) is a button instead; ⌘F's is the
  Search in document field itself, which runs on Return, the one key a text
  field does report.
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
range names, a find snippet's match in bold, an avatar's initials, a
comment's date, the storage line's words, a paragraph's alignment read from
its lines, what an edit sends, and what the Edit panel says an edit would do
(a font's name, the warnings and "Apply anyway", a line in pieces). They
also parse the whole script, views and handlers included, which the VM there
doesn't run: one parse error fails the whole app when it loads. The
pdf service's own tests hold `pdf.edit_check`'s answers to what the engine
then does, on the `--edit-sample` page (`cargo test --locked -p
octosense-pdf-service`).

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

On Linux, where `MAKEPAD_HIDE_WINDOWS` does nothing, `--shell` gives each run
a headless weston of its own instead of a Terminal.app tab: the pixman
renderer, never idle, in a private runtime folder. The shell runs on it with
Makepad's Wayland backend and the run home's XDG folders, and weston stops
after `/quit`. Another OctoSense on the machine doesn't hold the run up. Give
`--grab-scale 1` for one pixel per point (the window's dpi is 1 there). Give
`--work` on the disk you mean to test, because Ubuntu's `/tmp` is tmpfs.

```sh
cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/pdftools-ui
# Linux
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --work <dir> --grab-scale 1 --output target/pdftools-ui
```

The runtime gives one handler 64 ms of wall-clock time, so on a busy machine
a run can fail on a handler that waited for the CPU (`script time budget
exceeded` in its log); `--budget-ms 1000` raises the hosts' budget
(`MAKEPAD_SPLASH_BUDGET_MS`) for such a machine.

| Run | What it covers |
| --- | --- |
| `shell` | Every mode on the samples in the shell's own window, light, then dark through the shell's own style menu (the dark pass works on what the light pass left): Home and its covers, the status line against the storage's own bytes, "Open a PDF from this device" refused (on Linux with no dialog helper, "Not available here"), reading and thumbnails, Read mode (entered from the pill, the window in full screen while it lasts, a page turned and the zoom changed in it, left by Exit read mode or the corner control on that page at that zoom, with the window back; then left by Esc), find ("9 matches on 4 pages") from the top bar alone, with the left panel given back when it ends, a highlight and its reply dated on this machine's clock ("Today 19:42"), a rotation and its undo, combine with a page range ("One PDF of 5 pages"), initials placed by Fill & Sign, a text edit with an alignment and a colour, save, and the damaged file; then one cover per PDF in the cover cache, each in `library.json` |
| `restart` | The same home again: the library, its titles, the page a PDF was left at, `Imported PDF.pdf` placed as an import leaves one, every current cover kept, not drawn again, and a PDF removed from Home taking its kept cover with it |
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
that refusal. On Linux the run's window is mapped on its weston and counts as
focused. With no dialog helper on `PATH` the runs check "Not available here"
and the button's note instead. With a helper the dialog would really open, so
they leave the step out and say so in `run.json`.

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
- **Verified on 10 Oct 2026 at the window's own size** (macOS, Apple silicon;
  [tests/evidence/window-20261010](tests/evidence/window-20261010/README.md)):
  the `shell` run, light and dark, with the real engine at the runtime's 64 ms
  budget, in the wide layout (an app area of 1292 x 662); and, opened from the
  shell's menu, a 1296 x 703 window on a 1380 x 845 work area, 32 points
  above the dock.
- **Verified on 10 and 11 Oct 2026, the follow-ups** (macOS, Apple silicon;
  [tests/evidence/v2-followups-20261010](tests/evidence/v2-followups-20261010/README.md)):
  the model tests; the `shell`, `restart`, `full` and `empty` runs in a hidden
  desktop shell with the real engine, at the window's own size (an app area
  of 1292 x 662): comment dates in local time, the Edit panel's Alignment and
  Colour, the storage line from `files.status`, covers kept across closes and
  a restart and removed with their PDF, Combine's button in view with two
  PDFs, one search in the top bar, and Read mode; and the `fixture` run in
  card-host at 1536 x 1024. On 10 Oct, before one search and Read mode, those
  runs passed at the runtime's 64 ms budget. On 11 Oct they ran with
  `--budget-ms 1000`: at 64 ms, under load averages of 54 to 74, the `shell`
  run passed every step but overran the budget after UI-thread stalls in the
  pdf service's file renames.
- **Verified on 11 Oct 2026 on Linux** (the Linux build host: Ubuntu 26.04,
  x86_64, software GL under a headless weston, homes on ext4;
  [tests/evidence/linux-20261011](tests/evidence/linux-20261011/README.md)):
  - the model tests and the pdf service's tests;
  - the `shell`, `restart`, `full` and `empty` runs with the real engine at
    the runtime's 64 ms budget, five times: four in UTC (the last two after
    rebases onto main with #485, #494 and #487), and once in UTC+14, where
    comment dates crossed midnight correctly;
  - PDF Tools opening at 1296 x 698 (an app area of 1292 x 662), as on the
    Mac;
  - "Open a PDF from this device" importing a PDF for real through Makepad's
    Linux dialog path, with a stand-in answering for the dialog helper.
- **Verified on 11 Oct 2026, Edit on real-world PDFs** (the Linux build host:
  a private headless weston and Makepad's Wayland backend, not macOS, where
  another session was running OctoSense;
  [tests/evidence/edit-20261011](tests/evidence/edit-20261011/README.md)):
  the model tests; the pdf service's tests; in a desktop shell with the real
  engine at the runtime's 64 ms budget, light and dark, on the
  `--edit-sample` page: the five-pieces note, the font and room warnings with
  Apply anyway, the plain refusal of Chinese text, an edit made anyway and
  one made at once. Edit on macOS since this change is **unverified**.
- **Verified on 13 Oct 2026, Read mode in full screen** (the Linux build
  host: Ubuntu 26.04, x86_64, a private headless weston;
  [tests/evidence/read-fullscreen-20261013](tests/evidence/read-fullscreen-20261013/README.md)):
  the model tests; the `shell` run with the real engine at the runtime's
  64 ms budget, light and dark, on #520 at `18c11642`. The pill's
  full-screen button put the window over the shell's whole window, Exit
  read mode and the corner control gave it back on the same page at the
  same zoom, and Esc ended both the full screen and Read mode. These are
  **unverified**: the same on macOS; the OctoSense window entering the
  host's full screen, which a hidden shell never asks for; and card-host,
  which serves no window service, so Read mode there should fill its view
  as before without a word.
- **Not verified**: the host's own file dialog (a hidden window on the Mac
  cannot open it; on Linux a stand-in helper answered); with the real engine,
  form fields, outlines, Redo, the other comment and page operations,
  password-protected PDFs, the eight-PDF cap and the unsaved-changes question
  (the dev fixture covers their screens); other screen sizes; on Linux, X11,
  a GPU and a desktop compositor; Windows; phones.
- **Known**: Read mode's full screen asks the shell to put the OctoSense
  window itself in full screen as well; a hidden test shell never asks the
  host window, so that step on a real display is **unverified**. On a
  heavily loaded machine a tap can overrun the 64 ms budget
  and be lost (seen at load averages of 60 to 150 before `library.json` moved
  off the tap path). The pdf service's file writes run on the UI thread and
  count against that budget too (overruns after 262 and 555 ms renames, at
  load averages of 54 to 74). On a screen 900 points tall (an app area of about
  1292 x 662), Home's last caption line (when each PDF was last opened) sits
  at the fold; the library scrolls. The engine reports neither a
  paragraph's alignment nor its colour: the Edit panel reads the alignment
  from the lines' boxes and shows the colour as the paragraph's own until you
  pick one.
- **Not part of this version**: export (no approved design), opening
  password-protected PDFs, keyboard shortcuts (the runtime gives script apps no
  key events).
