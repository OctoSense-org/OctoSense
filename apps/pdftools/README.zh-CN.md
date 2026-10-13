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
| 侧栏与左侧面板 | Pages（缩略图）、Outline（PDF 的书签）、Comments；面板可以收起。查找进行时，结果占用左侧面板（侧栏中没有项目高亮），查找结束后交还 |
| 画布 | 页面以白色显示在桌面背景上：从你前往的那一页开始的至多八页，每页都是引擎按屏幕分辨率渲染的结果 |
| 浮动控制条 | 页码“3 / 24”、上一页和下一页、缩小、缩放比例、放大、适合宽度、适合页面，以及全屏（阅读模式）；在阅读模式中，该位置换成 Exit read mode |
| 右侧面板 | Comment：评论线程。Fill & Sign：表单字段以及 Add text、Add date、Add initials。Edit：正在编辑的段落 |
| 状态栏 | Saved 或 Edited，以及应用存储已用多少、最多可用多少，来自 Shell 的 `files.status`（“14.6 of 64 MB used”、“37 KB of 64 MB used”） |

设计图按 1536 x 1024 绘制，清单请求桌面按这个尺寸打开窗口：`"window": {"width": 1536,
"height": 1024}`，即 App Hub 的可选窗口提示，需要 `"schema_minor": 1`。桌面会把它限制在自身
尺寸以内，并减去每个新窗口都保留的边距（左右各 42 点，上下各 32 点）。程序坞浮在桌面之上时
（OctoSense 的程序坞就是这样），底部边距从程序坞上沿算起，所以缩放条和状态栏永远不会位于
程序坞后面（`crates/shell/src/desktop_layout.rs`）。在隐藏 Shell 的 1400 x 897 窗口中，
PDF Tools 以 1296 x 703 点打开，应用区域为 1292 x 667，即它的宽布局
（[tests/evidence/window-20261010](tests/evidence/window-20261010/README.zh-CN.md)）。有这个
提示之前，它得到的是默认窗口：宽度取桌面的 72%、高度取 76%，最大 1000 x 720 点，应用区域为
990 x 603。早于这个提示的 Shell 会忽略它，打开默认窗口。两种情况下用户都可以调整窗口大小或将其
最大化。窗口宽度达到 1180 点及以上时（宽布局），右侧面板在左侧面板旁边打开。宽度小于
1180 点时，模式标签和 Save 会收紧内边距，两种右侧面板都取 320 点宽，右侧面板会取代左侧面板的
位置；在用户自己缩放之前，页面会适合画布宽度，最大 100%。

## 模式

