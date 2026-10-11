# PDF Tools v2 follow-ups — 2026-10-10

English | [简体中文](README.zh-CN.md)

[ui.py](../../ui.py) ran the follow-ups end to end on this branch, rebased on
main with #478 (macOS, Apple silicon). [receipt.json](receipt.json) holds the
binaries', bundle's and sources' digests, the commands, what each run checked,
the numbers it saw and what was not verified.

**The hidden desktop shell, with the real `pdf` engine** (`cargo build
--locked -p octosense --no-default-features --features app-hub,craft-engines`,
debug). Each run had a fresh `OCTOSENSE_HOME` with the sample PDFs that
octosense-pdf-service's `pdftools_fixture` example writes. The shell started
from a Terminal.app tab with `MAKEPAD_HIDE_WINDOWS=1`,
`MAKEPAD_REMOTE=127.0.0.1:8915` and `MAKEPAD_WM_TEST_APP=pdftools`, under the
machine's shell-run lock, one instance at a time, and ended with `/quit`.
Every run used the runtime's 64 ms script budget. PDF Tools opened at its own
window size (#478): an app area of 1292 x 662 points, the wide layout. Every
engine call was real. Each frame is the app's area cut from an original `/g`
grab, one pixel per point. The runs were after 17:00 in California, when UTC
is already the next day.

- [light/](light):
  - **01-home:** each PDF's cover from `pdf.cover`. The status line reads
    "187 KB of 64 MB used" from `files.status`; the storage held 191,793
    bytes. The PDFs alone are 38,356 bytes, which is all the old line
    counted.
  - **04-comment, 04b-replied:** a highlight and its reply, dated "Today
    20:21" at 03:21 UTC on 11 Oct. The old reading said "11 Oct".
  - **06-combine, 06a-combine-end, 06b-combined:** two PDFs in Combine. The
    file list scrolls inside the card, and "One PDF of 5 pages", Name and the
    Combine button stay in view with no scroll: the button sits at 613 to 659
    of the app's 662 points. Before, it was 85 points below the fold. Then
    the combined PDF.
  - **08-edit, 08a-colours, 08b-edited:** clause 1 of the lease with
    Alignment centred and Colour Red; the Colour box opens into its seven
    colours in its own row. After Apply, the engine draws the paragraph
    centred and red.
  - **09-saved.**
- [dark/](dark): the same after the shell's own switch to its dark style, on
  what the light pass left. The Edit step sets the paragraph flush right and
  keeps its own colour.
- [restart/](restart): the same home after a restart. Every current cover
  was kept, the same files with the same times, none drawn again. One new
  cover was drawn, for `Imported PDF.pdf`, placed in the library between the
  runs.
- [full/](full): the storage filled to 8 KB short of its 64 MiB.
  - The status line reads "63.9 of 64 MB used", with the bar full and red.
  - There is no room for covers, but each card still shows its pages and
    size.
  - Removing a PDF asks once more, then removes it.
- [empty/](empty): no PDFs, light and dark.

**App Hub's `card-host`**, which serves no host services, at the designs'
1536 x 1024 ([fixture/](fixture)). It ran ui.py's `fixture` run: a scratch
copy of the bundle with the stand-in engine and the designs' sample
documents, light then dark, at `--budget-ms 1000` because the stand-in engine
answers in script.

The card-host on this machine (App Hub 8347a489) predates the manifest's
`window` hint and refuses unknown manifest keys. So the scratch copy's
manifest dropped `window` and `schema_minor`; `main.splash` is the branch's
own.

The sheets, design on the left and app on the right, are halved and reduced
to 256 colours:
- **06-combine:** the card fits as design 06 draws it at this height.
- **08-edit:** the Edit panel with Font, Alignment and Colour, as design 08
  draws it.

Before the rebase, the same `shell`, `restart`, `full` and `empty` runs
passed in the default window, an app area of 990 x 603 (frames not kept).
There, Combine's card fills the pane with its footer in view, and the Edit
panel draws its gaps tighter so Apply stays in view.

Every frame was inspected.
