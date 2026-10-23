# PDF Tools v2 后续改进的运行记录 — 2026-10-11

[English](README.md) | 简体中文

[ui.py](../../ui.py) 在这个分支上端到端运行了这些后续改进（macOS，Apple 芯片），当时分支
位于包含 #478 的 main 07b63dac 之上。之后分支变基到 main bcd52cf4，这只带来了 #484：WASI
套接字，同时 App Hub 的固定版本从 d86101d7 移到 c317f787。变基后没有重新运行。
[receipt.json](receipt.json) 记录了二进制、包和源文件的摘要、所用命令、每次运行检查的内容、
观察到的数值，以及未验证的部分。这个文件夹以 10 月 10 日的第一轮命名；这一轮的帧替换了
那一轮的帧。

**隐藏的桌面 Shell，使用真实的 `pdf` 引擎**（`cargo build --locked -p octosense
--no-default-features --features app-hub,craft-engines`，debug 构建）：
- 每次运行使用全新的 `OCTOSENSE_HOME`，其中放有 octosense-pdf-service 的
  `pdftools_fixture` 示例写出的示例 PDF。
- Shell 从 Terminal.app 标签页启动，带 `MAKEPAD_HIDE_WINDOWS=1`、
  `MAKEPAD_REMOTE=127.0.0.1:8915` 和 `MAKEPAD_WM_TEST_APP=pdftools`。运行时持有本机的
  Shell 运行锁，同一时间只有一个实例，并以 `/quit` 结束。
- PDF Tools 按自身的窗口尺寸打开（#478）：应用区域为 1292 x 662 点，即宽版布局。每次引擎
  调用都是真实的。
