# PDF Tools at its own window size — 2026-10-10

English | [简体中文](README.zh-CN.md)

PDF Tools' manifest asks the desktop to open its window at the designs'
1536 x 1024 points (`"window"`, with `"schema_minor": 1`). The shell clamps
that to the desk, less the margins every new window keeps. OctoSense's dock
floats over the desk instead of reserving a strip, so the bottom margin is
kept above the dock. A hidden desktop shell built from this branch showed
it, with the real `pdf` engine (macOS, Apple silicon;
[receipt.json](receipt.json) holds the binary's digest, the commands, the
numbers and the findings). Each run had a fresh `OCTOSENSE_HOME` and started
from a Terminal.app tab with `MAKEPAD_HIDE_WINDOWS=1` and
`MAKEPAD_REMOTE=127.0.0.1:8915`, one instance on the machine. Each was
driven over the remote bridge and ended with `/quit`. Frames are `/g` grabs
at one pixel per point.

- [opened-from-the-menu.png](opened-from-the-menu.png): the whole hidden
  window, after the shell's menu, "pdf tools", Return. The 1400 x 897 shell
  has a 1380 x 845 work area whose bottom 78 points the dock covers. The
  window opens at 1296 x 703 points, 32 points above the dock, and the app
  gets 1292 x 667. Before this change the app got the default window, an app
  area of 990 x 603 ([v2-20261010](../v2-20261010/README.md)).
- [wide/](wide): [ui.py](../../ui.py)'s `shell` run, unchanged, light and
  then dark, with PDF Tools opened at startup (`MAKEPAD_WM_TEST_APP`). The
  window was 1296 x 698, sized on the shell's startup proportions as every
  window opened before the desk's first draw is, and the app had 1292 x 662.
  That is its wide layout (1180 points and up), where a right panel opens
  beside the left panel. Every step of both passes passed at the runtime's 64 ms script
  budget, with no script errors. The frames are the app's area: Home,
  Reading, Find, Comment with a highlight, Combine (before and after one
  scroll), Edit, and Reading in the dark pass. In every frame the status line
  ("Saved" or "Edited", and the storage used) and the zoom bar sit above the
  dock.

What the run showed:

- **Combine:** with two PDFs listed, the card's own Combine button is 85
  points below the fold at this size. [06-combine.png](wide/06-combine.png)
  ends at "One PDF of 5 pages". One scroll shows the name and the button
  ([06a-combine-end.png](wide/06a-combine-end.png)).
- **Home:** the cards' third caption line (when each PDF was last opened)
  sits at the fold, with 2 of its 18 points showing.
- Both come from height. Keeping the window above the dock costs 78 points on
  this 898-point-tall window. Before that change the app area was 740 points
  tall: Home's captions fitted and the Combine button was about 7 points
  short.
- A comment made on 10 Oct (PDT) is dated 11 Oct: the known UTC reading in
  the [README](../../../README.md#status).
