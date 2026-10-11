# PDF Tools v2 后续改进的运行记录 — 2026-10-10

[English](README.md) | 简体中文

[ui.py](../../ui.py) 在这个分支上端到端运行了这些后续改进（macOS，Apple 芯片）。
分支已变基到包含 #478 的 main。[receipt.json](receipt.json) 记录了二进制、包和源文件的
摘要、所用命令、每次运行检查的内容、观察到的数值，以及未验证的部分。

**隐藏的桌面 Shell，使用真实的 `pdf` 引擎**（`cargo build --locked -p octosense
--no-default-features --features app-hub,craft-engines`，debug 构建）：
- 每次运行使用全新的 `OCTOSENSE_HOME`，其中放有 octosense-pdf-service 的
  `pdftools_fixture` 示例写出的示例 PDF。
- Shell 从 Terminal.app 标签页启动，带 `MAKEPAD_HIDE_WINDOWS=1`、
  `MAKEPAD_REMOTE=127.0.0.1:8915` 和 `MAKEPAD_WM_TEST_APP=pdftools`。运行时持有本机的
  Shell 运行锁，同一时间只有一个实例，并以 `/quit` 结束。
- 所有运行都使用运行时的 64 ms 脚本预算，每次引擎调用都是真实的。
- PDF Tools 按自身的窗口尺寸打开（#478）：应用区域为 1292 x 662 点，即宽版布局。
- 每一帧都是从原始 `/g` 截图中按应用区域裁出的，每点一个像素。
- 运行时间在加州下午 5 点之后，此时 UTC 已是第二天。

- [light/](light)：
  - **01-home**：每个 PDF 的封面来自 `pdf.cover`。状态栏从 `files.status` 读出
    “187 KB of 64 MB used”，当时存储中共有 191,793 字节；仅 PDF 是 38,356 字节，旧的
    状态栏只统计这一部分。
  - **04-comment、04b-replied**：一处高亮及其回复，日期显示为“Today 20:21”，当时 UTC
    是 10 月 11 日 03:21。旧的读法会显示“11 Oct”。
  - **06-combine、06a-combine-end、06b-combined**：合并中列出两个 PDF。文件列表在卡片
    内滚动；“One PDF of 5 pages”、Name 和 Combine 按钮不用滚动就一直可见，按钮位于应用
    662 点高度中的 613 到 659 点之间。此前它在可见区域下方 85 点。然后是合并得到的 PDF。
  - **08-edit、08a-colours、08b-edited**：租约第 1 条，Alignment 选居中，Colour 选红色；
    Colour 框在自己这一行里展开为七种颜色。按 Apply 后，引擎把这一段画成居中、红色。
  - **09-saved**。
- [dark/](dark)：切换到 Shell 自己的深色样式后，在浅色一轮留下的状态上重复同样的步骤。
  编辑这一步把段落设为右对齐，并保留它原有的颜色。
- [restart/](restart)：重启后的同一个主目录。所有当前封面都被保留，文件和时间都相同，
  没有重新绘制；只为两次运行之间放入资料库的 `Imported PDF.pdf` 新画了一个封面。
- [full/](full)：存储填到离 64 MiB 只差 8 KB。
  - 状态栏为“63.9 of 64 MB used”，进度条已满并显示为红色。
  - 没有空间存封面，但每张卡片仍显示页数和大小。
  - 移除一个 PDF 时会再问一次，然后移除。
- [empty/](empty)：没有 PDF，浅色和深色。

**App Hub 的 `card-host`**（不提供宿主服务），按设计稿的 1536 x 1024
（[fixture/](fixture)）。它运行 ui.py 的 `fixture`：包的临时副本使用替身引擎和设计稿中的
示例文档，先浅色后深色。因为替身引擎在脚本中作答，所以使用 `--budget-ms 1000`。

本机的 card-host（App Hub 8347a489）早于清单的 `window` 提示，会拒绝未知的清单字段，
因此临时副本的清单去掉了 `window` 和 `schema_minor`；`main.splash` 与分支中的相同。

对比图左边是设计稿、右边是应用，缩小一半并减为 256 色：
- **06-combine**：在这个高度下，卡片按设计稿 06 完整显示。
- **08-edit**：Edit 面板中的 Font、Alignment 和 Colour，与设计稿 08 一致。

变基之前，同样的 `shell`、`restart`、`full` 和 `empty` 运行在默认窗口（应用区域
990 x 603）中也都通过了（帧未保留）。在那里，合并卡片会占满窗格并让页脚一直可见，
Edit 面板也会收紧间距，让 Apply 保持可见。

每一帧都已检查过。