- 每一帧都是从原始 `/g` 截图中按应用区域裁出的，每点一个像素。
- 运行时间是加州 10 月 11 日午夜刚过，比 UTC 晚 7 小时。
- 这些运行使用 `--budget-ms 1000`，而不是运行时的 64 ms，见下文
  [64 ms 预算](#64-ms-预算)。

各帧：
- [light/](light)：
  - **01-home**：每个 PDF 的封面来自 `pdf.cover`。状态栏从 `files.status` 读出
    “187 KB of 64 MB used”，当时存储中共有 191,793 字节；仅 PDF 是 38,356 字节，旧的
    状态栏只统计这一部分。
  - **02c-read-mode、02d-read-mode-zoomed、02e-read-mode-left**：从浮动工具条的最后一个
    按钮进入阅读模式，位于第 2 页（共 4 页）。
    - 标签页、模式栏、侧栏、面板和状态栏都隐藏了。
    - 工具条保留翻页箭头、“2 / 4”、缩放、适合宽度、适合页面和“Exit read mode”。右上角
      另有一个退出控件。
    - 然后翻到第 3 页并缩放到 125%。按 Exit read mode 退出后，仍在第 3 页、125%。
  - **03-find、03b-find-done**：在评论面板打开时，在顶栏的搜索框中输入“revenue”。左侧
    面板显示“9 matches on 4 pages”，侧栏没有任何一项高亮；侧栏本身也没有 Search。按 Done
    后，评论面板恢复。
  - **04-comment、04b-replied**：一处高亮及其回复，日期显示为“Today 00:34”，当时 UTC 是
    07:34。旧的读法把引擎的 UTC 时间当作本地时间，会显示 07:34。
  - **06-combine、06a-combine-end、06b-combined**：合并中列出两个 PDF。文件列表在卡片
    内滚动；“One PDF of 5 pages”、Name 和 Combine 按钮不用滚动就一直可见，按钮位于应用
    662 点高度中的 613 到 659 点之间。此前它在可见区域下方 85 点。然后是合并得到的 PDF。
  - **08-edit、08a-colours、08b-edited**：租约第 1 条，Alignment 选居中，Colour 选红色。
    Colour 框在自己这一行里展开为七个色块：段落原有的颜色和另外六种颜色。按 Apply 后，引擎把
    这一段画成居中、红色。
  - **09-saved**。
- [dark/](dark)：切换到 Shell 自己的深色样式后，在浅色一轮留下的状态上重复同样的步骤。
  因此报告的标签页仍因浅色一轮的高亮而显示为已编辑，合并得到的 PDF 名为
  “Quarterly report combined 2”。
  - **02c-read-mode、02e-read-mode-left**：这次用角落的退出控件离开阅读模式，仍在第 3 页、
    125%。
  - **03-find**：搜索之前，用侧栏的 Pages 收起了左侧；之后用结果面板自己的关闭按钮结束
    搜索，左侧再次收起（已检查，没有截图）。
  - **08-edit、08b-edited**：段落设为右对齐，保留它原有的颜色。
- [restart/](restart)：重启后的同一个主目录。
  - **r01-home、r02-imported**：Shell 运行时保留的封面没有重新绘制，文件和时间都相同；只为
    两次运行之间放入资料库的 `Imported PDF.pdf` 新画了一个封面。
  - **r03-place-kept**：报告在第 2 页重新打开，正是 Shell 运行结束时的位置。
  - **r04-removed**：移除“Quarterly report combined.pdf”时，它保留的封面也一并删除。提示
    旁边的卡片是深色一轮生成的“Quarterly report combined 2.pdf”。
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
- **02-reading**：浮动工具条以阅读模式的按钮结尾。
- **03-find、d03-find**：只从顶栏发起的搜索界面，浅色和深色。设计稿 03 的侧栏中仍画有
  Search 一项；应用已去掉它，所以侧栏没有任何一项高亮。
- **06-combine**：在这个高度下，卡片按设计稿 06 完整显示。
- **08-edit、d08-edit**：Edit 面板中的 Font、Alignment 和 Colour，与设计稿 08 一致。

每一帧都已检查过。

## 64 ms 预算

收到开始运行的指示后，Shell 运行先按运行时的 64 ms 脚本预算进行。

- 第一次尝试停在 ui.py 的一个错误上：新的侧栏检查把标签页标题和“Saved”也算了进去。
  应用是对的，ui.py 已修正。
- 接下来两次尝试都通过了 Shell 流程中的每个功能步骤，浅色和深色都是。包括阅读模式、
  从顶栏搜索、一处高亮及其回复、页面、合并、填写与签名、带 Alignment 和 Colour 的编辑、
  保存以及损坏的文件：共 41 帧，直到 d10-damaged。
- 两次都只在 ui.py 的日志检查中失败，原因是脚本超出预算，因此 `restart`、`full` 和
  `empty` 没有运行：
  - **第二次尝试**记录了两条“script time budget exceeded”，分别位于 `pdf.save` 中 UI 线程
    停顿 555 ms 的报告之前和之后：停顿来自一次文件重命名，以及随后删除其暂存文件夹。第三次
    尝试覆盖了它的日志；[run-64ms/attempt-2-excerpt.txt](run-64ms/attempt-2-excerpt.txt)
    保存了在此之前从中读到的几行。
  - **第三次尝试**记录了一条，出现在渲染缓存的文件重命名（`pdf.page`、`cache::store`）
    造成的 262 ms 停顿之后。它的日志是 [run-64ms/shell.log](run-64ms/shell.log) 和
    [run-64ms/ui.log](run-64ms/ui.log)，本机路径已换成占位符。
- App Hub 在 UI 线程上直接运行宿主服务（#399），所以一次缓慢的文件重命名会计入调用脚本
  64 ms 的实际耗时。这个拉取请求没有改动这些路径：`pdf.save` 的提交和渲染缓存的写入。
- 第二次尝试刚结束时，一分钟平均负载为 54.8 到 56.9，第三次刚结束时为 74.1。当时其他会话
  的六个 cargo 构建和测试同时在运行，恶意软件扫描程序占用 130% 到 200% 的 CPU，媒体分析
  守护进程占用 150%。10 月 10 日早些时候，在 15 到 35 的负载下，这个文件夹的第一轮在
  64 ms 下通过了全部四项运行。
- 因此上面的四项运行使用了 `--budget-ms 1000`，#473 的 fixture 运行也曾这样做。其中第一次
  在深色一轮中停下：卡片重新绘制时，一次点按合并的 Add files 丢失了。ui.py 现在会在列表
  出现之前再次点按，下一次运行通过了全部四项。
