# PDF Tools

English | [简体中文](README.zh-CN.md)

PDF Tools (`os.pdftools`) is OctoSense's PDF app: the PDFs in its own
storage, a viewer with page thumbnails, merge, split and text extraction,
and PDFs opened from the device. The shell's `pdf` engine service (pdfcraft,
[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.md)) does
the work, in the app's own storage; the app is a contained Splash program in
`bundle/` and never reads a PDF itself. It ships in the desktop shell only,
because the ten craft engines are desktop only (`craft-engines`).

It declares `storage`, `files` and `pdf`, and no `storage.max_bytes`, so its
storage is App Hub's system ceiling, 64 MiB.

## Screens

| Screen | What it shows | Its one action |
| --- | --- | --- |
| Library | The PDFs in `accounts/device/library/`: first page, name, pages and size; a damaged file says it can't be opened; "Open a PDF from this device" with the largest file it takes; a card when a file couldn't be added, or when storage is full | **Merge PDFs** (an empty library: **Open a PDF from this device**) |
| Document › Pages | Page thumbnails, 24 at a time; tap one for the page view | **Split into files** |
| Document › Text | The text of each page in reading order, with Find in the text | – |
| Document › Info | File, pages, page size, file size, title, author, subject, keywords, creator, producer, PDF version, security, fonts, bookmarks | – |
| Document › Remove | Asks once more: the PDF is deleted from the app's storage; PDFs made from it stay | **Remove** / Keep it |
| Page view | One page, large; Previous and Next, or swipe | – |
| Merge | Three steps: Choose (numbered picks, 2 to 16 files), Order (move up or down, remove, name), Done (the result, Open) | Next / **Merge** / Open |
| Split | Every few pages (a stepper) or Where I choose (tap the pages that start a new file); a live list of the files it makes, and which existing files it replaces; then Done | **Split into N files** |

Merged and split PDFs stay in the app's storage: exporting them is not part
of this version.

## Opening a PDF from this device

"Open a PDF from this device" asks the shell's files service to copy one
file the person chooses in the host's own dialog into the library. The app
names the new file first, because `files.import` takes its destination
before the dialog opens: `Imported PDF.pdf`, then `Imported PDF 2.pdf` and so
on (the import refuses an existing destination). The copy then opens like
any other PDF here: the engine's `info`, `render` and `text` on the same
relative path.

The host also reports the chosen file's display name (`name`, cleaned of
folders, control characters and direction marks), and that becomes the
PDF's title, without `.pdf`: "Lease 2026", or "Lease 2026 (2)" when another
PDF here already has that title. The file keeps the name the app gave it
(app storage has no rename), and the title lives in `library.json`. Without
`name` (on Android, or when nothing was left of it after cleaning) the title
is the file name, "Imported PDF 2". Parts split from a titled PDF are titled
after it ("Lease 2026-part2"). The library lists files in their storage
order.

`files.status` (asked once, without opening anything) gives the largest file
an import takes, 64 MiB on the desktop, which the library shows; free storage
bounds it too. A refusal shows as a card at the top of the library, in plain
words with the host's own message below: the app is not in the foreground,
storage is full, too many files, the file is too large, another transfer is
running, or no files service.

## How it works

