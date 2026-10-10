# PDF Tools

[English](README.md) | 简体中文

PDF Tools（`os.pdftools`）是 OctoSense 桌面版的 PDF 应用：阅读、搜索、审阅、填写与签名、
整理页面、合并和编辑它自己存储中的 PDF，并可从设备打开 PDF。实际工作由 Shell 的 `pdf`
引擎服务（pdfcraft，[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.zh-CN.md)）
在应用自己的存储中完成，所用方法见 [design/SERVICE.md](design/SERVICE.md)；应用本身是
`bundle/` 中一个隔离运行的 Splash 程序，从不自己读取 PDF。它只随桌面 Shell 发布，因为
craft 引擎只在桌面版中提供（`craft-engines`）。

它申请 `storage`、`files` 和 `pdf`，不声明 `storage.max_bytes`，因此其存储上限是 App Hub
的系统上限 64 MiB。

第 2 版通过 OctoSense App Flow，依据九张已获批准的生成设计图重建：产品需求见
[design/BRIEF.md](design/BRIEF.md)，未经修改的图像及其测量数据在 `design/source/`，
`design/map/` 中每个界面一份像素映射，把每个测得的区域与重建它的组件、它的几何尺寸、
颜色、字号以及适用的评审决定一一对应。

## 窗口

| 区域 | 内容 |
| --- | --- |
| 文档标签 | 显示主页时有 Home 标签，每个打开的 PDF 一个标签（名称、有未保存更改时的圆点、关闭叉号），以及前往主页的 Open |
| 模式栏 | Read · Comment · Fill & Sign · Pages · Combine · Edit，当前模式带下划线；Search in document；Undo、Redo；Save |
| 侧栏与左侧面板 | Pages（缩略图）、Outline（PDF 的书签）、Comments、Search（结果）；面板可以收起 |
| 画布 | 页面以白色显示在桌面背景上：从你前往的那一页开始的至多八页，每页都是引擎按屏幕分辨率渲染的结果 |
| 浮动控制条 | 页码“3 / 24”、上一页和下一页、缩小、缩放比例、放大、适合宽度、适合页面 |
| 右侧面板 | Comment：评论线程。Fill & Sign：表单字段以及 Add text、Add date、Add initials。Edit：正在编辑的段落 |
| 状态栏 | Saved 或 Edited，以及 PDF 占用的存储（“14.6 of 64 MB used”） |

## 模式

| 模式 | 作用 | 引擎 |
| --- | --- | --- |
| 主页 | 最近的 PDF 以卡片显示首页、页数、大小和打开时间；Open a PDF from this device（最大 64 MB）；移除 PDF（会再确认一次）；首次运行时的空状态 | `pdf.open`、`pdf.info`、`pdf.page`、`pdf.close` |
| Read | 页面按 72 dpi × 缩放 × 2 逐页渲染并保留；缩略图逐个填入；大纲可跳转到章节 | `pdf.open`、`pdf.page` |
| 查找 | 在 Search in document 中输入后按 Return：“12 matches on 7 pages”，结果按页分组、匹配处加粗，每处匹配在页面上高亮，当前匹配更醒目，可前后切换 | `pdf.find` |
| Comment | 对点按的那一行做高亮、下划线或删除线（点按两次作用于整段），在点按处添加 Note 和 Text box，四种颜色可选；带回复、状态和删除的评论线程。评论使用在“Comment as”中保存的名字 | `pdf.lines`、`pdf.comments`、`pdf.comment` |
| Fill & Sign | 用框线标出表单字段（未填的必填字段为红色）并列出其值；选中一个字段即可输入值；Add text、Add date 和 Add initials 会在点按处放置标记；姓名缩写会被保存 | `pdf.fields`、`pdf.fill`、`pdf.fill_sign` |
| Pages | 每页一张大缩略图：选择页面，向左或向右旋转、删除、提取（在资料库中生成新 PDF）、从文件插入（这里的另一个 PDF），拖动页面以移动 | `pdf.pages` |
| Combine | 当前 PDF 与其他 PDF 按顺序排列（拖动手柄调整顺序），每个都可指定页面（“1-4, 9”或 All），“One PDF of 31 pages”，名称，Combine；新 PDF 随即打开 | `pdf.merge` |
| Edit | 点击一个段落：它的文字可以就地编辑，右侧面板显示字体和字号；Apply 或 Cancel | `pdf.lines`、`pdf.edit_text` |

