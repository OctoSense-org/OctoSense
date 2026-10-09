# PDF Tools

English | [简体中文](README.zh-CN.md)

PDF Tools (`os.pdftools`) is OctoSense's PDF app: the PDFs in its own
storage, a viewer with page thumbnails, merge, split and text extraction.
The shell's `pdf` engine service (pdfcraft, [ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.md))
does the work; the app is a contained Splash program in `bundle/` and never
reads a PDF itself. It ships in the desktop shell only, because the ten craft
engines are desktop only (`craft-engines`).

## Screens

| Screen | What it shows | Its one action |
| --- | --- | --- |
| Library | The PDFs in `accounts/device/library/`: first page, name, pages and size; a damaged file says it can't be opened; "Open from this device" says it is coming | **Merge PDFs** |
| Document › Pages | Page thumbnails, 24 at a time; tap one for the page view | **Split into files** |
| Document › Text | The text of each page in reading order, with Find in the text | – |
| Document › Info | File, pages, page size, file size, title, author, subject, keywords, creator, producer, PDF version, security, fonts, bookmarks | – |
| Page view | One page, large; Previous and Next, or swipe | – |
| Merge | Three steps: Choose (numbered picks, 2 to 16 files), Order (move up or down, remove, name), Done (the result, Open) | Next / **Merge** / Open |
| Split | Every few pages (a stepper) or Where I choose (tap the pages that start a new file); a live list of the files it makes, and which existing files it replaces; then Done | **Split into N files** |

Opening a PDF from the device is coming. The shell's `files.import` (on
`main` since #413) copies one picked document into an app's storage, at most
1 MiB per file, and needs the `files` capability; PDF Tools does not use it
yet, so its entry point says the feature is coming. Until then the library
holds what is already in PDF Tools' storage, including what merge and split
write there.

## How it works

Every path below is relative to the app's storage, the folder `fs.*` sees.
Once the shell's per-caller engine areas are in place, an app's own
`host.request` to an engine works in that same folder (ADR 0013, "Engines
work in the caller's own folder").

| Path | Holds |
| --- | --- |
| `accounts/device/library/*.pdf` | The PDFs (ADR 0004 §11: a system app's data is its `device` account folder) |
| `accounts/device/library.json` | What the engine said about each file: pages and size |
| `cache/covers/<name>.png` | Each file's first page (evictable) |
| `cache/pages/<name>/p<n>.png` | The open document's thumbnails; other documents' are removed when one opens, since app storage holds at most 256 entries |
| `cache/view.png` | The page view's image |

| Engine call | When | Arguments |
| --- | --- | --- |
| `pdf.info` | a file is listed for the first time, a document opens | `{path}` |
| `pdf.render` | covers, thumbnails (`max_side` 360) and the page view (`max_side` 1400) | `{path, page, out, max_side}` |
| `pdf.text` | the Text tab | `{path}`, or the first 512 `pages` |
| `pdf.merge` | Merge | `{paths, out}` |
| `pdf.split` | Split | `{path, out_dir, every}` or `{path, out_dir, before}` |

Notes for whoever changes the script:

- The calls go one at a time: the service runs on the UI thread (#399).
- A refusal answers inside `host.request` itself. Before the runtime overlay
  that came with #413, a UI call made in such a callback lost the rest of the
  callback; `engine()` hands every answer to a later turn (`start_timeout`),
  which also keeps the fixture's answers as asynchronous as a service's.
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

## Status

- **The `pdf` capability is not declarable yet.** App Hub's contract does not
  know it, so `manifest.json` asks for `storage` only, and every shell
  refuses `pdf.*` with `this app was not granted "pdf", which "pdf.info"
  needs`. The app shows that refusal on its screens and keeps listing its
  files. Declaring `pdf` is a one-line manifest change once App Hub has it.
- **Per-caller engine areas are not in yet.** Until they land, the engine
  works in its own private folder, `<apps root>/.host/pdf`, not in this app's
  storage, so its renders would not be visible to the app.
- **Verified** in App Hub's `card-host` on macOS with the developer fixture
  below, in light and dark: every screen, the merge and split flows, the
  damaged-file and engine-refused states and the empty library. **Not
  verified**: the real engine through the app in a shell, which needs both
  items above.

## Developer fixture

`card-host` serves no host services, so PDF Tools cannot reach the engine
there. To exercise every screen anyway, the pdf service's
`pdftools_fixture` example writes four sample PDFs and a damaged file into a
card-host app-data folder, runs the real service on them (App Hub's own
dispatcher), and saves what it answered: `info`, `text`, every page rendered
at both sizes, one merge (Board minutes, then Quarterly report, into
`Merged.pdf`) and two splits of the field guide (every 2 pages, and the cover
on its own).

```text
<app-data>/os.pdftools/accounts/device/library/*.pdf   the samples
<app-data>/os.pdftools/dev/engine-replay.json          the recorded answers
<app-data>/os.pdftools/dev/replay/**.png               the recorded renders
```

The app answers its engine calls from `dev/engine-replay.json` only when that
file is in its storage **and** the real engine refuses the app or is missing;
it checks with one `pdf.info` call first, and logs
`PDF Tools: replaying the engine answers recorded in dev/engine-replay.json
(developer fixture)`. Nothing in the shells writes `dev/`, and with the
engine available the file is ignored, so a person never sees recorded data.
A replayed merge or split writes small placeholder files where the engine
would write its output; the recorded answers stand in for their contents.
A merge or split the fixture did not record fails with a message naming the
fixture.

The fixture lives in app storage, not behind `card-host --static`: a script
can draw an image served from `{{assets}}` but cannot read a file from there
without `net`, and the replay needs to read its answers.

Run it from the repository root, with `card-host` built from App Hub (see
[Running a bundle during development](../README.md#running-a-bundle-during-development)):

```sh
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- target/pdftools-fixture
MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=127.0.0.1:8911 card-host --bundle apps/pdftools/bundle --system --app-data target/pdftools-fixture
```

The example refuses an app-data folder that already holds the app's data;
give it a new one each time. `tests/ui.py` drives the whole journey hidden,
in light and dark, then the engine-refused state (the samples without the
fixture) and the empty library, and saves every original grab:

```sh
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host \
    --fixture target/pdftools-fixture --output target/pdftools-ui
```
