# PDF Tools in card-host — 2026-10-09

English | [简体中文](README.zh-CN.md)

[ui.py](../../ui.py) drove the PDF Tools bundle in App Hub's `card-host`
(App Hub `95e4831a`, OctoSense's pin, built against this repository's
`.sources` runtime) with hidden windows on macOS, on the developer fixture
that `octosense-pdf-service`'s `pdftools_fixture` example records (see
[the app's README](../../../README.md#developer-fixture)). The answers were
the real pdf service's, recorded; `card-host` serves no host services, so no
call reached a live engine. [receipt.json](receipt.json) holds the binary,
bundle and source digests and the checks. Every frame here is an original
`/g?raw=1` grab, inspected one by one.

- [light/](light): the library, the "coming soon" note for opening from the
  device, the page grid, the page view and Next, the text and find, the info
  list (top and end), split every 2 pages and "where I choose", a split at
  page 2, merge (choose, order, done, the merged file), a damaged file, and
  the library and merged file after a restart on the same storage.
- [dark/](dark): the same journey in the dark appearance, splitting every 2
  pages instead.
- [missing/](missing): the four samples without the fixture, so the host
  refuses `pdf.*` (`this app was not granted "pdf", which "pdf.info" needs`):
  the library, a document and the Merge button's explanation.
- [empty/](empty): no PDFs in storage.

**Not verified**: the live engine through this app in a shell (it needs the
`pdf` capability in App Hub and the per-caller engine areas), the desktop
shell's window sizes and `on_app_resize` columns, Linux, Windows, phones.
