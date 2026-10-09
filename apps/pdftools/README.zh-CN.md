# PDF Tools

[English](README.md) | 简体中文

PDF Tools（`os.pdftools`）是 OctoSense 的 PDF 应用：管理它自己存储中的 PDF，
提供带页面缩略图的查看器、合并、拆分和文字提取，并可从设备打开 PDF。实际工作由
Shell 的 `pdf` 引擎服务（pdfcraft，[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.zh-CN.md)）
在应用自己的存储中完成；应用本身是 `bundle/` 中一个隔离运行的 Splash 程序，从不
自己读取 PDF。它只随桌面 Shell 发布，因为十个 craft 引擎只在桌面版中提供
（`craft-engines`）。

它申请 `storage`、`files` 和 `pdf`，不声明 `storage.max_bytes`，因此其存储上限是
App Hub 的系统上限 64 MiB。

## 界面

| 界面 | 显示内容 | 唯一的主要操作 |
| --- | --- | --- |
| 资料库 | `accounts/device/library/` 中的 PDF：首页、名称、页数和大小；损坏的文件会说明无法打开；“Open a PDF from this device”及可添加的最大文件；文件无法添加或存储已满时显示一张卡片 | **Merge PDFs**（资料库为空时：**Open a PDF from this device**） |
| 文档 › Pages | 页面缩略图，每次 24 页；点按一页进入单页视图 | **Split into files** |
| 文档 › Text | 按阅读顺序显示每页文字，可用 Find in the text 查找 | – |
| 文档 › Info | 文件、页数、页面尺寸、文件大小、标题、作者、主题、关键词、创建程序、生成程序、PDF 版本、安全性、字体、书签 | – |
| 文档 › Remove | 再确认一次：PDF 会从应用的存储中删除；由它生成的 PDF 保留 | **Remove** / Keep it |
| 单页视图 | 放大显示一页；Previous、Next 或滑动翻页 | – |
| 合并 | 三步：Choose（带序号的选择，2 到 16 个文件）、Order（上移或下移、移除、命名）、Done（结果，可 Open） | Next / **Merge** / Open |
| 拆分 | Every few pages（步进器）或 Where I choose（点按每个新文件起始的页面）；实时列出将生成的文件，以及会替换哪些已有文件；然后 Done | **Split into N files** |

合并和拆分得到的 PDF 保留在应用的存储中：这个版本不提供导出。

## 从设备打开 PDF

“Open a PDF from this device”请 Shell 的文件服务把用户在宿主自己的对话框中选择的
一个文件复制到资料库中。应用要先为新文件命名，因为 `files.import` 在对话框打开前
就接收目标路径，并且从不告诉应用所选文件的名称或位置：依次为 `Imported PDF.pdf`、
`Imported PDF 2.pdf` 等（目标已存在时导入会被拒绝）。复制进来的文件随后像这里的其他
PDF 一样打开：引擎在同一个相对路径上执行 `info`、`render` 和 `text`。存储隔离区不
支持重命名，所以导入的 PDF 保留这个名字。

`files.status`（只问一次，不打开任何东西）给出一次导入可接受的最大文件：桌面上是
64 MiB，资料库会显示这个值；剩余存储空间同样会限制导入。导入被拒绝时，资料库顶部
会显示一张卡片，先用平实的话说明原因，下面附上宿主自己的消息：应用不在前台、存储
已满、文件太多、文件太大、另一个传输正在进行，或者没有文件服务。

## 工作方式

下文所有路径都相对于应用的存储，即 `fs.*` 看到的目录。引擎也在同一目录中工作
（ADR 0013“Engines work in the caller's own folder”）：应用打开时发出的调用可以
替换文件，引擎写入的内容计入应用的存储。

| 路径 | 内容 |
| --- | --- |
| `accounts/device/library/*.pdf` | PDF 文件（ADR 0004 §11：系统应用的数据位于其 `device` 账户目录），包括导入的文件以及合并和拆分写入的文件 |
| `accounts/device/library.json` | 引擎对每个文件给出的信息：页数和大小 |
| `cache/covers/<name>.png` | 每个文件的首页（可清除） |
| `cache/pages/<name>/p<n>.png` | 当前打开文档的缩略图；打开一个文档时会删除其他文档的缩略图，因为应用存储最多容纳 256 个条目 |
| `cache/view.png` | 单页视图的图片 |

| 调用 | 何时调用 | 参数 |
| --- | --- | --- |
| `pdf.info` | 首次列出某个文件、打开文档时 | `{path}` |
| `pdf.render` | 首页图、缩略图（`max_side` 360）和单页视图（`max_side` 1400） | `{path, page, out, max_side}` |
| `pdf.text` | Text 标签页 | `{path}`，或前 512 页的 `pages` |
| `pdf.merge` | 合并 | `{paths, out}` |
| `pdf.split` | 拆分 | `{path, out_dir, every}` 或 `{path, out_dir, before}` |
| `files.status` | 应用启动时 | `{}` |
| `files.import` | Open a PDF from this device | `{path: "accounts/device/library/Imported PDF.pdf"}` |

应用从不读取 `pdf.info` 的 `document.path`：在 #434 之前，它是引擎在宿主上的绝对
路径，而应用使用的每个路径都是它自己的相对路径。

