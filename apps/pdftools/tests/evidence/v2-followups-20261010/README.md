# PDF Tools v2 follow-ups — 2026-10-11

English | [简体中文](README.zh-CN.md)

[ui.py](../../ui.py) ran the follow-ups end to end on this branch (macOS,
Apple silicon), on main 07b63dac, which has #478. Afterwards the branch was
rebased onto main bcd52cf4. That brought in #484 alone: WASI sockets, with
App Hub's pin moved from d86101d7 to c317f787. The runs were not repeated on
it. [receipt.json](receipt.json) holds the binaries', bundle's and sources'
digests, the commands, what each run checked, the numbers it saw and what was
not verified. The folder is named for the first round, on 10 Oct; this
round's frames replaced that round's.

**The hidden desktop shell, with the real `pdf` engine** (`cargo build
--locked -p octosense --no-default-features --features app-hub,craft-engines`,
debug):
- Each run had a fresh `OCTOSENSE_HOME` with the sample PDFs that
  octosense-pdf-service's `pdftools_fixture` example writes.
- The shell started from a Terminal.app tab with `MAKEPAD_HIDE_WINDOWS=1`,
  `MAKEPAD_REMOTE=127.0.0.1:8915` and `MAKEPAD_WM_TEST_APP=pdftools`, under
  the machine's shell-run lock, one instance at a time, and ended with
  `/quit`.
