# PDF Tools 第 2 版的运行记录 — 2026-10-10

[English](README.md) | 简体中文

[ui.py](../../ui.py) 在这个分支上端到端地运行了 PDF Tools 第 2 版（macOS，Apple
芯片）。[receipt.json](receipt.json) 记录了二进制文件、应用包和源文件的摘要、所用命令、
每次运行检查的内容以及未验证的内容。Shell 中的运行和 `missing` 使用运行时 64 ms 的脚本
预算；`fixture` 使用 `--budget-ms 1000`，因为它的替身引擎在脚本中作答。

**隐藏的桌面 Shell，使用真实的 `pdf` 引擎**（`cargo build --locked -p octosense
--no-default-features --features app-hub,craft-engines`，debug 构建）：每次运行都使用
一个新的 `OCTOSENSE_HOME`，其中放有 octosense-pdf-service 的 `pdftools_fixture` 示例写入
的示例 PDF；Shell 从 Terminal.app 的标签页以 `MAKEPAD_HIDE_WINDOWS=1`、
`MAKEPAD_REMOTE=127.0.0.1:8915` 和 `MAKEPAD_WM_TEST_APP=pdftools` 启动，持有本机的
Shell 运行锁，同一时间只运行一个实例，并以 `/quit` 结束。每一次引擎调用都是真实的。每一帧
都是从原始 `/g` 截图中裁出的应用窗口，每点一个像素：Shell 给 PDF Tools 的是它的默认窗口，
990 x 603 点。

- [light/](light)：主页及引擎渲染的首页；“Open a PDF from this device”被拒绝（隐藏窗口
  永远不在最前面）；打开报告（第 1 页，再通过缩略图到第 2 页）；查找（“9 matches on 4
  pages”，第一处匹配在页面上高亮）；在 Comment 模式下高亮 Summary 的第一行，然后回复；
  在 Pages 中把第 2 页向右旋转，然后撤销；Combine 加入 Board minutes 的第 1 页（“One PDF
  of 5 pages”），随后合并出的 PDF 打开；在租约上用 Fill & Sign 放置姓名缩写（它没有表单
  字段）；Edit 中把第 1 条放进编辑框并输入“ (amended)”，然后应用；保存；损坏的文件。
- [dark/](dark)：通过 Shell 自己的样式切换到深色后的同一流程，在浅色一轮留下的状态上进行
  （同样的标签、它保存的名字和姓名缩写、它的编辑）。
- [restart/](restart)：重启后的同一个主目录：资料库和各次运行写入的内容、在两次运行之间
  按 `files.import` 留下文件的方式放入资料库的 `Imported PDF.pdf` 被打开，以及报告回到
  离开时所在的页面。
- [full/](full)：存储被填到距 64 MiB 只差 8 KB：主页提示存储已满；移除一个 PDF 时会再
  确认一次，然后将其移除。
- [empty/](empty)：没有 PDF，浅色和深色：空资料库及其 Open 按钮，然后是拒绝提示。
- [compare/](compare)：Shell 截图与其设计图等高并排（设计图为 1536 x 1024；Shell 的窗口
  为 990 x 603，因此布局是 README.md 中描述的窄窗口布局）。

**App Hub 的 `card-host`**，它不提供任何宿主服务：

- [missing/](missing)：发布的应用包：没有引擎，也没有文件服务。
- [fixture/](fixture)：应用包的一个临时副本，使用替身引擎（`dev-fixture/engine.splash`）
  和取自设计图的示例文档（`dev-fixture/make_fixture.py`），按设计图的 1536 x 1024 显示：
  每个设计过的界面都与其设计图并排（左为设计图，右为应用），先浅色，再深色（文件名以 `d`
  开头；只有 09 有深色设计图），另有损坏、受保护和未保存三种状态。并排图缩小一半，并减为
  256 色。

每一帧都经过检查。