当引擎没有空间写入时（`… more than the N bytes left in this storage`），资料库、
页面以及合并和拆分界面会说明 PDF Tools 的存储已满，Merge 或 Split 也会说明失败
原因。移除一个 PDF 会释放它占用的空间，并让应用重新尝试生成页面图片。

修改脚本时请注意：

- 调用逐个进行：服务在 UI 线程上运行（#399）。
- 拒绝会在 `host.request` 内部直接回调。在随 #413 而来的运行时补丁之前，在这样的
  回调里调用 UI 会丢失回调的其余部分；`engine()` 和文件调用总是把结果交给下一轮
  处理（`start_timeout`）。
- 页面图片用 `fs.read_bytes` 读取，用 `binary_resource` 绘制（运行时
  `platform/src/script/res.rs`，可通过 widgets prelude 使用），每张图保存一个句柄，
  重绘时会复用已解码的纹理。每个界面有自己的滚动视图，在页面中声明，并通过显式
  `render()` 填充。这样设计是因为在 `card-host` 中观察到两点：在另一个视图的
  `on_render` 中创建的滚动视图要到下一次绘制后才存在，而它自动进行的首次渲染在
  持有已保存的图片时是空的；另外，带 `on_render` 且初始为 `visible: false` 的视图
  在 `set_visible(true)` 之后仍然隐藏。因此这些滚动视图放在普通 `View` 中，通过
  显示和隐藏外层视图来切换。
- 每个处理函数、定时器或回调有 64 ms 的预算；合并或拆分之后的步骤（清除过期缓存、
  重新扫描、进入下一个界面）分在不同的轮次中执行。

## 测试

pdf 服务的 `pdftools_fixture` 示例会在应用启动前，把四个示例 PDF 和一个损坏的文件
写入某个应用根目录（OctoSense home 的 `apps/`，或 `card-host --app-data` 目录）下
PDF Tools 的存储中。加上 `--import-sample <file>` 时，它再写出一个 PDF：一份三页的
花园计划。它从不调用引擎；如果存储中已有数据，它会拒绝写入。

```sh
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- <apps root>
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- --import-sample <file>
```

`tests/ui.py` 在由本仓库构建的隐藏桌面 Shell 中端到端运行应用，使用真实引擎：每次
运行都创建一个新的 `OCTOSENSE_HOME`，把示例写入 `apps/os.pdftools/`，在
Terminal.app 标签页中启动 `target/debug/octosense`（`MAKEPAD_HIDE_WINDOWS=1`、
`MAKEPAD_REMOTE`、`MAKEPAD_WM_TEST_APP=pdftools`），通过远程桥驱动它，保存每张
截图，最后以 `/quit` 结束。一个锁目录保证本机同时只运行一个隐藏 Shell；如果另一个
OctoSense 正在运行，它会拒绝启动。

```sh
cargo build --locked -p octosense
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --only missing --output target/pdftools-ui
```

| 运行 | 覆盖内容 |
| --- | --- |
| `shell` | 在示例上先以浅色、再通过 Shell 自己的样式菜单切换到深色走完每个界面：首页图、页面、单页视图、文字和查找、信息、两种拆分（其中一次替换已有文件）、合并、打开、移除、损坏的文件，以及“Open a PDF from this device” |
| `restart` | 再次使用同一个 home：合并和拆分写入的文件仍在；在两次运行之间按导入留下文件的方式放入资料库的 `Imported PDF.pdf` 会被读取、绘制并打开 |
| `full` | 存储被填到距 64 MiB 上限只差 8 KB：资料库和一次合并上显示引擎的拒绝，然后移除一个文件 |
| `empty` | 没有 PDF：空资料库及其 Open 按钮，浅色和深色各一次 |
| `missing` | App Hub 的 `card-host`，它不提供任何宿主服务：没有引擎，也没有文件服务 |

隐藏的 Shell 无法驱动宿主的对话框，而且隐藏窗口从不获得焦点，因此文件服务会在
对话框打开前拒绝 `files.import`（`foreground_required: …`）；这些运行会检查应用
显示了这个拒绝。导入之后发生的事情就是 `restart` 运行所走的路径：一个索引从未见过
的资料库文件，由引擎读取，再由应用打开。

## 状态

- **已验证**：在 macOS（Apple silicon）上的隐藏桌面 Shell 中使用真实引擎：上文的
  每个界面（浅色和深色）、在同一存储上重启并按导入留下文件的方式放入一个 PDF、
  存储已满、空资料库，以及不提供宿主服务的 `card-host`。截图、摘要和检查记录在
  [tests/evidence/shell-20261009](tests/evidence/shell-20261009/README.zh-CN.md)。
- **未验证**：宿主的文件对话框以及一次真实导入之后的流程（脚本中的 `imported()`）、
  导入的其他拒绝情况、Shell 默认尺寸以外的窗口尺寸、Linux、Windows 和手机。
- **已知问题**：切换浅色和深色会重新运行脚本（运行时的样式重新应用），所以应用会
  回到资料库；页面网格在窗口尺寸改变之前保持两列，因为 Shell 不会再次调用
  `on_app_resize`。
