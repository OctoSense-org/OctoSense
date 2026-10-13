# Quick Deck

[English](README.md) | 简体中文

Quick Deck（`os.quickdeck`）是 OctoSense 桌面版的演示文稿应用：演示文稿库、带幻灯片导航栏、画布和检查器的编辑器、带分节的幻灯片浏览视图、从大纲新建（手动输入，或用用户自己的 AI 提供方起草）、放映和导出。工作由 Shell 的 `deck` 引擎服务（deckcraft，[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.md)）在应用自己的存储中完成，所用方法见 [design/SERVICE.md](design/SERVICE.md)；应用本身是 `bundle/` 中受限运行的 Splash 程序，从不自己读取演示文稿。它只随桌面版 Shell 发布，因为这些创作引擎只在桌面版中（`craft-engines`）。

它声明 `storage`、`files`、`deck` 和 `model`，没有声明 `storage.max_bytes`，所以存储上限是 App Hub 的系统上限 64 MiB。

第 2 版通过 OctoSense App Flow，依据十张已批准的生成设计图（2026 年 10 月 11 日批准）重建：产品需求见 [design/BRIEF.md](design/BRIEF.md)，未改动的原图及其测量数据在 `design/source/`，每个页面一份像素映射在 `design/map/`，把每个测得的区域对应到重建它的控件、几何尺寸、颜色、字号、所依赖的运行时源码以及适用的评审决定。

## 窗口

| 区域 | 内容 |
| --- | --- |
| 标签栏 | Home、每个打开的演示文稿一个标签（标题；有未保存的更改时显示圆点；关闭按钮）、大纲页面打开时它自己的标签，以及回到 Home 的 New。标题平分这一行，放不下时用省略号截断 |
| 工具栏 | Slides 和 Sorter；Text、Picture、Shape、Table、Chart（各自打开插入条）；Undo 和 Redo；Save、Export 和 Present |
| 导航栏 | New slide、各分节（可折叠）、带编号的缩略图（当前幻灯片有外框，隐藏的幻灯片变淡） |
| 画布 | 当前幻灯片，即引擎按画布宽度渲染的图片，以及选中对象的边框和控制点；浮动条：上一张、"5 / 12"、下一张、缩小、缩放比例、放大、Fit |
| 检查器 | Format（显示什么取决于选中的对象：文本、文本框、形状、图片、表格、图表、排列）、Layout（十一种版式、背景、切换效果、放映时跳过）、Theme（主题、配色、字体、幻灯片尺寸、页脚）、Notes。每个开关和标记都显示演示文稿的实际设置：文本框的垂直对齐和自动调整（`deck.format_state`）、图表的元素和配色以及表格的选项（`deck.slide`）、配色方案、字体搭配和页脚（`deck.open`、`deck.state`）；改动会立即显示，服务的下一次应答予以确认，撤销或重做后无需重新打开即显示演示文稿自己的设置 |
| 状态栏 | Saved 或 Edited、"Slide 5 of 12 · 16:9"（浏览视图中为 "12 slides · 2 selected"），以及应用存储已用量与上限，数据来自 Shell 的 `files.status` |

设计图按 1536 x 1024 绘制，清单请求桌面以这个尺寸打开窗口（`"window": {"width": 1536, "height": 1024}`，App Hub 的窗口提示，需要 `"schema_minor": 1`）。桌面会按自身大小收窄：在 Linux 上隐藏运行的 Shell（1400 x 900 窗口）中，Quick Deck 以 1296 x 698 点打开。窗口宽度小于 1180 点时，导航栏缩窄到 200 点，检查器改由工具栏中的 Inspector 按钮在画布上方打开（**未验证**：没有任何一次运行使用这么窄的窗口）。

## 页面

