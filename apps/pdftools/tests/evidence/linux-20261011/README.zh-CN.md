# PDF Tools 在 Linux 上的运行记录 — 2026-10-11

[English](README.md) | 简体中文

此前 PDF Tools 只在 macOS 上验证过。这一次，[ui.py](../../ui.py) 在 Linux 上运行了它的四个
Shell 流程，所用的是 main 5cab54ad，其中包含 #479（后续改进）和 #478（窗口提示）。这个分支
只修改了 ui.py，使它在 Linux 上也能启动 Shell。运行期间 main 前进了两次：先到 3632d376，新增
#485（OctoSense 全屏时程序坞自动隐藏）和 #494（更新 pdfcraft 的固定版本），再到 3a563057，
新增 #487（最大化和吸附窗口的底边位于程序坞之上）。每次前进后分支都变基到新的 main 上，各流程、
导入和窗口检查都再次通过；各帧来自第一次运行，即 5cab54ad 上的运行。
[receipt.json](receipt.json) 记录了二进制和源文件的摘要、所用命令、每次运行观察到的内容、
Linux 上的发现，以及未验证的部分。

**环境：**
- Linux 构建主机：Ubuntu 26.04 LTS，x86_64，72 个 CPU，没有 GPU，也没有桌面会话。
- main 的一个临时检出，用 `tools/setup.py` 针对主机上的源码中心仓库准备。
- 桌面 Shell，使用真实的 `pdf` 引擎（`cargo build --locked -p octosense
  --no-default-features --features app-hub,craft-engines`，debug 构建）。
- 在 Linux 上，ui.py 为每个会话在一个私有的运行时文件夹中启动一个独占的无头 weston
  （`weston --backend=headless --renderer=pixman --idle-time=0 --socket=… --no-config`）。
  - Shell 在它上面以独立会话运行，使用 Makepad 的 Wayland 后端，XDG 文件夹都放在本次运行的
    主目录中。
  - 由 Mesa 的 llvmpipe 绘制（软件 GL）。
  - 会话以 `/quit` 结束，Shell 退出后 weston 随之停止。
  - `MAKEPAD_HIDE_WINDOWS` 在 Linux 上不起作用，所以窗口会映射到这个 weston 上。
- 每个主目录都在 ext4 上（`--work`）：这台主机的 `/tmp` 是 tmpfs。
- 全程使用运行时 64 ms 的脚本预算。
- 每一帧都是从 `/g` 截图中按应用区域裁出的，截图为每点一个像素（`--grab-scale 1`；窗口的
  dpi 为 1）。

```sh
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <work>/octosense-shell.lock \
    --work <work>/homes --grab-scale 1 --output <work>/ui-64ms
TZ=Pacific/Kiritimati python3 apps/pdftools/tests/ui.py --shell target/debug/octosense \
    --lock <work>/octosense-shell.lock --work <work>/homes --grab-scale 1 --output <work>/ui-tz
python3 apps/pdftools/tests/evidence/linux-20261011/import/check.py \
    --shell target/debug/octosense --output <work>/import-check
```

**各次运行：**
- **使用主机自己的时区（UTC），** UTC 14:25 到 14:29，平均负载 18 到 35：`shell`（先浅色，
  再深色）、`restart`、`full` 和 `empty` 第一次运行就全部通过。
- **使用 `TZ=Pacific/Kiritimati`（UTC+14），** UTC 14:32 到 14:36，那里已经是 10 月 12 日：
  同样四个流程再次全部通过。
- **使用最终版本的 ui.py，在 UTC 时区，** UTC 14:54 到 14:58，当时主机上另一个会话的本地 CI
  正在运行：同样四个流程第三次全部通过。前两次运行和导入检查使用的是最后一次修改之前的
  ui.py，那次修改只是在远程桥始终不应答时停止 Shell 及其 weston。
- **先后变基到 main 3632d376 和 3a563057 之后，** UTC 15:02 到 15:06 以及 15:10 到 15:14：
  同样四个流程第四次和第五次全部通过，PDF Tools 以相同的尺寸打开，导入检查每次都再次通过。
