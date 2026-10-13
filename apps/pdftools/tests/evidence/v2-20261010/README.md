# PDF Tools v2 runs — 2026-10-10

English | [简体中文](README.zh-CN.md)

[ui.py](../../ui.py) ran PDF Tools v2 end to end on this branch (macOS,
Apple silicon). [receipt.json](receipt.json) holds the binaries', bundle's
and sources' digests, the commands, what each run checked and what was not
verified. The shell runs and `missing` ran at the runtime's 64 ms script
budget; `fixture` ran with `--budget-ms 1000`, because its stand-in engine
answers in script.

**The hidden desktop shell, with the real `pdf` engine** (`cargo build
--locked -p octosense --no-default-features --features app-hub,craft-engines`,
debug): each run had a fresh `OCTOSENSE_HOME` with the sample PDFs that
octosense-pdf-service's `pdftools_fixture` example writes; the shell started
from a Terminal.app tab with `MAKEPAD_HIDE_WINDOWS=1`,
`MAKEPAD_REMOTE=127.0.0.1:8915` and `MAKEPAD_WM_TEST_APP=pdftools`, under the
machine's shell-run lock, one instance at a time, and ended with `/quit`.
Every engine call was real. Each frame is the app's window cut from an
original `/g` grab, one pixel per point: the shell gave PDF Tools its default
window, 990 x 603 points.

- [light/](light): Home with the engine's first pages; "Open a PDF from this
  device" refused (a hidden window is never in front); the report open
  (page 1, then page 2 from its thumbnail); find ("9 matches on 4 pages",
  the first match highlighted on its page); a highlight on the Summary's
  first line in Comment mode, then its reply; Pages with page 2 turned right,
  then Undo; Combine with Board minutes' page 1 ("One PDF of 5 pages"), then
  the combined PDF open; initials placed by Fill & Sign on the lease (it has
  no form fields); Edit with clause 1 in the editor and " (amended)" typed,
  then applied; Save; the damaged file.
- [dark/](dark): the same journey after the shell's own switch to its dark
  style, on what the light pass left (the same tabs, the name and initials
  it kept, its edits).
- [restart/](restart): the same home after a restart: the library and what
  the runs wrote, `Imported PDF.pdf` (placed in the library between the runs
  as `files.import` leaves a file) opened, and the report back at the page
  it was left at.
- [full/](full): storage filled to 8 KB short of its 64 MiB: Home says
  storage is full; removing a PDF asks once more, then removes it.
- [empty/](empty): no PDFs, light and dark: the empty library and its Open
  button, then the refusal.
- [compare/](compare): shell frames beside their designs, the same height
  (the designs are 1536 x 1024; the shell's window is 990 x 603, so the
  layout is the narrow one README.md describes).

**App Hub's `card-host`**, which serves no host services:

- [missing/](missing): the shipped bundle: no engine, no files service.
- [fixture/](fixture): a scratch copy of the bundle with the stand-in engine
  (`dev-fixture/engine.splash`) and the designs' sample documents
  (`dev-fixture/make_fixture.py`), at the designs' 1536 x 1024: every
  designed screen beside its design (design left, app right), light, then
  dark (`d`-prefixed; only 09 has a dark design), and the damaged,
  protected and unsaved states. Sheets are halved and reduced to 256
  colours.

Every frame was inspected.
