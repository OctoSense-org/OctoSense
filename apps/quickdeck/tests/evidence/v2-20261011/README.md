# Quick Deck v2 runs — 2026-10-11

English | [简体中文](README.zh-CN.md)

[ui.py](../../ui.py) ran Quick Deck v2 end to end. [receipt.json](receipt.json)
holds the binaries', the bundle's and the sources' digests, the commands,
what each run checked, the rounds that failed before these and what was not
verified.

**App Hub's `card-host`** (macOS 26.6.2, Apple silicon), which serves no host
services, at the runtime's 64 ms script budget (this card-host's runtime has
no `MAKEPAD_SPLASH_BUDGET_MS`, so `--budget-ms` would change nothing):

- [fixture/](fixture): a scratch copy of the bundle with the stand-in engine
  (`dev-fixture/engine.splash`, which answers as the deck service of #503
  does) and the designs' sample decks (`dev-fixture/make_fixture.py`), at
  the designs' 1536 x 1024: every designed screen beside its design (design
  left, app right), light, then dark (`d`-prefixed; only 10 has a dark
  design, so the other dark sheets set a dark app beside a light design).
- [states/](states): a damaged deck, a protected, a held and an unsupported
  presentation, the unsaved-changes bar, storage full, and the empty library
  of a first run, light and dark.
- [missing/](missing): the shipped bundle as it is: no deck engine and no
  files service, on Home, in a deck's tab and on the outline page, light and
  dark.

Sheets and frames are halved and reduced to 256 colours.

**A hidden desktop shell with the real `deck` engine** (Ubuntu 26.04 LTS,
x86_64, 72 CPUs, no GPU): the shell (`cargo build --locked -p octosense
--no-default-features --features app-hub,craft-engines`) was built from this
branch rebased on main 202d7cc8, which has the deck service (#503, merged
from a1a0b496, with the import guard's final rules). ui.py gave the run a
fresh `OCTOSENSE_HOME` and a headless weston of its own (pixman, never idle),
as PDF Tools' Linux runs do, and ended the shell with `/quit`. Every engine
call was real. The shell opened Quick Deck at 1296 x 698 points (the dock
covers 78 points of its 1400 x 900 desk), so the app area is 1292 x 662, not
the designs' 1536 x 1024. Each frame is the app's window cut from an original
`/g` grab at one pixel per point.

- [linux/](linux): Home on a first run (the engine's eight themes; "Not
  available here" beside "Open a presentation from this device", because
  this host has no file dialog helper); Draft with AI with no provider set
  up ("Set up an AI provider in Settings to draft outlines (Settings → AI
  providers). You can still type the outline.", from the shell's
  `model.complete`); a four-slide deck made from the typed outline
  (`deck.create`); slide 2's body edited in place ("Questions" added) and
  then made bold; a chart inserted on slide 4 with its data grid; the Ember
  theme; the sorter after Duplicate, Hide and a new section; Save; the .pptx
  and PDF exports; Present. Then the same through the shell's own style
  menu in dark (`d`-prefixed), on what the light pass left.
- [linux-restart/](linux-restart): the same home after a restart: the deck in
  the library with its cover and the slide count its last save wrote, and
  the deck opened again.

The rounds before these ran on a local merge of this branch with the
service's branch at 74be4c07; what they found and what was fixed is in the
receipt.

Every frame was inspected.