- 所有日志中都没有脚本错误或超出预算的记录，每个 Shell 在 `/quit` 后都退出了。
- **[import/check.py](import/check.py)** 用一个替身对话框助手运行了“Open a PDF from this
  device”，见下文。
- pdf 服务的测试（`cargo test --locked -p octosense-pdf-service`）和 PDF Tools 的模型测试
  （`cargo test --locked -p octosense-shell --lib pdftools_model`，18 个）在这台主机上也都通过。

**各帧：**
- [light/](light)、[dark/](dark)、[restart/](restart)、[full/](full) 和 [empty/](empty)
  来自第一次运行。除了文件对话框之外，它们显示的内容与
  [v2-followups-20261010](../v2-followups-20261010/README.zh-CN.md) 中 macOS 的各帧相同。
  - **01-home、01b-open-from-device**：按钮旁显示“Not available here”，以及点按按钮后的横幅
    （见下文）。状态栏显示“187 KB of 64 MB used”，与 Mac 上相同。
  - **02c-read-mode、02d-read-mode-zoomed、02e-read-mode-left**：在第 2 页（共 4 页）进入
    阅读模式，然后翻到第 3 页、缩放到 125%，退出时仍在第 3 页、125%。
  - **03-find、03b-find-done**：从顶栏查找，“9 matches on 4 pages”。按 Done 后评论面板恢复。
  - **04-comment、04b-replied**：一处高亮及其回复，日期为“Today 14:25”，当时是 UTC 14:25。
  - **05-pages-rotated、06-combine、06b-combined、07-fill-sign、08a-colours、08b-edited、
    09-saved、10-damaged**：旋转、合并（按钮保持可见）、姓名缩写、设为居中和红色的段落、
    保存以及损坏的文件。
  - **dark/**：深色样式下的同样步骤。阅读模式用右上角的控件退出，段落设为右对齐。
  - **restart/**：保留的封面、PDF 离开时所在的页面、导入的文件，以及连同封面一起的移除。
  - **full/**：“63.9 of 64 MB used”，以及一次移除。
  - **empty/**：浅色和深色。
- [compare/](compare)：左边是 Linux，右边是 macOS，缩小一半：主页、一个页面、查找和编辑后的
  段落。
- [tz/](tz)：第二次运行中的高亮及其回复。**04b-replied** 显示“Today 04:32”，当时是 10 月
  11 日 UTC 14:32，也就是 UTC+14 的 10 月 12 日 04:32。
- [import/](import)：通过替身助手完成的导入。
- [window/opened-from-the-menu.png](window/opened-from-the-menu.png)：整个窗口，PDF Tools
  从 Shell 的菜单打开。

## Linux 上的不同之处

- **文件对话框。** makepad 在 Linux 上的文件对话框既不是 xdg-desktop-portal，也不是 GTK。
  它运行桌面自带的助手程序，即 `PATH` 上 zenity、qarma、matedialog 和 kdialog 中的第一个
  （KDE 下 kdialog 优先），并从它的输出中读取所选路径
  （`platform/src/os/linux/file_dialog.rs`）。
  - 这台构建主机上一个都没有，所以 Shell 不提供 `files.import`，`files.status` 回答
    `import_supported: false`。
  - PDF Tools 在“Open a PDF from this device”旁显示“Not available here”，点按按钮会显示横幅
    “This OctoSense can't open files from the device for PDF Tools.”。没有卡住，也没有记录
    任何错误。在 Linux 上，ui.py 现在检查的是这一点，而不是 macOS 上的拒绝提示。
  - 在 Mac 上，隐藏窗口永远不会获得焦点，所以对话框会被拒绝。在 Linux 上，窗口映射在本次
    运行的 weston 上，而文件服务把 macOS 以外的窗口视为有焦点，直到它报告失去焦点。所以只要
    `PATH` 上有助手程序，对话框就会真的打开。在这样的机器上，ui.py 会跳过这一步，并在它的
    run.json 中说明。
- **借助替身助手完成的真实导入。** [import/check.py](import/check.py) 把一个 zenity 的替身放在
  Shell 的 `PATH` 最前面。替身记录下调用它的参数，并用示例中野外指南的一个副本作答，副本名为
  `Lease 2026.pdf`。之后的一切都是发布的代码。
  - 助手程序被调用为 `zenity --file-selection --title=Import a file into this app`。
  - 16,477 个字节被逐字节复制到 `accounts/device/library/Imported PDF.pdf`。
  - `library.json` 为它记下标题“Lease 2026”、7 页和封面。
  - 它在一个标签中打开，标签为“Lease 2026.pdf”，位于“1 / 7”
    （[import.json](import/import.json)）。
  - 此前没有任何测试运行过真实的导入：在 Mac 上，隐藏窗口会让对话框被拒绝。真实助手程序
    自己的窗口没有运行。
  - 导入时不传任何文件过滤条件（文件服务只为照片添加过滤条件），所以助手程序会列出所有文件。
    这在每个平台上都一样。
- **字体与文字。** 这台主机只有 DejaVu（8 个字体文件），与 macOS 相比没有任何不同。Shell 的
  界面使用 Makepad 自带的字体绘制。引擎对示例中无衬线和衬线文字的渲染与 Mac 上一致：
  compare/ 中的文字、换行和布局都相同。没有测试中日韩文字。
- **软件 GL。** Shell 启动时，llvmpipe 编译着色器，使 UI 线程停顿：第一个事件耗时 3.1 到
  3.6 秒，draw-list 停顿 0.5 到 1.2 秒（显示第一页时还有一次）。除此之外，五次运行中记录到的
  停顿都不超过 409 ms，也没有任何脚本处理函数超出 64 ms。
- **窗口。** 在 weston 上，Shell 的窗口为 1400 x 900 点，dpi 为 1。
  - 启动时日志显示“wm: pdftools asks for a 1536x1024 window; it opens at 1296x698 (the dock
    covers 78 points of the desk)”。应用区域为 1292 x 662，即宽布局，与 macOS 上的隐藏 Shell
    相同。
  - 桌面绘制完成后从菜单打开时，窗口为 1296 x 709（应用区域 1292 x 673），底边位于程序坞之上。
    macOS 上的隐藏 Shell（1400 x 897 的窗口）得到的是 1296 x 703。
  - 有了 #485 之后，程序坞只在 OctoSense 全屏时隐藏（在 Wayland 上，最大化也算）。weston 上的
    Shell 窗口两者都不是，所以程序坞保持显示，PDF Tools 在 main 3632d376 和 3a563057（后者还包含
    #487）上都以相同的尺寸打开。
- **评论日期。** 在 UTC 时区，本地时间就是 UTC，所以单靠第一次运行无法区分两者。在 UTC+14
  下，日期正确地跨过了午夜：高亮显示“Today 04:32”，而不是 14:32，也不是“Yesterday”。
  - pdf 服务从 `localtime_r` 取得时差。
  - Shell 用同一个调用设置脚本中 `std.local_time` 的时差，所以两者一致。
- **ext4。** `pdf.save` 的暂存和重命名、渲染缓存的写入以及封面，都在 64 ms 预算下运行，没有超出
  预算，平均负载为 10 到 35。
  - 在 Mac 上，64 ms 的运行曾在 262 ms 和 555 ms 的重命名之后超出预算，当时平均负载为 54 到
    74，所以这不是同等条件下的比较。
  - 重启后，五个当前封面保持了纳秒级的修改时间不变，只为新导入的文件绘制了封面。
  - 移除操作删除了该 PDF 的封面。
  - 缓存按修改时间淘汰，而不是按目录顺序（ext4 和 APFS 的目录顺序不同）。

没有发现只在 Linux 上出现的问题，所以这个分支不包含修复。

## 未验证

- Windows：这里没有相应的机器。
- 真实助手程序自己的对话框（zenity、qarma、matedialog、kdialog）。主机上一个都没有安装，这次
  也没有安装任何软件；作答的是替身。makepad 没有 xdg-desktop-portal 路径。
- X11：weston 的 Xwayland 在这台主机上无法启动（没有 `/tmp/.X11-unix`）。另外还有 GNOME 或
  KDE 这样的桌面合成器、GPU 驱动，以及 Linux 上大于 1 的缩放比例。
- ui.py 在 card-host 中的运行（`missing`、`fixture`）在 Linux 上的情况，以及 release 构建。
- 在没有相应字体的主机上显示中日韩文字及其他非拉丁文字。
