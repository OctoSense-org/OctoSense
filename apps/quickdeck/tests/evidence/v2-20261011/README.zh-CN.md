# Quick Deck 第 2 版运行记录 — 2026-10-11

[English](README.md) | 简体中文

[ui.py](../../ui.py) 端到端运行了 Quick Deck 第 2 版。[receipt.json](receipt.json) 记录了二进制文件、bundle 和源码的摘要、所用命令、每次运行检查的内容、之前失败的各轮次以及未验证的内容。

**App Hub 的 `card-host`**（macOS 26.6.2，Apple 芯片），不提供任何宿主服务，使用运行时默认的 64 ms 脚本预算（这个 card-host 的运行时没有 `MAKEPAD_SPLASH_BUDGET_MS`，所以 `--budget-ms` 不起作用）：

- [fixture/](fixture)：bundle 的临时副本，换上引擎替身（`dev-fixture/engine.splash`，按 #503 的 deck 服务那样应答）和设计图中的示例演示文稿（`dev-fixture/make_fixture.py`），按设计图的 1536 x 1024：每个设计过的页面与其设计图并排（左边是设计图，右边是应用），先浅色，再深色（文件名以 `d` 开头；只有 10 有深色设计图，所以其他深色对比图是深色应用与浅色设计图并排）。
- [states/](states)：损坏的演示文稿、受保护的、被暂缓的和不支持的演示文稿、未保存更改提示条、存储已满，以及首次运行时空的演示文稿库，浅色与深色。
- [missing/](missing)：原样发布的 bundle：没有 deck 引擎也没有文件服务，覆盖 Home、演示文稿标签和大纲页面，浅色与深色。

对比图和截图都缩小一半并减少到 256 色。

**隐藏运行、使用真实 `deck` 引擎的桌面 Shell**（Ubuntu 26.04 LTS，x86_64，72 个 CPU，没有 GPU）：Shell（`cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines`）由变基到 main 202d7cc8 的本分支构建，main 已包含 deck 服务（#503，由 a1a0b496 合并，含导入防护的最终规则）。ui.py 为这次运行准备了全新的 `OCTOSENSE_HOME` 和专用的无头 weston（pixman，从不空闲），与 PDF Tools 在 Linux 上的运行相同，并用 `/quit` 结束 Shell。每一次引擎调用都是真实的。Shell 以 1296 x 698 点打开 Quick Deck（程序坞占去 1400 x 900 桌面中的 78 点），因此应用区域是 1292 x 662，而不是设计图的 1536 x 1024。每张截图都是从一像素对应一点的原始 `/g` 截图中裁出的应用窗口。

- [linux/](linux)：首次运行时的 Home（引擎的八个主题；"Open a presentation from this device" 旁显示 "Not available here"，因为这台主机没有文件对话框助手）；没有设置提供方时的 Draft with AI（"Set up an AI provider in Settings to draft outlines (Settings → AI providers). You can still type the outline."，来自 Shell 的 `model.complete`）；由输入的大纲生成的四张幻灯片的演示文稿（`deck.create`）；第 2 张幻灯片的正文被原地编辑（加了 "Questions"），然后加粗；第 4 张幻灯片插入的图表及其数据表格；Ember 主题；执行 Duplicate、Hide 并新建分节后的浏览视图；保存；.pptx 和 PDF 导出；放映。然后通过 Shell 自己的样式菜单切到深色，在浅色流程留下的状态上再走一遍（文件名以 `d` 开头）。
- [linux-restart/](linux-restart)：重启后同一个主目录：演示文稿库中的演示文稿，带封面和最后一次保存写下的幻灯片张数，以及再次打开的演示文稿。

此前的各轮运行使用本分支与服务分支（74be4c07）的本地合并；它们发现的问题和所做的修复记录在回执中。

每张截图都经过检查。
