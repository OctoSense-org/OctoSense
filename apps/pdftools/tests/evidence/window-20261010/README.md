# PDF Tools' window size — 2026-10-10

English | [简体中文](README.zh-CN.md)

PDF Tools' manifest asks the desktop to open its window at the designs'
1536 x 1024 points (`"window"`, with `"schema_minor": 1`). The shell clamps
that to the desk, less the margins every new window keeps. A hidden
desktop shell built from this branch showed it (macOS, Apple silicon;
[receipt.json](receipt.json) holds the binary's digest, the commands and the
numbers). Each run had a fresh `OCTOSENSE_HOME` and started from a
Terminal.app tab with `MAKEPAD_HIDE_WINDOWS=1` and
`MAKEPAD_REMOTE=127.0.0.1:8915`, one instance on the machine. Each was
driven over the remote bridge and ended with `/quit`.

- [opened-from-the-menu.png](opened-from-the-menu.png): the shell's menu,
  "pdf tools", Return, once the desk had drawn. The 1400 x 899 shell has a
  1380 x 847 tile area. The window opens at 1296 x 783 points, 42 points in
  on the left and right and 32 at the top and bottom, and the app gets
  1292 x 747. Before this change it got the default window, an app area of
  990 x 603 ([v2-20261010](../v2-20261010/README.md)). The OctoSense style's
  dock overlays the desk instead of reserving a strip, so the window's lowest
  points lie behind it.
- Opened at startup (`MAKEPAD_WM_TEST_APP=pdftools`, as `tests/ui.py` opens
  it), no frame kept: 1296 x 776 points. The shell sizes every window it
  opens before the desk's first draw on its startup proportions.