Undo 和 Redo 使用引擎的历史记录；Save 把 PDF 写回（`pdf.undo`、`pdf.redo`、`pdf.save`、
`pdf.state`）。关闭有未保存更改的标签时会询问：Save、Don't save 或 Cancel。

每个界面都处理的状态：资料库为空、页面仍在渲染、PDF 损坏（“Couldn't open this PDF”以及
引擎给出的原因）、PDF 受密码保护（“This PDF is protected with a password”：OctoSense 只在
宿主自己的面板上处理密码，目前还没有 PDF 的密码面板，因此不打开它）、存储已满（“Remove a
PDF to make room”）、已打开八个 PDF、关闭时有未保存的更改，以及设备上没有引擎。

## 从设备打开 PDF

“Open a PDF from this device”请 Shell 的文件服务把用户在宿主自己的对话框中选择的一个文件
复制到资料库中。应用要先为新文件命名，因为 `files.import` 在对话框打开前就接收目标路径：
依次为 `Imported PDF.pdf`、`Imported PDF 2.pdf` 等。宿主会报告所选文件的显示名称
（`name`），它去掉 `.pdf` 后成为 PDF 的标题（“Lease 2026”；若该标题已被使用则为
“Lease 2026 (2)”）；文件本身保留应用为它取的名字，标题记录在 `library.json` 中。随后它在
一个标签中打开。若被拒绝，主页会用通俗的文字说明原因，下面附上宿主自己的消息。

## 工作原理

所有路径都相对于应用的存储，即 `fs.*` 看到的文件夹；引擎也在同一个文件夹中工作。

| 路径 | 内容 |
| --- | --- |
| `accounts/device/library/*.pdf` | 所有 PDF，包括导入、提取和合并得到的 |
| `accounts/device/library.json` | 每个文件的标题（与文件名不同时）、页数、大小、上次打开时间、离开时所在的页面和缩放比例、封面的渲染结果；以及 `who`：评论使用的名字和 Fill & Sign 放置的姓名缩写 |
| `.cache/pages/<doc>/<page>@<dpi>.png` | 引擎的页面渲染结果：它的缓存，最多 16 MiB、64 个文件 |

给修改脚本的人的说明：

- **引擎工作在 Shell 的 UI 线程上运行（#399）。** 应用同时只发出一个后台渲染
  （`pump()`）：先是当前页及其后两页，然后是可见的缩略图，然后是这一组页面中的其余页面，
  再是其他缩略图，最后是主页的封面。一处更改只丢弃它所涉及页面的渲染结果；Undo 和 Redo
  不说明改了什么，因此重新绘制当前显示的那一组页面。
- **脚本应用拿不到滚动位置。** 运行时不给脚本应用读取或设置滚动视图位置的方法。所以画布是
  一个静态的 `ScrollXYView`，在渲染结果陆续到达时保持位置不变；前往某一页（缩略图、大纲、
  查找结果、浮动控制条）时，先把画布绘制成空的一帧，使其滚动位置被限制回顶部，再填入从该页
  开始的一组页面。该页之前的页面可以用浮动控制条的上一页回去。
- **脚本应用收不到键盘事件。** 需求中列出的每个快捷键（⌘F、⌘+ 和 ⌘−、⌘0、方向键、⌘Z、
  ⇧⌘Z、⌘S）都改为按钮；Search in document 在按 Return 时执行，这是文本框会报告的事件。
- **状态保存在 `mod.pdftools` 上。** 切换浅色/深色会重新运行脚本（#440）；每个 `let` 都会
  重新开始，`mod` 保留下来，`resume()` 用新的外观重新绘制每个区域。
- **在这个运行时中，Label 会把自己的 margin 和 padding 绘制两次**
  （`widgets/src/label.rs` 把加了内边距的 walk 同时交给它的 turtle 和文字）：不要给 Label
  设置 margin 或 padding，改为在外面套一个 View。
