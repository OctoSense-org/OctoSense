# OctoSense 系统应用

[English](README.md) | 简体中文

**初次阅读源码**？先读[桌面、Home、ROM 与系统应用导读](../desktop/docs/code-walkthrough.zh-CN.md)，再读 [Agent 与 Tokio 导读](../docs/architecture-walkthrough.zh-CN.md)。前者追踪启动、原生托管、脚本 bundle、应用数据和 Android 平台边界。

> **在整个系统中的位置**。系统应用在 App Hub 的 Card runner 中运行。新闻、邮件、日历、相册、地图、YouTube 和相机声明应用 Agent；AI providers 配置宿主，自身不声明 Agent，Writer 也不声明。Shell 为每个启用的应用/账户提供 peer，替系统 Agent、“Ask <app>” 面板和卡内聊天驱动对话。声明的工具经过 Shell 的 relay 和审批路由进入宿主服务；只暴露 `<namespace>.notify` 的应用由 Shell 共用通知服务处理。具体工具见[应用 Agent](#应用-agent)，两条通道和信任边界见[架构](../docs/architecture.zh-CN.md)。Glance 接受 L0 和 Splash 卡片，按发布应用的策略运行。

[OctoSense](https://github.com/OctoSense-org/.github/blob/main/profile/README.zh-CN.md)（运行在操作系统之上的 Agent 交互 Shell）自带的第一方应用，以及它们背后的宿主服务。
它们位于 [OctoSense 仓库](../README.zh-CN.md)的 `apps/`；2026-09-27 之前它们是
OctoSense-System-Apps 仓库（已归档）。

- **新闻（News）、相册（Photos）、地图（Maps）、相机（Camera）、邮件（Mail）、日历（Calendar）、AI providers、YouTube、Quick Deck、PDF Tools 和 Writer**
  是*隔离运行的脚本应用*。每个应用都是 `bundle/` 里的一个 Makepad Script/Splash
  程序，由 App Hub 的 Card runner 在独立的 isolate 中运行，权限由其
  `manifest.json` 经过准入后确定，与商店应用受到的隔离完全相同。它们同时也是
  任何开发者通过 App Hub 发布的应用形态的完整示例。
- **邮件的宿主服务**（`mail/host-service`）是 Mail 的 Rust 部分：
  IMAP/POP3/SMTP、账户存储和登录面板，由 Shell 运行。应用拿到的是邮件，
  永远拿不到密码或 socket。它还运行邮件 Agent 的账户绑定读取/同步工具，以及 `mail.notify`/`mail.publish_card`，把卡片
  放到 glance 屏幕上。
- **日历的宿主服务**（`calendar/host-service`）把日历的日程保存在宿主目录中，并运行日历
  Agent 的工具：`calendar.events`、`add_event`、`remove_event`，以及把日程卡片或议程卡片
  放到 glance 屏幕上的 `notify` 和 `agenda`。日历同时随桌面与手机 Shell 发布
  （`desktop/system-apps.json` 与 `phone/system-apps.json`）。
- **新闻的宿主服务**（`news/host-service`）按定时器收集新闻条目，不使用模型，并运行新闻
  Agent 的 `news.list`、`news.read` 和 `news.notify`（通知由 Shell 绘制）。
- **word 引擎服务**（`word/host-service`，ADR 0013）把 wordcraft 文档引擎放在类型化的 `word.*` 方法后面：文档信息、纯文本提取、结构检查、docx、md、html、rtf、odt、txt 与 pdf 之间的转换，以及写出一个最小新文档，全部限制在调用方自己的文件夹内。它的应用是 Writer（`writer/bundle`，仅桌面端）；Writer 的调用在 Writer 自己的存储中进行。
- **`llm` 宿主服务**（`ai-providers/host-service`）是 AI providers 的 Rust
  部分：基于 octos 模型目录的大模型服务商、存放在平台密钥库中的密钥、“测试连接”，
  以及通过受 PIN 保护的 `OCTOS1E` 二维码在设备之间迁移服务商（相机、图片或粘贴）。
  密钥只在宿主自己的面板上输入，二维码也只在那里显示；应用只能看到打码后的状态。
- **`model` 媒体 API** 使用宿主持有的供应商配置，执行有上限的图片、语音、向量和异步
  视频请求。应用声明 `model` 和精确宿主 API 版本；调用时检查供应商权益。见
  [媒体契约与验证范围](ai-providers/host-service/MEDIA.zh-CN.md)。
- **`deck` 宿主服务**（`deck/host-service`，ADR 0013）把 deckcraft 演示文稿引擎放在 `deck.*` 之后：检查幻灯片、提取大纲文本、渲染 PNG、按标题和要点新建演示文稿，以及格式转换（pptx、原生格式、大纲、PDF），全部限制在调用方自己的文件夹内。它的应用是 Quick Deck（`quickdeck/bundle`），其调用在 Quick Deck 自己的存储中运行。
- **`cad` 引擎服务**（`cad/host-service`）：cadcraft 制图引擎，提供 `cad.*` 方法（ADR 0013，暂无 bundle）——在调用方自己的文件夹内检查、查询、测量、渲染和转换 DXF/DWG 图纸。
- **`light` 引擎服务**（`light/host-service`）把 lightcraft 的 RAW 显影引擎
  放在 `light.*` 方法之后（ADR 0013）：EXIF/XMP 元数据、显影控制目录、单张与
  批量参数化显影；仅服务系统应用，且只在调用方自己的文件夹内读写。
  暂无 bundle。
- **`sound` 宿主服务**（`sound/host-service`）：soundcraft 音频引擎置于 `sound.*` 之后（ADR 0013），只做离线文件处理——info、convert、trim、mix 和波形峰值，数据都在调用方自己的文件夹内；它从不打开音频或 MIDI 设备，暂时也没有应用包。
- **`design` 宿主服务**（`design/host-service`）把 designcraft 排版引擎（ADR 0013）放在 `design.*` 后面：文档信息、页面渲染为 PNG、导出 PDF/IDML/EPUB，所有路径都限制在调用方自己的文件夹内；暂无应用包。
- **`film` 宿主服务**（`film/host-service`）是锁定版本的 filmcraft 视频引擎
  （ADR 0013），以 `film.*` 提供探测、抽帧为 PNG 和有上限的导出，全部使用其
  纯 Rust 编解码器——无界面、离线，暂无应用包。
- **`effect` 宿主服务**（`effect/host-service`）是 effectcraft 动态图形引擎
  （ADR 0013），通过 `effect.*` 方法服务系统应用：工程信息、把合成帧渲染为
  PNG、引擎命令目录以及 Lottie 导入/导出，所有文件都在调用方自己的文件夹
  内；尚无应用包。
- **矢量引擎服务**（`vector/host-service`，`octosense-vector-service`）以无界面方式
  运行固定版本的 vectorcraft 矢量引擎，提供 `vector.*` 方法（ADR 0013）：在调用方
  自己的文件夹内检查、转换、按引擎命令绘制并渲染矢量文档
  （SVG、PDF、EPS、DXF、位图）；仅服务系统应用，暂无应用包。
- **AppCard**（`appcard`）是可选的原生应用：“Ask anything”助手，
  一个由 Shell 进程内链接的 Rust 模块（`octos-app`），运行在 Shell 的
  octos 内核之上。它**需显式启用**：两个 Shell 只有在使用 `--features app-appcard`
  时才链接它，默认不随产品发布。
- **Reference**（`reference`）：Shell 通过 `app-reference` 链接的 Rust
  模块（手机上始终链接）。Shell 链接的所有原生应用都在
  [`../native-apps.json`](../native-apps.json) 中声明。News、Photos 和 Maps
  只有脚本应用版本：早期的原生模块已删除（原生应用 ADR 0004 §1，[#113](https://github.com/OctoSense-org/OctoSense/pull/113)）。Home 挂载的
  Photos 示例图库位于 `photos/resources/`。

这些应用依赖的 Shell 服务就在旁边：
[`../crates/kernel`](../crates/kernel)（octos 内核服务，见 [octos 内核](#octos-内核)）和
[`../crates/app-peers`](../crates/app-peers)（应用访问助手的通道）。

在这里工作的 Agent 规则见 [AGENTS.md](AGENTS.md) 和
[appcard/AGENTS.md](appcard/AGENTS.md)，它们在仓库根目录的
[AGENTS.md](../AGENTS.md) 基础上补充。

**要开发自己的应用？** 不需要构建或修改本仓库。请从 [OctoSense-org 主页](https://github.com/OctoSense-org)的阅读列表开始：先读 OctoSense App Flow（原 Design Flow）的 `AGENTS.md`，再读它的 `docs/QUICKSTART.md`。把这里的应用包当作完整示例来读（`apps/<name>/bundle/main.splash`）。想在自己的应用旁边运行其中一个：把 OctoSense 仓库克隆到同一个工作区，然后在 App Flow 检出目录中执行 `tools/octo run ../OctoSense/apps/photos/bundle --system --no-stamp --app-data /tmp/sys-apps`（`--no-stamp` 不会改动检出目录；Mail 需要在 Shell 中运行，见下文）。

## 应用一览

### 共用外观

新闻、相册、邮件、日历、地图、AI providers、YouTube、PDF Tools 和 Writer 共用
[`interface.splash`](interface.splash)：页面、卡片和输入框跟随主题，
辅助文字保持可读，操作按钮高 44 点，输入框高 48 点。桌面内容限制最大宽度，
窄窗口保留相同操作。相机和媒体查看器保留深色观看背景。应用大厅的原生模块
采用相同的间距和操作层级。展开的 Glance 卡片及卡片内聊天在桌面和手机端
使用同一套 Shell。

键盘缩小当前应用的可视区域时，Shell 通过应用自己的滚动区域显示当前输入框，
固定操作栏保留原位。地图图块随宿主切换明暗外观。相机按钮使用深色底板，
在明亮取景画面上仍可阅读；Android 等待首次权限弹窗的结果后才准备预览。
权限弹窗只能暂停正在播放的视频，不能将尚未初始化的相机在返回后误标为正在播放。
预览按布局完成后的实际显示尺寸等比填充，支持 `Fill` 和窗口缩放；
旋转后的相机画面裁切边缘，不再拉伸。AI providers 的 Android 扫码器
先选择相机声明支持的预览/YUV 输出尺寸，再打开相机；显示时保持比例，
旧扫码会话的回调不会关闭新打开的扫码器。

修改后在仓库根目录运行 `python3 tools/sync-app-interface.py`，将共用样式
原样嵌入各个独立应用包；CI 通过 `python3 tools/sync-app-interface.py --check`
检查是否同步。样式重载时读取当前宿主主题，同时保留未保存的输入。
[本机原生 UX 检查](../tools/app-ux/README.zh-CN.md) 在独立配置中验证编辑、
持久化、主题切换、浏览和搜索。手机尺寸的桌面截图不代表真机验证。

| 应用 | Id | 功能 | 权限（manifest） | 网络主机（manifest） | 宿主服务 |
| --- | --- | --- | --- | --- | --- |
| [News](news/bundle) | `os.news` | Hacker News、TechMeme 和 Google News 的订阅源，分标签页（Today、HN、TechMeme、Google、Saved），带文章阅读器 | `storage`、`net`、`images`、`web`、`news`、`glance` | `hn.algolia.com`、`www.techmeme.com`、`news.google.com`、`api.gdeltproject.org`、`feeds.bbci.co.uk`、`feeds.npr.org`、`www.theguardian.com`、`feeds.arstechnica.com` | [`news`](news/host-service) |
| [Photos](photos/bundle) | `os.photos` | 示例相册：AI 整理的回忆、可选主题提示、保存的故事和幻灯片；本地回忆、相簿、人物、收藏、可多选的网格、全屏查看器 | `storage`、`glance`、`model` | 无（宿主调用模型） | `model.complete`；桌面上用自己的 `photos` 服务：`photos.notify` 经 Shell 的通知回调、`photos.info` 在照片引擎上；手机不带该引擎，由 Shell 通知服务应答 `photos.notify`，`photos.info` 不可用（原图使用资源挂载） |
| [Maps](maps/bundle) | `os.maps` | 随时可拖动和缩放的 `MapView` 地点地图：按可见区域搜索、带 OpenStreetMap 详情（营业时间、电话、网站、菜系）的地点卡片、以图钉显示的收藏地点、长按查看“这里是什么”、可更改起点并最多添加两个途经点的路线，以及带逐向导航和 2D/3D 视图的驾驶模式；有 GPS 定位时从当前位置开始；浏览地图使用 makepad 预先烘焙的世界地图（`makepad.nl`），驾驶地图和地点详情通过 Overpass 读取 OpenStreetMap | `storage`、`net`、`location`、`web`、`glance` | `photon.komoot.io`、`router.project-osrm.org`、`overpass-api.de`、`overpass.kumi.systems`、`maps.mail.ru`、`overpass.openstreetmap.fr`、`makepad.nl` | Shell 通知服务的 `maps.notify` |
| [Camera](camera/bundle) | `os.camera`（Home） | 基于运行时 `CameraPreview` 控件的拍照和录像，闪光灯和变焦，最近一张的缩略图和查看器 | `storage`、`camera`、`microphone`、`library`、`glance` | 无 | Shell 通知服务的 `camera.notify` |
| [Mail](mail/bundle) | `os.mail` | 账户、文件夹、邮件列表、阅读（HTML 由服务重建）和写信；它的 Agent 把通知卡片放到 glance 屏幕上（`mail.notify`） | `storage`、`mail`、`glance` | 无（由服务联网，而不是应用） | [`mail`](mail/host-service) |
| [AI providers](ai-providers/bundle) | `os.ai-providers` | 助手的大模型服务商：一个主用与若干备用，每项都有来自 octos 模型目录的型号下拉菜单和“测试连接”；添加向导（系列、型号、线路、密钥、测试）；“为手机显示二维码”，以及通过相机、图片或粘贴导入 | `storage`、`llm` | 无（由服务联网，而不是应用） | [`llm`](ai-providers/host-service) |
| [YouTube](youtube/bundle) | `os.youtube` | YouTube 搜索（运行时无需密钥的 `sys.video`，读取 YouTube 自己的搜索结果页），带缩略图和直播或时长角标的结果列表、话题标签，在 `WebReader` 中播放 YouTube 移动版观看页，以及本机播放记录 | `storage`、`net`、`glance` | `www.youtube.com`、`m.youtube.com`、`i.ytimg.com` | Shell 通知服务的 `youtube.notify` |
| [Calendar](calendar/bundle) | `os.calendar` | 月历、按日列表、日程详情与编辑器；Glance 使用应用自有卡片，并能打开已保存日程 | `calendar`、`glance` | 无 | [`calendar`](calendar/host-service)（日历持有执行器；跨应用工具需授权） |
| [Quick Deck](quickdeck/bundle) | `os.quickdeck`（桌面） | 分四步把大纲变成演示文稿：写幻灯片（每张一个标题和若干要点）、生成、逐张检查（缩略图网格，以及带缩略图条的单张视图）、导出 PowerPoint 或 PDF；保留自己的演示文稿列表 | `storage`、`deck` | 无 | [`deck`](deck/host-service)：`new`、`info`、`render`、`convert`，在 Quick Deck 自己的存储中运行 |
| [PDF Tools](pdftools/bundle) | `os.pdftools`（桌面） | 管理自己存储中的 PDF：带首页预览的资料库，带页面缩略图、可查找的文字和信息的文档视图；单页视图；三步合并（选择、排序、完成），以及按固定页数或在指定页面处拆分；从设备打开 PDF（最大 64 MiB）和移除 PDF。见[它的 README](pdftools/README.zh-CN.md)，其中介绍了测试它的隐藏 Shell 流程 | `storage`、`files`、`pdf` | 无 | 在应用自己的存储中工作的 [`pdf`](pdf/host-service) 引擎服务：`pdf.info`、`pdf.render`、`pdf.text`、`pdf.merge`、`pdf.split`；Shell 的 `files.status` 和 `files.import` |
| [Writer](writer/bundle) | `os.writer`（桌面端） | 白纸上的安静编辑器（以 `#` 开头的行是标题，`-` 是列表项，`>` 是引用）、文档列表、保存为 Word 文档、预览已保存的文档并按其标题生成大纲，以及导出为 PDF、Markdown、HTML 或 OpenDocument；草稿自动保存在应用自己的存储中 | `storage`、`word` | 无 | [`word`](word/host-service)（`word.convert`、`word.info`、`word.inspect`），在 Writer 自己的存储中 |
| [AppCard](appcard) | 原生，需显式启用 | AppCard 助手：路由大脑选择或组合一个应用 Agent，由它生成实时的 Splash 或 webview 卡片。Shell 只在启用 `app-appcard` 时链接它；默认不发布 | 不适用（不是 bundle） | 不适用 | Shell 的 octos 内核 |

能力名称由共享的 `octosense-app-contract` 1.x crate 定义（App Hub 的
`crates/app-contract/src/manifest.rs` 中的 `KNOWN_CAPABILITIES`），用于描述图片、
WebView 页面、图库导出、邮件、新闻、Glance 或单次模型调用等用途。`net` 和
`network.hosts` 披露联网用途，不限制公开 API 的执行。声明不会授予设备同意、账户访问
或跨应用工具权限；拍摄/导出意图必须明确指定，`llm` 服务商管理仍仅限系统应用。
见[能力与执行边界](../docs/capabilities.zh-CN.md)。

### 状态与已知问题

- **YouTube**：在 OnePlus 6 上测试（2026-09-27），搜索、结果、播放和播放记录都正常；
  关闭播放器会结束页面（makepad#43，已在运行时中）。播放打开的是 YouTube 移动版观看页，
  它会静音自动播放，并显示自己的“Open App”提示。搜索读取 YouTube 的搜索结果页，依赖其布局。
- **Camera**：Redmi Note 12 独立测试包（2026-10-07）已通过延迟首次授权、
  前后镜头切换、Photo/Video 预览模式切换和后台恢复。`Fill` 预览拉伸已修正，
  原生 GPU 的圆形尺寸对比通过；手机明亮场景的外观对比仍待验证。
  2026-09-25 报告的 OnePlus 6 黑屏尚未复测。本轮未测试拍照；Android 录像仍未实现。
- **AI providers 相机导入**：Redmi Note 12 扫码器使用设备支持的预览尺寸，
  已通过打开、取消、系统返回、后台中断和重新打开；本轮未测试二维码解码及服务商导入。
- **Photos 回忆**：打开 **Memories → Create memories**，可选输入“summer with family”等主题。
  宿主的 `model.complete` 使用 **设置 → AI providers** 中配置的服务商，整理最多三个
  带标题、简短叙述和有序幻灯片的故事。只发送相片目录的元数据（日期、地点、姓名、
  标签、标题和收藏标记），不发送图像数据或凭据。本版本使用示例目录，不导入设备相册，
  也不分析图像像素。只有用户点击时才生成。最近 12 个故事保存在
  交替写入的 `accounts/device/memories.json` 和 `memories-backup.json` 快照中，
  与相簿和收藏分开；写入失败时仍可恢复上一份快照。
  没有 AI 时仍可浏览本地时刻和已保存的故事。请求失败、无效相片 ID、未配置服务商
  或预算限制不会覆盖已有故事。模型服务为服务商尝试留出 270 秒；
  Photos 会在超时后清除加载状态并允许重试，另设 300 秒后备超时以应对没有回调的情况。
  **Stop waiting** 会忽略迟到的回复；宿主请求仍可能
  完成并计入预算。独立的 `card-host` 不提供模型服务。
- **Photos 图像**：bundle 只带 75 张缩略图（`bundle/thumbs/`，约 2 MB）。主屏幕预览
  保持三列，根据卡片的可用宽高显示一、二或三行，并优先显示收藏。照片铺满卡片，
  标题叠加在左下角，不再单独占用顶部空间。卡片尺寸变化
  或重新加载时，Shell 的进程内卡片宿主会在应用自己的 isolate 中调用可选的
  `on_app_resize(width, height)` 回调，并在绘制主屏幕预览帧之前完成队列中的 UI 更新。查看器
  显示的原图只有在 Shell 挂载后才会出现在 `{{assets}}/photos/...`：Home
  挂载 `photos/resources/photos`（约 87 MB，见 `phone/system-apps.json`）；
  桌面端不挂载任何目录（`desktop/system-apps.json`），所以那里的查看器没有原图，
  会保留缩略图作为后备。回忆功能在实体手机及真实 AI 服务商上均**未验证**。
- **Maps**：在 Pixel 7 Pro 上（2026-10-08，深色模式）地点地图运行正常：打开时位于
  GPS 定位处，◎ 能飞回该处；地图可拖动，也可双指缩放；按可见地图搜索、收藏与最近、
  带 OpenStreetMap 详情的地点卡片及其网站阅读器、收藏与图钉、长按、路线只取景一次
  之后由使用者掌控、‹ Back 和 Close 移除路线，以及驾驶视图和 End 都正常。3D 驾驶
  视图在路线附近画出了地图瓦片，30 秒后仍有部分缺失。改为地点地图之前，在 OnePlus 6
  上（2026-09-27）搜索、地点详情、路线、添加和移除途经点、逐向导航驾驶以及 2D 视图
  都正常；3D 驾驶视图会画出路线但没有地图瓦片，手机和桌面上都是如此，途经点改动前后
  一样。OnePlus 6T、iOS 和 OpenHarmony **未验证**。
- **News**：开发时在 `card-host` 中运行过，但在 Shell PR 的测试中没有
  端到端验证（测试手机没有网络）。
- **Mail**：已在桌面和 OnePlus 6 上用演示邮箱验证。Mail 与 `llm` 两个宿主服务使用根目录
  `Cargo.toml` 选定的 App Hub 版本，与 Shell 共用，
  因此一次构建中只有一份 `octosense-appstore` 和一个宿主服务注册表。
  Manifest 与策略验证使用共享且按版本依赖的 `octosense-app-contract` crate。
- **脚本 bundle 的检查尚不全面。** [`apps.yml`](../.github/workflows/apps.yml)
  通过 `cargo test --locked -p octosense-llm-service` 运行 Photos 的 app-contract 准入、Splash 回忆逻辑
  与模型响应结构检查；News 也有脚本测试。尚未为所有 bundle 提供完整的自动 UI 检查。
- **AppCard 的 `personal-data` 技能**读取旧原生 Mail 模块的 `mailbox-*.json`
  文件。脚本版 Mail 的邮件现在存放在宿主服务自己的目录
  （`<host_dir>/mail/box-*.json`），该技能大概率已读不到；未验证。
- **日历**（2026-10-01，桌面端，隐藏窗口的 `--remote` 运行，接真实模型）：系统 Agent
  请日历的 Agent 放一张卡片；首次使用面板弹出，Agent 添加了一个日程，它的卡片打开了
  glance 面板。手机上没有打包。它的宿主服务测试还不在 `apps.yml` 中。
- **Quick Deck**（2026-10-09，macOS，隐藏窗口，浅色与深色）：在用其分支构建的桌面
  Shell 中端到端运行。输入的大纲通过 `deck.new`、`deck.info` 和每张幻灯片一次
  `deck.render` 变成演示文稿，全部写入 Quick Deck 自己的存储（`decks/<id>/g<n>/`）；
  检查网格和单张视图显示引擎生成的 PNG，PowerPoint 和 PDF 导出也保存在旁边。
  拒绝都会在应用中显示：存储几乎已满、引擎无法读取的演示文稿文件、存储之外的路径。
  切换浅色或深色时，应用停留在用户所在的页面。`deck` 服务运行在 Shell 的 UI 线程上
  （#399），所以它工作时 Shell 会停顿：五张幻灯片约 3 秒，会话中的第一次调用 16 秒。
  每个页面和状态（包括各种拒绝）也由 `quickdeck/tests/ui.py` 用下方的开发夹具在
  `card-host` 中覆盖。
- **Writer**（2026-10-09）。在用本仓库构建的隐藏桌面 Shell 中（`MAKEPAD_WM_TEST_APP=writer`，
  隐藏窗口的 `--remote` 运行，浅色与深色），使用在 Writer 自己存储中工作的真实 `word` 引擎：
  通过 instrument 输入的文档被保存为 Word 文档（`word.convert` 转换 Writer 写到 `work/` 的
  Markdown），在预览中重新打开（`word.info`、`word.inspect`），编辑后覆盖保存当前文档并再次打开，
  并导出为 PDF、Markdown、HTML 和 OpenDocument（`exports/`）。损坏的 DOCX 显示了引擎的拒绝信息，
  丢失的 DOCX 会在预览前重新保存。在浅色与深色之间切换后，打开的文档和它的预览都保留，
  切换后输入的文字也被保留并保存。`card-host` 没有 `word` 服务，保存和预览在那里回答
  “Writer can't … here: this device has no word engine”并保留草稿；`writer/tests/ui.py` 在那里以浅色和深色运行
  每个界面（见下文的 Writer 的测试）。Writer 只在桌面端，所以没有手机上的运行。
- Camera 和 AI providers 自带 PNG 启动器图案，YouTube、Quick Deck 和 Writer 自带 SVG 图案；Shell 按当前平台风格统一绘制外形。

## Shell 如何打包它们

本仓库中的两个 Shell 都内置系统应用：桌面端（[`../desktop`](../desktop/README.zh-CN.md)）
和 Home（[`../phone`](../phone/README.zh-CN.md)，独立启动器与 ROM 镜像）。每种打包形态：

1. 在各自的 `system-apps.json`（`desktop/system-apps.json`、`phone/system-apps.json`）
   中列出应用，通过 `OCTOSENSE_SYSTEM_APPS` 找到它：根目录的 `.cargo/config.toml`
   指向桌面端的文件，`phone/.cargo/config.toml` 指向手机端的文件（因此手机构建要在
   `phone/` 中运行）。App Hub 的 Shell crate `octosense-app-hub-app` 在构建时读取该文件，
   把每个 `apps/<name>/bundle/` 打包进二进制并填入摘要。`assets` 把额外目录挂载到
   应用的 `{{assets}}` 下（手机上的 Photos）：

   ```json
   {
     "schema": 1,
     "source": "../apps",
     "apps": ["news", "photos", "maps", "camera", "mail", "ai-providers"],
     "assets": { "photos": { "photos": "../apps/photos/resources/photos" } }
   }
   ```

2. 通过 Shell（[`crates/shell`](../crates/shell) 的 `app-hub` 特性）链接宿主服务
   `octosense-mail-service`、`octosense-calendar-service`、`octosense-news-service`、
   `octosense-llm-service`，以及 sheet 引擎的 `octosense-sheets-service`（workspace 内的
   path 依赖；photo 引擎的 `octosense-photo-service` 随仅限桌面的 `craft-engines` 引入，
   按引擎权衡见 [ADR 0013](../docs/adr/0013-craft-engines-as-pinned-services.zh-CN.md)），并在启动时注册
   （`crates/shell/src/apps.rs` 的 `register_host_services`）：Mail 服务在真实账户下用
   `register()`，Shell 的应用配置中 `mail_demo: true` 时用 `register_demo()`。
   Mail、News 以及（带 photo 引擎时）Photos 安装 `on_notify` 回调，调用 Shell 共用通知渲染器；Calendar 安装卡片发布器。
   Shell 为其余系统应用命名空间注册 `NoticeService`；`llm` 服务使用 octos 内核的 core 目录以及
   Shell 的二维码扫描器和图片选择器（见 [`llm` 服务](#llm-服务)）。App Hub 只在根目录
   `Cargo.toml` 中固定一次，因此只有一个宿主服务注册表。
3. 通过统一入口 [`crates/ai-host`](../crates/ai-host/README.md)（`octosense-ai-host`）
   启动 Shell 的 AI 服务：它从 `../crates/kernel` 链接 `octosense-kernel`（两个 Shell
   的 feature `octos-core`，默认开启），在启动时配置内核，并以内核的 core 目录注册
   `llm` 服务（开启其 `octos-core` feature），这样修改服务商会重启内核。见 [octos 内核](#octos-内核)。
4. 可选（需显式开启 `app-appcard`）以 `default-features = false` 链接 AppCard
   的 `octos-app`，并通过其 `AppShell` 控件挂载（见 [AppCard 助手](#appcard-助手)）；
   它连接的是同一个内核。

没有需要升级的固定版本：这里的改动在同一个 PR 中就会到达两个 Shell。

## 目录结构

```
<name>/bundle/               隔离运行的脚本应用：manifest.json、main.splash、图片资源
photos/resources/            Home 挂载的 Photos 示例图库
mail/host-service/           octosense-mail-service，`mail` 宿主服务（Rust）；通知回调交给 Shell
mail/docs/                   邮件的计划（邮件操作卡片）
calendar/host-service/       octosense-calendar-service，`calendar` 宿主服务；resources/event.card、agenda.card
news/host-service/           octosense-news-service，`news` 宿主服务（新闻的数据服务）
pdf/host-service/            octosense-pdf-service，`pdf` 宿主服务（pdfcraft 引擎，ADR 0013）；examples/pdftools_fixture.rs 为 PDF Tools 的测试写出示例 PDF
pdftools/                    PDF Tools：bundle/、tests/ui.py（在隐藏的 Shell 中运行的流程）、README.md
<name>/bundle/tools.json     新闻、邮件、日历、相册、地图、YouTube、相机的 Agent 工具
<family>/host-service/tools.json  craft 引擎（word、deck、cad、light、sound、design、film、effect、vector、pdf）的工具，供系统 Agent 使用（ADR 0013）
../crates/shell/src/glance_notice.rs   共用通知服务；../crates/shell/resources/glance/notice.card
ai-providers/                `llm` 宿主服务（host-service/）和 octosense-llm-config（config/：
                             octos 模型目录与服务商注册表、profile 合并、OCTOS1/OCTOS1E 二维码）
writer/                      Writer：bundle/、tests/ui.py（它的 card-host 界面测试）、dev-fixture/engine.splash（该测试换入的替身 word 引擎；不随应用发布）
reference/                   reference 模块
appcard/                     原生 AppCard 助手
  app/                       octos-app 及 store/transport/render crate（根 workspace 的成员）
  module/                    octosense-appcard：挂载它的 Shell 模块
  a2app/                     Splash 卡片记忆（需求规格、控件模式、lint 规则），编译进应用
  a2app-l0/                  L0 卡片框架、目录和各应用示例卡片，编译进应用
  personal-data/             octos 技能：对邮件和日历数据的只读搜索
  vendor/                    内置的第三方 crate（rustyline、mmap-rs；见 NOTICE）
  tools/                     setup-native.py、octos macOS/OpenHarmony 启动器、build-android.sh 等
  docs/                      架构、构建和评审笔记
  native-runtime.lock.json   AppCard 构建所用的 Octoscript-Makepad 版本（与根目录相同）
../crates/shell/             octosense-shell：两种打包形态共同链接的唯一 Shell
../crates/ai-host/           octosense-ai-host：Shell 的 AI 服务（内核、`llm`、app peers），统一入口
../crates/kernel/            octosense-kernel：Shell 的 octos 内核（每进程一个，共享）
../crates/app-peers/         octosense-app-peers：应用 Agent 的代理，每个（应用，账户）一个 peer
../crates/l0-chat/           octosense-l0-chat：卡片卡内对话（sys.chat）的宿主一侧
../.github/workflows/apps.yml   宿主服务、AppCard 和 Shell 服务的 CI
```

## 系统应用的 bundle

```
apps/<name>/bundle/
  manifest.json     id、version、name、capabilities、network.hosts、integrity；
                    可选的 window：桌面窗口的尺寸（schema_minor 1）
  main.splash       程序
  icon.png|svg      可选的启动器图案；外形由 Shell 统一控制
  thumbs/ ...       应用加载的其他文件，路径为 {{assets}}/<path>
```

`main.splash` 通过 `{{assets}}` 占位符引用自身文件，runner 会把它替换为提供
bundle 的源地址（Photos：`let assets = "{{assets}}"`，然后
`assets + "/thumbs/" + id + ".jpg"`）。

系统应用与商店应用结构相同，区别如下：

| | 系统应用（本仓库） | 商店应用（App Hub） |
| --- | --- | --- |
| Id | `os.<name>`。`os.` 为保留前缀：`hub check` 会拒绝，任何设备都不会从商店安装 | 其他任意 id |
| 分发 | 构建时根据 `system-apps.json` 打包进 Shell 二进制 | 从签名目录下载 |
| 准入 | 只校验摘要（`HostLimits::system()`）；源 manifest 的 `integrity.bundle_blake3` 留空，由构建填入 | 摘要加发布者签名 |
| 上限 | `HostLimits::system()`：64 MB 存储、128 MB 内存、更大的指令预算，因为应用在打开期间一直存活 | `HostLimits::default()`：按卡片规模设定 |
| 额外文件 | Shell 可以把目录挂载到 `{{assets}}` | 只有 bundle 内的文件 |

两者使用同一 isolate 和公开 API 策略：声明说明用途，仍检查已验证应用身份、存储隔离、实际同意和服务自身的规则。如何编写这类应用（语言、API、`octo` 命令行）见 [App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)（`docs/QUICKSTART.md`、`docs/SCRIPT-API.md`）。

## 启动器图标规范

应用负责品牌图案，当前 Shell 风格负责外形。Home、Dock、应用资源库、
分组预览和 Shell 中的应用标识统一使用 `octosense::style::AppIconDraw`，
包括从 App Hub 安装的应用。不要在这些位置直接绘制 bundle 图片。

| Shell 风格 | 外形 |
| --- | --- |
| Android | 圆形，与 OctoSense 现有 Android 图标一致 |
| macOS / OctoSense | 圆角方形，使用 macOS 图标集的边距 |
| iOS | 圆角方形，使用 iOS 图标集的边距 |
| Windows | 轻度圆角方块 |
| NextStep | 方块 |
| Omarchy / Windows 2000 | 保留主题的自由外形惯例 |

原生 Android 应用的图标已经由 Android 按设备蒙版绘制，并可能带用户或
工作资料标记；Shell 保留这些像素，不再次裁剪。Android 并非所有设备都
必须使用圆形：[自适应图标蒙版由 OEM 决定](https://source.android.com/docs/core/display/adaptive-icons)。
[Apple 图标指南](https://developer.apple.com/design/human-interface-guidelines/app-icons)
规定 iOS、iPadOS 和 macOS 使用圆角矩形。

新的 bundle 图标应使用正方形 PNG 或受支持的 SVG，背景铺满画布。重要
图案放在画布中央 66% 区域内，避免被平台外形裁掉。不要在原图中烘焙
圆形蒙版、圆角或外部阴影。支持透明图案；使用方块或圆形外框的风格会
添加中性的底色。保留 Logo 和品牌颜色，外框适配不应重新设计应用标识。

Shell 自有 SVG 使用 `icon_frame::styled_svg` 和显式背景元素；bundle
PNG/SVG 在绘制时使用同一外框策略，因此切换风格也会更新已安装应用的
图标。检查 Android、macOS、iOS 风格下的启动器和小标识尺寸，以及明暗
背景。外框和风格回归测试随 Phone 工作流执行。

需要视觉回归检查时，在 `phone/` 中启用 `mobile-apps` 构建 `icon_shapes`
示例。使用隔离的 `OCTOSENSE_HOME`、隐藏窗口和 Makepad 远程控制接口运行
（参见根目录 `AGENTS.md`）。示例覆盖七种风格、内置和商店 PNG/SVG 图案、
透明背景、小尺寸标识及半透明图标。通过 `/g?scale=1` 截图，再用
`tools/check_icon_shape_preview.py` 检查 PNG（可选依赖：Pillow）。默认偏移
对应 macOS 标题栏，其他后端可传入 `--body-y`。原生 Android 应用图标仍需
在真实设备上检查。

## 开发时运行 bundle

App Hub 的 `card-host` 按 manifest 解析出的策略运行单个 bundle，准入顺序与设备
一致。`--system`、`--static` 参数以及宿主服务支持在 App Hub 的
`main` 上（自
[OctoSense-App-Hub#4](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/4) 起）。

```sh
# 在 OctoSense-App-Hub 的检出中；<OctoSense> 是本仓库的检出
cargo build --release -p octosense-card-host --bin card-host

card-host --bundle <OctoSense>/apps/news/bundle --system
card-host --bundle <OctoSense>/apps/photos/bundle --system --static photos=<原图目录>
```

| 参数 | 作用 |
| --- | --- |
| `--bundle <dir>` | 要运行的 bundle（默认：当前目录） |
| `--system` | 按系统应用准入：只校验摘要，使用系统上限；空摘要在内存中补齐 |
| `--static <prefix>=<dir>` | 在 `{{assets}}/<prefix>/...` 提供 `<dir>` 的文件，与 Shell 提供挂载目录的方式相同 |
| `--app-data <dir>` | 应用存储沙箱的位置（默认 `$TMPDIR/octosense-card-apps`） |
| `--allow-unsigned`、`--stamp` | 用于商店 bundle；使用 `--system` 时不需要 |

日志行 `card-host: <id> <version> admitted — capabilities …, hosts …` 显示应用
获得了什么；`card-host: refused: …` 表示被拒绝，不会绘制任何内容。

设置 `MAKEPAD_REMOTE=<port>` 可通过本地 HTTP 操控窗口（`/snap`、
`/click?x=..&y=..`、`/g` 截图、`/quit`）；完整路由见 App Hub 的
`docs/DEVELOPMENT.md`。

**Mail** 需要宿主服务，而 `card-host` 不注册任何服务。请在链接了该服务的 Shell
构建中用演示邮箱运行 Mail（任意地址，密码 `demo`，示例邮件，发送不会真正发出）：

```sh
# 桌面端，在仓库根目录
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense
# 手机尺寸窗口中的 Home，在 phone/ 中
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense-home --features mobile-only
```

演示邮箱的密码存放在文件中，因此不会弹出钥匙串提示。

**Quick Deck 的开发夹具。** `card-host` 没有 `deck` 服务，而随 Shell 发布的
bundle 中没有任何夹具代码。Quick Deck 的 UI 测试使用 bundle 之外的
`quickdeck/dev-fixture/`：`engine.splash` 是引擎的替身，它把每次调用要求的内容
写到调用给出的路径；`fixture.json` 是它的设置；图片是固定版本的 deckcraft
（d0e57d7e）根据 `outline.txt` 生成的真实渲染图。`quickdeck/tests/ui.py` 会复制一份
临时 bundle，用 `engine.splash` 替换其中的 `deck_call()`，把夹具复制到应用存储的
`dev/` 中，然后在隐藏的 `card-host` 中以浅色和深色外观走遍每个页面和状态。第一轮
直接使用随 Shell 发布的 bundle，展示没有引擎应答时的样子。

```sh
python3 apps/quickdeck/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/quickdeck-ui
```

截图和每种外观的回执保存在 `target/quickdeck-ui/light` 与
`target/quickdeck-ui/dark`。`fixture.json` 中，`delay` 是每次调用耗费的秒数，
`fail` 指定失败的调用（`new`、`info`、`render`、`convert` 或 `all`），
`unreadable: true` 让 `render` 正常应答但不留下图片，于是每张幻灯片显示其文字。

**Writer 的测试。** 发布的应用包只有一条引擎路径 `host.request("word.*")`，不含 fixture 代码。
它的文本规则（列表为草稿显示什么、交给引擎的 Markdown、文件名、大纲、相对时间以及拒绝时的提示）
都是纯函数，由 `../crates/shell/src/writer_model_tests.rs` 在脚本 VM 中运行：

```sh
cargo test --locked -p octosense-shell --lib writer_model
```

`card-host` 没有 `word` 服务，所以 `writer/tests/ui.py` 在隐藏的 `card-host` 中以浅色和深色
驱动每个界面。前几轮直接使用发布的应用包：写作、列表、重启后仍在的自动保存、删除，以及
“Writer can't … here: this device has no word engine”的回答。其余几轮使用应用包的临时副本，其中 `engine()`
被换成 `writer/dev-fixture/engine.splash`：一个替身引擎，在应用存储中读写每次调用给出的路径，
设置（`delay`、`fail`）放在存储中的 `dev/fixture.json`。

```sh
python3 apps/writer/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/writer-ui
```

截图和每种外观的回执保存在 `target/writer-ui/light` 和 `target/writer-ui/dark`。
真实引擎的流程已在 Shell 中验证（见上文的状态）。

## 宿主服务与面板

某些操作需要宿主持有的凭据或设备权限。**宿主服务**在 Shell 中用 Rust 完成这些工作；
应用在宿主的身份与同意规则下调用公开 API。应用通过
`host.request("<family>.<method>", args, fn(r){…})` 调用，以已准入应用的真实身份
执行；服务检查实际账户作用域、同意和操作规则。`mail` 等服务族声明只描述用途，
不授予或拒绝调用。服务返回数据，不返回凭据或审批权。
运行时部分在 App Hub（`crates/appstore/src/services.rs`）。

当需要用户操作时（输入密码、批准账户），服务会弹出一个**面板**：由宿主自有、
绘制在应用之上的 Splash 界面，运行在不受任何应用策略约束的独立 isolate 中。
来自面板的调用带有 `from_sheet` 标记。

**密钥归宿主所有。** 任何应用都不收集密码、PIN 或一次性验证码：

- 隔离运行的应用中的密码输入框不接受输入；
- 携带密钥的方法位于 `<family>.sheet.*` 之下（`mail.sheet.submit`、
  `mail.sheet.cancel`），只有来自面板的调用才会被分发，且在任何服务看到之前就已检查；
- 只有服务能打开面板，应用不能。

### `mail` 服务

普通 App Hub 应用现可使用按应用/账户隔离的撰写、状态查询和原生 SMTP 审阅接口。见 [Mail 服务与迁移示例](mail/host-service/README.zh-CN.md)；Agent 和合成输入仍不能直接批准发送。

`octosense-mail-service`（`apps/mail/host-service/src/`）：

| 文件 | 作用 |
| --- | --- |
| `lib.rs` | 服务本体：`mail.accounts`、`add_account`（弹出登录面板）、`remove_account`、`folders`、`sync`、`list`、`message`、`mark_read`、`send`、`notify`（Agent 的通知：交给 Shell，由 Shell 以 Mail 的身份发布它的通知卡片，`on_notify`）；`register()`、`register_demo()`、`register_with*()`；`Transport` trait；给 Shell 的账户事件（`on_account_event`） |
| `imap.rs` | IMAP 客户端（文件夹、已读标记回写服务器） |
| `network.rs` | POP3 和 SMTP、MIME 解码；错误信息中从不包含凭据 |
| `html.rs` | 把邮件重建为 Mail 的 `Html` 视图能绘制的少量标签，不含任何远程内容 |
| `vault.rs` | 密码的存放位置：macOS/iOS 钥匙串；Android 上用 Android Keystore 密钥加密的文件；其他平台为仅所有者可读的文件，这些文件放在宿主的密钥文件夹 `<home>/secrets/os.mail/` 中；`OCTOSENSE_MAIL_VAULT=file` 强制使用文件存储，便于未签名的开发构建 |
| `contacts.rs` | 用户的账户发过邮件的地址，供审批规则“收件人在我的联系人中”使用（用户在设置中打开之前不生效） |

账户元数据（不含密码）和已拉取的邮件存放在宿主自己的目录（`<host_dir>/mail`），
位于所有应用沙箱之外。每个账户只授权给添加它的应用。服务会先测试账户可用，再保存。

Mail 收件箱的 **Reconnect account**（重新连接账户）会打开同一个宿主登录面板。
输入相同邮箱地址、用户名与收件服务器设置，即可更新凭据，同时保留邮件缓存和已保存草稿。
Android 的密码文件由该安装包的 Keystore 密钥加密；从测试包复制到 Home 并不能
恢复登录。请在目标包内重新连接，不要用 Remove account 删除账户来重置凭据。

### `calendar` 服务

`octosense-calendar-service`（`apps/calendar/host-service/src/lib.rs`）以日历
（`os.calendar`）身份执行。Mail 与系统 Agent 均显式获授可共享的
`calendar.events`、`calendar.add_event`、`calendar.notify`；Shell 中转检查调用者，
再交给日历执行器。日历自身 UI 以已准入的 `os.calendar` 身份调用，`calendar` 声明只描述用途，
服务仍校验该身份。删除、更新、UI 查看方法及议程工具不在这些跨应用授权中。

| 方法 | 参数 | 返回 |
| --- | --- | --- |
| `calendar.view`（仅 UI） | `{month?, day?, direction?, take_focus?}` | 月历、日期标记、所选日期的日程以及待打开的日程 |
| `calendar.update_event`（仅 Calendar UI/Agent） | `{id, expected, title, start, end?, timezone?, location?, notes?}` | 保存日程，拒绝过期快照覆盖，并静默刷新已有卡片 |
| `calendar.events` | `{from?, to?, limit?}` | `{events: [{id, title, start, end, location, notes, timezone?, request_id?}]}`，最近的在前 |
| `calendar.add_event` | `{title, start, end?, location?, notes?, timezone?, request_id?}` | `{id, start, reused?}` |
| `calendar.remove_event` | `{id}` | `{removed}` |
| `calendar.notify` | `{event}` 或 `{title, when, location?, notes?}`，以及 `{card_id?, priority?}` | 日程卡片（`resources/event.card`）上了 glance 屏幕并发出通知后返回 `{card_id, replaced, expires_at}` |
| `calendar.agenda` | `{days?}` | 同上，用于议程卡片（`resources/agenda.card`）：`days`（7）天内接下来的三个日程 |

日程保存在 `<host_dir>/calendar/events.json`，位于所有应用沙箱之外。月历、按日列表
与编辑器使用同一份存储。已保存日程卡片带 `event/<id>` 路由；**Open Calendar**
位于卡片内部、日期和时间下方，在真实日历应用中打开同一条记录。L0 `sys.link`
动作使用 `app://calendar/event/<id>`，宿主只接受当前发布记录声明的所属应用目标；
其他 URL 或路由不会启动应用。`calendar/cards.json` 保存发布记录、原始有效期
及用户隐藏状态；重启静默恢复未过期卡片，重复通知复用同一卡片，编辑刷新卡片数据。
临时通知及议程卡片不属于此持久化日程卡片记录。显式 IANA
`timezone` 保留日程所在地的时间，并在卡片显示时区；省略时区沿用设备本地时间。
夏令时切换中不存在或有歧义的时间会被拒绝。精确的 `from`/`to` 过滤请带 RFC3339
偏移量。稳定的 `request_id` 让完全相同的重试复用已保存日程；相同键但字段不同会
被拒绝。未知的结束时间应省略。

Mail 仅在用户请求或系统明确配置了安排日程策略时执行，先读日历，验证保存结果后
再发布日历卡片。Android Mail 冷启动任务注册日历服务并加载获授执行器，无需打开
日历或启动第二个代理。这是本地日程，不是 Google Calendar 同步、邀请或定时提醒。

验证：[Mail → 日历检查与手机证据](../docs/testing/mail-calendar-2026-10-05.zh-CN.md)。

### `llm` 服务

`octosense-llm-service`（`apps/ai-providers/host-service`）是 AI providers 的 Rust
部分。它把 octos 内核的大模型服务商保存在内核的 profile
`<core_dir>/profiles/_main.json` 中（由 `octosense-llm-config` 合并 `config.llm` 和
`config.env_vars`，其他键保持不变），密钥则放在 octos 读取的位置：macOS 上是钥匙串
`octos` 服务（profile 中写 `keychain:` 标记），桌面 Linux 上是 `<core_dir>/secrets/`，
其他平台（Android，以及内嵌内核读不到密钥文件夹的 HarmonyOS）写在应用私有的 profile 中。密钥只在宿主面板上输入，二维码只在
宿主面板上显示和扫描；应用只能看到打码后的状态。开启其 `octos-core` feature（Shell
的默认设置）后，它写入 `octosense_kernel::core_dir()`，并在每次更改后调用
`octosense_kernel::restart()`，让正在运行的内核读取新的服务商。方法列表与注册
方式见其 [README（英文）](ai-providers/host-service/README.md)。

**Talk to Octos**（默认关闭）：在 **AI providers → Talk to Octos** 中打开后，本机会启动一个仅监听回环地址的服务，让 Web 客户端或终端界面与本设备的助手对话。开启期间内核以 `octos serve --host-managed` 代替 `--stdio` 运行，原生应用继续通过其 WebSocket 工作；外部客户端使用单独的令牌，只能打开 UI Protocol 套接字。Web 客户端通过一次性配对码或其链接的二维码配对；本用户的终端客户端读取私有连接文件。原生应用关闭后服务仍保持运行，直到关闭该功能或 Shell 退出。见 [ADR 0003（英文）](../docs/adr/0003-shared-octos-client-access.md) 和[内核指南](../crates/kernel/README.zh-CN.md)。

## 应用 Agent

应用 Agent 是应用自己的 octos peer，归系统 Agent 所有：有自己的工作区（应用的账户文件夹
`apps/<id>/accounts/<account>/`）、记忆、模型通道和工具。哪些系统应用有 Agent，以及如何声明
（[`../crates/shell/src/apps.rs`](../crates/shell/src/apps.rs) 的 `agent_apps`、
[`../crates/shell/src/host_tools/script_apps.rs`](../crates/shell/src/host_tools/script_apps.rs)）：

| 应用 | `manifest.json` | `tools.json` | 卡片 |
| --- | --- | --- | --- |
| 新闻 | `agent` 块、`glance` | `news.list`、`news.read`（read，可共享）、`news.notify`（act，后台） | Shell 的通知卡片 |
| 邮件 | `agent` 块、`glance`、`storage.accounts`（Agent 代表已登录的账户工作） | `mail.accounts`、`mail.folders`、`mail.sync`、`mail.list`、`mail.peek`、`mail.draft`（read）；`mail.notify`、`mail.publish_card`、`mail.skip_event`、`mail.propose_reply`、`mail.suggest_reply`、`mail.propose_send`（act，后台） | L0 卡片或 Shell 通知卡片 |
| 日历 | `agent` 块、`glance` | `calendar.events`（read）、`calendar.add_event`（act）、`calendar.remove_event`（destructive，`confirm: host`）、`calendar.notify`、`calendar.agenda`（act） | `event.card`、`agenda.card` |
| 照片 | `agent` 块、`glance` | `photos.notify`（act，后台）、`photos.info`（read：照片引擎检查照片 Agent 自己文件夹内的文件，[ADR 0013](../docs/adr/0013-craft-engines-as-pinned-services.zh-CN.md)；仅限桌面，手机上返回 `unavailable`） | Shell 的通知卡片 |
| 地图、YouTube、相机 | `agent` 块、`glance` | `maps.notify`、`youtube.notify`、`camera.notify`（act，后台） | Shell 的通知卡片 |
| AI providers | 无 | 暂无：App Hub 只接受 `[a-z0-9_]` 形式的工具命名空间（octos 也只接受由 `[a-z][a-z0-9_]` 段组成的工具名），所以 `ai-providers.notify` 会被拒绝 | – |

**引擎工具（[ADR 0013](../docs/adr/0013-craft-engines-as-pinned-services.zh-CN.md)）。**
十个 craft 引擎（word、deck、cad、light、sound、design、film、effect、vector、
pdf）没有应用，也没有应用 Agent；它们的工具随服务一起发布，位于
`<family>/host-service/tools.json`。Shell 以虚拟所有者 `os.<family>` 的名义声明这些
工具，并只授予系统 Agent（[`../crates/shell/src/system_chat/grants.rs`](../crates/shell/src/system_chat/grants.rs)
中的 `ENGINE_TOOLS`），共 27 个。七个引擎（word、deck、cad、light、film、effect、
vector）提供 `<family>.info`（read）和 `<family>.run`：经过审查的命令入口，在一份文档上
依次运行引擎目录中最多 64 条命令，再把结果写成新文件。每个服务在运行任何命令之前，先用
由引擎审查后的分类（`skill/safety.json`；
[`../crates/engine-area/src/door.rs`](../crates/engine-area/src/door.rs)）构建的允许列表
检查调用中的每一条命令：只在打开的文档内部起作用的命令可以运行，服务审查过的、读取调用方
文件夹内文件的命令也可以；触及其他文件、代码、网络、设备或应用的 id 一律拒绝，分类中没有的
id、批处理或宏、应用级设置项以及插件效果同样拒绝。引擎工作在 Shell 的 UI 线程上运行，因此每个入口还
限制一次调用能要求的工作量：数量、尺寸、帧范围、在一次调用内相乘的复制，每条命令之后文档的大小，以及
它自己的输出。sound（没有命令目录）、design（按决定不设
入口）和 pdf（只有几个固定操作）保留固定工具：`sound.info`、`peaks`、`convert`、`trim`、
`mix`；`design.info`、`render`、`export`；`pdf.info`、`text`、`render`、`merge`、`split`。
ADR 0013 记录了每个引擎的工具面。引擎在调用方自己的
文件夹里工作（ADR 0013，`../crates/shell/src/host_tools/areas.rs`）：对系统 Agent
而言就是它的工作区，它自己的文件工具能看到引擎写出的文件，每个引擎也能打开其他引擎
做出的文件。所有路径都相对于该文件夹并留在其中，任何调用都不会替换已有文件，写进
应用存储的输出也受其配额限制。Sheets 的 `sheets.*` 和照片的 `photos.info` 在各自
应用的 Agent 文件夹里工作。

**引擎技能。** 每个引擎（包括 sheet 和 photo 引擎）还在 `<family>/host-service/skill/` 中为系统 Agent
附带一个技能：手写的 `SKILL.md`（引擎能做什么、系统 Agent 用哪些工具调用它、文件规则、示例），以及按引擎固定版本
生成的参考文件：`commands.md`（命令目录，每个 id 一行，并标出每条命令能触及什么）、light 的 `controls.md` 和 sheet 的
`functions.md`。与它们并列的 `safety.json`（每个命令 id 的类别，每个命令入口据此构建允许列表）只留在仓库中。内核服务在每次
启动前把已链接引擎的技能安装到 `_main` profile 的技能目录（[`../crates/kernel/README.zh-CN.md`](../crates/kernel/README.zh-CN.md#系统智能体的技能)）；
octos 把它们的一行描述列入系统 Agent 的提示词，Agent 需要某个引擎时再读取对应技能。各服务的 `tests/skill.rs` 会在参考文件
与固定版本的引擎不一致时失败；用 `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-<family>-service --test skill`
重新生成。

**邮件卡片的回复方式。** 系统代理可以配置：可回复的重要邮件自动生成草稿；自动发送或 no-reply 邮件等用户点击 Compose reply（撰写回复）后再生成。宿主在邮件事件的信息卡片上提供该操作，核实原邮件，再请 Mail 代理创建草稿。同一张卡片随即变成 Email/Chat，支持持久编辑和宿主审阅界面。见[邮件事件导读](../docs/mail-agent-events.zh-CN.md)。

**宿主服务 API 不会自动成为 Agent 工具。** Mail 显式声明了账户绑定的读取/同步、
发布、事件决策和草稿/提议工具。`mail.peek` 不标记已读；`mail.message` 仍是 UI API。UI 的 `mail.send` 路径现已改为打开宿主的审阅界面，而非未经批准调用 SMTP。Agent 工具不能批准或发送。[组合 Mail 卡片](../docs/mail-composable-cards.zh-CN.md)介绍了持久编辑、上下文聊天和审批边界：只有亲手点按（Android 上触摸屏幕，macOS 上用鼠标或触控板点击）才能发送，合成输入和远程输入都会被拒绝。macOS 路径和双模型手机集成验收仍**未验证**。
Peer 的工作目录不会挂载 Mail 的宿主数据库或凭据保险库。Calendar
展示了通过显式声明的 Rust 工具读写应用数据的路径；它的脚本窗口目前只是 Agent
使用说明。见[数据访问源码导读](../desktop/docs/code-walkthrough.zh-CN.md)。

- **声明。** manifest 的 `agent` 块列出 Agent 可以使用的内核工具（`"tools": ["ask_user_question"]`；
  其中带点的名称表示申请另一个应用的可共享工具），`bundle/tools.json` 声明应用自己的工具：
  `<app>.<tool>`、`input_schema`、`output_schema`、`risk`（`read`、`act`、`destructive`）、
  `background`、`confirm`（`host` 或 `app`）、`shareable` 和 `implemented_by: "host-service"`。
  App Hub 准入并固定这两个文件。`agent` 块中的 `profile` 和 `model` 会被准入，但 Shell 还没有使用。
- **运行。** 在用户于首次使用面板上允许之前什么都不会运行（用户用顶栏的 “Ask <app>”、
  Shift+F8 或菜单项 “Ask this app's agent” 打开 Shell 的 “Ask <app>” 面板时，或系统
  Agent 用 `agents.ask` 询问时，弹出这个面板）。之后 Shell 准备好 peer，系统 Agent
  就能用 `peer_send_input` 找到它。`agents.ask` 会等待用户的回答和 peer 就绪（它声明为
  `outward` 且 `confirm: app`，内核会像对待审批一样一直等它，而不是只给读取类工具的 30 秒），
  然后把 peer 的 slug 交给系统 Agent，让请求在同一轮里继续。Mail 还支持由
  `agents.provision` 启用的 `mail.messages.new`：OctoSense 进程存活时，持久队列自动
  启动 incoming 回合。收件箱收取独立于这些回合运行，失败事件分别重试，避免一次失败
  阻塞后续所有邮件。宿主记录发布成功或明确跳过，且回合成功后，才确认事件。
  通用应用触发器/cron 仍待实现。见[邮件事件导读](../docs/mail-agent-events.zh-CN.md)。
- **直接与它对话。** 用户可以直接与应用的 Agent 对话，而不只是通过系统 Agent：在 Shell
  为每个拥有 Agent 的应用提供的 “Ask <app>” 面板里（这些应用都不绘制自己的对话界面）。
  这些回合在用户的通道里运行，与系统 Agent 的通道并列，带着应用的工具。
  独立的 `sys.chat` 功能见[卡内对话的可用情况](../README.zh-CN.md#卡内对话)。
  面板的“停止”只停止用户自己的回合。手机上还没有打开这个面板的触控入口。详见根目录的
  [README](../README.zh-CN.md#直接与应用的-agent-对话)。
- **它的工具在应用的宿主服务上运行**，以应用的身份运行，在此之前 Shell 的中转已检查授权、
  schema 和预算。破坏性和对外调用（这里是 `calendar.remove_event`）进入审批路由；
  路由应用用户的常设规则、宿主或已注册的应用确认面板，以及用户开启的开发者模式，并记录决定。
  见[审批](../docs/architecture.zh-CN.md#5-审批)。
- **卡片。** `<app>.notify {title, body, card_id?, priority?}` 以应用的名义在 glance 屏幕上
  放一张通知卡片，并发出一条通知：所有应用共用 Shell 自带的一张固定 L0 卡片
  （[`../crates/shell/resources/glance/notice.card`](../crates/shell/resources/glance/notice.card)，
  由 [`../crates/shell/src/glance_notice.rs`](../crates/shell/src/glance_notice.rs) 填充），带有应用的
  图标和名称、时间，以及 Agent 写的标题（最多 80 个字符）和正文（最多 600 个字符）；同一个
  `card_id` 会替换该应用之前的通知。邮件、新闻以及链接了照片引擎时（桌面）照片的服务把 `notify` 交给 Shell（照片的 `photos` 服务还在照片引擎上应答
  `photos.info`）；地图、YouTube
  和相机没有自己的服务，手机上的照片也没有，由 Shell 的通知服务应答（此时 `photos.info` 回答本设备不可用）。`calendar.notify` 和 `calendar.agenda` 填充日历
  自己的日程卡片和议程卡片。每张卡片都以应用的身份、带 `notify` 通过 Shell 的 `glance`
  服务发布，保留已准入发布者及其账户身份。这些固定模板工具由模型提供文字；`mail.publish_card`
  另接收经宿主校验的模型 L0 源码。
- **试一试**（桌面端）：打开助手（F8），请系统 Agent 让某个应用的 Agent（邮件、日历、新闻、照片、
  地图或 YouTube）在 glance 屏幕上放一张卡片；在弹出的面板上允许该 Agent。邮件需要一个已登录的账户（下文的演示邮箱即可）。
  邮件更完整的操作卡片（[计划（英文）](mail/docs/2026-10-01-email-action-card-plan.md)）目前只是
  使用假数据的演示：`OCTOSENSE_GLANCE_DEMO=mail` 会在启动时发布它。**未验证配方。**

## octos 内核

octos Agent 内核是 **Shell 服务**，不属于任何应用。
[`crates/kernel`](../crates/kernel)（`octosense-kernel`）就是这个服务；
Shell 默认链接它（cargo feature `octos-core`，在 `mobile-apps` 和原生移动构建中
同样开启）：

- **每进程一个，按需启动。** 第一个使用方调用 `connect()` 时启动：桌面和
  Android 上以子进程运行 `octos serve --stdio`（Android 上是 APK 内置的
  `liboctos.so`），OpenHarmony 上在进程内运行标准内核。之后的使用方共享它；
  每个使用方只收到自己请求的回复和自己会话的通知。最后一个使用方离开时内核停止。
- **由 AI 服务商配置。** `llm` 宿主服务写入内核的 profile
  `<core_dir>/profiles/_main.json` 以及密钥（macOS 钥匙串 `octos` 服务配合
  `keychain:` 标记，桌面 Linux 上是 `<core_dir>/secrets/`，其他平台（包括 HarmonyOS）写在 profile 中），
  然后调用 `restart()`：正在运行的内核停止，使用方重新连接，新内核读取新的服务商。
- **使用方。** AppCard（需显式开启）通过其传输层的 `kernel` 模块连接；Rinx 的
  原生小程序宿主可以用同样方式拿到自己的连接，而不是共用 AppCard 的连接。
- **core 目录。** Shell 指定的目录，否则 `$OCTOS_APP_CORE_DIR`，否则手机上是
  `<应用数据目录>/octos-home/.octos`，否则 `$HOME/octos-home/.octos`。桌面上只有
  配置了内核二进制（Shell 指定，或 `$OCTOS_APP_CORE_BIN`）时才运行内核；没有时
  服务商设置照样保存。

测试（在仓库根目录）：`cargo test --locked -p octosense-kernel`；有编译好的 `octos` 时，
`OCTOS_CORE_TEST_KERNEL=<octos> cargo test --locked -p octosense-kernel --test real_kernel` 会用
`octosense-llm-config` 写入的 profile 启动真实内核，并在修改服务商后重启它。
详见 [crates/kernel/README.md（英文）](../crates/kernel/README.md)。

## AppCard 助手

即“Ask anything”磁贴。你输入一个请求；路由大脑（AMA）选择或组合一个应用 Agent；
该 Agent 生成一张实时卡片（Splash 或 webview），在渲染时绑定真实数据。它通过
octos UI Protocol v1 与 octos 通信。

- **代码**：`apps/appcard/app`，根 workspace 中的 crate： `octos-app`（路由、
  组合、多 Agent 调度、Splash 渲染与校验、L0 卡片生成、WebView 浮层）、
  `octos-app-store`（状态 reducer，不依赖 Makepad）、`octos-app-transport`
  （经由 Shell 内核、WebSocket 或 REST 的 octos UI Protocol 客户端）和
  `octos-app-render`（流式 markdown 渲染）。
- **octos**：所有 octos crate 都来自 git `octos-org/octos`，版本为根目录
  `Cargo.toml` 的 `[workspace.dependencies]` 中唯一的 rev（具体版本见该文件），与 `crates/kernel` 和 Shell 共用。AppCard 不再自己启动内核，而是连接 Shell 的内核
  （见 [octos 内核](#octos-内核)）。
- **Makepad**：不内置。Makepad、Octoscript 和 Octoscript-Makepad 是仓库根目录下
  `.sources/` 中由 `tools/setup.py` 准备的检出，版本由 `native-runtime.lock.json`
  选定；根目录的 `.cargo/config.toml` 设置 `OCTOSENSE_WORKSPACE=.sources`，AppCard
  构建时从那里嵌入框架资源。

在仓库根目录构建与测试（详见 [appcard/README.zh-CN.md](appcard/README.zh-CN.md)）：

```sh
python3 tools/setup.py                               # 准备 .sources/
(cd apps/appcard && PYTHONPATH=tools python3 -m unittest core.test_native_runtime)
cargo clippy --locked -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
cargo test --locked -p octos-app-transport -p octos-app-store
cargo run -p octos-app                               # 独立窗口（默认 feature `standalone`）
```

独立运行的应用通过 `~/.config/octos-app/server.json`、
`OCTOS_BASE_URL`/`OCTOS_BEARER`/`OCTOS_PROFILE_ID`，或经由
`OCTOS_APP_CORE_BIN` 和 `OCTOS_APP_CORE_DIR` 指定的本地内核二进制连接 octos
（`tools/octos-macos.py` 会设置这些变量；见 `tools/OCTOS-MACOS.md`）。Android
和 OpenHarmony 构建见 `docs/BUILDING-ANDROID.md`、`docs/BUILDING-OPENHARMONY.md`。

**Shell 如何嵌入。** Shell 以 `default-features = false`（不含 `fn main`）依赖
`octos-app`，调用 `octos_app::register_script_mods(vm)`，然后挂载
`AppShell::create(vm)`：一个持有应用、绘制 `OctosAppBody`（去掉独立 `Window`
的应用根视图）的控件。`AppShell::ask` 像用户输入一样提交文本。在两个 Shell 中，
它被包在实现了 Shell 的 `AppModule` trait 的 `AppCardModule` 里
（[`appcard/module`](appcard/module)，包名 `octosense-appcard`）。

**CI**：[.github/workflows/apps.yml](../.github/workflows/apps.yml) 在 `apps/`、
`crates/`、workspace 文件和 `tools/setup.py` 有改动时运行。其 macOS 任务准备
`.sources/`，运行 Mail 与 `llm` 宿主服务测试、AppCard 运行时锁测试、AppCard 四个
crate 的 clippy（这一步会编译整个应用）、AppCard 的 transport 与 store 测试，并检查
依赖图中只有一份 octos、Makepad、App Hub 和 Rinx。其 Ubuntu 任务测试
`crates/kernel`、`crates/app-peers` 和 `octosense-llm-config`。
`apps/appcard/app/.github/workflows/` 是原仓库遗留的，不会运行。

## 修改应用

1. 编辑 `apps/<name>/bundle/`。只使用 App Flow 的 `docs/SCRIPT-API.md` 中有文档的 API，或本仓库其他应用已经在用的 API；用其他东西之前先查运行时源码。
2. 用 `capabilities` 和 `network.hosts` 描述实际用途，能力名称取自 App Hub 的
   `KNOWN_CAPABILITIES`。这些是披露，不是授权；必需 API 的版本写入 `host_api.required`。
3. 绝不添加密码或验证码输入框。应用需要密钥时，由宿主服务及其面板处理。
4. 用 `card-host --system` 运行（Mail：在 Shell 中用演示邮箱）。在手机上用
   以独立测试包构建的 Home 测试，绝不替换设备上已安装的 Home。
5. 提一个 PR 即可。Shell 直接打包 `apps/`，没有需要升级的固定版本。

**新增**系统应用即新建一个 `apps/<name>/bundle/`，id 为 `os.<name>`，并在每个
Shell 的 `system-apps.json` 中加入它。

## 测试

| 对象 | 方法 |
| --- | --- |
| Mail 服务 | 在仓库根目录：`cargo test --locked -p octosense-mail-service`。钥匙串测试默认忽略：`cargo test -p octosense-mail-service -- --ignored keychain` |
| 日历和新闻服务 | `cargo test --locked -p octosense-calendar-service -p octosense-news-service`（日历的测试还不在 `apps.yml` 中） |
| 卡片的卡内对话 | `cargo test --locked -p octosense-l0-chat` |
| octos 内核服务 | `cargo test --locked -p octosense-kernel`（替身内核）；`OCTOS_CORE_TEST_KERNEL=<octos> cargo test -p octosense-kernel --test real_kernel`（真实内核） |
| `llm` 服务与配置 | `cargo test --locked -p octosense-llm-service -p octosense-llm-config`；加 `--features octosense-llm-service/octos-core` 即 Shell 的构建方式 |
| AppCard | 上文的命令 |
| CI | 除真实内核和钥匙串测试外的以上全部：[apps.yml](../.github/workflows/apps.yml) |
| 脚本 bundle | 在 `card-host` 和 Shell 中手动测试，通过 `MAKEPAD_REMOTE` 操控。本仓库暂无自动化 UI 测试 |

## 相关仓库

| 仓库 | 作用 |
| --- | --- |
| [OctoSense](../README.zh-CN.md)（本仓库） | 内置这些应用的 Shell：[`desktop/`](../desktop/README.zh-CN.md) 和 [`phone/`](../phone/README.zh-CN.md) 中的 Home（独立启动器，或由 [`rom/`](../rom/README.zh-CN.md) 镜像预装）；`crates/` 中的 Shell 服务 |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | 签名目录、准入检查（`hub stamp`、`check`、`scan`、`publisher-verify`）、`card-host`、Card runner 与宿主服务注册表，以及每个 Shell 都链接的 `octosense-app-hub-app` |
| [OctoSense-App-Flow](https://github.com/OctoSense-org/OctoSense-App-Flow) | 如何设计、构建、检查和发布应用 |
| [OctoScript](https://github.com/OctoSense-org/OctoScript)、[OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad)、[makepad](https://github.com/OctoSense-org/makepad) | 语言与运行时 |
| [Rinx](https://github.com/hagency-org/Rinx) | Matrix 聊天与小程序，原生模块；通过 `crates/app-peers` 访问助手 |
| [octos](https://github.com/octos-org/octos) | Agent 内核：由 `crates/kernel` 作为 Shell 服务运行，由 AI providers 配置，供 AppCard 等使用方使用（统一使用根 `Cargo.toml` 选定的版本） |

## 参与贡献

- 向 `main` 提 PR；绝不强推 `main`。
- 改动保持小，并在 Shell 中测试。遵循 [AGENTS.md](AGENTS.md)。
- `apps/` 下的改动必须通过 `apps.yml`，以及 Shell 的 `desktop.yml` 和 `phone.yml`。

## 历史与许可

本目录在 2026-09-27 之前是 OctoSense-System-Apps 仓库，已连同历史导入这里。这些 bundle 和 Mail 服务最初写在 OctoSense-mobile（已归档）和 OctoSense-App-Flow（最初名为 Octoscript-AppCard，后来改名 OctoScript-App-Design-Flow）中，历史记录保留在那里。AppCard 来自 OctoSense-org/OctoSense-AppCard（`d0a836b8`），它是在 commit `cbbda4da` 时从 App Flow 的 `app/` 拆分出来的。

Apache-2.0（[LICENSE](LICENSE)）。第三方组件见 [NOTICE](NOTICE)。