| 页面 | 功能 | 引擎 |
| --- | --- | --- |
| Home | 用引擎的某个主题新建演示文稿；从大纲新建；从本设备打开演示文稿（.pptx，不超过宿主的导入上限）；最近的演示文稿以卡片显示，带引擎渲染的首张幻灯片封面、张数和大小、编辑时间；复制和删除（删除前再确认一次） | `deck.themes`、`deck.cover`、`deck.create`、`deck.open`、`deck.save`、`deck.close` |
| 编辑器 | 导航栏、画布和检查器。点按选中（`deck.hit`），长按加入或移出选择，再点一次已选中的文本框（或双击）即可在其中输入，拖动可移动，拖动控制点可调整大小或旋转。插入条用于文本、图片（来自 images/ 或本设备）、形状（引擎的形状库）、表格和图表 | `deck.slides`、`deck.slide`、`deck.view`、`deck.hit`、`deck.set_text`、`deck.format_state`、`deck.format`、`deck.insert`、`deck.shape`、`deck.picture`、`deck.table`、`deck.chart`、`deck.layouts`、`deck.slides_op`、`deck.design`、`deck.transition`、`deck.notes`、`deck.fonts`、`deck.shapes` |
| 浏览视图 | 每张幻灯片以磁贴显示在所属分节中，带编号和切换效果：选择幻灯片、New slide、Duplicate、Delete、Hide 或 Show、前移和后移、拖动磁贴移动，添加、重命名和删除分节 | `deck.slides_op`、`deck.view` |
| 从大纲新建 | 每张幻灯片一张卡片（一个标题，下面每行一个要点）、一个主题和一种幻灯片尺寸，然后 Make deck。Draft with AI 请用户自己的提供方起草卡片（`model.complete`，fast 档）；没有提供方时提示到 Settings → AI providers 设置，手动输入大纲始终可用 | `deck.create`；宿主的 `model.complete` |
| 放映 | 幻灯片单独显示在应用窗口内的深色舞台上，下方一个小控制条（上一张、"4 / 12"、下一张、Notes、计时、Exit），角落还有一个 Exit；每个动画步骤显示其最终状态，不播放任何动画。Notes 打开显示本张备注的条带 | `deck.show`、`deck.view {step}`、`deck.slide` |
| 导出 | Presentation（.pptx）、PDF（幻灯片、备注页或讲义，每页 1 到 9 张）或 Images（每张幻灯片一个 PNG）；全部幻灯片、当前幻灯片或一个范围；是否包含隐藏的幻灯片；名称。列出该演示文稿以前的导出，并可删除 | `deck.export` |

Undo 和 Redo 使用引擎的历史记录；Save 写入原生 `.deckcraft` 文件（`deck.undo`、`deck.redo`、`deck.save`、`deck.state`）。关闭有未保存更改的标签时会询问：保存、不保存或取消。

每个页面都会处理这些状态：空的演示文稿库、正在渲染的幻灯片、损坏的演示文稿（"Couldn't open this deck" 及引擎给出的原因）、受密码保护的演示文稿（OctoSense 只在宿主的表单中处理密码，目前没有演示文稿用的表单，因此不打开）、引擎暂不打开的演示文稿（内含 zip，或超出服务导入防护的部分，#448）、引擎不读取的格式（.ppt、.key、.odp、PDF）、已打开八个演示文稿、存储已满（标签栏下方出现提示条，状态栏变红）、关闭时有未保存的更改，以及设备上没有引擎（"The deck engine isn't available here"）或引擎版本较旧（"This OctoSense's deck engine is older than Quick Deck"）。

## 从本设备打开演示文稿

"Open a presentation from this device" 请 Shell 的文件服务把用户在宿主自己的对话框中选择的文件复制到 `imports/`。应用要先给新文件命名，因为 `files.import` 在对话框打开前就需要目标路径：`Imported presentation.pptx`，然后是 `Imported presentation 2.pptx`，依此类推。宿主会报告所选文件的显示名称，去掉 `.pptx` 后作为演示文稿的标题（"Lease 2026"；如果这个标题已被使用，则为 "Lease 2026 (2)"）。随后它经过服务的导入防护，在一个标签中打开。第一次 Save 会写入 `decks/<标题>.deckcraft` 并删除导入的文件。被拒绝时，Home 会用通俗的话说明原因，并在下方附上宿主自己的消息。

## 工作原理

所有路径都相对于应用的存储，也就是 `fs.*` 看到的文件夹；引擎也在同一个文件夹中工作。

| 路径 | 内容 |
| --- | --- |
| `library.json` | 第 2 版：每个演示文稿文件的标题、张数、大小、主题、创建/编辑/打开时间、离开时所在的幻灯片和缩放比例、封面和导出记录；导入的图片；"Show notes while presenting"；正在编写的大纲 |
| `decks/*.deckcraft` | 引擎原生格式的演示文稿 |
| `imports/*.pptx` | 从本设备导入、尚未第一次保存的演示文稿 |
| `images/` | 为幻灯片导入的图片 |
| `exports/` | 导出文件：`.pptx`、`.pdf`，以及每次图片导出一个装有 PNG 的文件夹 |
| `.cache/` | 服务的渲染图和封面：最多 16 MiB、64 个文件，其中封面最多 32 个 |

第 1 版的演示文稿（`decks.json` 和 `decks/<id>/g<n>/deck.deckcraft`）会在第 2 版第一次运行时加入演示文稿库，以其大纲第一张幻灯片的标题命名。

给修改脚本的人的说明：

