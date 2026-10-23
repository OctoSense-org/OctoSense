# PDF Tools 阅读模式全屏 — 2026-10-13

[English](README.md) | 简体中文

进入阅读模式的那次点按现在还会请 Shell 执行 `window.fullscreen {on: true}`（#520），离开阅读模式时
窗口恢复原位。Shell 结束全屏时（Esc、⌘F、其他窗口获得焦点），阅读模式也随之结束。用这个分支构建的
桌面 Shell 在 Linux 构建主机上以真实的 `pdf` 引擎演示了这一点：`tests/ui.py` 的 `shell` 运行，先浅色
再深色。

- **构建：**`cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines`，
  基于 #520 的 `18c11642`，加上这次改动的 `main.splash` 和 `tests/ui.py`（它们的 SHA-1 以及二进制文件
  的 SHA-256 见 [receipt.json](receipt.json)）。
- **主机：**Ubuntu 26.04，x86_64，私有的无头 weston 14.0.2（pixman 渲染器，从不空闲），使用 Makepad
  的 Wayland 后端。Shell 窗口为 1400 x 900 点。
- **运行：**`python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --work <dir> --grab-scale 1 --only shell --output <dir>`。
  每一步都在运行时默认的 64 ms 脚本预算内通过。Shell 日志中没有 panic，也没有脚本错误。

隐藏窗口的 Shell（`MAKEPAD_HIDE_WINDOWS`）按设计从不让宿主窗口进入全屏（#520），所以这里的“全屏”是
应用窗口铺满 Shell 的整个窗口，没有栏、标题栏或程序坞。OctoSense 窗口随后是否会在真实显示器上进入
macOS 全屏**尚未验证**。

画面是整个窗口的 `/g` 截取，每点一个像素。每张都附有一个 `.json`，记录应用区域和窗口尺寸。

| 画面 | 显示内容 |
| --- | --- |
| [02c-read-mode.png](02c-read-mode.png) | 在第 2 / 4 页按浮动控制条上的全屏按钮：进入阅读模式，窗口铺满整个 1400 x 900（之前在 54, 76，大小 1292 x 662），没有栏、标题栏或程序坞 |
| [02e-read-mode-left.png](02e-read-mode-left.png) | 在阅读模式中翻到 3 / 4 页、把缩放从 100% 调到 125%，然后按 Exit read mode：窗口回到 54, 76，大小 1292 x 662，栏和程序坞也回来了，仍在 3 / 4 页、125% |
| [02g-read-mode-escape.png](02g-read-mode-escape.png) | 再次进入阅读模式（窗口再次铺满），然后按 Esc（这是 Shell 的按键）：Shell 结束全屏，应用收到 `on_app_fullscreen(false)` 并在 3 / 4 页、100% 处离开阅读模式，窗口恢复原位 |
| [d02c-read-mode.png](d02c-read-mode.png) | 深色这一轮：同样的阅读模式全屏。之后用右上角按钮离开，窗口恢复原位 |

Shell 日志中有三次演示全屏。第一次由 Exit read mode 之后应用发出的 `{on: false}` 结束，第二次由 Esc
结束（之后没有再发出请求），第三次由右上角按钮之后应用发出的 `{on: false}` 结束。
