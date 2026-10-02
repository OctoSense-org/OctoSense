# OctoSense 系统应用

[English](README.md) | 简体中文

> **在整个系统中的位置。**系统应用是运行在 App Hub 的 Card runner 中的脚本应用。它们从不直接与 octos 内核通信。新闻、邮件和日历有 Agent（每个应用、每个账号一个由宿主拥有的 octos peer，`card.<应用 id>`），由 manifest 的 `agent` 块和 `tools.json` 声明；Shell 替系统 Agent、Shell 的 “Ask <app>” 面板和卡片的卡内对话驱动它，应用自己也可以用 `host.request("octos.*")` 使用它（目前没有应用这样做）。Agent 对应用工具的调用经由 Shell 的中转和审批路由回到应用，在应用的宿主服务上运行；邮件和日历的工具会把卡片放到 glance 屏幕上（[应用 Agent](#应用-agent)）。应用的 glance 卡片在应用自己的策略下运行，用户在卡片上的操作是应用自己的操作，而不是 Agent 的工具调用。进程、应用 Agent 的两条通道以及一次带审批的工具调用的图示：[整体如何运作](../README.zh-CN.md#整体如何运作)；详细说明：[docs/architecture.zh-CN.md](../docs/architecture.zh-CN.md) 和 [ADR 0004（英文）](../docs/adr/0004-native-apps-hosting-and-peers.md)。

[OctoSense](https://github.com/OctoSense-org/.github/blob/main/profile/README.zh-CN.md)（运行在操作系统之上的 Agent 交互 Shell）自带的第一方应用，以及它们背后的宿主服务。
它们位于 [OctoSense 仓库](../README.zh-CN.md)的 `apps/`；2026-09-27 之前它们是
OctoSense-System-Apps 仓库（已归档）。

- **新闻（News）、相册（Photos）、地图（Maps）、相机（Camera）、邮件（Mail）、日历（Calendar）、AI providers 和 YouTube**
  是*隔离运行的脚本应用*。每个应用都是 `bundle/` 里的一个 OctoScript（Splash）
  程序，由 App Hub 的 Card runner 在独立的 isolate 中运行，权限严格等于其
  `manifest.json` 所申请的内容，与商店应用受到的隔离完全相同。它们同时也是
  任何开发者通过 App Hub 发布的应用形态的完整示例。
- **邮件的宿主服务**（`mail/host-service`）是 Mail 的 Rust 部分：
  IMAP/POP3/SMTP、账户存储和登录面板，由 Shell 运行。应用拿到的是邮件，
  永远拿不到密码或 socket。它还运行邮件 Agent 的工具 `mail.notify`，把一张通知卡片
  放到 glance 屏幕上。
- **日历的宿主服务**（`calendar/host-service`）把日历的日程保存在宿主目录中，并运行日历
  Agent 的工具：`calendar.events`、`add_event`、`remove_event`，以及把日程卡片或议程卡片
  放到 glance 屏幕上的 `notify` 和 `agenda`。日历是桌面端的系统应用
  （`desktop/system-apps.json`）。
- **新闻的宿主服务**（`news/host-service`）按定时器收集新闻条目，不使用模型，并运行新闻
  Agent 的 `news.list` 和 `news.read`。
- **`llm` 宿主服务**（`ai-providers/host-service`）是 AI providers 的 Rust
  部分：基于 octos 模型目录的大模型服务商、存放在平台密钥库中的密钥、“测试连接”，
  以及通过受 PIN 保护的 `OCTOS1E` 二维码在设备之间迁移服务商（相机、图片或粘贴）。
  密钥只在宿主自己的面板上输入，二维码也只在那里显示；应用只能看到打码后的状态。
- **AppCard**（`appcard`）是唯一的原生应用：“Ask anything”助手，
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

**要开发自己的应用？** 不需要构建或修改本仓库。请从
[OctoSense-org 主页](https://github.com/OctoSense-org)的阅读列表开始（先读
OctoScript-App-Design-Flow 的 `AGENTS.md`，再读 `docs/QUICKSTART.md`），把这里的
应用包当作完整示例来读（`apps/<name>/bundle/main.splash`）。想在自己的应用旁边运行
其中一个：把 OctoSense 仓库克隆到同一个工作区，然后在 OctoScript-App-Design-Flow 中执行
`tools/octo run ../OctoSense/apps/photos/bundle --system --no-stamp --app-data /tmp/sys-apps`
（`--no-stamp` 不会改动检出；Mail 需要在 Shell 中运行，见下文）。

## 应用一览

| 应用 | Id | 功能 | 权限（manifest） | 网络主机（manifest） | 宿主服务 |
| --- | --- | --- | --- | --- | --- |
| [News](news/bundle) | `os.news` | Hacker News、TechMeme 和 Google News 的订阅源，分标签页（Today、HN、TechMeme、Google、Saved），带文章阅读器 | `storage`、`net`、`images`、`web` | `hn.algolia.com`、`www.techmeme.com`、`news.google.com` | 无 |
| [Photos](photos/bundle) | `os.photos` | 示例相册：回忆、相簿、人物、收藏、可多选的网格、全屏查看器 | `storage` | 无 | 无（原图来自 Shell 的资源挂载，见下文） |
| [Maps](maps/bundle) | `os.maps` | `MapView` 地图、地点搜索、地点详情、可更改起点并最多添加两个途经点的路线，以及带逐向导航和 2D/3D 视图的驾驶模式；有 GPS 定位时从当前位置开始 | `storage`、`net`、`location` | `photon.komoot.io`、`router.project-osrm.org`、`overpass-api.de`、`overpass.kumi.systems`、`maps.mail.ru`、`overpass.openstreetmap.fr` | 无 |
| [Camera](camera/bundle) | `os.camera` | 基于运行时 `CameraPreview` 控件的拍照和录像，闪光灯和变焦，最近一张的缩略图和查看器 | `storage`、`camera`、`microphone`、`library` | 无 | 无 |
| [Mail](mail/bundle) | `os.mail` | 账户、文件夹、邮件列表、阅读（HTML 由服务重建）和写信；它的 Agent 把通知卡片放到 glance 屏幕上（`mail.notify`） | `storage`、`mail`、`glance` | 无（由服务联网，而不是应用） | [`mail`](mail/host-service) |
| [AI providers](ai-providers/bundle) | `os.ai-providers` | 助手的大模型服务商：一个主用与若干备用，每项都有来自 octos 模型目录的型号下拉菜单和“测试连接”；添加向导（系列、型号、线路、密钥、测试）；“为手机显示二维码”，以及通过相机、图片或粘贴导入 | `storage`、`llm` | 无（由服务联网，而不是应用） | [`llm`](ai-providers/host-service) |
| [YouTube](youtube/bundle) | `os.youtube` | YouTube 搜索（运行时无需密钥的 `sys.video`，读取 YouTube 自己的搜索结果页），带缩略图和直播或时长角标的结果列表、话题标签，在 `WebReader` 中播放 YouTube 移动版观看页，以及本机播放记录 | `storage`、`net` | `www.youtube.com`、`m.youtube.com`、`i.ytimg.com` | 无 |
| [Calendar](calendar/bundle) | `os.calendar`（桌面端） | 它的 Agent 保存用户的日程，并把日程卡片和议程卡片放到 glance 屏幕上；它自己的窗口还不能列出日程（需要 App Hub 提供 `calendar` 权限） | `storage`、`glance` | 无 | [`calendar`](calendar/host-service)（只供日历的 Agent 使用） |
| [AppCard](appcard) | 原生，需显式启用 | AppCard 助手：路由大脑选择或组合一个应用 Agent，由它生成实时的 Splash 或 webview 卡片。Shell 只在启用 `app-appcard` 时链接它；默认不发布 | 不适用（不是 bundle） | 不适用 | Shell 的 octos 内核 |

每项权限的含义由 App Hub 的封闭列表定义（`crates/app-policy/src/manifest.rs`
中的 `KNOWN_CAPABILITIES`）：`images` 可显示任意公网 https 主机的图片，`web`
在系统 WebView 中打开网页，`library` 把拍摄内容提供给系统相册，`mail` 访问
宿主的邮件服务，`llm` 访问宿主的大模型服务商服务。`net` 只能访问 manifest 列出的主机。

### 状态与已知问题

- **YouTube**：在 OnePlus 6 上测试（2026-09-27），搜索、结果、播放和播放记录都正常；
  关闭播放器会结束页面（makepad#43，已在运行时中）。播放打开的是 YouTube 移动版观看页，
  它会静音自动播放，并显示自己的“Open App”提示。搜索读取 YouTube 的搜索结果页，依赖其布局。
- **Camera**：在 OnePlus 6 测试中（2026-09-25），Camera 能拍照并在后台释放
  相机，但实时预览是纯黑的，尚未解决。桌面构建没有相机，Android 模拟器拒绝
  提供相机，因此其他环境下拍摄未经测试。
- **Photos**：bundle 只带 75 张缩略图（`bundle/thumbs/`，约 2 MB）。查看器
  显示的原图只有在 Shell 挂载后才会出现在 `{{assets}}/photos/...`：Home
  挂载 `photos/resources/photos`（约 87 MB，见 `phone/system-apps.json`）；
  桌面端不挂载任何目录（`desktop/system-apps.json`），所以那里的查看器没有原图。
- **Maps**：在 OnePlus 6 上（2026-09-27）搜索、地点详情、路线、添加和移除途经点、
  逐向导航驾驶以及 2D 视图都正常。3D 驾驶视图会画出路线但没有地图瓦片，手机和桌面
  上都是如此，途经点改动前后一样。
- **News**：开发时在 `card-host` 中运行过，但在 Shell PR 的测试中没有
  端到端验证（测试手机没有网络）。
- **Mail**：已在桌面和 OnePlus 6 上用演示邮箱验证。Mail 与 `llm` 两个宿主服务使用根目录
  `Cargo.toml` 固定的唯一 App Hub 版本（`46d67e51`，OctoSense-App-Hub#15 合并后的 main；#11 新增了 `llm` 能力，#14 新增了 Matrix 与 Octos 宿主服务能力），与 Shell 链接的版本相同，
  因此一次构建中只有一份 `octosense-appstore` 和一个宿主服务注册表。
- **脚本 bundle 没有 CI。** [`apps.yml`](../.github/workflows/apps.yml)
  测试宿主服务、AppCard 和 Shell 服务，不测试 bundle。
- **AppCard 的 `personal-data` 技能**读取旧原生 Mail 模块的 `mailbox-*.json`
  文件。脚本版 Mail 的邮件现在存放在宿主服务自己的目录
  （`<host_dir>/mail/box-*.json`），该技能大概率已读不到；未验证。
- **日历**（2026-10-01，桌面端，隐藏窗口的 `--remote` 运行，接真实模型）：系统 Agent
  请日历的 Agent 放一张卡片；首次使用面板弹出，Agent 添加了一个日程，它的卡片打开了
  glance 面板。手机上没有打包。它的宿主服务测试还不在 `apps.yml` 中。
- 只有 Camera 自带启动器图标（`bundle/icon.png`），其他应用的图标由 Shell 绘制。

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
   `octosense-mail-service`、`octosense-calendar-service`、`octosense-news-service` 和
   `octosense-llm-service`（workspace 内的 path 依赖），并在启动时注册
   （`crates/shell/src/apps.rs` 的 `register_host_services`）：Mail 服务在真实账户下用
   `register()`，Shell 的应用配置中 `mail_demo: true` 时用 `register_demo()`，并装上
   `mail.notify`（与日历的卡片一样）发布卡片所用的发布器；`llm` 服务使用 octos 内核的 core 目录以及
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
mail/host-service/           octosense-mail-service，`mail` 宿主服务（Rust）；resources/notice.card
mail/docs/                   邮件的计划（邮件操作卡片）
calendar/host-service/       octosense-calendar-service，`calendar` 宿主服务；resources/event.card、agenda.card
news/host-service/           octosense-news-service，`news` 宿主服务（新闻的数据服务）
<name>/bundle/tools.json     应用 Agent 自己的工具（新闻、邮件、日历）
ai-providers/                `llm` 宿主服务（host-service/）和 octosense-llm-config（config/：
                             octos 模型目录与服务商注册表、profile 合并、OCTOS1/OCTOS1E 二维码）
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
../crates/app-peers/         octosense-app-peers：应用 Agent 的代理，每个（应用，账号）一个 peer
../crates/l0-chat/           octosense-l0-chat：卡片卡内对话（sys.chat）的宿主一侧
../.github/workflows/apps.yml   宿主服务、AppCard 和 Shell 服务的 CI
```

## 系统应用的 bundle

```
apps/<name>/bundle/
  manifest.json     id、version、name、capabilities、network.hosts、integrity
  main.splash       程序
  icon.png|svg      可选的启动器图标（Camera 有）
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

其余完全一致：同样的 isolate、同样的权限检查、同样的网络白名单。如何编写这类
应用（语言、API、`octo` 命令行）见
[OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
（`docs/QUICKSTART.md`、`docs/SCRIPT-API.md`）。

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

## 宿主服务与面板

有些工作需要隔离运行的应用绝不能持有的东西：socket、凭据、设备。**宿主服务**
在 Shell 中用 Rust 完成这些工作。应用通过
`host.request("<family>.<method>", args, fn(r){…})` 调用；除非 manifest 授予了
对应的 family（`mail`），isolate 会拒绝调用；服务返回数据，而不是能力本身。
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

`octosense-mail-service`（`apps/mail/host-service/src/`）：

| 文件 | 作用 |
| --- | --- |
| `lib.rs` | 服务本体：`mail.accounts`、`add_account`（弹出登录面板）、`remove_account`、`folders`、`sync`、`list`、`message`、`mark_read`、`send`、`notify`（Agent 的通知卡片，以 Mail 的身份通过 Shell 的 glance 服务发布）；`register()`、`register_demo()`、`register_with*()`；`Transport` trait；给 Shell 的账户事件（`on_account_event`） |
| `imap.rs` | IMAP 客户端（文件夹、已读标记回写服务器） |
| `network.rs` | POP3 和 SMTP、MIME 解码；错误信息中从不包含凭据 |
| `html.rs` | 把邮件重建为 Mail 的 `Html` 视图能绘制的少量标签，不含任何远程内容 |
| `vault.rs` | 密码的存放位置：macOS/iOS 钥匙串；Android 上用 Android Keystore 密钥加密的文件；其他平台为仅所有者可读的文件，这些文件放在宿主的密钥文件夹 `<home>/secrets/os.mail/` 中；`OCTOSENSE_MAIL_VAULT=file` 强制使用文件存储，便于未签名的开发构建 |
| `contacts.rs` | 用户的账户发过邮件的地址，供审批规则“收件人在我的联系人中”使用（用户在设置中打开之前不生效） |
| `resources/notice.card` | `mail.notify` 填充的固定 L0 通知卡片 |

账户元数据（不含密码）和已拉取的邮件存放在宿主自己的目录（`<host_dir>/mail`），
位于所有应用沙箱之外。每个账户只授权给添加它的应用。服务会先测试账户可用，再保存。

### `calendar` 服务

`octosense-calendar-service`（`apps/calendar/host-service/src/lib.rs`）只为日历
（`os.calendar`）服务：App Hub 的权限列表中没有 `calendar`，因此其他应用无法获得它；
Shell 把日历 Agent 的 `calendar.*` 工具当作这个系统应用自己的服务在这里运行。

| 方法 | 参数 | 返回 |
| --- | --- | --- |
| `calendar.events` | `{from?, to?, limit?}` | `{events: [{id, title, start, end, location, notes}]}`，最近的在前 |
| `calendar.add_event` | `{title, start, end?, location?, notes?}` | `{id, start}` |
| `calendar.remove_event` | `{id}` | `{removed}` |
| `calendar.notify` | `{event}` 或 `{title, when, location?, notes?}`，以及 `{card_id?, priority?}` | 日程卡片（`resources/event.card`）上了 glance 屏幕并发出通知后返回 `{card_id, replaced, expires_at}` |
| `calendar.agenda` | `{days?}` | 同上，用于议程卡片（`resources/agenda.card`）：`days`（7）天内接下来的三个日程 |

时间使用本地时间（`2026-10-02T15:00`，或只写日期表示全天）。日程保存在
`<host_dir>/calendar/events.json`，位于所有应用沙箱之外。

### `llm` 服务

`octosense-llm-service`（`apps/ai-providers/host-service`）是 AI providers 的 Rust
部分。它把 octos 内核的大模型服务商保存在内核的 profile
`<core_dir>/profiles/_main.json` 中（由 `octosense-llm-config` 合并 `config.llm` 和
`config.env_vars`，其他键保持不变），密钥则放在 octos 读取的位置：macOS 上是钥匙串
`octos` 服务（profile 中写 `keychain:` 标记），Linux 上是 `<core_dir>/secrets/`，
其他平台（Android）写在应用私有的 profile 中。密钥只在宿主面板上输入，二维码只在
宿主面板上显示和扫描；应用只能看到打码后的状态。开启其 `octos-core` feature（Shell
的默认设置）后，它写入 `octosense_kernel::core_dir()`，并在每次更改后调用
`octosense_kernel::restart()`，让正在运行的内核读取新的服务商。方法列表与注册
方式见其 [README（英文）](ai-providers/host-service/README.md)。

**Talk to Octos**（默认关闭）：在 **AI providers → Talk to Octos** 中打开后，本机会启动一个仅监听回环地址的服务，让 Web 客户端或终端界面与本设备的助手对话。开启期间内核以 `octos serve --host-managed` 代替 `--stdio` 运行，原生应用继续通过其 WebSocket 工作；外部客户端使用单独的令牌，只能打开 UI Protocol 套接字。Web 客户端通过一次性配对码或其链接的二维码配对；本用户的终端客户端读取私有连接文件。原生应用关闭后服务仍保持运行，直到关闭该功能或 Shell 退出。见 [ADR 0003（英文）](../docs/adr/0003-shared-octos-client-access.md) 和[内核指南](../crates/kernel/README.zh-CN.md)。

## 应用 Agent

应用 Agent 是应用自己的 octos peer，归系统 Agent 所有：有自己的工作区（应用的账号文件夹
`apps/<id>/accounts/<account>/`）、记忆、模型通道和工具。哪些系统应用有 Agent，以及如何声明
（[`../crates/shell/src/apps.rs`](../crates/shell/src/apps.rs) 的 `agent_apps`、
[`../crates/shell/src/host_tools/script_apps.rs`](../crates/shell/src/host_tools/script_apps.rs)）：

| 应用 | `manifest.json` | `tools.json` | 卡片 |
| --- | --- | --- | --- |
| 新闻 | `agent` 块 | `news.list`、`news.read`（read，可共享） | – |
| 邮件 | `agent` 块、`glance`、`storage.accounts`（Agent 代表已登录的账户工作） | `mail.notify`（act，后台） | `notice.card` |
| 日历 | `agent` 块、`glance` | `calendar.events`（read）、`calendar.add_event`（act）、`calendar.remove_event`（destructive，`confirm: host`）、`calendar.notify`、`calendar.agenda`（act） | `event.card`、`agenda.card` |

- **声明。** manifest 的 `agent` 块列出 Agent 可以使用的内核工具（`"tools": ["ask_user_question"]`；
  其中带点的名称表示申请另一个应用的可共享工具），`bundle/tools.json` 声明应用自己的工具：
  `<app>.<tool>`、`input_schema`、`output_schema`、`risk`（`read`、`act`、`destructive`）、
  `background`、`confirm`（`host` 或 `app`）、`shareable` 和 `implemented_by: "host-service"`。
  App Hub 接纳并固定这两个文件。`agent` 块中的 `profile` 和 `model` 会被接纳，但 Shell 还没有使用。
- **运行。** 在用户于首次使用面板上允许之前什么都不会运行（用户用顶栏的 “Ask <app>”、
  Shift+F8 或菜单项 “Ask this app's agent” 打开 Shell 的 “Ask <app>” 面板时，或系统
  Agent 用 `agents.ask` 询问时，弹出这个面板）。之后 Shell 准备好 peer，系统 Agent
  就能用 `peer_send_input` 找到它。只有系统 Agent、用户或卡片的卡内对话发起请求时才会
  开始一轮：还没有触发器或定时任务（ADR 0002 M3，计划中）。
- **直接与它对话。** 用户可以直接与应用的 Agent 对话，而不只是通过系统 Agent：在 Shell
  为每个拥有 Agent 的应用提供的 “Ask <app>” 面板里（这些应用都不绘制自己的对话界面），
  或在卡片的卡内对话里。这些回合在用户的通道里运行，与系统 Agent 的通道并列，带着应用的
  工具。面板的“停止”只停止用户自己的回合。手机上还没有打开这个面板的触控入口。详见根目录的
  [README](../README.zh-CN.md#直接与应用的-agent-对话)。
- **它的工具在应用的宿主服务上运行**，以应用的身份运行，在此之前 Shell 的中转已检查授权、
  schema 和预算。octos 只对破坏性和对外的工具请求审批（这里是 `calendar.remove_event`），
  由用户在 Shell 的面板上回答。
- **卡片。** `mail.notify`、`calendar.notify` 和 `calendar.agenda` 填充宿主服务自带的固定
  L0 卡片，并以应用的身份、带 `notify` 通过 Shell 的 `glance` 服务发布（应用需要 `glance`
  权限）。模型只提供文字，从不编写卡片代码。卡片里可以有与应用自己的 Agent 的卡内对话
  （`sys.chat`，[`../crates/l0-chat`](../crates/l0-chat/README.md)）。
- **试一试**（桌面端）：打开助手（F8），请系统 Agent 让邮件或日历的 Agent 在 glance 屏幕上
  放一张卡片；在弹出的面板上允许该 Agent。邮件需要一个已登录的账户（下文的演示邮箱即可）。
  邮件更完整的操作卡片（[计划（英文）](mail/docs/2026-10-01-email-action-card-plan.md)）目前只是
  使用假数据的演示：`OCTOSENSE_GLANCE_DEMO=mail` 会在启动时发布它。**未验证**（在 #267 的检查中
  运行过，没有为本页重新运行）。

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
  `keychain:` 标记，Linux 上是 `<core_dir>/secrets/`，其他平台写在 profile 中），
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
  `Cargo.toml` 的 `[workspace.dependencies]` 中唯一的 rev（目前是 octos `main` 上的
  `ae230ce0`），与 `crates/kernel` 和 Shell 共用。AppCard 不再自己启动内核，而是连接 Shell 的内核
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

1. 编辑 `apps/<name>/bundle/`。只使用 OctoScript-App-Design-Flow 的
   `docs/SCRIPT-API.md` 中有文档的 API，或本仓库其他应用已经在用的 API；
   用其他东西之前先查运行时源码。
2. 只申请应用实际用到的权限。新的网络主机写进 `network.hosts`；新的权限必须
   已存在于 App Hub 的 `KNOWN_CAPABILITIES` 中。
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
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | 目录、准入检查（`hub stamp`、`check`、`scan`、`sign-manifest`、`publish`）、`card-host`、Card runner 与宿主服务注册表，以及每个 Shell 都链接的 `octosense-app-hub-app` |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | 如何设计、构建、检查和发布应用 |
| [OctoScript](https://github.com/OctoSense-org/OctoScript)、[OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad)、[makepad](https://github.com/OctoSense-org/makepad) | 语言与运行时 |
| [Rinx](https://github.com/hagency-org/Rinx) | Matrix 聊天与小程序，原生模块；通过 `crates/app-peers` 访问助手 |
| [octos](https://github.com/octos-org/octos) | Agent 内核：由 `crates/kernel` 作为 Shell 服务运行，由 AI providers 配置，供 AppCard 等使用方使用（只用一个版本 `ae230ce0`） |

## 参与贡献

- 向 `main` 提 PR；绝不强推 `main`。
- 改动保持小，并在 Shell 中测试。遵循 [AGENTS.md](AGENTS.md)。
- `apps/` 下的改动必须通过 `apps.yml`，以及 Shell 的 `desktop.yml` 和 `phone.yml`。

## 历史与许可

本目录在 2026-09-27 之前是 OctoSense-System-Apps 仓库，已连同历史导入这里。
这些 bundle 和 Mail 服务最初写在 OctoSense-mobile（已归档）和
OctoScript-App-Design-Flow（原名 Octoscript-AppCard）中，历史记录保留在那里。
AppCard 来自 OctoSense-org/OctoSense-AppCard（`d0a836b8`），它是从
OctoScript-App-Design-Flow 的 `app/` 在 `cbbda4da` 拆分出来的。

Apache-2.0（[LICENSE](LICENSE)）。第三方组件见 [NOTICE](NOTICE)。