- **引擎工作在 Shell 的 UI 线程上运行（#399）。** 同一时间只有一个调用在进行（`pump()`）：当前幻灯片按画布宽度渲染（按修订号保留；一次修改只重绘其应答 `changed` 中列出的幻灯片），然后依次是幻灯片的形状、选中对象的格式、版式图片、导航栏缩略图（每个计时器周期读取一个 PNG）、浏览视图的磁贴和 Home 的封面。Shell 启动后的第一次调用可能要等引擎的字体预热完成。
- **脚本应用拿不到滚动位置。** 导航栏和浏览视图显示当前幻灯片附近的一段（分别为 40 张和 30 张），用 "‹ Slides 1–n" 和 "Slides n–N ›" 移动这一段。
- **脚本应用收不到按键事件。** 需求中的每个快捷键都改成了按钮：Undo、Redo、Save、Present、Exit、Duplicate、Delete、前移和后移。
- **脚本应用拿不到 TextInput 的选区。** 格式应用于整个文本框，或应用于检查器中选定的段落（Applies to）。正在编辑的文本框是覆盖在幻灯片上的文本输入框，此时引擎绘制的幻灯片不含该文本框的文字；输入框显示纯文本行，没有项目符号。
- **重绘一个区域会重建其中的控件**（`widgets/src/view.rs`），因此由其中某个控件保持的拖动会结束：在幻灯片上拖动时，轮廓画在单独的一层中，松手时才修改一次形状；浏览视图的工具条会说明被拖动的磁贴将放到哪里。
- **状态保存在 `mod.quickdeck_state` 上。** 切换浅色/深色会重新运行脚本；每个 `let` 都重新开始，`mod` 保留下来，`resume()` 用新外观重绘每个区域。
- **Label 会把自己的 margin 和 padding 计算两次**，而且 `on_render` 一开始就隐藏的视图会一直隐藏（与 PDF Tools 相同）：Label 自身不设 padding，需要隐藏的区域包在普通 View 中。
- **计算出来的键不是具名字段。** 对字面量写出的对象执行 `d[key] = v` 不会改变 `d.key`，所以脚本在需要读回的地方直接给具名字段赋值。
- 每个处理函数、计时器或回调有 64 ms 的实际耗时预算；幻灯片图片用 `fs.read_bytes` 读取，用 `binary_resource` 显示。

## 测试

演示文稿库的规则是 `main.splash` 中的纯函数，由 Rust 在脚本虚拟机中执行 bundle 的函数来测试（`crates/shell/src/quickdeck_model_tests.rs`）：导入的演示文稿的标题、演示文稿库索引在重启后保留的内容、第 1 版演示文稿变成什么、存储/卡片/备注/计时的文字、大纲的计数和问题（包括服务的上限）、大纲和模型草稿发给引擎的内容、范围对应的幻灯片、前移和后移、拒绝时的说明、引擎使用的名称、画布的适应和缩放级别、选中对象的缩放和旋转，以及图表数据表格的数字。

```sh
cargo test --locked -p octosense-shell --lib quickdeck_model
```

`tests/ui.py` 端到端运行应用。使用 `--card-host` 时它用 App Hub 的 `card-host`，后者不提供任何宿主服务。使用 `--shell` 时它用从本检出构建的桌面 Shell 和真实的引擎：在 macOS 上从 Terminal.app 标签隐藏启动，在 Linux 上运行在私有的无头 weston 上，与 PDF Tools 的 ui.py 相同；一个锁文件夹保证同一台机器上同时只有一个隐藏的 Shell。

```sh
python3 apps/quickdeck/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/quickdeck-ui
cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines
python3 apps/quickdeck/tests/ui.py --shell target/debug/octosense --lock <dir> --work <dir> --grab-scale 1 --output target/quickdeck-ui
```

运行时给每个处理函数 64 ms 的实际耗时，所以在繁忙的机器上，一次运行可能因为某个处理函数等待 CPU 而失败（日志中出现 `script time budget exceeded`）；`--budget-ms 1000` 会为这样的机器提高 Shell 的预算（`MAKEPAD_SPLASH_BUDGET_MS`，OctoSense 的运行时补丁）。基于没有这个补丁的运行时构建的 `card-host` 会忽略它。

