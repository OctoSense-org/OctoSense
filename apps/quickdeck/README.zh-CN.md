# Quick Deck

[English](README.md) | 简体中文

Quick Deck（`os.quickdeck`）是 OctoSense 桌面版的演示文稿应用：演示文稿库、带幻灯片导航栏、画布和检查器的编辑器、带分节的幻灯片浏览视图、New from an Outline（手动输入，或用用户自己的 AI 提供方起草）、放映和导出；演示文稿中的视频和音频可在编辑器和放映中播放。工作由 Shell 的 `deck` 引擎服务（deckcraft，[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.md)）在应用自己的存储中完成，所用方法见 [design/SERVICE.md](design/SERVICE.md)；应用本身是 `bundle/` 中受限运行的 Splash 程序，从不自己读取演示文稿。它只随桌面版 Shell 发布，因为这些创作引擎只在桌面版中（`craft-engines`）。

它声明 `storage`、`files`、`deck` 和 `model`，没有声明 `storage.max_bytes`，所以存储上限是 App Hub 的系统上限 64 MiB。

第 2 版通过 OctoSense App Flow，依据十张已批准的生成设计图（2026 年 10 月 11 日批准）重建：产品需求见 [design/BRIEF.md](design/BRIEF.md)，未改动的原图及其测量数据在 `design/source/`，每个页面一份像素映射在 `design/map/`，把每个测得的区域对应到重建它的控件、几何尺寸、颜色、字号、所依赖的运行时源码以及适用的评审决定。此后它在共享界面上采用了 [apps/AGENTS.md](../AGENTS.md) 规定的 Mac 外观：各页面保留设计图的组成和顺序，尺寸和颜色则遵循 Mac。

## 窗口

