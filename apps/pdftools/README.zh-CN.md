# PDF Tools

[English](README.md) | 简体中文

PDF Tools（`os.pdftools`）是 OctoSense 的 PDF 应用：管理它自己存储中的 PDF，
提供带页面缩略图的查看器、合并、拆分和文字提取。实际工作由 Shell 的 `pdf`
引擎服务（pdfcraft，[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.zh-CN.md)）完成；
应用本身是 `bundle/` 中一个隔离运行的 Splash 程序，从不自己读取 PDF。它只随桌面
Shell 发布，因为十个 craft 引擎只在桌面版中提供（`craft-engines`）。

## 界面

| 界面 | 显示内容 | 唯一的主要操作 |
| --- | --- | --- |
| 资料库 | `accounts/device/library/` 中的 PDF：首页、名称、页数和大小；损坏的文件会说明无法打开；“Open from this device”说明该功能即将推出 | **Merge PDFs** |
| 文档 › Pages | 页面缩略图，每次 24 页；点按一页进入单页视图 | **Split into files** |
| 文档 › Text | 按阅读顺序显示每页文字，可用 Find in the text 查找 | – |
| 文档 › Info | 文件、页数、页面尺寸、文件大小、标题、作者、主题、关键词、创建程序、生成程序、PDF 版本、安全性、字体、书签 | – |
| 单页视图 | 放大显示一页；Previous、Next 或滑动翻页 | – |
| 合并 | 三步：Choose（带序号的选择，2 到 16 个文件）、Order（上移或下移、移除、命名）、Done（结果，可 Open） | Next / **Merge** / Open |
| 拆分 | Every few pages（步进器）或 Where I choose（点按每个新文件起始的页面）；实时列出将生成的文件，以及会替换哪些已有文件；然后 Done | **Split into N files** |

从设备打开 PDF 即将推出。Shell 的 `files.import`（自 #413 起已在 `main` 中）可把选中
的一个文档复制到应用的存储中，每个文件最大 1 MiB，并需要 `files` 能力；PDF Tools
还没有使用它，所以其入口说明该功能即将推出。在此之前，资料库只显示 PDF Tools 存储中
已有的文件，包括合并和拆分写入的文件。

## 工作方式

下文所有路径都相对于应用的存储，即 `fs.*` 看到的目录。Shell 的“按调用方划分的
引擎区域”落地后，应用自己对引擎的 `host.request` 也在同一目录中工作（ADR 0013
“Engines work in the caller's own folder”）。

| 路径 | 内容 |
| --- | --- |
| `accounts/device/library/*.pdf` | PDF 文件（ADR 0004 §11：系统应用的数据位于其 `device` 账户目录） |
| `accounts/device/library.json` | 引擎对每个文件给出的信息：页数和大小 |
| `cache/covers/<name>.png` | 每个文件的首页（可清除） |
| `cache/pages/<name>/p<n>.png` | 当前打开文档的缩略图；打开一个文档时会删除其他文档的缩略图，因为应用存储最多容纳 256 个条目 |
| `cache/view.png` | 单页视图的图片 |

| 引擎调用 | 何时调用 | 参数 |
| --- | --- | --- |
| `pdf.info` | 首次列出某个文件、打开文档时 | `{path}` |
| `pdf.render` | 首页图、缩略图（`max_side` 360）和单页视图（`max_side` 1400） | `{path, page, out, max_side}` |
| `pdf.text` | Text 标签页 | `{path}`，或前 512 页的 `pages` |
| `pdf.merge` | 合并 | `{paths, out}` |
| `pdf.split` | 拆分 | `{path, out_dir, every}` 或 `{path, out_dir, before}` |

修改脚本时请注意：

- 调用逐个进行：服务在 UI 线程上运行（#399）。
- 拒绝会在 `host.request` 内部直接回调。在随 #413 而来的运行时补丁之前，在这样的
  回调里调用 UI 会丢失回调的其余部分；`engine()` 总是把结果交给下一轮处理
  （`start_timeout`），这也让夹具的回答像服务一样保持异步。