Every path below is relative to the app's storage, the folder `fs.*` sees.
The engine works in that same folder (ADR 0013, "Engines work in the
caller's own folder"): a call made while the app is open may replace a file,
and what the engine writes counts against the app's storage.

| Path | Holds |
| --- | --- |
| `accounts/device/library/*.pdf` | The PDFs (ADR 0004 §11: a system app's data is its `device` account folder), including imported ones and what merge and split write |
| `accounts/device/library.json` | What the engine said about each file (pages and size), and its title when that is not its file name |
| `cache/covers/<name>.png` | Each file's first page (evictable) |
| `cache/pages/<name>/p<n>.png` | The open document's thumbnails; other documents' are removed when one opens, since app storage holds at most 256 entries |
| `cache/view.png` | The page view's image |

| Call | When | Arguments |
| --- | --- | --- |
| `pdf.info` | a file is listed for the first time, a document opens | `{path}` |
| `pdf.render` | covers, thumbnails (`max_side` 360) and the page view (`max_side` 1400) | `{path, page, out, max_side}` |
| `pdf.text` | the Text tab | `{path}`, or the first 512 `pages` |
| `pdf.merge` | Merge | `{paths, out}` |
| `pdf.split` | Split | `{path, out_dir, every}` or `{path, out_dir, before}` |
| `files.status` | the app starts | `{}` |
| `files.import` | Open a PDF from this device | `{path: "accounts/device/library/Imported PDF.pdf"}`; the result's `name`, when present, is the title |

The app never reads `pdf.info`'s `document.path`: until #434 it is the
engine's absolute host path, and every path the app uses is its own relative
one.

When the engine has no room to write (`… more than the N bytes left in this
storage`), the library, the pages and the merge and split screens say that
PDF Tools' storage is full, and Merge or Split says why it failed. Removing a
PDF frees its room and lets the app try its page pictures again.

Notes for whoever changes the script:

- The calls go one at a time: the service runs on the UI thread (#399).
- A refusal answers inside `host.request` itself. Before the runtime overlay
  that came with #413, a UI call made in such a callback lost the rest of the
  callback; `engine()` and the files calls hand every answer to a later turn
  (`start_timeout`).
- Page images are read with `fs.read_bytes` and drawn with
  `binary_resource` (runtime `platform/src/script/res.rs`, reachable through
  the widgets prelude), one stored handle per image, so a redraw reuses the
  decoded texture. Each screen has its own scroller, declared in the page
  and filled by an explicit `render()`. Two things observed in `card-host`
  shaped this: a scroller made inside another view's `on_render` exists only
  after the next draw, and its automatic first render came out empty when it
  held stored images; and a view with `on_render` that starts with
  `visible: false` stayed hidden after `set_visible(true)`. So the scrollers
  sit in plain `View`s, which are shown and hidden instead.
- One handler, timer or callback has a 64 ms budget; the steps after a merge
  or split (forgetting stale caches, rescanning, the next screen) run on
  separate turns.

## Testing

The pdf service's `pdftools_fixture` example writes four sample PDFs and a
damaged file into PDF Tools' storage under an apps root (an OctoSense home's
`apps/`, or a `card-host --app-data` folder), before the app starts. With
`--import-sample <file>` it writes one more PDF, a three-page garden plan.
It never calls the engine; it refuses storage that already holds data.

```sh
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- <apps root>
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- --import-sample <file>
```

`tests/ui.py` runs the app end to end in a hidden desktop shell built from
this checkout, with the real engine: it makes a fresh `OCTOSENSE_HOME` for
each run, writes the samples into `apps/os.pdftools/`, starts
`target/debug/octosense` from a Terminal.app tab (`MAKEPAD_HIDE_WINDOWS=1`,
`MAKEPAD_REMOTE`, `MAKEPAD_WM_TEST_APP=pdftools`), drives it over the remote
bridge, saves every grab and ends with `/quit`. A lock folder keeps it to one
hidden shell on the machine, and it refuses to start while another OctoSense
runs.

```sh
cargo build --locked -p octosense
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --only missing --output target/pdftools-ui
```

| Run | What it covers |
| --- | --- |
| `shell` | Every screen on the samples in light, then dark through the shell's own style menu: covers, pages, the page view, text and find, info, both splits (one replaces a file), merge, open, remove, the damaged file, and "Open a PDF from this device" |
| `restart` | The same home again: what merge and split wrote is still there, and `Imported PDF.pdf`, placed in the library between the runs as an import leaves one, is read, drawn and opened |
| `full` | Storage filled to 8 KB short of its 64 MiB: the engine's refusal on the library and on a merge, then a removal |
| `empty` | No PDFs: the empty library and its Open button, in light and dark |
| `missing` | App Hub's `card-host`, which serves no host services: no engine and no files service |
The library's rules are pure functions in `main.splash`, tested from Rust by
evaluating the bundle's functions in a script VM
(`crates/shell/src/pdftools_model_tests.rs`, as Maps' and Photos' are): an
imported PDF's title with a name, without one, for a name another PDF
already shows, and when nothing is left of the name; a split part's title;
and what the library index keeps across a restart.

```sh
cargo test --locked -p octosense-shell --lib pdftools_model
```

The host's dialog cannot be driven in a hidden shell, and a hidden window
never has focus, so the files service refuses `files.import` there before the
dialog opens (`foreground_required: …`); the runs check that the app shows
that refusal. What follows an import: the title rules above, and the `restart`
run, which takes a library file the index has never seen, read by the real
engine and opened by the app.

## Status

- **Verified** in a hidden desktop shell on macOS (Apple silicon), with the
  real engine: every screen above in light and dark, a restart on the same
  storage with a PDF placed as an import leaves one, full storage, the empty
  library, and `card-host` without host services. The grabs, digests and
  checks are in [tests/evidence/shell-20261009](tests/evidence/shell-20261009/README.md).
  The titles of imported PDFs are verified by the Rust tests of the library's
  rules above.
- **Not verified**: the host's file dialog and a real import through it
  (`imported()` in the script, which applies the tested title rule), the
  import's other refusals, window sizes other than the shell's default,
  Linux, Windows, phones.
- **Known**: a light/dark switch runs the script again (the runtime's style
  reapply), so the app returns to its library. The shell evidence's dark
  frames keep two columns of pages: they were made before the shell sent
  hosted apps their size again after a restyle (9d7a386e).