| 区域 | 内容 |
| --- | --- |
| 工具栏 | 在 Mac 上位于窗口顶边，红绿灯按钮占它开头的 84 点：在这 52 点内没有控件接收的按压会移动窗口（其他样式下在标题栏下方，高 40 点）。Home 上：应用名称、演示文稿数量、New from an Outline… 和 Open…。大纲页面和无法打开的演示文稿上：它的标题及其状况。演示文稿中：Slides 和 Sorter；上一张与下一张之间的幻灯片编号；缩小、缩放比例、放大、Fit；Text、Picture、Shape、Table、Chart（桌面上 Picture、Shape、Table 和 Chart 各自从按钮下方垂下一个弹出框，与 Keynote 相同：在弹出框外按下或按 Esc 关闭它，按另一个插入工具则打开那个工具的弹出框；手机上各自打开插入条）；手机上有 Undo 和 Redo（桌面上它们在 Edit 菜单中，即 ⌘Z 和 ⇧⌘Z，与 Keynote 相同）；Save、Export… 和 Present（唯一的填充按钮）。表单打开时它们变暗，不接收按压 |
| 标签栏 | 有内容打开后以一条细带显示在工具栏下方：最左边是 Home 图标，每个打开的演示文稿一个标签（标题居中），大纲页面打开时有它自己的标签，最右边是回到 Home 的 +。有未保存更改的演示文稿在标签左端显示圆点；指针移到标签上时那里显示关闭按钮；右键菜单有 Close Tab、Save、Export… 和 Present。标题平分这一行，放不下时用省略号截断 |
| 导航栏 | New slide、各分节（可折叠）、带编号的缩略图（当前幻灯片有外框，隐藏的幻灯片变淡），所有幻灯片都在一个可滚动的列表中。在幻灯片上点右键先有 Cut、Copy 和 Paste，再有 New Slide、Duplicate、Hide Slide、Delete、Move Earlier、Move Later 和 New Section |
| 画布 | 当前幻灯片，即引擎按画布宽度渲染的图片，以及选中对象的边框和控制点；选中的媒体剪辑下方有它的播放控件（Play；播放时可暂停或继续、停止，显示时间和声音开关），视频在它的框中播放（见[播放视频和音频](#播放视频和音频)） |
| 检查器 | 顶部是分段控件：Format（显示什么取决于选中的对象：文本、文本框、形状、图片、表格、图表、媒体剪辑的播放、排列）、Layout（十一种版式、背景、切换效果、放映时跳过）、Theme（主题、配色、字体、幻灯片尺寸、页脚）、Notes。桌面上它的开关行是复选框，与 Keynote 相同（手机上仍是开关）；表格的行、列和单元格命令是写明名称的下拉菜单，与单元格的右键菜单一致。每个开关和标记都显示演示文稿的实际设置：文本框的垂直对齐和自动调整（`deck.format_state`）、图表的元素和配色以及表格的选项（`deck.slide`）、配色方案、字体搭配和页脚（`deck.open`、`deck.state`）；改动会立即显示，服务的下一次应答予以确认，撤销或重做后无需重新打开即显示演示文稿自己的设置 |
| 状态栏 | 细条小字：Saved、Saving…（保存期间 Save 变暗）或 Edited，然后是刚导出保存了什么；"Slide 5 of 12 · 16:9"（浏览视图中为 "12 slides" 和选中了多少张）；应用存储已用量与上限，数据来自 Shell 的 `files.status`；存储已满时还有 Storage Is Full…，点击可再次显示提醒框 |

它的外观与 Mac 应用一致（见 [apps/AGENTS.md](../AGENTS.md) 的最后一节）：13 点文字、28 点控件、细线，浅色和深色都使用共享界面的颜色。清单请求桌面以 1536 x 1024 打开窗口（`"window": {"width": 1536, "height": 1024}`，App Hub 的窗口提示，需要 `"schema_minor": 1`）。桌面会按自身大小收窄：在隐藏运行的 Shell（1400 x 900 窗口）中，Quick Deck 打开时应用区域为 1296 x 719 点。窗口宽度小于 1160 点（Mac 的统一窗口为 1232 点，红绿灯按钮多占 72 点）时，工具栏的工具（包括浏览视图的工具）只显示图标，不显示名称；小于 960 点时，导航栏缩窄到 154 点，检查器收到工具栏中的 Inspector 按钮后面。打开时它位于画布旁边，画布变窄让出空间，所以它不会盖住幻灯片；小于 860 点时，它打开期间占据导航栏的位置。小于 860 点时工具栏不再显示缩放，小于 740 点时不再显示幻灯片编号（统一窗口中各提前 72 点）。窗口缩小到 700 点时也没有内容被裁切。

## 页面

| 页面 | 功能 | 引擎 |
| --- | --- | --- |
| Home | 引擎主题的网格：点击其中一个即以该主题新建演示文稿。工具栏中有 New from an Outline… 和 Open…（.pptx，不超过宿主的导入上限）；最近的演示文稿带引擎渲染的首张幻灯片封面、张数和大小、编辑时间；在卡片上点右键或点它的 … 按钮有 Open、Duplicate 和 Remove…（删除前用提醒框再确认一次）。无论有没有 Recent，主题缩略图的大小都不变 | `deck.themes`、`deck.cover`、`deck.create`、`deck.open`、`deck.save`、`deck.close` |
| 编辑器 | 导航栏、画布和检查器。点击选中（`deck.hit`）；右键菜单与 Keynote 相同，先是 Cut、Copy 和 Paste，再是工具栏和检查器为该对象提供的命令，其中包括 Add to Selection（按住 Shift 或 Command 点按，或长按，效果相同）。Cut 和 Copy 在当前 Quick Deck 会话中保留选中的对象（没有选中对象时则保留选中的幻灯片），即使关闭来源标签也可粘贴；Paste 把对象放到当前幻灯片，把幻灯片放在当前幻灯片之后，没有复制内容时显示为灰色。普通对象可跨主题粘贴并保留原样；幻灯片需要相同的设计，部分复杂或导入对象及链接暂不支持；Edit 菜单有同样三项及其快捷键，Select All 选中幻灯片上的全部对象（在幻灯片浏览器中则选中全部幻灯片）；再点一次已选中的文本框（或双击）即可原地输入，项目符号、编号和行距保持不变；点击表格的单元格即可输入，Return 转到下方的单元格；拖动可移动，拖动控制点可调整大小或旋转。Font 弹出菜单是真正的菜单，带 More Fonts…，后者打开一个可搜索全部字体的表单：单击选中一个字体，按 Choose（默认按钮）或双击即应用。Text 立即在幻灯片上放一个文本框；图片（来自 images/ 或本设备）、形状（引擎形状库的每个预设一张小图，按类别显示）、表格和图表在桌面上从弹出框插入，在手机上从插入条插入。选中的视频或音频可用它的控件播放 | `deck.slides`、`deck.slide`、`deck.view`、`deck.hit`、`deck.set_text`、`deck.format_state`、`deck.format`、`deck.insert`、`deck.shape`、`deck.picture`、`deck.table`、`deck.chart`、`deck.layouts`、`deck.slides_op`、`deck.design`、`deck.transition`、`deck.notes`、`deck.fonts`、`deck.shapes`、`deck.media` |
| 浏览视图 | 每张幻灯片以磁贴显示在所属分节中，带编号和切换效果：选择幻灯片；工具栏和右键菜单中有 New Slide、Duplicate、Hide 或 Show、Delete、Earlier、Later 和 New Section；拖动磁贴移动；重命名和删除分节 | `deck.slides_op`、`deck.view` |
| New from an Outline | 每张幻灯片一张卡片（一个标题，下面每行一个要点）、一个主题和一种幻灯片尺寸，以及默认按钮 Make Deck，第一张幻灯片有标题之前它是灰色的。Draft with AI 请用户自己的提供方起草卡片（`model.complete`，fast 档）；没有提供方时提供 Open AI Providers…（`settings.open`），Draft Outline 变灰，Try Again 再问一次，手动输入大纲始终可用。卡片的拖动手柄只在指针移到它上面时显示 | `deck.create`；宿主的 `model.complete` |
| 放映 | 幻灯片单独显示在占满整个屏幕的深色舞台上：点击 Present 时会请 Shell 让窗口进入全屏（`window.fullscreen`）；宿主做不到时（card-host、手机），舞台占满应用窗口。幻灯片按屏幕比例尽量占满宽度。一个小控制条（上一张、"4 / 12"、下一张、Notes、计时、Exit）在幻灯片下方有自己的一条带；脚本无法得知指针移动，所以它不会自动隐藏：右键菜单有 Next、Previous、Hide Notes、Hide Controls 和 Exit Show。全屏时 Esc 是 Shell 的按键，也会结束放映，窗口恢复原样。每个动画步骤显示其最终状态，不播放任何动画；为较小窗口渲染的幻灯片会按全屏尺寸重新渲染。Notes 打开显示本张备注的条带。设为自动播放的媒体剪辑在其幻灯片出现时开始播放，点击剪辑可播放或暂停，切换到其他幻灯片或 Exit 时停止 | `deck.show`、`deck.view {step}`、`deck.slide`、`deck.media`；Shell 的 `window.fullscreen` |
| 导出 | 工具栏下方的表单，标签在控件左侧：Format 弹出菜单（Presentation（.pptx）、PDF 或 Images，每张幻灯片一个 PNG），PDF 另有 Pages 弹出菜单（Slides、Notes Pages 或 Handouts）和 Slides per page（1 到 9）；全部幻灯片、当前幻灯片或一个范围；是否包含隐藏的幻灯片。Cancel 在默认按钮 Save… 左侧，范围有效之前 Save… 是灰色的。Save… 生成文件并交给 Mac 的存储面板，由它询问名称和文件夹（图片导出每张图片询问一次，Cancel 在那里停止）；表单随后关闭，状态栏说明保存了什么 | `deck.export`；Shell 的 `files.export` |

Undo 和 Redo 使用引擎的历史记录；Save 写入原生 `.deckcraft` 文件（`deck.undo`、`deck.redo`、`deck.save`、`deck.state`）。关闭有未保存更改的标签时会用提醒框询问：保存、不保存或取消。

每个页面都会处理这些状态：空的演示文稿库、正在渲染的幻灯片、损坏的演示文稿（"Couldn't Open This Deck" 及引擎给出的原因）、受密码保护的演示文稿（OctoSense 只在宿主的表单中处理密码，目前没有演示文稿用的表单，因此不打开）、引擎暂不打开的演示文稿（内含 zip，或超出服务导入防护的部分，#448；提示文字说的是 "something embedded in it"）、引擎不读取的格式（.ppt、.key、.odp、PDF）、已打开八个演示文稿、存储已满（写入时没有空间会出现提醒框，提供 Show Pictures（打开图片弹出框，手机上是图片条，可逐个删除图片）和 Go to Home（Home 的横幅提供 Remove Largest…，删除没有在标签中打开的最大演示文稿），之后状态栏显示 Storage Is Full…；太大而无法播放的剪辑在剪辑旁说明，不出现提醒框）、关闭时有未保存的更改，以及设备上没有引擎（"Quick Deck Can't Open Decks on This Device"）或引擎版本较旧（"This Version of OctoSense Is Too Old for Quick Deck"）。可能恢复的失败提供 Try Again；不会改变的拒绝提供 Close Tab 和 Remove…。

## 播放视频和音频

演示文稿中的媒体剪辑，即视频和音频对象（SERVICE.md 的 "Playback"），在应用内用运行时的 `Video` 控件（`.sources/makepad/widgets/src/video.rs`）播放，它把文件交给系统自己的播放器。应用和 deck 服务都不解码剪辑。

- **在编辑器中**，选中的剪辑下方显示它的控件（幻灯片下方没有空间时显示在上方或其框内底部）：Play；播放时可暂停或继续、停止，显示时间和声音开关（静音）。视频在画布当前缩放比例下覆盖在它的框上播放；音频只有控件，以扬声器图标开头。检查器说明剪辑在放映中如何播放（自动开始或点击开始，播放一次或在切换幻灯片前循环播放），并提供同样的 Play、Pause 和 Stop。剪辑播放时，它的边框没有控制点，对它的修改要等停止后进行（"Stop the clip first"）；切换幻灯片、改选其他对象、切换标签、浏览视图或导出都会停止播放。播放从不改变演示文稿。
- **在放映中**，设为自动播放的剪辑在其幻灯片出现时开始播放，点击剪辑可播放或暂停，切换到其他幻灯片、放映结束或 Exit 时停止（同一张幻灯片上的动画步骤不会停止它）。暂停的剪辑，以及正在播放的音频，会显示一个带时间的小提示。
- **原理**：`deck.media {doc, slide, id}`（使用形状自己的 `id`）把剪辑的字节写入存储（`.cache/media/<sha256>.<扩展名>`）；在剪辑框上创建的 `Video` 以该文件作为存储来源读取（`VideoDataSource.Filesystem`，运行时只在应用存储内解析这个路径）。同一时间只播放一个剪辑。
- **提示文字**：准备时显示 "Preparing the clip…"；deck.media 的拒绝用通俗的话说明：`storage_full:` 是 "There isn't room in Quick Deck's storage to play this clip."，`unsupported:` 表示剪辑链接到演示文稿之外的文件、内容为空，或不是 MP4、MOV、M4V、MP3、M4A、AAC 或 WAV。`Video` 控件无法播放文件时，一次会话中第一次失败显示 "Playback isn't available on this system"（与没有播放器的系统相同），本次会话中已有剪辑播放过之后则显示 "This clip couldn't be played."；检查器还会附上播放器自己的说明。

能否播放取决于运行时在该系统上的后端（`.sources/makepad/platform/src/os/`），而不是系统的名字：macOS 上是 AVFoundation；Linux 桌面上，安装了 GStreamer 库以及该剪辑所需的插件时使用 GStreamer，否则没有播放器（OctoSense 不注册软件解码器，所以控件报告 "playback session unavailable"）；Windows 上是 Media Foundation（从未为 Quick Deck 运行过）。

## 从本设备打开演示文稿

Open… 请 Shell 的文件服务把用户在宿主自己的对话框中选择的文件复制到 `imports/`。应用要先给新文件命名，因为 `files.import` 在对话框打开前就需要目标路径：`Imported presentation.pptx`，然后是 `Imported presentation 2.pptx`，依此类推。宿主会报告所选文件的显示名称，去掉 `.pptx` 后作为演示文稿的标题（"Lease 2026"；如果这个标题已被使用，则为 "Lease 2026 (2)"）。随后它经过服务的导入防护，在一个标签中打开。第一次 Save 会写入 `decks/<标题>.deckcraft` 并删除导入的文件。被拒绝时，Home 会用通俗的话说明原因，并在下方附上宿主自己的消息。

## 工作原理

所有路径都相对于应用的存储，也就是 `fs.*` 看到的文件夹；引擎也在同一个文件夹中工作。

| 路径 | 内容 |
| --- | --- |
| `library.json` | 第 2 版：每个演示文稿文件的标题、张数、大小、主题、创建/编辑/打开时间、离开时所在的幻灯片和缩放比例、封面；导入的图片；"Show notes while presenting"；正在编写的大纲 |
| `decks/*.deckcraft` | 引擎原生格式的演示文稿 |
| `imports/*.pptx` | 从本设备导入、尚未第一次保存的演示文稿 |
| `images/` | 为幻灯片导入的图片 |
| `exports/` | 正要交给存储面板的导出：`.pptx`、`.pdf`，以及每次图片导出一个装有 PNG 的文件夹。每次导出后和每次启动时清空，旧版本留下的导出也一起清除 |
| `.cache/` | 服务的渲染图和封面：最多 16 MiB、64 个文件，其中封面最多 32 个 |
| `.cache/media/` | `deck.media` 为播放写入的剪辑，以其字节的哈希命名：最多 32 MiB、8 个剪辑，最久未使用的先删除；演示文稿关闭时删除它的剪辑 |

第 1 版的演示文稿（`decks.json` 和 `decks/<id>/g<n>/deck.deckcraft`）会在第 2 版第一次运行时加入演示文稿库，以其大纲第一张幻灯片的标题命名。

给修改脚本的人的说明：

- **引擎工作在 Shell 的 UI 线程上运行（#399）。** 同一时间只有一个调用在进行（`pump()`）：当前幻灯片按画布宽度渲染（按修订号保留；一次修改只重绘其应答 `changed` 中列出的幻灯片），然后依次是幻灯片的形状、选中对象的格式、版式图片、导航栏缩略图（每个计时器周期读取一个 PNG）、浏览视图的磁贴和 Home 的封面。Shell 启动后的第一次调用可能要等引擎的字体预热完成。
- **脚本应用拿不到滚动位置。** 导航栏和浏览视图在滚动视图中绘制所有幻灯片。脚本无法滚动它，所以工具栏的上一张和下一张不会把当前幻灯片滚到视野内。一次绘制 200 张幻灯片会超出脚本单次进入的限额（200,000 条指令、64 ms），所以两者都分成每个只含几张幻灯片的区域绘制（`nav_chunk_*`、`sorter_chunk_*`）：新图片或当前幻灯片的改变只重绘它所在的区域，列表的滚动位置不变。
- **脚本应用收不到按键事件，** 持有键盘的视图除外（`GestureView` 的 `on_key`）：插入弹出框所在的层持有键盘，按 Esc 时关闭。需求中的每个快捷键都改成了按钮：Undo、Redo、Save、Present、Exit、Duplicate、Delete、前移和后移。在 Mac 上 Undo 和 Redo 改为 Edit 菜单中的命令，即 ⌘Z 和 ⇧⌘Z：Shell 把菜单项的快捷键作为命令发给应用。其他桌面没有菜单栏，所以工具栏保留它们。
- **脚本应用拿不到 TextInput 的选区。** 格式应用于整个文本框，或应用于检查器中选定的段落（Applies to）。正在编辑的文本框是覆盖在幻灯片上的文本输入框，此时引擎绘制的幻灯片不含该文本框的文字。为了让它保持放映时的样子，输入框含有每个段落的标记（项目符号或编号及其缩进，用字符表示，文字交给引擎前由 `edit_plain` 去掉）和引擎的行距（`edit_look_next`、`edit_seed`、`edit_pitch`）。它只有一种字体、一种字号，没有悬挂缩进，光标从第一个标记之前开始（没有脚本调用可以定位光标），新的一行在结束编辑后才有项目符号。表格的单元格是覆盖在单元格矩形上的同样的输入框，它假定行列等分：`deck.slide` 不报告宽度、高度和合并。
- **重绘一个区域会重建其中的控件**（`widgets/src/view.rs`），因此由其中某个控件保持的拖动会结束：在幻灯片上拖动时，轮廓画在单独的一层中，松手时才修改一次形状；状态栏会说明被拖动的磁贴将放到哪里。
- **脚本顶层把同一个名字定义两次会分裂它的作用域**（`def_scope_value`）：第二次定义之上的函数再也看不到它下面的任何名字（"variable not found"），而且报错的地方远离原因。顶层名字必须唯一。
- **`if … else` 的两个分支必须以同样的方式结束，函数中也一样：** 一个以 `for` 结束而另一个以调用结束，会使栈不平衡（"pop_stack_value on empty stack"）并结束这次回调。
- **一个生成的文件夹。** `bundle/shapes/` 是每个预设一张线条小图，来自 deckcraft d0e57d7e 版本的引擎几何数据：`cargo run --locked --release --manifest-path apps/quickdeck/tests/make_shapes/Cargo.toml -- apps/quickdeck/bundle/shapes`。
- **状态保存在 `mod.quickdeck_state` 上。** 切换浅色/深色会重新运行脚本；每个 `let` 都重新开始，`mod` 保留下来，`resume()` 用新外观重绘每个区域。
- **Label 会把自己的 margin 和 padding 计算两次**，而且 `on_render` 一开始就隐藏的视图会一直隐藏（与 PDF Tools 相同）：Label 自身不设 padding，需要隐藏的区域包在普通 View 中。
- **计算出来的键不是具名字段。** 对字面量写出的对象执行 `d[key] = v` 不会改变 `d.key`，所以脚本在需要读回的地方直接给具名字段赋值。
- 每个处理函数、计时器或回调有 64 ms 的实际耗时预算；幻灯片图片用 `fs.read_bytes` 读取，用 `binary_resource` 显示。
- **`Video` 控件不会调用脚本。** 应用每秒四次询问它在做什么（`state()`、`error()`、`current_position_ms()`、`total_duration_ms()`，来自 `Video::script_call`），用模型测试覆盖的纯函数 `clip_step` 推进剪辑的状态。应用创建控件时设 `autoplay: false`，控件存在后再调用 `begin_playback()`：在当前运行时中，以 `autoplay: true` 创建的控件会一直停在 `preparing`。控件创建时是隐藏的（视频事件仍会送到隐藏的视图），开始播放后才显示，所以在此之前显示的是幻灯片上的海报图。
- **Mac 的播放器从不报告剪辑播放结束**（Apple 后端不发送 `VideoPlaybackCompleted`）：位置停在距结尾 0.3 秒以内的剪辑即视为播放结束。重绘画布或舞台会重新创建控件；正在播放的剪辑会从原来的位置继续播放（`seek_to`）。
- **嵌套在另一个区域中的区域创建时是空的**：画布在下一轮绘制剪辑的控件，舞台在下一轮绘制它的提示。
- **`loop` 是关键字**，所以剪辑自己的字段名为 `looping`；服务返回的 `loop` 按名称读取。
- **不要把控件命名为 `bar`**（也不要用 Shell 在其窗口中按名字查找自己控件时用到的任何名字）：自 #516 起，Shell 查找自己的顶栏时会先找到 Quick Deck 的工具栏而崩溃（#519）。工具栏区域改名为 `qd_toolbar`，并有一个模型测试保证两边的名字不冲突。
- **工具栏的工具用 Label 显示名称。** 插入工具、Undo 和 Redo 是一个图标按钮上面叠一个 Label，因为 `tests/ui.py` 按 Label 查找它们。窗口宽度小于 1160 点时只显示图标。Mac 的工具栏没有 Undo 和 Redo，所以 Shell 的运行在 Mac 上改按 ⌘Z 和 ⇧⌘Z（`undo`、`redo`），在其他样式中点工具栏上的按钮。
- **导出是覆盖在幻灯片上的表单，** 所以幻灯片的各个区域仍然显示，渲染泵也继续在下面绘制它们（`t.view != "sorter"`）。
- **GestureView 会收到经过它的每一次按下，** 包括落在上方的表单、遮罩或弹出列表上的按下：它与其他控件共享按下，这样外层的滚动视图拿不走，而且只让位给自身矩形之内的控件。所以浮在上面的部分用 `press_spent()` 声明（关闭表单、提示框、列表或插入弹出框的操作；提示条；放映控制条），其下的部分先问 `press_free()`，弹出框打开时它为假。`tests/ui.py` 检查了表单的遮罩。
- **`UNIFIED` 就是共享的 `ui_unified`：** Shell 的 `unified_chrome` 列出了 Quick Deck，所以在 Mac 样式下它没有标题栏，工具栏高 52 点，红绿灯按钮在工具栏开头的 84 点之上。其他样式和 `card-host` 都保留标题栏和 40 点高的工具栏。表单的遮罩不占用这条带（`QdScrim`），窗口仍可移动；Shell 拒绝全屏时，放映的幻灯片从红绿灯按钮下方开始（`stage_band`）。
- **`enabled: false` 只阻止按压：** 按钮在调用 `set_disabled` 之前保持原有颜色，而脚本无法调用它。所以不可用的控件用自己的颜色表明状态（`PrimaryOff`，以及绘制工具时用的 `ink`）。

## 测试

演示文稿库的规则是 `main.splash` 中的纯函数，由 Rust 在脚本虚拟机中执行 bundle 的函数来测试（`crates/shell/src/quickdeck_model_tests.rs`）：导入的演示文稿的标题、演示文稿库索引在重启后保留的内容、第 1 版演示文稿变成什么、存储/卡片/备注/计时的文字、大纲的计数和问题（包括服务的上限）、大纲和模型草稿发给引擎的内容、范围对应的幻灯片、前移和后移、拒绝时的说明、引擎使用的名称、画布的适应和缩放级别、选中对象的缩放和旋转、图表数据表格的数字，以及播放：哪些形状是媒体剪辑（按形状自己的 id）、幻灯片自动开始的剪辑、deck.media 的拒绝和控件失败时的文字，以及剪辑根据控件的应答推进的每一步（播放前反复启动、开始播放后显示画面、暂停、Mac 播放器不报告的结束、重建后的控件从原来的位置继续播放）。

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
| `states` | 开发夹具中损坏、受保护、被暂缓和不支持的演示文稿，未保存更改提醒框，存储已满，以及首次运行时空的演示文稿库 |
| `shell` | 用真实引擎从大纲生成一个演示文稿，然后编辑：原地编辑文本（保留项目符号）、加粗、图表、在幻灯片上输入表格单元格、撤销和重做、版式、主题、备注、浏览视图、保存、导出（演示文稿和 PDF 都经过 `files.export`，并从 `MAKEPAD_TEST_SAVE_DIR` 读取每个文件的字节）和放映；先浅色，再通过 Shell 自己的样式菜单切到深色。浅色流程中，检查器的状态跟随真实的改动及其撤销：文本框的垂直对齐和自动调整、图表的数据标签、表格的汇总行、配色方案和页脚，每一项都重新从引擎读取（重新选中形状，或重新查询演示文稿的设计） |
| `restart` | 同一个主目录再次启动：演示文稿库、演示文稿的封面和幻灯片 |
| `playback`（`card-host`） | `make_fixture.py --media` 的 Launch Film：选中剪辑的控件，示例剪辑在其框中播放（相隔一秒的两张截图在该处必须不同，控件上的时间也要前进），暂停（时间不动）、继续、停止，切换幻灯片时停止，音频的控件，`unsupported:` 和 `storage_full:` 的文字说明，以及已有剪辑播放过之后播放器打不开的剪辑；放映：自动开始的剪辑（相隔一秒的两张截图不同）、点按暂停再点按继续、下一张幻灯片；然后在一次尚未播放过任何剪辑的会话中，同样的失败显示 "Playback isn't available on this system"；浅色与深色 |
| `playback`（Shell） | 单独的主目录，放入 `tests/media` 中的演示文稿：`deck.media` 把每个剪辑写入存储，并与剪辑自己的字节核对（`.cache/media/<sha256>.mp4` 和 `.wav`），演示文稿关闭后其剪辑离开缓存；原生演示文稿的视频在编辑器和放映中（放映中自动开始）、它的音频，以及 `.pptx` 副本的视频在放映中（只在点按后开始）：在 Mac 上每个视频都必须播放（相隔一秒的两张截图不同；音频从不播放，因为这些运行不打开任何音频设备），在 Linux 上每个都必须显示 "Playback isn't available on this system"；先浅色，后深色 |

开发夹具不会发布。`dev-fixture/engine.splash` 是 deck 引擎的替身，像服务一样应答 SERVICE.md 中的方法；`tests/ui.py` 在 bundle 的临时副本中用它替换 `engine()`。`dev-fixture/make_fixture.py` 在应用存储中填入已批准设计图中的示例演示文稿（Northwind 第三季度董事会汇报，其幻灯片用设计图自己的示例文字画成图片）、Home 上的另外五个演示文稿、版式图片，以及替身应答的内容（引擎真实的主题、带颜色的配色方案、字体搭配、形状和字体，以及 #507 报告的每个文本框、图表、表格和演示文稿的状态）；替身为演示文稿的数据保留撤销历史。`--states`、`--full` 和 `--empty` 生成其他运行使用的演示文稿库，`--media` 加入带媒体剪辑的演示文稿 Launch Film：一个设为自动播放的视频（示例剪辑 `dev-fixture/media/clip.mp4`，即仓库中 `crates/video-api-smoke/resources/` 下合成的五秒无声 H.264 剪辑）、一个链接到演示文稿之外文件的音频、一个文件不存在的视频，以及一个超出存储空间的视频；替身为每个剪辑应答 `deck.media`。`--bundle DIR --grabs-only` 为另一个 bundle（例如 main 上的，用于前后对比）截取同样的截图，但不核对状态。

```sh
python3 apps/quickdeck/dev-fixture/make_fixture.py <card-host app data>/os.quickdeck
```

Shell 的 `playback` 运行使用 `tests/media/launch-film.deckcraft`（第 1 张：示例剪辑，设为自动播放；第 2 张：一秒钟的音调，点按播放）和 `launch-film.pptx`（同一演示文稿保存为 PowerPoint，所以其剪辑只在点按后播放）。deck 服务暂缓了引擎的 `insert.video`、`insert.audio` 和 `media.options`（#448），所以 `tests/make_media_decks.py` 用固定版本的 deckcraft（d0e57d7e）自带的命令行工具生成它们：

```sh
cargo build --locked --release -p deckcraft-cli   # 在 d0e57d7e 版本的 deckcraft 检出中
python3 apps/quickdeck/tests/make_media_decks.py --cli <deckcraft-cli> --out apps/quickdeck/tests/media
```

## 状态

第 2 版的截图、并排对比和回执见 [tests/evidence/v2-20261011](tests/evidence/v2-20261011/README.zh-CN.md)，检查器状态（#507）的见 [tests/evidence/inspector-states-20261011](tests/evidence/inspector-states-20261011/README.zh-CN.md)，播放的见 [tests/evidence/playback-20261013](tests/evidence/playback-20261013/README.zh-CN.md)。

- **2026 年 10 月 11 日在 `card-host` 中验证**（macOS，Apple 芯片，运行时默认的 64 ms 预算）：`missing`、`fixture` 和 `states` 三种运行，浅色与深色，日志中没有脚本错误。
- **2026 年 10 月 11 日用真实引擎验证**（Linux，运行在无头 weston 上的隐藏桌面 Shell，由基于 main 202d7cc8 的本分支构建，main 已包含 #503 的 deck 服务）：`shell` 运行（先浅色后深色）和 `restart`，使用运行时默认的 64 ms 预算，没有脚本错误也没有超时。Shell 以 1296 x 698 点打开 Quick Deck（应用区域 1292 x 662）。模型测试在那里通过（22 个；加 `--features app-hub` 时 23 个，多出的是清单的窗口提示）。
- **2026 年 10 月 11 日验证检查器的状态**（#507）：在 `card-host` 中，`fixture` 和 `states` 运行，浅色与深色，64 ms 预算，开关和标记与替身核对；在 Linux 上用真实引擎（Shell 由基于 main eb2fd5d1 的本分支构建，main 已包含 #507），`shell` 运行的浅色流程改动了文本框的垂直对齐和自动调整、图表的数据标签、表格的汇总行、配色方案和页脚，每一项都从引擎读回，再逐一撤销并读回演示文稿自己的设置；然后是深色流程和 `restart`。模型测试通过（27 个）。
- **2026 年 10 月 13 日验证播放**：在 `card-host` 中（macOS，Apple 芯片；其运行时由本检出构建），`playback` 运行，浅色与深色：示例剪辑在其框中播放（相隔一秒的两张截图在该处不同，13,850 个像素，控件上的时间也在前进），Pause 让它停住，Play 继续播放，Stop 和切换幻灯片使它停止，各种拒绝都有文字说明；放映中自动开始的剪辑在播放（一秒内 33,418 个像素发生变化），点按暂停，再点按继续；然后在一次尚未播放过任何剪辑的会话中显示 "Playback isn't available on this system"（在 Mac 上，针对 AVFoundation 打不开的剪辑）；同一次会话中 `missing`、`fixture` 和 `states` 也都通过。用真实引擎（由基于 main ffd5e488 的本分支构建的隐藏 Shell，main 已包含 `deck.media`），先浅色后深色：在 macOS 上剪辑都能播放（编辑器中的视频、放映中自动开始的视频及其点按暂停和继续，以及 `.pptx` 副本中点按播放的视频）；在 Linux 上每个剪辑都显示 "Playback isn't available on this system"（主机上的 GStreamer 没有 MP4 解复用器和解码器），`shell` 和 `restart` 照旧通过。两边的 `deck.media` 都把每个剪辑以其自身的字节写为 `.cache/media/<sha256>.<扩展名>`，关闭演示文稿后它们被删除，并且每一步之后 Quick Deck 都保持打开。模型测试通过（35 个；加 `--features app-hub` 时 36 个）。
- **2026 年 11 月 2 日验证 Mac 外观**（macOS，Apple 芯片）：在隐藏运行的 Shell 中（本分支的 release 构建，每次运行首次启动解包后换入当前 bundle；`--budget-ms 5000`，因为多个隐藏 Shell 共用这台机器），用真实引擎和 Mac 的播放器完成 Shell 的 `shell`、`restart` 和 `playback` 运行，先浅色后深色；以及 `card-host`（2026 年 10 月 9 日的构建）的 `fixture`、`states` 和 `missing`。此外，在隐藏运行的 Shell 中以浅色和深色截取并检查了本文列出的页面和状态（已打开八个演示文稿和引擎版本较旧这两种除外），分别使用真实引擎和开发夹具，每个等待状态都保持到足以截图，并把窗口拖到 1100、900 和 700 点宽。这次没有运行模型测试；外观可能影响到的表达式（画布适配、缩放步进、剪辑控件、拒绝提示文字）已在 bundle 自己的虚拟机中求值核对。
- **2026 年 11 月 3 日验证统一窗口和按下的去向**（macOS，Apple 芯片；本分支的 release 构建，每次运行首次启动解包后换入当前 bundle）：用运行时默认的 64 ms 预算，在真实引擎和 Mac 的播放器上完成 Shell 的 `shell`、`restart` 和 `playback` 运行，并以 64 ms 完成 `card-host`（2026 年 10 月 9 日的构建）的 `missing`、`fixture` 和 `states`，日志中没有脚本错误。此外，在隐藏运行的 Shell 中以 1000 ms 预算、浅色和深色，在 1296 x 719 点和约 700 点下截取并检查了本文列出的页面和状态，使用真实引擎；只有引擎变慢或缺失时才出现的等待和拒绝状态用变慢或缺失的引擎。落在表单、遮罩、演示文稿卡片上方的提示框遮罩、幻灯片上方的浮动检查器文字以及放映控制条计数上的按下都停留在原处。这一轮最后的修改之后没有重新运行模型测试。
- **2026 年 11 月 3 日验证第二次评审带来的修改**（macOS，Apple 芯片；本分支的 release 构建，每次运行首次解包后换入当前 bundle）：用运行时默认的 64 ms 预算，在真实引擎和 Mac 的播放器上完成 Shell 的 `shell`、`restart` 和 `playback` 运行，以及 `card-host` 的 `missing`、`fixture`、`states` 和 `playback` 运行，日志中没有脚本错误。此外，在隐藏运行的 Shell 中以 1000 ms 预算、浅色和深色，在 1296 x 719 点和约 700 点下截取并检查了本文列出的页面和状态：Font 菜单及其表单、形状小图、保留项目符号并按引擎行距编辑的文本框、在幻灯片上输入的表格单元格、幻灯片、对象、卡片、标签和舞台上的右键菜单、存储提醒框，以及有无 Recent 时大小相同的 Home 主题缩略图。在隐藏窗口中，先发送 `/w?k=focus` 后，`files.export` 把 `.pptx`、`.pdf` 和一个演示文稿的图片保存到 `MAKEPAD_TEST_SAVE_DIR`，核对了每个文件开头的字节，`exports/` 中没有留下任何文件。一个 200 张幻灯片的演示文稿在 64 ms 预算下没有脚本错误：导航栏滚动到第 200 张，打开它后列表位置不变，浏览视图也正常。拖动大纲卡片的手柄可以移动卡片。这一轮没有运行模型测试；其中三处断言写的是这一轮修改过的文字。
- **未验证**：从本设备真正导入文件（macOS 上隐藏窗口的对话框会被拒绝，Linux 主机没有对话框助手），因此也未验证导入防护经由应用显示的拒绝；屏幕上真正的 Mac 存储面板（隐藏窗口由一个文件夹应答）；用真实提供方的 Draft with AI（只验证了没有提供方时的回答）；用真实引擎时长备注的放映备注条（该页面由开发夹具覆盖）；其他屏幕尺寸；手机。播放方面：在任何系统上带声音输出的音频播放（这些运行不打开任何音频设备）；Windows；循环播放的剪辑；剪辑播放时切换浅色/深色。
- **已知问题**：Shell 的窗口小于设计图的 1536 x 1024，所以检查器中较长的面板和大纲页面需要滚动。主题预览用应用自己的字体绘制（OctoSense 不附带主题字体）。正在编辑的文本框只有一种字体和一种字号，没有悬挂缩进，新的一行在结束编辑后才有项目符号。表格单元格的输入框假定行列等分（`deck.slide` 不报告宽度、高度和合并），所以在行列不等分的导入表格上会有偏差。脚本应用收不到指针移动，所以放映控制条不会自动隐藏（用右键菜单的 Hide Controls）。图片导出每张图片询问一次（`files.export` 一次接受一个文件）。内置主题自己的字体搭配以主题命名，所以标记的是字体相同的那组搭配（Harbor 的字体与 Classic 相同）；导入的演示文稿自己的配色或字体会在提示中写明名称。`.pptx` 中的剪辑只在点击后播放：d0e57d7e 版本的 deckcraft 不从 PowerPoint 文件读取自动播放和循环设置（SERVICE.md 的 "As built in #515"），所以只有原生演示文稿中的剪辑会自动开始。编组中的剪辑无法选中或播放（`deck.slide` 只列出顶层形状）。同一时间只播放一个剪辑；一张幻灯片上有两个设为自动播放的剪辑时，只开始第一个。无法添加或修改剪辑：`insert.video`、`insert.audio` 和 `media.options` 仍被暂缓（#448）。在一次尚未播放过任何剪辑的 Mac 会话中，AVFoundation 打不开的剪辑会显示 "Playback isn't available on this system"，因为运行时用其软件解码器的错误来回应这次失败。按 ⌘W 或 ⌘Q（两者都是关闭窗口）、点窗口的关闭按钮，或在顶栏的窗口标题上点右键或中键，Shell 会立即关闭 Quick Deck，即使正在放映或有未保存的修改：Splash 应用无法先询问，而且 Escape 不能退出放映（脚本应用收不到按键事件）。
