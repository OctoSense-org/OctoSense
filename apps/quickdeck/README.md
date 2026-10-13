# Quick Deck

English | [简体中文](README.zh-CN.md)

Quick Deck (`os.quickdeck`) is OctoSense's presentation app for the desktop:
a library of decks, an editor with a slide navigator, a canvas and an
inspector, a slide sorter with sections, New from an outline (typed, or
drafted with the person's own AI provider), Present and Export. The shell's
`deck` engine service (deckcraft,
[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.md)) does the
work in the app's own storage, through the methods in
[design/SERVICE.md](design/SERVICE.md); the app is a contained Splash program
in `bundle/` and never reads a deck itself. It ships in the desktop shell
only, because the craft engines are desktop only (`craft-engines`).

It declares `storage`, `files`, `deck` and `model`, and no
`storage.max_bytes`, so its storage is App Hub's system ceiling, 64 MiB.

Version 2 was rebuilt through OctoSense App Flow from ten approved generated
designs (approved 11 Oct 2026): the product requirements in
[design/BRIEF.md](design/BRIEF.md), the untouched images and their
measurements in `design/source/`, and one pixel map per screen in
`design/map/` that ties every measured region to the widget that rebuilds it,
its geometry, colours, type size, the runtime source it relies on and the
review decision that applies.

## The window

| Region | What it holds |
| --- | --- |
| Tabs | Home, one tab per open deck (its title, a dot while it has unsaved changes, a close cross), the outline page's tab while it is open, and New, which goes Home. Titles share the row and are cut with an ellipsis when it is full |
| Toolbar | Slides and Sorter; Text, Picture, Shape, Table, Chart (each opens its insert strip); Undo and Redo; Save, Export and Present |
| Navigator | New slide, the sections (each folds), numbered thumbnails (the current one outlined, a hidden one dimmed) |
| Canvas | The current slide, the engine's render at the canvas's width, with the selection's frame and handles; the pill: previous, "5 / 12", next, zoom out, the zoom, zoom in, Fit |
| Inspector | Format (what the selection is decides what shows: text, box, shape, picture, table, chart, arrange), Layout (the eleven layouts, background, transition, skip), Theme (themes, colours, fonts, slide size, footer), Notes. Every switch and mark shows what the deck has: a box's anchor and autofit (`deck.format_state`), a chart's elements and palette and a table's options (`deck.slide`), the colour set, font pair and footer (`deck.open`, `deck.state`); a change shows at once, the service's next answer confirms it, and after Undo or Redo the deck's own come back without reopening |
| Status line | Saved or Edited, "Slide 5 of 12 · 16:9" (in the sorter, "12 slides · 2 selected"), and what the app's storage holds against what it may hold, from the shell's `files.status` |

The designs are drawn at 1536 x 1024, and the manifest asks the desktop to
open the window at that size (`"window": {"width": 1536, "height": 1024}`,
App Hub's window hint, `"schema_minor": 1`). The desk clamps it: in the
hidden shell's 1400 x 900 window on Linux, Quick Deck opens at 1296 x 698
points. Below 1180 points wide the navigator narrows to 200 points and the
inspector opens over the canvas from an Inspector button in the toolbar
(**unverified**: no run used a window that narrow).

## Screens

| Screen | What it does | Engine |
| --- | --- | --- |
| Home | Start a new deck from one of the engine's themes; New from an outline; Open a presentation from this device (.pptx, up to the host's import limit); Recent decks as cards with the engine's cover of their first slide, slides and size, when edited; Duplicate and Remove (asks once more) | `deck.themes`, `deck.cover`, `deck.create`, `deck.open`, `deck.save`, `deck.close` |
| Editor | The navigator, the canvas and the inspector. Tap to choose (`deck.hit`), a long press adds to the choice, tap a chosen text box again (or double-tap) to type in it, drag to move, a handle to resize or rotate. Insert strips for text, pictures (from images/ or this device), shapes (the engine's gallery), tables and charts | `deck.slides`, `deck.slide`, `deck.view`, `deck.hit`, `deck.set_text`, `deck.format_state`, `deck.format`, `deck.insert`, `deck.shape`, `deck.picture`, `deck.table`, `deck.chart`, `deck.layouts`, `deck.slides_op`, `deck.design`, `deck.transition`, `deck.notes`, `deck.fonts`, `deck.shapes` |
| Sorter | Every slide as a tile in its sections, with its number and transition: choose slides, New slide, Duplicate, Delete, Hide or Show, Move earlier and later, drag a tile to move it, add, rename and remove sections | `deck.slides_op`, `deck.view` |
| New from an outline | A card per slide (a title, then its points, one per line), a theme and a slide size, Make deck. Draft with AI asks the person's own provider for the cards (`model.complete`, fast class); without a provider it says to set one up in Settings → AI providers, and typing the outline works either way | `deck.create`; the host's `model.complete` |
| Present | The slide alone on a dark stage inside the app's window, a small bar (previous, "4 / 12", next, Notes, the time, Exit) and an Exit in the corner; each build step shows its end state, and nothing animates. Notes opens a strip with the slide's notes | `deck.show`, `deck.view {step}`, `deck.slide` |
| Export | Presentation (.pptx), PDF (slides, notes pages or handouts, 1 to 9 per page) or Images (one PNG a slide); all slides, the current one or a range; hidden slides in or out; a name. The deck's earlier exports are listed and can be removed | `deck.export` |

Undo and Redo use the engine's history; Save writes the native `.deckcraft`
file (`deck.undo`, `deck.redo`, `deck.save`, `deck.state`). Closing a tab with
unsaved changes asks: Save, Don't save, or Cancel.

States every screen handles: an empty library, a slide still rendering, a
damaged deck ("Couldn't open this deck" and the engine's reason), a protected
presentation (OctoSense keeps passwords on host sheets and there is none for
presentations, so it is not opened), one the engine holds back (a zip
inside it, or parts beyond the service's import guard, #448), one it doesn't read
(.ppt, .key, .odp, PDF), eight decks already open, storage full (the bar
under the tabs, and the status line goes red), unsaved changes when closing,
and no engine on the device ("The deck engine isn't available here"), or an
older one ("This OctoSense's deck engine is older than Quick Deck").

## Opening a presentation from this device

"Open a presentation from this device" asks the shell's files service to copy
one file the person chooses in the host's own dialog into `imports/`. The app
names the new file first, because `files.import` takes its destination before
the dialog opens: `Imported presentation.pptx`, then `Imported presentation
2.pptx` and so on. The host reports the chosen file's display name, which
becomes the deck's title without `.pptx` ("Lease 2026", or "Lease 2026 (2)"
when that title is taken). Then it opens in a tab, through the service's
import guard. Its first Save writes `decks/<title>.deckcraft` and removes the
import. A refusal shows on Home in plain words with the host's own message
below it.

## How it works

Every path is relative to the app's storage, the folder `fs.*` sees; the
engine works in the same folder.

| Path | Holds |
| --- | --- |
| `library.json` | Version 2: per deck file its title, slides, size, theme, when it was made, edited and opened, the slide and zoom it was left at, its cover and its exports; the pictures brought in; "Show notes while presenting"; the outline being written |
| `decks/*.deckcraft` | The decks, in the engine's native format |
| `imports/*.pptx` | A presentation brought in from this device, until its first save |
| `images/` | Pictures brought in for slides |
| `exports/` | Exports: `.pptx`, `.pdf`, and a folder of PNGs per images export |
| `.cache/` | The service's renders and covers: at most 16 MiB and 64 files, at most 32 of them covers |

v1's decks (`decks.json` and `decks/<id>/g<n>/deck.deckcraft`) join the
library on the first run of v2, titled with their outline's first slide.

Notes for whoever changes the script:

- **Engine work runs on the shell's UI thread (#399).** One call is out at a
  time (`pump()`): the current slide at the canvas's width (kept by
  revision; a change draws again only the slides its answer lists in
  `changed`), the slide's shapes, the selection's format, the layout
  pictures, then the navigator's thumbnails one PNG read per timer tick, the
  sorter's tiles, and Home's covers. The first call after the shell starts
  can wait for the engine's font warm-up.
- **No scroll position for script apps.** The navigator and the sorter show a
  window of slides around the current one (40 and 30), with "‹ Slides 1–n"
  and "Slides n–N ›" to move it.
- **No key events for script apps.** Every shortcut in the brief is a button:
  Undo, Redo, Save, Present, Exit, Duplicate, Delete, Move earlier and later.
- **No TextInput selection for script apps.** Formatting applies to a whole
  text box, or to the paragraphs chosen in the inspector (Applies to). A text
  box being edited is a text field over the slide, which the engine draws
  without that box's text; the field shows plain lines, without bullets.
- **Drawing a region again rebuilds its widgets** (`widgets/src/view.rs`), so
  a drag held by one of them ends: a drag on the slide draws its outline in a
  layer of its own and changes the shape once, when it is let go; the
  sorter's strip names where a dragged tile would go.
- **State lives on `mod.quickdeck_state`.** A light/dark switch runs the
  script again; every `let` starts over, `mod` survives, and `resume()` draws
  every region with the new look.
- **Labels draw their own margin and padding twice**, and a view whose
  `on_render` starts hidden stays hidden (as in PDF Tools): Labels have no
  padding of their own, and regions that hide are wrapped in plain Views.
- **A computed key is not a named field.** `d[key] = v` on an object written
  as a literal leaves `d.key` as it was, so the script assigns named fields
  where it reads them back.
- One handler, timer or callback has a 64 ms budget of wall-clock time;
  slide images are read with `fs.read_bytes` and shown with
  `binary_resource`.

## Testing

The library's rules are pure functions in `main.splash`, tested from Rust by
evaluating the bundle's functions in a script VM
(`crates/shell/src/quickdeck_model_tests.rs`): an imported presentation's
title, what the library index keeps across a restart, what v1's decks
become, the storage, card, notes and timer words, the outline's counts and
problems (with the service's caps), what an outline and a model's draft send
to the engine, the slides a range names, Move earlier and later, what a
refusal says, the names the engine uses, the canvas's fit and zoom steps,
the selection's resize and rotation, and the chart grid's numbers.

```sh
cargo test --locked -p octosense-shell --lib quickdeck_model
```

`tests/ui.py` runs the app end to end. With `--card-host` it uses App Hub's
`card-host`, which serves no host services. With `--shell` it uses a desktop
shell built from this checkout, with the real engine: on macOS hidden from a
Terminal.app tab, on Linux on a private headless weston, as PDF Tools' ui.py
does; a lock folder keeps it to one hidden shell on the machine.

```sh
python3 apps/quickdeck/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/quickdeck-ui
cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines
python3 apps/quickdeck/tests/ui.py --shell target/debug/octosense --lock <dir> --work <dir> --grab-scale 1 --output target/quickdeck-ui
```

The runtime gives one handler 64 ms of wall-clock time, so on a busy machine
a run can fail on a handler that waited for the CPU (`script time budget
exceeded` in its log); `--budget-ms 1000` raises the shell's budget
(`MAKEPAD_SPLASH_BUDGET_MS`, OctoSense's runtime patch) for such a machine. A
`card-host` built against a runtime without that patch ignores it.

| Run | What it covers |
| --- | --- |
| `missing` | `card-host` with the shipped bundle: no engine and no files service, on Home, in a deck's tab and on the outline page with its Draft with AI |
| `fixture` | `card-host` with the dev fixture: every designed screen at the designs' 1536 x 1024, light and dark, each grab beside its design in `compare/`, then a table's options and the footer; the inspector's switches and marks are checked against what the stand-in reports, and a table option and the footer's date against what it reports after a change and its Undo |
| `states` | The dev fixture's damaged, protected, held and unsupported presentations, the unsaved-changes bar, storage full and the empty library of a first run |
| `shell` | A deck made from an outline with the real engine, then edited: text in place, bold, a chart, undo and redo, a layout, a theme, notes, the sorter, save, export and Present; light, then dark through the shell's own style menu. In the light pass the inspector's states follow real changes and their Undo: a box's anchor and autofit, a chart's data labels, a table's total row, the colour set and the footer, each read again from the engine (the shape chosen again, or the deck's design asked again) |
| `restart` | The same home again: the library, the deck's cover and slides |

The dev fixture never ships. `dev-fixture/engine.splash` is a stand-in for
the deck engine that answers SERVICE.md's methods as the service does;
`tests/ui.py` puts it in place of `engine()` in a scratch copy of the bundle.
`dev-fixture/make_fixture.py` fills the app's storage with the approved
designs' sample deck (the Northwind Q3 board update, its slides drawn as
pictures from the designs' own sample text), five more decks for Home, the
layout pictures, and what the stand-in answers (the engine's real themes,
colour sets with their colours, font pairs, shapes and fonts, and each box's,
chart's, table's and the deck's states as #507 reports them); the stand-in
keeps an undo history of the deck's data. `--states`, `--full` and `--empty`
make the other runs' libraries. `--bundle DIR --grabs-only` takes the same
frames of another bundle (main's, for a before and after) without checking
the states.

```sh
python3 apps/quickdeck/dev-fixture/make_fixture.py <card-host app data>/os.quickdeck
```

## Status

See [tests/evidence/v2-20261011](tests/evidence/v2-20261011/README.md) for
v2's frames, side-by-sides and receipt, and
[tests/evidence/inspector-states-20261011](tests/evidence/inspector-states-20261011/README.md)
for the inspector's states (#507).

- **Verified on 11 Oct 2026, in `card-host`** (macOS, Apple silicon, at the
  runtime's 64 ms budget): the `missing`, `fixture` and `states` runs, light
  and dark, with no script error in their logs.
- **Verified on 11 Oct 2026, with the real engine** (Linux, a hidden desktop
  shell on headless weston, built from this branch on main 202d7cc8, which
  has the deck service of #503): the `shell` run, light then
  dark, and `restart`, at the runtime's 64 ms budget with no script error or
  overrun. The shell opened Quick Deck at 1296 x 698 points (an app area of
  1292 x 662). The model tests pass there (22; 23 with `--features app-hub`,
  which adds the manifest's window hint).
- **Verified on 11 Oct 2026, the inspector's states** (#507): in `card-host`,
  the `fixture` and `states` runs, light and dark, at the 64 ms budget, the
  switches and marks checked against the stand-in; with the real engine on
  Linux (shell built from this branch on main eb2fd5d1, which has #507), the
  `shell` run's light pass changed a box's anchor and autofit, a chart's
  data labels, a table's total row, the colour set and the footer, read each
  back from the engine, then undid each and read the deck's own back; then
  dark and `restart`. The model tests pass (27).
- **Not verified**: a real import from the device (a hidden window on macOS
  has the dialog refused, and the Linux host has no dialog helper), so the
  import guard's refusals through the app; Draft with AI with a real provider
  (only the no-provider answer); a hidden shell on macOS; with the real
  engine, pictures, tables, the shape gallery, backgrounds, Present's notes
  strip with long notes and an images export (the dev fixture covers their
  screens); windows narrower than 1180 points; other screen sizes; phones.
- **Known**: the shell's window is smaller than the designs' 1536 x 1024, so
  the theme panel, Export and the outline scroll. Theme previews draw the
  app's own font (OctoSense bundles no theme fonts). A box being edited
  shows plain lines without bullets. A built-in theme's own font pair is
  named after the theme, so the pair with the same families is marked (Harbor's
  fonts are Classic's); an imported deck's own colours or fonts are named in a
  hint. Present fills the app's window, not the screen.
