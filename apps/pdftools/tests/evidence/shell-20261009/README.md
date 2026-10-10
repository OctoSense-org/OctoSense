# PDF Tools in the desktop shell — 2026-10-09

English | [简体中文](README.zh-CN.md)

[ui.py](../../ui.py) ran PDF Tools end to end in a hidden OctoSense desktop
shell built from this branch (`cargo build --locked -p octosense`, debug, on
macOS, Apple silicon), with the real `pdf` engine working in the app's own
storage. Each run had a fresh `OCTOSENSE_HOME` with the sample PDFs that
octosense-pdf-service's `pdftools_fixture` example writes; the shell started
from a Terminal.app tab with `MAKEPAD_HIDE_WINDOWS=1`,
`MAKEPAD_REMOTE=127.0.0.1:8915` and `MAKEPAD_WM_TEST_APP=pdftools`, under the
machine's shell-run lock, and ended with `/quit`. [receipt.json](receipt.json)
holds the binary, bundle and source digests, the commands and the checks.

Each frame is the app's window cut from an original `/g` grab of the shell
and halved with macOS `sips`; the `missing/` frames are original `/g` grabs
of App Hub's `card-host`. Every frame was inspected.

- [light/](light): the library with the engine's first pages, "Open a PDF
  from this device" refused because a hidden window is never in front, the
  page grid, the page view and Next, the text and find, the info list (top
  and end), split every 2 pages and "where I choose", a split at page 2, a
  damaged file refused from a merge, merge (choose, order, done, the merged
  file), removing a PDF (the question, then the library), and the damaged
  file.
- [dark/](dark): the same journey after the shell's own switch to its dark
  style, splitting every 2 pages instead (which replaces a part the light run
  made). The app keeps two columns of pages there: the switch re-runs the
  app's script and the shell does not call `on_app_resize` again until the
  window's size changes.
- [restart/](restart): the same home after a restart. What merge and split
  wrote is still there, and `Imported PDF.pdf`, placed in the library between
  the runs as `files.import` leaves a file, was read, drawn and opened
  (pages, text, a page).
- [full/](full): storage filled to 8 KB short of its 64 MiB: the engine
  refuses to write the first pages and a merge, and the app says storage is
  full; then a PDF is removed.
- [empty/](empty): no PDFs, in light and dark: the empty library and its
  Open button, then the refusal.
- [missing/](missing): App Hub's `card-host`, which serves no host
  services: no engine (`no service answers "pdf" on this device`) and no
  files service.

**Not verified**: the host's file dialog and what follows a real import (a
hidden window cannot open it: the files service refuses first), the import's
other refusals (storage full, too large, busy), window sizes other than the
shell's default, Linux, Windows, phones.