| 模式 | 作用 | 引擎 |
| --- | --- | --- |
| 主页 | 最近的 PDF 以卡片显示首页、页数、大小和打开时间；Open a PDF from this device（最大 64 MB）；移除 PDF（会再确认一次）；首次运行时的空状态 | `pdf.open`、`pdf.info`、`pdf.page`、`pdf.close` |
| Read | 页面按 72 dpi × 缩放 × 2 逐页渲染并保留；缩略图逐个填入；大纲可跳转到章节 | `pdf.open`、`pdf.page` |
| 阅读模式 | 浮动控制条上的全屏按钮：只显示页面。文档标签、模式栏、侧栏和两侧面板、工具行以及状态栏都让出空间，浮动控制条保留上一页和下一页、页码、缩放、适合宽度、适合页面以及 Exit read mode。右上角一个不显眼的按钮也可以退出（脚本收不到按键事件，因此没有 Esc）。进入和退出时，当前页和你选择的缩放保持不变；适合宽度、适合页面或自动缩放会适应新的画布。在页面上点按只会选中该页 | `pdf.page` |
| 查找 | Search in document 是唯一的查找入口，输入后按 Return：左侧面板显示“12 matches on 7 pages”，结果按页分组、匹配处加粗，每处匹配在页面上高亮，当前匹配更醒目，可前后切换。按 Done、结果面板的关闭按钮或搜索框的清除按钮结束查找后，左侧面板恢复之前显示的内容（之前的内容不明时显示 Pages）；文档发生更改时也以同样方式结束查找 | `pdf.find` |
| Comment | 对点按的那一行做高亮、下划线或删除线（点按两次作用于整段），在点按处添加 Note 和 Text box，四种颜色可选；带回复、状态和删除的评论线程。评论使用在“Comment as”中保存的名字 | `pdf.lines`、`pdf.comments`、`pdf.comment` |
| Fill & Sign | 用框线标出表单字段（未填的必填字段为红色）并列出其值；选中一个字段即可输入值；Add text、Add date 和 Add initials 会在点按处放置标记；姓名缩写会被保存 | `pdf.fields`、`pdf.fill`、`pdf.fill_sign` |
| Pages | 每页一张大缩略图：选择页面，向左或向右旋转、删除、提取（在资料库中生成新 PDF）、从文件插入（这里的另一个 PDF），拖动页面以移动 | `pdf.pages` |
| Combine | 当前 PDF 与其他 PDF 按顺序排列（拖动手柄调整顺序），每个都可指定页面（“1-4, 9”或 All），“One PDF of 31 pages”，名称，Combine；新 PDF 随即打开。列表放不下时在卡片内滚动，总页数、名称和 Combine 保持可见 | `pdf.merge` |
| Edit | 点击一个段落：它的文字可以就地编辑；右侧面板显示字体和字号、Alignment（左对齐、居中、右对齐或两端对齐：默认选中各行所显示的那种，因为引擎在未指定时会把编辑后的段落设为左对齐）和 Colour（在选择六种颜色之一前保持段落原有颜色）；在 Apply 之前，面板会说明这次编辑除了改字之外还会带来什么变化，或者为什么无法进行（见[编辑文字](#编辑文字)）；Apply（或 Apply anyway）或 Cancel | `pdf.lines`、`pdf.edit_check`、`pdf.edit_text` |

Undo 和 Redo 使用引擎的历史记录；Save 把 PDF 写回（`pdf.undo`、`pdf.redo`、`pdf.save`、
`pdf.state`）。关闭有未保存更改的标签时会询问：Save、Don't save 或 Cancel。

每个界面都处理的状态：资料库为空、页面仍在渲染、PDF 损坏（“Couldn't open this PDF”以及
引擎给出的原因）、PDF 受密码保护（“This PDF is protected with a password”：OctoSense 只在
宿主自己的面板上处理密码，目前还没有 PDF 的密码面板，因此不打开它）、存储已满（“Remove a
PDF to make room”）、已打开八个 PDF、关闭时有未保存的更改，以及设备上没有引擎。

## 编辑文字

Edit 用引擎自带的编辑器在原处重新绘制一个段落。真实世界中的 PDF 限制了它能做到的程度，
右侧面板会在 Apply 之前把这些限制说出来。点击段落时，以及输入停顿时，应用都会询问引擎这次
编辑会产生什么结果（`pdf.edit_check`：它在文档的一份副本上进行编辑，然后丢弃副本），因此
任何编辑都不会在没有提示的情况下弄乱页面：

- **换用其他字体。** PDF 通常只嵌入它用到的字形（子集字体），网页浏览器打印的文字也常常使用
  没有名字的 Type 3 字体。当段落自己的字体无法显示新文字时，引擎会改用 Helvetica、Times 或
  Courier 来排这个段落，外观不同，宽度也不同。面板会写明将使用哪种字体。
- **需要更多空间。** 编辑后的单行段落会向右延伸，最远到页面边距；较长的段落会向下延伸；周围
  的内容都不会移动。如果新文字会压到其他文字行上，或者超出页面，面板会说明。
- **中文、日文和韩文。** 段落字体中没有的字符，引擎要从一个后备字体中绘制，而本版本不包含这个
  字体，所以引擎会拒绝这样的文字，面板会直接说明：“PDF Tools can't edit Chinese, Japanese
  or Korean text yet.”（PDF Tools 目前还不能编辑中文、日文或韩文文字。）字体中已有的字符（例如
  同样的字换个顺序）仍然可以使用。西里尔字母、希腊字母等其他标准字体无法显示的文字也会以同样
  的方式被拒绝，并列出这些字符。
- **一行文字被拆成几块。** 如果一个标题的字形分布在几种字体中，浏览器会按字体把它打印成几块，
  引擎则把每一块读作一个独立的段落，所以点击只会选中那一块。面板会说明这一行被拆成了几块。

只要面板上有警告，Apply 就显示为“Apply anyway”；还没有按当前内容检查过的编辑会先检查，
只有在没有任何需要说明的情况时才会立即进行。拒绝的原因以红色显示，此时按 Apply 只会再次
显示它。面板显示说明时，卡片占满面板的高度：其中的字段可以滚动，Apply 和 Cancel 固定在卡片
底部，即使在桌面的默认窗口中也始终可见。安全设置禁止修改的文档，或者只能以只读方式打开的文档，也会以同样的方式被拒绝。

pdf 服务的 `pdftools_fixture` 示例可以写出一页“网页浏览器打印出来”的页面，包含上述全部四种
情况（一个以无名 Type 3 字体分成五块的标题，以及使用嵌入式子集字体的中文文字），可用来试用
Edit：

```sh
cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- --edit-sample <file>
```

## 从设备打开 PDF

“Open a PDF from this device”请 Shell 的文件服务把用户在宿主自己的对话框中选择的一个文件
复制到资料库中。应用要先为新文件命名，因为 `files.import` 在对话框打开前就接收目标路径：
依次为 `Imported PDF.pdf`、`Imported PDF 2.pdf` 等。宿主会报告所选文件的显示名称
（`name`），它去掉 `.pdf` 后成为 PDF 的标题（“Lease 2026”；若该标题已被使用则为
“Lease 2026 (2)”）；文件本身保留应用为它取的名字，标题记录在 `library.json` 中。随后它在
一个标签中打开。若被拒绝，主页会用通俗的文字说明原因，下面附上宿主自己的消息。

在 Linux 上，宿主的对话框是桌面自带的助手程序，由 Makepad 运行：即 `PATH` 上 zenity、
qarma、matedialog 和 kdialog 中的第一个。一个都没有的机器得不到 `files.import`
（`files.status` 回答 `import_supported: false`），所以主页在按钮旁显示“Not available
here”，点按按钮会说明这个 OctoSense 无法为 PDF Tools 从设备打开文件。

## 工作原理

所有路径都相对于应用的存储，即 `fs.*` 看到的文件夹；引擎也在同一个文件夹中工作。

| 路径 | 内容 |
| --- | --- |
| `accounts/device/library/*.pdf` | 所有 PDF，包括导入、提取和合并得到的 |
| `accounts/device/library.json` | 每个文件的标题（与文件名不同时）、页数、大小、上次打开时间、离开时所在的页面和缩放比例、封面；以及 `who`：评论使用的名字和 Fill & Sign 放置的姓名缩写 |
| `.cache/pages/<doc>/<page>@<dpi>.png` | 引擎的页面渲染结果：它的缓存，与封面合计最多 16 MiB、64 个文件 |
| `.cache/covers/<name>-<size>-<modified>@48.png` | 每个 PDF 的封面，即第 1 页（`pdf.cover`）：关闭和重启后仍保留，PDF 改变后重新绘制；最多占缓存 64 个文件中的 32 个，空间不足时先移除页面渲染 |

给修改脚本的人的说明：

- **引擎工作在 Shell 的 UI 线程上运行（#399）。** 应用同时只发出一个后台渲染
  （`pump()`）：先是当前页及其后两页，然后是可见的缩略图，然后是这一组页面中的其余页面，
  再是其他缩略图，最后是主页的封面。每次启动时，主页为它显示的前 32 个 PDF 各请求一次
  `pdf.cover`（服务最多保留 32 个封面）：已保留的封面只凭文件作答（一次 stat 和读取 24 字节），
  在此期间先显示 `library.json` 中记下的图片。封面换了路径，说明 PDF 已经改变（例如保存过），
  因此会从 `pdf.info` 重新读取页数和大小；前 32 个以外、还没有这些信息的 PDF 也这样读取。保存时
  还会记下打开的文档的页数，移除会删除封面。一处更改只丢弃它所涉及页面的渲染结果；Undo 和 Redo
  不说明改了什么，因此重新绘制当前显示的那一组页面。
- **脚本应用拿不到滚动位置。** 运行时不给脚本应用读取或设置滚动视图位置的方法。所以画布是
  一个静态的 `ScrollXYView`，在渲染结果陆续到达时保持位置不变；前往某一页（缩略图、大纲、
  查找结果、浮动控制条）时，先把画布绘制成空的一帧，使其滚动位置被限制回顶部，再填入从该页
  开始的一组页面。该页之前的页面可以用浮动控制条的上一页回去。
- **脚本应用收不到键盘事件。** 需求中列出的每个快捷键（⌘F、⌘+ 和 ⌘−、⌘0、方向键、⌘Z、
  ⇧⌘Z、⌘S）都改为按钮；⌘F 对应的就是 Search in document 搜索框本身，它在按 Return 时执行，
  这是文本框唯一会报告的按键。
- **状态保存在 `mod.pdftools` 上。** 切换浅色/深色会重新运行脚本（#440）；每个 `let` 都会
  重新开始，`mod` 保留下来，`resume()` 用新的外观重新绘制每个区域。
- **在这个运行时中，Label 会把自己的 margin 和 padding 绘制两次**
  （`widgets/src/label.rs` 把加了内边距的 walk 同时交给它的 turtle 和文字）：不要给 Label
  设置 margin 或 padding，改为在外面套一个 View。
- **`on_render` 一开始就隐藏的视图会一直隐藏**：需要隐藏的区域都套在普通的 View 中，改为
  显示和隐藏外层 View。
- 每个处理函数、定时器或回调有 64 ms 的挂钟时间预算；页面图像用 `fs.read_bytes` 读取并用
  `binary_resource` 显示，每张图像保存一个句柄。翻页或打开文件改动了 `library.json` 时，
  由心跳定时器写入（至多每 2 秒一次），因此点按从不等待磁盘；新的标题、移除操作或
  “Comment as”中的名字会立即写入。

## 测试

资料库的规则是 `main.splash` 中的纯函数，由 Rust 在脚本虚拟机中执行包内函数来测试
（`crates/shell/src/pdftools_model_tests.rs`）：导入的 PDF 的标题、拆分出的部分的标题、
资料库索引在重启后保留的内容、页面范围所指的页面、查找摘要中加粗的匹配、头像中的姓名缩写、
评论的日期、状态栏的存储用量文字、从各行读出的段落对齐方式、一次编辑发送的内容，以及 Edit
面板对一次编辑会产生什么结果的说明（字体的名称、警告和“Apply anyway”、被拆成几块的一行）。
它们还会解析整个脚本，包括那里的虚拟机不会运行的界面和处理函数：只要有一处解析错误，整个应用
在加载时就会失败。
pdf 服务自己的测试在 `--edit-sample` 页面上核对 `pdf.edit_check` 的回答与引擎随后实际所做的
是否一致（`cargo test --locked -p octosense-pdf-service`）。

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

在 Linux 上，`MAKEPAD_HIDE_WINDOWS` 不起作用，所以 `--shell` 不使用 Terminal.app 标签页，而是
为每次运行启动一个独占的无头 weston：使用 pixman 渲染器，从不进入空闲状态，运行时文件夹是私有
的。Shell 在它上面运行，使用 Makepad 的 Wayland 后端，XDG 文件夹都放在本次运行的主目录中；
`/quit` 之后 weston 随之停止。机器上的其他 OctoSense 不会让运行等待。用 `--grab-scale 1`
得到每点一个像素（那里窗口的 dpi 为 1）。用 `--work` 指定你要测试的磁盘，因为 Ubuntu 的
`/tmp` 是 tmpfs。

```sh
cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/pdftools-ui
# Linux
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --work <dir> --grab-scale 1 --output target/pdftools-ui
```

运行时给每个处理函数 64 ms 的挂钟时间，因此在繁忙的机器上，一次运行可能因为某个处理函数
等待 CPU 而失败（日志中出现 `script time budget exceeded`）；对这样的机器，
`--budget-ms 1000` 会提高宿主的预算（`MAKEPAD_SPLASH_BUDGET_MS`）。

| 运行 | 覆盖内容 |
| --- | --- |
| `shell` | 在 Shell 自己的窗口中、在示例上覆盖每种模式，先浅色，再通过 Shell 自己的样式菜单切到深色（深色这一轮在浅色一轮留下的状态上进行）：主页及其封面、状态栏与存储实际字节数相符、“Open a PDF from this device”被拒绝（在没有对话框助手程序的 Linux 上为“Not available here”）、阅读和缩略图、阅读模式（从浮动控制条进入，在其中翻页并改变缩放，再用 Exit read mode 或右上角按钮退出，回到同一页、同一缩放）、只用顶栏的查找（“9 matches on 4 pages”，结束时交还左侧面板）、一处高亮及其回复（日期按本机时钟显示，如“Today 19:42”）、一次旋转及其撤销、带页面范围的合并（“One PDF of 5 pages”）、Fill & Sign 放置的姓名缩写、一次带对齐方式和颜色的文字编辑、保存，以及损坏的文件；之后检查封面缓存中每个 PDF 只有一个封面，且都记在 `library.json` 中 |
| `restart` | 再次使用同一个主目录：资料库、其中的标题、PDF 离开时所在的页面、按导入方式放入的 `Imported PDF.pdf`、所有当前封面都被保留且没有重新绘制，以及从主页移除的 PDF 连同其保留的封面一起删除 |
| `full` | 存储被填到距 64 MiB 只差 8 KB：主页上的拒绝提示，然后移除一个 PDF（会再确认一次） |
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
拒绝。在 Linux 上，运行中的窗口映射在它的 weston 上，并被视为有焦点。`PATH` 上没有对话框
助手程序时，各次运行改为检查“Not available here”和按钮给出的提示。有助手程序时，对话框会真的
打开，所以各次运行会跳过这一步，并在 `run.json` 中说明。

## 状态

- **已于 2026 年 10 月 10 日验证**（macOS，Apple 芯片；截图、并排对比图和记录见
  [tests/evidence/v2-20261010](tests/evidence/v2-20261010/README.zh-CN.md)）：模型测试；在
  隐藏的桌面 Shell 中使用真实引擎、按运行时 64 ms 的脚本预算进行的 `shell`、`restart`、
  `full` 和 `empty` 运行（在其他构建占用机器时又用 `--budget-ms 1000` 运行了一次）；在
  card-host 中进行的 `missing` 和 `fixture` 运行，浅色和深色。`fixture` 使用了
  `--budget-ms 1000`：它的替身引擎在脚本中作答，按 64 ms 运行时，深色一轮中有一次作答超时。
  这些运行使用的是默认窗口，应用区域为 990 x 603 点。
- **已于 2026 年 10 月 10 日在窗口自身尺寸下验证**（macOS，Apple 芯片；
  [tests/evidence/window-20261010](tests/evidence/window-20261010/README.zh-CN.md)）：使用真实
  引擎、按运行时 64 ms 的预算，以宽布局（应用区域 1292 x 662）进行的 `shell` 运行，浅色和深色；
  以及从 Shell 菜单打开时，在 1380 x 845 的工作区上打开的 1296 x 703 窗口，距程序坞 32 点。
- **已于 2026 年 10 月 10 日和 11 日验证后续改进**（macOS，Apple 芯片；
  [tests/evidence/v2-followups-20261010](tests/evidence/v2-followups-20261010/README.zh-CN.md)）：
  模型测试；在隐藏的桌面 Shell 中使用真实引擎、在窗口自身尺寸（应用区域 1292 x 662）下进行的
  `shell`、`restart`、`full` 和 `empty` 运行：按本地时间显示的评论日期、Edit 面板的 Alignment
  和 Colour、来自 `files.status` 的状态栏存储用量、关闭和重启后仍保留并随 PDF 一起移除的封面、
  列出两个 PDF 时合并按钮保持可见、只在顶栏的搜索，以及阅读模式；以及在 card-host 中按
  1536 x 1024 进行的 `fixture` 运行。10 月 10 日（加入顶栏搜索和阅读模式之前），这些运行按
  运行时 64 ms 的预算通过。10 月 11 日它们使用 `--budget-ms 1000`：在 64 ms 下、54 到 74 的
  平均负载中，`shell` 运行通过了每个步骤，但在 pdf 服务的文件重命名造成 UI 线程停顿之后超出了
  预算。
- **已于 2026 年 10 月 11 日在 Linux 上验证**（Linux 构建主机：Ubuntu 26.04，x86_64，在无头
  weston 下使用软件 GL，主目录位于 ext4；
  [tests/evidence/linux-20261011](tests/evidence/linux-20261011/README.zh-CN.md)）：
  - 模型测试和 pdf 服务的测试；
  - 使用真实引擎、按运行时 64 ms 的预算进行的 `shell`、`restart`、`full` 和 `empty` 运行，
    共五次：四次在 UTC 时区（最后两次是在变基到包含 #485、#494 和 #487 的 main 之后），一次在
    UTC+14 时区，后者中评论日期正确地跨过了午夜；
  - PDF Tools 以 1296 x 698 打开（应用区域 1292 x 662），与 Mac 上相同；
  - “Open a PDF from this device”经由 Makepad 在 Linux 上的对话框路径真实地导入了一个 PDF，
    由一个替身代替对话框助手程序作答。
- **已于 2026 年 10 月 11 日验证真实世界 PDF 上的 Edit**（在 Linux 构建主机上，使用私有的无头
  weston 和 Makepad 的 Wayland 后端，而不是 macOS：当时另一个会话正在 Mac 上运行 OctoSense；
  [tests/evidence/edit-20261011](tests/evidence/edit-20261011/README.zh-CN.md)）：模型测试；
  pdf 服务的测试；在桌面 Shell 中使用真实引擎、按运行时 64 ms 的预算，在 `--edit-sample`
  页面上进行的浅色和深色运行：五块的说明、字体和空间的警告以及 Apply anyway、对中文文字的
  直白拒绝、一次仍然进行的编辑和一次立即生效的编辑。本次改动后 macOS 上的 Edit **未验证**。
- **未验证**：宿主自己的文件对话框（Mac 上的隐藏窗口无法打开它；在 Linux 上由替身助手程序
  作答）；使用真实引擎时的表单字段、大纲、Redo、其他评论和页面操作、受密码保护的 PDF、八个
  PDF 的上限以及未保存更改的询问（开发夹具覆盖了它们的界面）；其他屏幕尺寸；Linux 上的
  X11、GPU 和桌面合成器；Windows；手机。
- **已知问题**：阅读模式占满的是应用窗口，而不是整个屏幕：应用无法让 OctoSense 窗口本身全屏
  （没有相应的宿主 API），想要完整效果请将窗口最大化。在负载很高的机器上，一次点按可能超出
  64 ms 预算而丢失（在 `library.json`
  移出点按路径之前，于 60 到 150 的平均负载下见到过）。pdf 服务的文件写入在 UI 线程上运行，
  也计入这个预算（在 54 到 74 的平均负载下，曾在 262 ms 和 555 ms 的重命名之后超出预算）。在 900 点高的屏幕上（应用区域约为
  1292 x 662），首页卡片的最后一行说明（每个 PDF 上次打开的时间）正好在可见区域边缘；资料库
  可以滚动。引擎既不报告段落的
  对齐方式，也不报告它的颜色：Edit 面板从各行的边框读出对齐方式，并在选择颜色之前把颜色显示为
  段落原有的颜色。
- **不在这个版本中**：导出（没有已批准的设计）、打开受密码保护的 PDF、键盘快捷键（运行时
  不给脚本应用按键事件）。
