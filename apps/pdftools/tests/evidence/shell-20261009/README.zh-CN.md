# PDF Tools 在桌面 Shell 中 — 2026-10-09

[English](README.md) | 简体中文

[ui.py](../../ui.py) 在由本分支构建的隐藏 OctoSense 桌面 Shell 中端到端运行了
PDF Tools（`cargo build --locked -p octosense`，debug 构建，macOS，Apple
silicon），真实的 `pdf` 引擎在应用自己的存储中工作。每次运行都使用一个新的
`OCTOSENSE_HOME`，其中放有 octosense-pdf-service 的 `pdftools_fixture` 示例写出
的示例 PDF；Shell 在本机的 Shell 运行锁之下，从 Terminal.app 标签页以
`MAKEPAD_HIDE_WINDOWS=1`、`MAKEPAD_REMOTE=127.0.0.1:8915` 和
`MAKEPAD_WM_TEST_APP=pdftools` 启动，并以 `/quit` 结束。
[receipt.json](receipt.json) 记录了二进制、应用包和源文件的摘要、命令以及各项检查。

每一帧都是从 Shell 的原始 `/g` 截图中裁出的应用窗口，再用 macOS 的 `sips` 缩小
一半；`missing/` 中的帧是 App Hub `card-host` 的原始 `/g` 截图。每一帧都已检查。

- [light/](light)：带引擎生成首页图的资料库；因为隐藏窗口从不在前台，
  “Open a PDF from this device”被拒绝；页面网格、单页视图和 Next、文字和查找、
  信息列表（顶部和末尾）、每 2 页拆分和“Where I choose”、在第 2 页处拆分、
  损坏的文件不能加入合并、合并（选择、排序、完成、合并后的文件）、移除一个 PDF
  （确认，然后回到资料库），以及损坏的文件。
- [dark/](dark)：通过 Shell 自己的深色样式切换后的同一流程，改为每 2 页拆分
  （会替换浅色运行生成的一个文件）。这里页面网格保持两列：切换会重新运行应用的
  脚本，而在窗口尺寸改变之前 Shell 不会再次调用 `on_app_resize`。
- [restart/](restart)：重启后同一个 home。合并和拆分写入的文件仍在；在两次运行
  之间按 `files.import` 留下文件的方式放入资料库的 `Imported PDF.pdf` 被读取、
  绘制并打开（页面、文字、单页）。
- [full/](full)：存储被填到距 64 MiB 上限只差 8 KB：引擎拒绝写入首页图和一次
  合并，应用说明存储已满；然后移除一个 PDF。
- [empty/](empty)：没有 PDF，浅色和深色：空资料库及其 Open 按钮，然后是拒绝。
- [missing/](missing)：App Hub 的 `card-host`，它不提供任何宿主服务：没有引擎
  （`no service answers "pdf" on this device`），也没有文件服务。

**未验证**：宿主的文件对话框以及一次真实导入之后的流程（隐藏窗口无法打开它：
文件服务会先拒绝）、导入的其他拒绝情况（存储已满、文件太大、正在忙）、Shell
默认尺寸以外的窗口尺寸、Linux、Windows 和手机。
