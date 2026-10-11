# PDF Tools 的窗口尺寸 — 2026-10-10

[English](README.md) | 简体中文

PDF Tools 的清单请求桌面按设计图的 1536 x 1024 点打开它的窗口（`"window"`，并写明
`"schema_minor": 1`）。Shell 会把这个尺寸限制在桌面以内，并减去每个新窗口都保留的边距。用这个
分支构建的隐藏桌面 Shell 验证了这一点（macOS，Apple 芯片；[receipt.json](receipt.json) 记录了
二进制文件的摘要、命令和数值）。每次运行都使用全新的 `OCTOSENSE_HOME`，从 Terminal.app 标签页以
`MAKEPAD_HIDE_WINDOWS=1` 和 `MAKEPAD_REMOTE=127.0.0.1:8915` 启动，机器上同时只运行一个实例。
每次运行都通过远程桥驱动，并以 `/quit` 结束。

- [opened-from-the-menu.png](opened-from-the-menu.png)：桌面绘制完成后，打开 Shell 的菜单，
  输入“pdf tools”，按 Return。1400 x 899 的 Shell 的平铺区域为 1380 x 847。窗口以
  1296 x 783 点打开，左右各留 42 点、上下各留 32 点，应用得到 1292 x 747。此前它得到的是
  默认窗口，应用区域为 990 x 603（[v2-20261010](../v2-20261010/README.zh-CN.md)）。OctoSense
  风格的程序坞覆盖在桌面之上，而不是预留一条区域，因此窗口最下面的部分位于程序坞后面。
- 启动时打开（`MAKEPAD_WM_TEST_APP=pdftools`，与 `tests/ui.py` 的打开方式相同），没有保留
  截图：1296 x 776 点。Shell 在桌面首次绘制之前打开的每个窗口，都按启动时的比例确定尺寸。
