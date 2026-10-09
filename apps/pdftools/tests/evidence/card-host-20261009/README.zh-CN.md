# PDF Tools 在 card-host 中 — 2026-10-09

[English](README.md) | 简体中文

[ui.py](../../ui.py) 在 macOS 上以隐藏窗口驱动 App Hub `card-host` 中的 PDF Tools
应用包（App Hub `95e4831a`，即 OctoSense 固定的版本，基于本仓库 `.sources` 中的
运行时构建），使用 `octosense-pdf-service` 的 `pdftools_fixture` 示例录制的开发者夹具
（见[应用的 README](../../../README.zh-CN.md#开发者夹具)）。回答来自真实 pdf 服务的
录制；`card-host` 不提供任何宿主服务，因此没有调用到达实时引擎。
[receipt.json](receipt.json) 记录了二进制、应用包和源文件的摘要以及各项检查。
这里每一帧都是原始的 `/g?raw=1` 截图，已逐张检查。

- [light/](light)：资料库、“从设备打开即将推出”的提示、页面网格、单页视图和
  Next、文字与查找、信息列表（顶部和末尾）、每 2 页拆分和“Where I choose”、在
  第 2 页处拆分、合并（选择、排序、完成、合并后的文件）、损坏的文件，以及在同一存储
  上重启后的资料库和合并后的文件。
- [dark/](dark)：深色外观下的同一流程，改为每 2 页拆分。
- [missing/](missing)：只有四个示例文件、没有夹具，因此宿主拒绝 `pdf.*`
  （`this app was not granted "pdf", which "pdf.info" needs`）：资料库、一个文档，
  以及 Merge 按钮给出的说明。
- [empty/](empty)：存储中没有 PDF。

**未验证**：在 Shell 中通过本应用使用实时引擎（需要 App Hub 支持 `pdf` 能力，并需要
按调用方划分的引擎区域）、桌面 Shell 的窗口尺寸和 `on_app_resize` 列数、Linux、
Windows 和手机。