| 运行 | 覆盖内容 |
| --- | --- |
| `missing` | `card-host` 加随 Shell 发布的 bundle：没有引擎也没有文件服务，覆盖 Home、演示文稿标签以及大纲页面及其 Draft with AI |
| `fixture` | `card-host` 加开发夹具：每个设计过的页面，按设计图的 1536 x 1024，浅色和深色，每张截图与其设计图并排放在 `compare/` 中，然后是表格的选项和页脚；检查器的开关和标记与替身报告的内容核对，一个表格选项和页脚的日期在改动及撤销后与替身报告的内容核对 |
| `states` | 开发夹具中损坏、受保护、被暂缓和不支持的演示文稿，未保存更改提示条，存储已满，以及首次运行时空的演示文稿库 |
| `shell` | 用真实引擎从大纲生成一个演示文稿，然后编辑：原地编辑文本、加粗、图表、撤销和重做、版式、主题、备注、浏览视图、保存、导出和放映；先浅色，再通过 Shell 自己的样式菜单切到深色。浅色流程中，检查器的状态跟随真实的改动及其撤销：文本框的垂直对齐和自动调整、图表的数据标签、表格的汇总行、配色方案和页脚，每一项都重新从引擎读取（重新选中形状，或重新查询演示文稿的设计） |
| `restart` | 同一个主目录再次启动：演示文稿库、演示文稿的封面和幻灯片 |

开发夹具不会发布。`dev-fixture/engine.splash` 是 deck 引擎的替身，像服务一样应答 SERVICE.md 中的方法；`tests/ui.py` 在 bundle 的临时副本中用它替换 `engine()`。`dev-fixture/make_fixture.py` 在应用存储中填入已批准设计图中的示例演示文稿（Northwind 第三季度董事会汇报，其幻灯片用设计图自己的示例文字画成图片）、Home 上的另外五个演示文稿、版式图片，以及替身应答的内容（引擎真实的主题、带颜色的配色方案、字体搭配、形状和字体，以及 #507 报告的每个文本框、图表、表格和演示文稿的状态）；替身为演示文稿的数据保留撤销历史。`--states`、`--full` 和 `--empty` 生成其他运行使用的演示文稿库。`--bundle DIR --grabs-only` 为另一个 bundle（例如 main 上的，用于前后对比）截取同样的截图，但不核对状态。

```sh
python3 apps/quickdeck/dev-fixture/make_fixture.py <card-host app data>/os.quickdeck
```

## 状态

第 2 版的截图、并排对比和回执见 [tests/evidence/v2-20261011](tests/evidence/v2-20261011/README.zh-CN.md)，检查器状态（#507）的见 [tests/evidence/inspector-states-20261011](tests/evidence/inspector-states-20261011/README.zh-CN.md)。

- **2026 年 10 月 11 日在 `card-host` 中验证**（macOS，Apple 芯片，运行时默认的 64 ms 预算）：`missing`、`fixture` 和 `states` 三种运行，浅色与深色，日志中没有脚本错误。
- **2026 年 10 月 11 日用真实引擎验证**（Linux，运行在无头 weston 上的隐藏桌面 Shell，由基于 main 202d7cc8 的本分支构建，main 已包含 #503 的 deck 服务）：`shell` 运行（先浅色后深色）和 `restart`，使用运行时默认的 64 ms 预算，没有脚本错误也没有超时。Shell 以 1296 x 698 点打开 Quick Deck（应用区域 1292 x 662）。模型测试在那里通过（22 个；加 `--features app-hub` 时 23 个，多出的是清单的窗口提示）。
- **2026 年 10 月 11 日验证检查器的状态**（#507）：在 `card-host` 中，`fixture` 和 `states` 运行，浅色与深色，64 ms 预算，开关和标记与替身核对；在 Linux 上用真实引擎（Shell 由基于 main eb2fd5d1 的本分支构建，main 已包含 #507），`shell` 运行的浅色流程改动了文本框的垂直对齐和自动调整、图表的数据标签、表格的汇总行、配色方案和页脚，每一项都从引擎读回，再逐一撤销并读回演示文稿自己的设置；然后是深色流程和 `restart`。模型测试通过（27 个）。
- **未验证**：从本设备真正导入文件（macOS 上隐藏窗口的对话框会被拒绝，Linux 主机没有对话框助手），因此也未验证导入防护经由应用显示的拒绝；用真实提供方的 Draft with AI（只验证了没有提供方时的回答）；macOS 上的隐藏 Shell；用真实引擎插入图片、表格、形状库、背景、长备注时的放映备注条以及图片导出（这些页面由开发夹具覆盖）；宽度小于 1180 点的窗口；其他屏幕尺寸；手机。
- **已知问题**：Shell 的窗口小于设计图的 1536 x 1024，所以主题面板、导出和大纲页面需要滚动。主题预览用应用自己的字体绘制（OctoSense 不附带主题字体）。正在编辑的文本框显示没有项目符号的纯文本行。内置主题自己的字体搭配以主题命名，所以标记的是字体相同的那组搭配（Harbor 的字体与 Classic 相同）；导入的演示文稿自己的配色或字体会在提示中写明名称。放映填满的是应用窗口，而不是整个屏幕。