- PDF Tools opened at its own window size (#478): an app area of 1292 x 662
  points, the wide layout. Every engine call was real.
- Each frame is the app's area cut from an original `/g` grab, one pixel per
  point.
- The runs were just after midnight on 11 Oct in California, 7 hours behind
  UTC.
- They ran with `--budget-ms 1000`, not the runtime's 64 ms. See
  [The 64 ms budget](#the-64-ms-budget) below.

The frames:
- [light/](light):
  - **01-home:** each PDF's cover from `pdf.cover`. The status line reads
    "187 KB of 64 MB used" from `files.status`; the storage held 191,793
    bytes. The PDFs alone are 38,356 bytes, which is all the old line
    counted.
  - **02c-read-mode, 02d-read-mode-zoomed, 02e-read-mode-left:** Read mode,
    from the floating bar's last button, on page 2 of 4.
    - The tabs, mode bar, rail, panels and status line are gone.
    - The bar keeps the page arrows, "2 / 4", zoom, fit width, fit page and
      "Exit read mode". An exit control sits in the top right corner.
    - Then page 3 at 125%. Exit read mode leaves on page 3 at 125%.
  - **03-find, 03b-find-done:** with the Comments panel open, "revenue" in
    the top bar's search. "9 matches on 4 pages" fills the left panel with
    no rail item lit; the rail has no Search of its own. Done gives the
    Comments panel back.
  - **04-comment, 04b-replied:** a highlight and its reply, dated "Today
    00:34" at 07:34 UTC. The old reading took the engine's UTC time for
    local time and showed 07:34.
  - **06-combine, 06a-combine-end, 06b-combined:** two PDFs in Combine. The
    file list scrolls inside the card. "One PDF of 5 pages", Name and the
    Combine button stay in view with no scroll: the button sits at 613 to
    659 of the app's 662 points. Before, it was 85 points below the fold.
    Then the combined PDF.
  - **08-edit, 08a-colours, 08b-edited:** clause 1 of the lease with
    Alignment centred and Colour Red. The Colour box opens into a row of
    seven chips: the paragraph's own colour and six others. After Apply, the
    engine draws the paragraph centred and red.
  - **09-saved.**
- [dark/](dark): the same after the shell's own switch to its dark style, on
  what the light pass left. So the report's tab is still Edited with the
  light pass's highlight, and Combine names its PDF "Quarterly report
  combined 2".
  - **02c-read-mode, 02e-read-mode-left:** Read mode, left this time by the
    corner control, on page 3 at 125%.
  - **03-find:** before the search, the rail's Pages folded the left side.
    The results' own cross then ended the search and folded it again
    (checked; no frame).
  - **08-edit, 08b-edited:** the paragraph set flush right, keeping its own
    colour.
- [restart/](restart): the same home after a restart.
  - **r01-home, r02-imported:** the covers kept from the shell run were not
    drawn again: the same files, with the same times. One new cover was
    drawn, for `Imported PDF.pdf`, placed in the library between the runs.
  - **r03-place-kept:** the report reopens on page 2, where the shell run
    left it.
  - **r04-removed:** removing "Quarterly report combined.pdf" deleted its
    kept cover too. The card beside the message is the dark pass's
    "Quarterly report combined 2.pdf".
- [full/](full): the storage filled to 8 KB short of its 64 MiB.
  - The status line reads "63.9 of 64 MB used", with the bar full and red.
  - There is no room for covers, but each card still shows its pages and
    size.
  - Removing a PDF asks once more, then removes it.
- [empty/](empty): no PDFs, light and dark.

**App Hub's `card-host`**, which serves no host services, at the designs'
1536 x 1024 ([fixture/](fixture)). It ran ui.py's `fixture` run: a scratch
copy of the bundle with the stand-in engine and the designs' sample
documents, light then dark. It used `--budget-ms 1000` because the stand-in
engine answers in script.

The card-host on this machine (App Hub 8347a489) predates the manifest's
`window` hint and refuses unknown manifest keys. So the scratch copy's
manifest dropped `window` and `schema_minor`; `main.splash` is the branch's
own.

The sheets, design on the left and app on the right, are halved and reduced
to 256 colours:
- **02-reading:** the floating bar ends with Read mode's button.
- **03-find, d03-find:** the find screen from the top bar alone, light and
  dark. Design 03 still draws a Search item in the rail. The app has
  dropped it, so no rail item is lit.
- **06-combine:** the card fits as design 06 draws it at this height.
- **08-edit, d08-edit:** the Edit panel with Font, Alignment and Colour, as
  design 08 draws it.

Every frame was inspected.

## The 64 ms budget

After the go, the shell run first ran at the runtime's 64 ms script budget.

- The first attempt stopped at a bug in ui.py: the new rail check also
  counted the tab title and "Saved". The app was right, and ui.py was
  fixed.
- The next two attempts passed every functional step of the shell journey,
  light and dark. That covers Read mode, find from the top bar, a highlight
  and its reply, Pages, Combine, Fill & Sign, Edit with Alignment and
  Colour, save and the damaged file: 41 frames, through d10-damaged.
- Each then failed only ui.py's log check, on script budget overruns. So
  `restart`, `full` and `empty` did not run:
  - **The second attempt** logged two "script time budget exceeded" around
    a 555 ms stall of the UI thread in `pdf.save`: a file rename, then the
    removal of its staging folder. The third attempt overwrote its log.
    [run-64ms/attempt-2-excerpt.txt](run-64ms/attempt-2-excerpt.txt) holds
    the lines read from it before then.
  - **The third attempt** logged one, after a 262 ms stall in the render
    cache's file rename (`pdf.page`, `cache::store`). Its logs are
    [run-64ms/shell.log](run-64ms/shell.log) and
    [run-64ms/ui.log](run-64ms/ui.log), with machine-local paths replaced by
    placeholders.
- App Hub runs a host service inline, on the UI thread (#399). So a slow
  file rename counts against the calling script's 64 ms of wall-clock time.
  This pull request does not change those paths: `pdf.save`'s commit and
  the render cache's write.
- The one-minute load average was 54.8 to 56.9 just after the second
  attempt and 74.1 just after the third. Six cargo builds and tests of other
  sessions ran at once, the malware scanner took 130 to 200% CPU, and the
  media analysis daemon took 150%. Earlier on 10 Oct, at load 15 to 35, this
  folder's first round passed all four runs at 64 ms.
- So the four runs above used `--budget-ms 1000`, as #473's fixture run once
  did. The first of them stopped in the dark pass: a tap on Combine's Add
  files was lost while the card was drawn again. ui.py now taps again while
  the list has not shown, and the next run passed all four.
