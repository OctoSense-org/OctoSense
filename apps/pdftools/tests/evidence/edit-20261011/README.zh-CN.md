# PDF Tools 在“网页浏览器打印出来的页面”上的 Edit — 2026-10-11

[English](README.md) | 简体中文

在 Apply 之前，Edit 面板会说明一次编辑除了改字之外还会带来什么变化（`pdf.edit_check`），
并用直白的话拒绝引擎做不到的编辑。用本分支 `c50104da` 构建的桌面 Shell 和真实的 `pdf`
引擎，在 `pdftools_fixture --edit-sample` 写出的页面上展示了这一点。这一页上有一个被网页
浏览器以无名 Type 3 字体打印成五块的标题，以及使用嵌入式子集字体的中文文字。

这次 Shell 运行在 Linux 构建主机上，而不是在 Mac 上，因为当时另一个会话正在 Mac 上运行
OctoSense。

- **构建：** `cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines`。
- **显示：** 一个私有的无头 weston 14.0.2（pixman 渲染器，永不空闲），使用 Makepad 的
  Wayland 后端。
- **主目录：** 一个全新的 `OCTOSENSE_HOME`，PDF Tools 的资料库里只有这份示例，并设置
  `MAKEPAD_REMOTE` 和 `MAKEPAD_WM_TEST_APP=pdftools`。
- **运行：** 只有一个实例，通过远程桥驱动，最后以 `/quit` 结束。驱动程序使用了 #495 中
  [ui.py](../../ui.py) 的 Linux 启动方式（它的 `Shell` 和 `click_page`），目前还不在仓库中；
  步骤见下文。

画面是每点一像素的 `/g` 截图，裁剪为应用区域（1292 x 662，即 Shell 为 PDF Tools 打开的
默认窗口）。每一步都在运行时 64 ms 的脚本时间预算内通过，日志中没有脚本错误。
[receipt.json](receipt.json) 记录了提交、二进制文件的摘要以及每一步的结果。

| 画面 | 内容 |
| --- | --- |
| [e01-piece.png](e01-piece.png) | Edit，点击标题的第一块（“Studio2026”）：Paragraph 2，Font 显示“Unnamed font · 27.8 pt”，以及“The PDF stores this line in 5 pieces: only this one changes.”（PDF 把这一行存成 5 块：只有这一块会改变。）原文保持原字体，所以按钮显示 Apply |
| [e02-warned.png](e02-warned.png) | 输入“ Beta”：输入停顿后显示“Set in Helvetica: this paragraph's font can't show all of this text.”（改用 Helvetica 排版：这个段落的字体无法显示全部文字。）和“Longer than its box: it runs into 2 lines of text nearby.”（超出原来的框：会压到附近 2 行文字。）按钮显示“Apply anyway”，并且在字段滚动时固定在卡片底部，始终可见 |
| [e03-refused.png](e03-refused.png) | 在“季”这一块中输入“夏”（字体中没有这个字），以红色拒绝：“PDF Tools can't edit Chinese, Japanese or Korean text yet.”（PDF Tools 目前还不能编辑中文、日文或韩文文字。） |
| [e04-apply-refused.png](e04-apply-refused.png) | 此时按 Apply 只会再次显示这个原因：“Couldn't change the text: PDF Tools can't edit Chinese, Japanese or Korean text yet.” 文档没有任何改变（状态栏仍显示 Saved） |
| [de02-warned.png](de02-warned.png)、[de03-refused.png](de03-refused.png) | Shell 切换到 OctoSense 深色模式后的同样两步 |
| [e05-applied-anyway.png](e05-applied-anyway.png) | 对“Studio2026 Beta”按 Apply anyway：这是引擎自己的结果，用 Helvetica 排版并压在“春”和“季”上，与面板的说明一致 |
| [e06-applied-at-once.png](e06-applied-at-once.png) | 一次检查没有任何需要说明的编辑：给下方的 Helvetica 段落多加一句话，它下面没有其他内容，于是按 Apply 后立即生效 |

这次运行发现的问题，以及为此做的改动：

- **Apply 不在视野内：** 第一次运行时，在这个窗口尺寸下，说明文字把 Apply 和 Cancel 挤出了
  卡片。从 `c50104da` 起，带说明的卡片会占满面板的高度，其中的字段可以滚动，Apply 和
  Cancel 固定在卡片底部。说明文字也更短了。
- **编辑框只显示一行：** 页面上的编辑框在点击段落时按段落原文的大小确定，输入时不会变大，
  所以较长的草稿只显示最后一行（e02 中的“Beta”）。这个问题在本次改动之前就存在，留作后续
  处理。
