# PDF Tools' Read mode in full screen — 2026-10-13

English | [简体中文](README.zh-CN.md)

Read mode's tap now also asks the shell for `window.fullscreen {on: true}`
(#520), and leaving Read mode gives the window back. When the shell ends the
full screen (Esc, ⌘F, another window taking the focus), Read mode ends too.
A desktop shell built from this branch showed it with the real `pdf` engine:
`tests/ui.py`'s `shell` run, light then dark, on the Linux build host.

- **Build:** `cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines`,
  from #520 at `18c11642` with this change's `main.splash` and `tests/ui.py`
  (their SHA-1s and the binary's SHA-256 are in [receipt.json](receipt.json)).
- **Host:** Ubuntu 26.04, x86_64, a private headless weston 14.0.2 (pixman
  renderer, never idle) with Makepad's Wayland backend. The shell's window
  is 1400 x 900 points.
- **Run:** `python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --work <dir> --grab-scale 1 --only shell --output <dir>`.
  Every step passed at the runtime's 64 ms script budget. The shell's log
  has no panic and no script error.

A hidden shell (`MAKEPAD_HIDE_WINDOWS`) never asks the host window to go full
screen, by design (#520), so "full screen" here is the app's window covering
the shell's whole window, with no bar, title bar or dock. Whether the
OctoSense window then enters macOS's full screen on a real display is
**unverified**.

Frames are `/g` grabs of the whole window at one pixel per point. Each has a
`.json` with the app's area and the window's size.

| Frame | What it shows |
| --- | --- |
| [02c-read-mode.png](02c-read-mode.png) | The pill's full-screen button on page 2 of 4: Read mode, and the window covers all of the 1400 x 900 (it was 1292 x 662 at 54, 76), with no bar, title bar or dock |
| [02e-read-mode-left.png](02e-read-mode-left.png) | A page turned to 3 / 4 and the zoom raised from 100% to 125% in Read mode, then Exit read mode: the window is back at 54, 76, 1292 x 662 with the bar and the dock, on 3 / 4 at 125% |
| [02g-read-mode-escape.png](02g-read-mode-escape.png) | Read mode again (the window covering all of it again), then Esc, the shell's key: the shell ends the full screen, the app hears `on_app_fullscreen(false)` and leaves Read mode on 3 / 4 at 100%, and the window is back |
| [d02c-read-mode.png](d02c-read-mode.png) | The dark pass: the same Read mode in full screen. The corner control left it, with the window back |

The shell's log has three presentations. The first was ended by the app's
`{on: false}` after Exit read mode, the second by Esc (with no request after
it), and the third by the app's `{on: false}` after the corner control.