- **`on_render` 一开始就隐藏的视图会一直隐藏**：需要隐藏的区域都套在普通的 View 中，改为
  显示和隐藏外层 View。
- 每个处理函数、定时器或回调有 64 ms 的时间预算；页面图像用 `fs.read_bytes` 读取并用
  `binary_resource` 显示，每张图像保存一个句柄。

## 测试

资料库的规则是 `main.splash` 中的纯函数，由 Rust 在脚本虚拟机中执行包内函数来测试
（`crates/shell/src/pdftools_model_tests.rs`）：导入的 PDF 的标题、拆分出的部分的标题，以及
资料库索引在重启后保留的内容。

```sh
cargo test --locked -p octosense-shell --lib pdftools_model
```

`tests/ui.py` 端到端地运行应用。使用 `--shell` 时，它使用由本检出构建的隐藏桌面 Shell 和
真实引擎，在 pdf 服务的 `pdftools_fixture` 示例写入的示例 PDF 上运行（每次运行都新建一个
`OCTOSENSE_HOME`，从 Terminal.app 的标签页以 `MAKEPAD_HIDE_WINDOWS=1`、`MAKEPAD_REMOTE`
和 `MAKEPAD_WM_TEST_APP=pdftools` 启动 `target/debug/octosense`，通过远程桥驱动它，保存
每一张截图并以 `/quit` 结束；一个锁文件夹保证机器上同一时间只有一个隐藏 Shell，有其他
OctoSense 在运行时它会等待）。使用 `--card-host` 时，它使用 App Hub 的 `card-host`，后者
不提供任何宿主服务。

```sh
cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/pdftools-ui
```

| 运行 | 覆盖内容 |
| --- | --- |
| `shell` | 在示例上覆盖每种模式，先浅色后通过 Shell 自己的样式菜单切到深色：主页及其封面、阅读和缩略图、查找、一处高亮和一条回复、一次旋转及其撤销、带页面范围的合并、一个 Fill & Sign 标记、一次文字编辑、保存、损坏的文件，以及“Open a PDF from this device” |
| `restart` | 再次使用同一个主目录：资料库、其中的标题、PDF 离开时所在的页面，以及按导入方式放入的 `Imported PDF.pdf` |
| `full` | 存储被填到距 64 MiB 只差 8 KB：主页上的拒绝提示，然后移除一个 PDF |
| `empty` | 没有 PDF：空资料库及其 Open 按钮，浅色和深色 |
| `missing` | 使用发布包的 `card-host`：没有引擎，也没有文件服务 |
| `fixture` | 使用开发夹具的 `card-host`：每个设计过的界面按设计图的 1536 x 1024 显示，浅色和深色，每张截图都与它的设计图并排放在 `compare/` 中，另有损坏、受保护和未保存三种状态 |

开发夹具从不随应用发布。`dev-fixture/engine.splash` 是 pdf 引擎的替身，回答 SERVICE.md 中
的方法；`tests/ui.py` 在包的临时副本中用它替换 `engine()`。`dev-fixture/make_fixture.py`
用取自已批准设计图示例文字的示例文档填充应用的存储：页面以图像形式保存，并附上替身要回答的
内容（大纲、带位置框的文字行和段落、表单字段、评论）。

```sh
python3 apps/pdftools/dev-fixture/make_fixture.py <card-host app data>/os.pdftools
```

隐藏的 Shell 中无法操作宿主的对话框，而且隐藏窗口永远不会获得焦点，所以文件服务会在对话框
打开前就拒绝 `files.import`（`foreground_required: …`）；各次运行会检查应用是否显示了这个
拒绝。

## 状态

- **尚未验证**：在这个版本上进行的运行及其证据见拉取请求。
- **不在这个版本中**：导出（没有已批准的设计）、受密码保护的 PDF、键盘快捷键（运行时不给
  脚本应用按键事件）、Edit 面板中的 Alignment 和 Colour（`pdf.edit_text` 只接收文字）、
  Linux、Windows、手机。
