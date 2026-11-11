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
decision that applies. The regions are still the designs'; their sizes and
colours now follow the Mac look in [apps/AGENTS.md](../AGENTS.md), with
Preview as the model, so the maps record how version 2 was designed, not
today's measures.

## The window

| Region | What it holds |
| --- | --- |
| Toolbar | On Home: the app's name, how many PDFs it holds, and Open PDF…. On a PDF that won't open: its name and Open PDF…. In a document, as in Preview: the sidebar button; page "3 / 24", previous and next; zoom out, zoom, zoom in, fit width, fit page; Read · Comment · Fill & Sign · Pages · Combine · Edit as a segmented control; Undo, Redo, Save; Search in document; and full screen (Read mode) |
| Tab strip | Under the toolbar, once a PDF is open, as in Quick Deck: a Home tab, one tab per open PDF sharing the width (its name, a dot while it has unsaved changes, a close cross under the pointer), and a + at the end, which goes to Home |
| Sidebar | One sidebar, as in Preview, with a pop-up at its head choosing Pages (thumbnails), Outline (the PDF's bookmarks) or Comments; the toolbar's button or ⌃⌘S hides it. A search's results take it while the search runs, and give it back when it ends |
| Tool row | Over the canvas: the find bar, Comment's tools and colours, Fill & Sign's tools and Highlight Fields, Pages' actions |
| Canvas | The pages, white on the window's gray: a run of up to eight pages from the page you went to, each the engine's render at screen resolution. In Read mode a bar at the foot has the page, the zoom and Exit Read Mode, and the pages stop above it |
| Inspector | Comment: the threads. Fill & Sign: what the form says and its fields, as an index. Edit: the paragraph being edited |
| Status bar | On Home, as Finder's: how many PDFs and the room left in the app's storage, from the shell's `files.status` ("3 PDFs, 49.4 MB available", "1 PDF, 37 KB available"); on a PDF, Saved or Edited. On a desktop it also says what just happened ("Inserted “Appendix.pdf” after page 3.") for eight seconds, with nothing to dismiss; a phone shows a banner instead |

The designs are drawn at 1536 x 1024, and the card host runs them at that size.
In the shell the manifest asks for the size of the other big windows:
`"window": {"width": 1180, "height": 740}`, App Hub's optional window hint,
which needs `"schema_minor": 1`. The desk clamps it to its own size, less the
margins every new window keeps (42 points left and right, 32 top and bottom).
Where the dock floats over the desk, as OctoSense's does, the bottom margin is
kept above the dock, so the status bar is never behind it
(`crates/shell/src/desktop_layout.rs`). In the hidden shell's 1400 x 900 window,
PDF Tools opens at 1180 x 719 points. (The first hint, 1536 x 1024, opened at
1296 x 703: [tests/evidence/window-20261010](tests/evidence/window-20261010/README.md).)
Before the hint it got the default window, 72% of the desk's width and 76%
of its height, at most 1000 x 720 points: an app area of 990 x 603. A
shell that predates the hint ignores it and opens the default. The person
can resize or maximize the window either way. At 980 points wide and up, an
inspector opens beside the left panel. Below 1120 the modes trim their padding,
Save shows its icon alone and the fit tools leave the toolbar. Below 980 the
modes fold into a pop-up, both inspectors take 256 points and an inspector takes
the left panel's place. Until the person zooms, the page fits the canvas, up to
100%.

## Modes

| Mode | What it does | Engine |
| --- | --- | --- |
| Home | Recent PDFs as a grid of first pages, each with its name, pages, size and when it was opened; Open PDF… as a tool in the toolbar (up to 64 MB, said under the button on an empty Home and when a file is too big); remove a PDF from its card's menu or with ⌘Delete (asks once more); on a first run, a line or two and the one button | `pdf.open`, `pdf.info`, `pdf.page`, `pdf.close` |
| Read | Pages render one at a time at 72 dpi x zoom x 2 and are kept; thumbnails fill in one by one; the outline jumps to a section | `pdf.open`, `pdf.page` |
| Read mode | The toolbar's full-screen button: the pages alone. The toolbar, the tabs, the sidebar and the inspector, the tool row and the status bar give the pages their room, and a bar at the foot has previous and next, the page, the zoom, fit width and fit page, and Exit Read Mode. In a desktop shell the same click puts the window in full screen too (`window.fullscreen`): the pages fill the screen, with no bar, title bar or dock, and no screen edge brings them back. There Esc, the shell's key, leaves Read mode, and so do ⌘F and another window taking the focus (`on_app_fullscreen`). The bar's Exit Read Mode and the View menu's row leave too. Where the shell can't (a click it doesn't count, or a host with no window service, such as card-host), Read mode fills the window, without a word. The page and the zoom you chose carry across entering and leaving; a fitted or automatic zoom fits the new canvas. A click on a page only chooses it | `pdf.page` |
| Find | Search in document, the one way to search, then Return: "12 matches on 7 pages" in the sidebar, the results grouped by page with the match in bold, every match highlighted under its text from the glyphs' own rectangles (a tint of the page's own picture), the current one stronger with a ring, previous and next (⌘G, ⇧⌘G). Done, the results' cross, the field's cross or Esc in the field ends the search, and the sidebar shows what it did before (Pages if it showed nothing known); a change to the document ends it the same way | `pdf.find` |
| Comment | Highlight, Underline or Strike out the line you click (click twice for its paragraph), Note and Text box where you click, in one of four colours; threads with replies, a status, delete. Comments carry the name kept under "Comment as" | `pdf.lines`, `pdf.comments`, `pdf.comment` |
| Fill & Sign | What a field holds shows on its page, in its box (required empty ones in red), and in the inspector's list. A click on a text field types there: Return or a click elsewhere keeps it, Esc leaves it as it was, Tab and Shift+Tab go on in the form's order. A check box turns over at a click, a choice opens its menu, a read-only field says so. The engine's own render takes over as each value lands. Highlight Fields hides the boxes. Add Text, Add Date and Add Initials place a mark where you click; initials are kept | `pdf.fields`, `pdf.fill`, `pdf.fill_sign` |
| Pages | Every page as a large thumbnail. A click selects one, with ⌘ it goes in or out, with ⇧ the run from the last one clicked; a click beside them lets go, and two clicks read that page. The arrows move, ⌘A takes them all. Rotate Left or Right, Delete, Extract (a new PDF in the library), Insert from File… (a menu of this library's other PDFs and Open PDF…), drag a page to move it, a line showing where it lands | `pdf.pages` |
| Combine | This PDF and others in order (drag the grip to reorder), each with its pages ("1-4, 9" or All), "One PDF of 31 pages", a name, Combine; the new PDF opens. The list scrolls, and the total, name and Combine stay at the pane's foot | `pdf.merge` |
| Edit | Click a paragraph: its text becomes editable in place; the inspector shows its font and size, its Alignment (left, centre, right or justify: the one its lines show is chosen, since the engine sets an edited paragraph flush left unless told) and its Colour (its own until you pick one of six); before Apply it says what the edit would do besides changing the words, or why it can't be made ([Editing text](#editing-text)); Apply (or Apply Anyway) or Cancel | `pdf.lines`, `pdf.edit_check`, `pdf.edit_text` |

Undo and Redo use the engine's history; Save writes the PDF back
(`pdf.undo`, `pdf.redo`, `pdf.save`, `pdf.state`). Closing a tab with unsaved
changes asks: Save, Don't Save, or Cancel.

On a Mac the menu bar has what the toolbar offers (File, Edit with Find… at ⌘F,
View with the sidebar at ⌃⌘S, Go, Tools), each row off while it can't act. Cards,
thumbnails, tiles, tabs, outline rows, comments, results, fields and Combine's rows
have right-click menus. Home takes the arrows, Return and ⌘⌫; a document takes the
arrows, Space, Page Up and Down, Home and End; a sheet takes Esc, and the unsaved
one Return.

States every screen handles: an empty library, a page still rendering, a
damaged PDF ("Couldn't Open This PDF" and the engine's reason), a protected
PDF ("This PDF Is Protected with a Password": OctoSense keeps passwords on
host sheets and there is none for PDFs yet, so it is not opened), storage
full ("Remove a PDF to make room"), eight PDFs already open, unsaved changes
when closing, and no engine on the device.

## Editing text

Edit redraws a paragraph in place with the engine's own editor. Real-world
PDFs limit what that can do, and the inspector says so before Apply. As
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

While the panel has a warning, Apply reads "Apply Anyway"; an edit that
hasn't been checked as it stands yet is checked first, and is made at once
only if there is nothing to say. A refusal shows in red and Apply only
repeats it. The inspector's fields scroll, and Apply and Cancel stay at its
foot, in view even in the desktop's default window. A document whose security settings forbid changes, or that opens
for reading only, is refused the same way.

The pdf service's `pdftools_fixture` example writes a page as a web browser
prints it, with all four (a title in five pieces in unnamed Type 3 fonts,
Chinese text in an embedded subset font), to try Edit on:

```sh
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- --edit-sample <file>
```

## Opening a PDF from this device

"Open PDF…" asks the shell's files service to copy one
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
- **The canvas is a run of pages.** It is a static `ScrollXYView` that keeps
  its place while renders land, and going to a page (a thumbnail, the outline,
  a find result, the toolbar) draws the canvas empty for one frame, which clamps
  its scroll to the top, then fills it with a run of pages that starts at that
  page. Pages before it come back with the toolbar's previous. `ui.canvas.scroll_y()`
  tells the toolbar which page the scroll has reached, and the keys scroll with
  `set_scroll_y`.
- **The keys are one view's.** `keys` (a `GestureView` with `on_key`) wraps Home and
  the document but not the toolbar, whose band moves the window. Whatever takes the
  keyboard (a field, a sheet) gives it back with `ui.keys.focus()`.
- **One input for a whole form.** `field_input` is drawn over the field being typed
  in, alone in its `nav_group`, so Tab is its `on_tab`. While it is up, nothing draws
  the canvas again (`canvas_held`), or it would lose the caret.
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
(a font's name, the warnings and "Apply Anyway", a line in pieces). They
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
| `shell` | Every mode on the samples in the shell's own window, light, then dark through the shell's own style menu (the dark pass works on what the light pass left): Home and its covers, the status bar against the storage's own bytes, "Open PDF…" refused (on Linux with no dialog helper, "Not available here"), reading and thumbnails, Read mode (entered from the toolbar's last button, the window in full screen while it lasts, a page turned and the zoom changed in it, left by Exit Read Mode or the View menu's row on that page at that zoom, with the window back; then left by Esc), find ("9 matches on 4 pages") from the top bar alone, with the sidebar given back when it ends, a highlight and its reply dated on this machine's clock ("Today 7:42 PM"), a rotation and its undo, combine with a page range ("One PDF of 5 pages"), initials placed by Fill & Sign, a text edit with an alignment and a colour, save, and the damaged file; then one cover per PDF in the cover cache, each in `library.json` |
| `form` | A real form (`form_pdf`: text, a check box, a choice, a read-only field), filled on its page by the real engine, light then dark: typing in place, Tab, Return and Esc, the value on the page once and in the page's own picture after, Undo and Redo, Save; then the values after a restart |
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
  - "Open PDF…" importing a PDF for real through Makepad's
    Linux dialog path, with a stand-in answering for the dialog helper.
- **Verified on 11 Oct 2026, Edit on real-world PDFs** (the Linux build host:
  a private headless weston and Makepad's Wayland backend, not macOS, where
  another session was running OctoSense;
  [tests/evidence/edit-20261011](tests/evidence/edit-20261011/README.md)):
  the model tests; the pdf service's tests; in a desktop shell with the real
  engine at the runtime's 64 ms budget, light and dark, on the
  `--edit-sample` page: the five-pieces note, the font and room warnings with
  Apply Anyway, the plain refusal of Chinese text, an edit made anyway and
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
- **Verified on 2 Nov 2026, the Mac look** (macOS, Apple silicon; the branch's
  release shell, hidden, load averages of 13 to 38): the `shell`, `restart`,
  `full` and `empty` runs with the real engine on a release build whose pack
  holds this bundle, with `--budget-ms 1000`; the `shell`, `full` and `empty` runs
  at the runtime's 64 ms budget, on the build before it with its unpacked bundle
  replaced by this one; Edit's note, warnings and refusal on the `--edit-sample`
  page; and, on the dev fixture's stand-in engine in the same shell (where a few
  handlers overran even 1000 ms, on file writes under that load), outlines,
  threads and their menus, form fields, Add Text and Add Date, the drop marker,
  an extract's note, Combine's refusal and its list of PDFs, and the damaged,
  protected and unsaved states; each light and dark. In card-host, the `missing`
  and `fixture` runs (that build ignores `--budget-ms`: `fixture` passed at load
  averages near 10, and above 20 the stand-in's find overran 64 ms). A smaller
  window (a 1000 x 640 hint in the unpacked manifest) was looked at on the
  stand-in engine too, and 990 x 603 in card-host.
  The model tests were **not** run on this change.
- **Verified on 4 Nov 2026, the Mac look's second pass** (macOS, Apple silicon,
  a Mac mini at 1x; a release shell, hidden, whose pack holds this bundle,
  `--budget-ms 1000`): the whole of `tests/ui.py`: the `shell`, `restart`, `form`,
  `full` and `empty` runs with the real engine, and the `missing` and `fixture`
  runs in card-host. A rig of its own (not in this repo) ran 30 journeys, each
  light and dark: the keys, the menu bar, right-click menus, a click while the
  pictures come in, a narrow window down to 700 points, an appearance switch with
  text typed, and with the real engine find, warnings on a printed page, Insert
  from File…, a form filled on its page, and a restart. The model tests passed
  (`pdftools_model_tests`, `system_app_theme_tests`, `module_resize_tests`).
- **Not verified**: the host's own file dialog (a hidden window on the Mac
  answers through `MAKEPAD_TEST_OPEN_FILE` and never opens it; on Linux a stand-in
  helper answered); a real mouse (every press in these runs came through the
  hidden window's remote route); with the real engine, outlines, the other comment
  and page operations, password-protected PDFs, the eight-PDF cap and the
  unsaved-changes question (the dev fixture covers their screens); a real display
  at 2x; on Linux, X11, a GPU and a desktop compositor; Windows; phones.
- **Known**: Read mode's full screen asks the shell to put the OctoSense
  window itself in full screen as well; a hidden test shell never asks the
  host window, so that step on a real display is **unverified**. On a
  heavily loaded machine a tap can overrun the 64 ms budget
  and be lost (seen at load averages of 60 to 150 before `library.json` moved
  off the tap path). The pdf service's file writes run on the UI thread and
  count against that budget too (overruns after 262 and 555 ms renames, at
  load averages of 54 to 74). The engine reports neither a
  paragraph's alignment nor its colour: the Edit panel reads the alignment
  from the lines' boxes and shows the colour as the paragraph's own until you
  pick one. A form field that carries a value but no appearance stream shows its
  value in Fill & Sign, which draws it on the page; elsewhere the engine's picture
  leaves it blank until the field is filled here. In Read mode the shell's Esc hint
  stands over the foot of the page for a few seconds.
- **Not part of this version**: export (no approved design), opening
  password-protected PDFs.
