# Quick Deck

English | [简体中文](README.zh-CN.md)

Quick Deck (`os.quickdeck`) is OctoSense's presentation app for the desktop:
a library of decks, an editor with a slide navigator, a canvas and an
inspector, a slide sorter with sections, New from an outline (typed, or
drafted with the person's own AI provider), Present and Export; a deck's video
and audio play in the editor and in Present. The shell's
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
| Canvas | The current slide, the engine's render at the canvas's width, with the selection's frame and handles; a chosen clip's control under it (Play; while it plays, pause or play on, stop, the time and the sound) and its video over its box ([Playing video and audio](#playing-video-and-audio)); the pill: previous, "5 / 12", next, zoom out, the zoom, zoom in, Fit |
| Inspector | Format (what the selection is decides what shows: text, box, shape, picture, table, chart, a clip's playback, arrange), Layout (the eleven layouts, background, transition, skip), Theme (themes, colours, fonts, slide size, footer), Notes. Every switch and mark shows what the deck has: a box's anchor and autofit (`deck.format_state`), a chart's elements and palette and a table's options (`deck.slide`), the colour set, font pair and footer (`deck.open`, `deck.state`); a change shows at once, the service's next answer confirms it, and after Undo or Redo the deck's own come back without reopening |
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
| Editor | The navigator, the canvas and the inspector. Tap to choose (`deck.hit`), a long press adds to the choice, tap a chosen text box again (or double-tap) to type in it, drag to move, a handle to resize or rotate. Insert strips for text, pictures (from images/ or this device), shapes (the engine's gallery), tables and charts. A chosen video or sound plays from its control | `deck.slides`, `deck.slide`, `deck.view`, `deck.hit`, `deck.set_text`, `deck.format_state`, `deck.format`, `deck.insert`, `deck.shape`, `deck.picture`, `deck.table`, `deck.chart`, `deck.layouts`, `deck.slides_op`, `deck.design`, `deck.transition`, `deck.notes`, `deck.fonts`, `deck.shapes`, `deck.media` |
| Sorter | Every slide as a tile in its sections, with its number and transition: choose slides, New slide, Duplicate, Delete, Hide or Show, Move earlier and later, drag a tile to move it, add, rename and remove sections | `deck.slides_op`, `deck.view` |
| New from an outline | A card per slide (a title, then its points, one per line), a theme and a slide size, Make deck. Draft with AI asks the person's own provider for the cards (`model.complete`, fast class); without a provider it says to set one up in Settings → AI providers, and typing the outline works either way | `deck.create`; the host's `model.complete` |
| Present | The slide alone on a dark stage that takes the whole screen: the Present tap asks the shell to put the window in full screen (`window.fullscreen`), and where the host can't (card-host, a phone), the stage fills the app's window. A small bar (previous, "4 / 12", next, Notes, the time, Exit) and an Exit in the corner; Esc, the shell's key in full screen, also ends the show, and the window comes back as it was. Each build step shows its end state, and nothing animates; a slide drawn for the smaller window is drawn again for the full screen. Notes opens a strip with the slide's notes. A clip set to play by itself starts when its slide shows, a tap on a clip plays or pauses it, and another slide or Exit stops it | `deck.show`, `deck.view {step}`, `deck.slide`, `deck.media`; the shell's `window.fullscreen` |
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

## Playing video and audio

A deck's clips, its video and audio objects (SERVICE.md "Playback"), play
inside the app with the runtime's `Video` widget
(`.sources/makepad/widgets/src/video.rs`), which hands the file to the
system's own player. Nothing in the app or the deck service decodes a clip.

- **In the editor** a chosen clip shows its control under it (above it, or
  inside its foot, when the slide has no room below): Play; while it plays,
  pause or play on, stop, the time and the sound (mute). A video plays over its box at the canvas's zoom;
  a sound has the control alone, led by a speaker. The inspector says how
  the clip plays in a show (by itself or on a tap, once or again until the
  slide changes) and has the same Play, Pause and Stop. While a clip plays its
  frame has no handles and a change to it waits ("Stop the clip first");
  another slide, another choice, another tab, the sorter or Export stops it.
  Playing never changes the deck.
- **In Present** a clip set to play by itself starts when its slide shows, a
  tap on a clip plays or pauses it, and another slide, the show's end or Exit
  stops it (a build step on the same slide doesn't). A paused clip, and a
  sound while it plays, show a small note with the time.
- **How**: `deck.media {doc, slide, id}`, with the shape's own `id`, writes
  the clip's bytes into the storage (`.cache/media/<sha256>.<ext>`); a `Video`
  made over the clip's box reads that file as its storage source
  (`VideoDataSource.Filesystem`, which the runtime resolves inside the app's
  storage only). One clip plays at a time.
- **What it says**: "Preparing the clip…" while it gets ready; deck.media's
  refusals in plain words: `storage_full:` is "There isn't room in Quick
  Deck's storage to play this clip.", and `unsupported:` a clip linked to a
  file outside the deck, an empty one, or one that isn't MP4, MOV, M4V, MP3,
  M4A, AAC or WAV. When the widget can't play the file it says "Playback
  isn't available on this system" the first time in a session, as on a
  system with no player, and "This clip couldn't be played." once a clip has
  played here; the inspector adds the player's own words.

Where it plays depends on the runtime's backend for the system
(`.sources/makepad/platform/src/os/`), not on the system's name: on macOS,
AVFoundation; on a Linux desktop, GStreamer when its libraries and the
plugins for the clip are installed, and no player otherwise (OctoSense
registers no software decoder, so the widget reports "playback session
unavailable"); on Windows, Media Foundation (never run for Quick Deck).

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
| `.cache/media/` | The clips `deck.media` wrote for playing, named by a hash of their bytes: at most 32 MiB and 8 clips, used longest ago first; a deck's clips go when it closes |

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
- **The `Video` widget calls no script.** The app asks it four times a second
  what it is doing (`state()`, `error()`, `current_position_ms()`,
  `total_duration_ms()`, from `Video::script_call`) and moves the clip on with
  `clip_step`, a pure function the model tests cover. It makes the widget with
  `autoplay: false` and calls `begin_playback()` once the widget exists: one
  made with `autoplay: true` stayed `preparing` in this runtime. The widget
  is made hidden (video events still reach a hidden view) and shown once it
  plays, so the slide's poster shows until then.
- **The Mac's player never says a clip ended** (the Apple backend sends no
  `VideoPlaybackCompleted`): a clip that stays within 0.3 s of its end has
  ended. Drawing the canvas or the stage again makes the widget again; a
  playing clip then starts again where it was (`seek_to`).
- **A region inside another one is made empty with it**: the canvas draws the
  clip's control, and the stage its note, on the next turn.
- **`loop` is a keyword**, so the clip's own field is `looping`; the
  service's `loop` is read by name.
- **No widget is called `bar`** (or any name the shell looks its own widgets
  up by in its window): since #516 the shell's search for its bar found
  Quick Deck's toolbar first and panicked (#519). The toolbar region is
  `qd_toolbar`, and a model test keeps the names apart.

## Testing

The library's rules are pure functions in `main.splash`, tested from Rust by
evaluating the bundle's functions in a script VM
(`crates/shell/src/quickdeck_model_tests.rs`): an imported presentation's
title, what the library index keeps across a restart, what v1's decks
become, the storage, card, notes and timer words, the outline's counts and
problems (with the service's caps), what an outline and a model's draft send
to the engine, the slides a range names, Move earlier and later, what a
refusal says, the names the engine uses, the canvas's fit and zoom steps,
the selection's resize and rotation, the chart grid's numbers, and playback:
which shapes are clips (by the shape's own id), the clip a slide starts by
itself, deck.media's refusals and the widget's failures in words, and each
step of a clip from what the widget answers (a kick until it plays, its
picture shown once it does, pause, the end the Mac's player doesn't report,
a widget made again starting where it was).

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
| `playback` (`card-host`) | `make_fixture.py --media`'s Launch Film: the chosen clip's control, the sample clip playing over its box (two grabs a second apart must differ there, and the control's time move on), pause (the time holds), play on, stop, another slide stopping it, a sound's control, `unsupported:` and `storage_full:` in words, a clip the player can't open once a clip has played; Present: the clip that plays by itself (two grabs a second apart differ), a tap that pauses it and one that plays it on, the next slide; then a session where nothing has played, where that failure reads "Playback isn't available on this system"; light and dark |
| `playback` (shell) | A home of its own holding `tests/media`'s decks: `deck.media` writes each clip into the storage, checked against the clip's own bytes (`.cache/media/<sha256>.mp4`, and `.wav`), and a closed deck's clips leave the cache; the native deck's video in the editor and in Present (where it starts by itself), its sound, and the `.pptx` copy's video in Present (which starts only on a tap): on a Mac each video must play (two grabs a second apart differ; the sound is never played, since these runs open no audio device), on Linux each must say "Playback isn't available on this system"; light, then dark |

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
make the other runs' libraries, and `--media` adds Launch Film, a deck with
clips: a video set to play by itself (the sample clip `dev-fixture/media/clip.mp4`,
the repository's synthetic five-second H.264 clip with no sound, from
`crates/video-api-smoke/resources/`), a sound linked to a file outside the
deck, a video whose file isn't there and one too big for the storage; the
stand-in answers `deck.media` for each. `--bundle DIR --grabs-only` takes the same
frames of another bundle (main's, for a before and after) without checking
the states.

```sh
python3 apps/quickdeck/dev-fixture/make_fixture.py <card-host app data>/os.quickdeck
```

The shell's `playback` run uses `tests/media/launch-film.deckcraft` (slide 1:
the sample clip, set to play by itself; slide 2: a one-second tone, played on
a tap) and `launch-film.pptx` (the same deck saved as PowerPoint, so its
clips play on a tap only). The deck service holds the engine's
`insert.video`, `insert.audio` and `media.options` (#448), so
`tests/make_media_decks.py` makes them with the engine's own command line at
the pinned deckcraft (d0e57d7e):

```sh
cargo build --locked --release -p deckcraft-cli   # in a deckcraft checkout at d0e57d7e
python3 apps/quickdeck/tests/make_media_decks.py --cli <deckcraft-cli> --out apps/quickdeck/tests/media
```

## Status

See [tests/evidence/v2-20261011](tests/evidence/v2-20261011/README.md) for
v2's frames, side-by-sides and receipt, and
[tests/evidence/inspector-states-20261011](tests/evidence/inspector-states-20261011/README.md)
for the inspector's states (#507), and
[tests/evidence/playback-20261013](tests/evidence/playback-20261013/README.md)
for playback.

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
- **Verified on 13 Oct 2026, playback**: in `card-host` (macOS, Apple
  silicon; its runtime built from this checkout), the `playback` run, light
  and dark: the sample clip played over the clip's box (two grabs a second
  apart differ there, 13,850 pixels, and the control's time moved on), Pause
  held it, Play played on, Stop and another slide stopped it, the refusals
  read in words, and in Present the clip that plays by itself played (33,418
  pixels changed in a second), a tap paused it and another played it on;
  then a session where nothing had played read "Playback isn't available on
  this system" (on a Mac, for a clip AVFoundation can't open); `missing`,
  `fixture` and `states` passed in the same session. With the real engine
  (hidden shells built from this branch on main ffd5e488, which has
  `deck.media`), light then dark: on macOS the clips played (the editor's
  video, Present's that starts by itself, with taps that paused it and
  played it on, and the `.pptx` copy's on a tap); on Linux every clip said
  "Playback isn't available on this system" (the host's GStreamer has no MP4
  demuxer or decoder), and `shell` and `restart` passed unchanged. Either way
  `deck.media` wrote each clip as `.cache/media/<sha256>.<ext>` with the
  clip's own bytes, closing the deck removed them, and Quick Deck stayed open
  through every step. The model tests pass (35; 36 with `--features
  app-hub`).
- **Not verified**: a real import from the device (a hidden window on macOS
  has the dialog refused, and the Linux host has no dialog helper), so the
  import guard's refusals through the app; Draft with AI with a real provider
  (only the no-provider answer); a hidden shell on macOS; with the real
  engine, pictures, tables, the shape gallery, backgrounds, Present's notes
  strip with long notes and an images export (the dev fixture covers their
  screens); windows narrower than 1180 points; other screen sizes; phones.
  For playback: sound with audio out on any system (these runs open no audio
  device); Windows; a looping clip playing; a light/dark switch while a clip
  plays.
- **Known**: the shell's window is smaller than the designs' 1536 x 1024, so
  the theme panel, Export and the outline scroll. Theme previews draw the
  app's own font (OctoSense bundles no theme fonts). A box being edited
  shows plain lines without bullets. A built-in theme's own font pair is
  named after the theme, so the pair with the same families is marked (Harbor's
  fonts are Classic's); an imported deck's own colours or fonts are named in a
  hint. Present fills the app's window, not the screen. A `.pptx`'s clips
  play only on a tap: deckcraft at d0e57d7e reads no autoplay or loop from a
  PowerPoint file (SERVICE.md "As built in #515"), so only a native deck's
  clip starts by itself. A clip inside a group can't be chosen or played
  (`deck.slide` lists top-level shapes only). One clip plays at a time, and
  a slide with two clips set to play by itself starts the first. Clips can't
  be added or changed: `insert.video`, `insert.audio` and `media.options`
  stay held (#448). In a Mac session where nothing has played yet, a clip
  AVFoundation can't open reads "Playback isn't available on this system",
  because the runtime answers its failure with its software decoder's error.
  The shell closes Quick Deck at once on ⌘W or ⌘Q (both are Close window),
  the window's close button, or a right or middle click on the bar's window
  title, even in Present or with unsaved edits: a Splash app can't ask
  first, and Escape doesn't leave Present (script apps get no key events).