- 页面图片用 `fs.read_bytes` 读取，用 `binary_resource` 绘制（运行时
  `platform/src/script/res.rs`，可通过 widgets prelude 使用），每张图保存一个句柄，
  重绘时会复用已解码的纹理。每个界面有自己的滚动视图，在页面中声明，并通过显式
  `render()` 填充。这样设计是因为在 `card-host` 中观察到两点：在另一个视图的
  `on_render` 中创建的滚动视图要到下一次绘制后才存在，而它自动进行的首次渲染在
  包含已保存的图片时结果为空；另外，带 `on_render` 且初始为 `visible: false` 的
  视图在 `set_visible(true)` 之后仍然隐藏。因此滚动视图放在普通 `View` 中，由外层
  `View` 负责显示和隐藏。
- 每个处理函数、计时器或回调有 64 毫秒的预算；合并或拆分之后的步骤（清除过期缓存、
  重新扫描、显示下一个界面）分在不同的轮次中执行。

## 状态

- **`pdf` 能力目前还不能声明。** App Hub 的契约还不认识它，所以 `manifest.json`
  只申请 `storage`，每个 Shell 都会以 `this app was not granted "pdf", which
  "pdf.info" needs` 拒绝 `pdf.*`。应用会在界面上显示这一拒绝，并继续列出它的文件。
  App Hub 支持后，声明 `pdf` 只需修改清单中的一行。
- **按调用方划分的引擎区域还未落地。** 在此之前，引擎在它自己的私有目录
  `<apps root>/.host/pdf` 中工作，而不是在本应用的存储中，因此应用看不到它渲染的图片。
- **已验证**：在 macOS 上用 App Hub 的 `card-host` 和下文的开发者夹具，在浅色和
  深色模式下验证了每个界面、合并和拆分流程、损坏文件和引擎被拒绝的状态，以及空的
  资料库。**未验证**：在 Shell 中通过应用使用真实引擎，这需要上面两项都完成。

## 开发者夹具

`card-host` 不提供任何宿主服务，因此 PDF Tools 在其中无法访问引擎。为了仍能检验
每个界面，pdf 服务的 `pdftools_fixture` 示例会把四个示例 PDF 和一个损坏文件写入
card-host 的 app-data 目录，用真实服务（App Hub 自己的调度器）处理它们，并保存服务
的回答：`info`、`text`、每一页在两种尺寸下的渲染、一次合并（先 Board minutes，后
Quarterly report，合并为 `Merged.pdf`）以及对野外指南的两次拆分（每 2 页一份，以及
把封面单独拆出）。

```text
<app-data>/os.pdftools/accounts/device/library/*.pdf   示例文件
<app-data>/os.pdftools/dev/engine-replay.json          记录的回答
<app-data>/os.pdftools/dev/replay/**.png               记录的渲染结果
```

只有当 `dev/engine-replay.json` 位于应用存储中，**并且**真实引擎拒绝本应用或不存在时，
应用才会用该文件回答引擎调用；它会先用一次 `pdf.info` 调用确认，并记录日志
`PDF Tools: replaying the engine answers recorded in dev/engine-replay.json
(developer fixture)`。Shell 中没有任何代码会写入 `dev/`，并且引擎可用时会忽略该文件，
因此用户永远不会看到记录的数据。回放的合并或拆分会在引擎本应写入输出的位置写入很小
的占位文件，由记录的回答代替它们的内容。夹具中没有记录的合并或拆分会失败，并给出
指明夹具的提示。

夹具放在应用存储中，而不是放在 `card-host --static` 后面：脚本可以绘制 `{{assets}}`
提供的图片，但在没有 `net` 的情况下无法读取那里的文件，而回放需要读取它的回答。

从仓库根目录运行，`card-host` 需从 App Hub 构建（见
[开发期间运行应用包](../README.zh-CN.md)）：

```sh
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- target/pdftools-fixture
MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=127.0.0.1:8911 card-host --bundle apps/pdftools/bundle --system --app-data target/pdftools-fixture
```

示例会拒绝已经存有本应用数据的 app-data 目录；每次请使用新目录。`tests/ui.py`
以隐藏窗口驱动整个流程，先后在浅色和深色模式下运行，然后检验引擎被拒绝的状态（只有
示例文件、没有夹具）和空资料库，并保存每张原始截图：

```sh
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host \
    --fixture target/pdftools-fixture --output target/pdftools-ui
```
