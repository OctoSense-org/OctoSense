# OctoSense System Apps

[English](README.md) | 简体中文

[OctoSense](https://github.com/OctoSense-org/.github/blob/main/profile/README.zh-CN.md)
（运行在操作系统之上的 Agent 交互 Shell）自带的第一方应用：

- **新闻（News）、相册（Photos）、地图（Maps）、相机（Camera）、邮件（Mail）和 AI providers**
  是*隔离运行的脚本应用*。每个应用都是 `bundle/` 里的一个 OctoScript（Splash）
  程序，由 App Hub 的 Card runner 在独立的 isolate 中运行，权限严格等于其
  `manifest.json` 所申请的内容，与商店应用受到的隔离完全相同。它们同时也是
  任何开发者通过 App Hub 发布的应用形态的完整示例。
- **邮件的宿主服务**（`apps/mail/host-service`）是 Mail 的 Rust 部分：
  IMAP/POP3/SMTP、账户存储和登录面板，由 Shell 运行。应用拿到的是邮件，
  永远拿不到密码或 socket。
- **`llm` 宿主服务**（`apps/ai-providers/host-service`）是 AI providers 的 Rust
  部分：基于 octos 模型目录的大模型服务商、存放在平台密钥库中的密钥、“测试连接”，
  以及通过受 PIN 保护的 `OCTOS1E` 二维码在设备之间迁移服务商（相机、图片或粘贴）。
  密钥只在宿主自己的面板上输入，二维码也只在那里显示；应用只能看到打码后的状态。
- **octos 内核服务**（`crates/octos-core`，crate `octosense-octos-core`）：
  把 [octos](https://github.com/octos-org/octos) Agent 内核作为 Shell 服务。
  Shell 每个进程按需启动一个内核；AI 服务商应用（通过 `llm` 服务）配置它；
  AppCard 等使用方连接到它。见 [octos 内核](#octos-内核)。
- **AppCard**（`apps/appcard`）是唯一的原生应用：“Ask anything”助手，
  一个由 Shell 进程内链接的 Rust 模块（`octos-app`），运行在 Shell 的
  octos 内核之上。它**需显式启用**：两个 Shell 只有在使用 `--features app-appcard`
  时才链接它，默认不随产品发布。

在本仓库工作的 Agent 规则见 [AGENTS.md](AGENTS.md) 和
[apps/appcard/AGENTS.md](apps/appcard/AGENTS.md)。

**要开发自己的应用？** 不需要构建或修改本仓库。请从
[OctoSense-org 主页](https://github.com/OctoSense-org)的阅读列表开始（先读
OctoScript-App-Design-Flow 的 `AGENTS.md`，再读 `docs/QUICKSTART.md`），把这里的
应用包当作完整示例来读（`apps/<name>/bundle/main.splash`）。想在自己的应用旁边运行
其中一个：把本仓库克隆到同一个工作区，然后在 OctoScript-App-Design-Flow 中执行
`tools/octo run ../OctoSense-System-Apps/apps/photos/bundle --system --no-stamp --app-data /tmp/sys-apps`
（`--no-stamp` 不会改动本仓库的检出；Mail 需要在 Shell 中运行，见下文）。

## 应用一览

| 应用 | Id | 功能 | 权限（manifest） | 网络主机（manifest） | 宿主服务 |
| --- | --- | --- | --- | --- | --- |
| [News](apps/news/bundle) | `os.news` | Hacker News、TechMeme 和 Google News 的订阅源，分标签页（Today、HN、TechMeme、Google、Saved），带文章阅读器 | `storage`、`net`、`images`、`web` | `hn.algolia.com`、`www.techmeme.com`、`news.google.com` | 无 |
| [Photos](apps/photos/bundle) | `os.photos` | 示例相册：回忆、相簿、人物、收藏、可多选的网格、全屏查看器 | `storage` | 无 | 无（原图来自 Shell 的资源挂载，见下文） |
| [Maps](apps/maps/bundle) | `os.maps` | `MapView` 地图、地点搜索、地点详情、路线和驾驶模式；有 GPS 定位时从当前位置开始 | `storage`、`net`、`location` | `photon.komoot.io`、`router.project-osrm.org`、`overpass-api.de`、`overpass.kumi.systems`、`maps.mail.ru`、`overpass.openstreetmap.fr` | 无 |
| [Camera](apps/camera/bundle) | `os.camera` | 基于运行时 `CameraPreview` 控件的拍照和录像，闪光灯和变焦，最近一张的缩略图和查看器 | `storage`、`camera`、`microphone`、`library` | 无 | 无 |
| [Mail](apps/mail/bundle) | `os.mail` | 账户、文件夹、邮件列表、阅读（HTML 由服务重建）和写信 | `storage`、`mail` | 无（由服务联网，而不是应用） | [`mail`](apps/mail/host-service) |
| [AI providers](apps/ai-providers/bundle) | `os.ai-providers` | 助手的大模型服务商：一个主用与若干备用，每项都有来自 octos 模型目录的型号下拉菜单和“测试连接”；添加向导（系列、型号、线路、密钥、测试）；“为手机显示二维码”，以及通过相机、图片或粘贴导入 | `storage`、`llm` | 无（由服务联网，而不是应用） | [`llm`](apps/ai-providers/host-service) |
| [AppCard](apps/appcard) | 原生，需显式启用 | AppCard 助手：路由大脑选择或组合一个应用 Agent，由它生成实时的 Splash 或 webview 卡片。Shell 只在启用 `app-appcard` 时链接它；默认不发布 | 不适用（不是 bundle） | 不适用 | Shell 的 octos 内核 |

每项权限的含义由 App Hub 的封闭列表定义（`crates/app-policy/src/manifest.rs`
中的 `KNOWN_CAPABILITIES`）：`images` 可显示任意公网 https 主机的图片，`web`
在系统 WebView 中打开网页，`library` 把拍摄内容提供给系统相册，`mail` 访问
宿主的邮件服务，`llm` 访问宿主的大模型服务商服务。`net` 只能访问 manifest 列出的主机。

### 状态与已知问题

- **Camera**：在 OnePlus 6 测试中（2026-09-25），Camera 能拍照并在后台释放
  相机，但实时预览是纯黑的，尚未解决。桌面构建没有相机，Android 模拟器拒绝
  提供相机，因此其他环境下拍摄未经测试。
- **Photos**：bundle 只带 75 张缩略图（`bundle/thumbs/`，约 2 MB）。查看器
  显示的原图只有在 Shell 挂载后才会出现在 `{{assets}}/photos/...`：ROM Home
  挂载 `home/apps/photos/resources/photos`（位于 OctoSense-ROM，约 87 MB）；
  OctoSense-Desktop 不挂载任何目录，所以那里的查看器没有原图。
- **News、Maps**：开发时在 `card-host` 中运行过，但在 Shell PR 的测试中没有
  端到端验证（测试手机没有网络）。
- **Mail**：已在桌面和 OnePlus 6 上用演示邮箱验证。Mail 与 `llm` 两个宿主服务都固定引用 App Hub
  `46d67e51`（OctoSense-App-Hub#15 合并后的 main；#11 新增了 `llm` 能力，#14 新增了 Matrix 与 Octos 宿主服务能力），与 Shell 链接的版本相同，
  因此无需 `[patch]`，Shell 的依赖图中就只有一份 `octosense-appstore` 和一个
  宿主服务注册表。
- **脚本 bundle 在本仓库没有 CI。** `.github/workflows/appcard.yml` 只覆盖
  `apps/appcard/**`。
- **AppCard 的 `personal-data` 技能**读取旧原生 Mail 模块的 `mailbox-*.json`
  文件。脚本版 Mail 的邮件现在存放在宿主服务自己的目录
  （`<host_dir>/mail/box-*.json`），该技能大概率已读不到；未验证。
- 只有 Camera 自带启动器图标（`bundle/icon.png`），其他应用的图标由 Shell 绘制。

## Shell 如何使用本仓库

Shell 固定引用本仓库的某个版本，并选择要内置哪些应用：OctoSense-ROM 的 Home
（独立启动器和 ROM 镜像，自
[OctoSense-ROM#18](https://github.com/OctoSense-org/OctoSense-ROM/pull/18) 起）和
OctoSense-Desktop（自
[OctoSense-Desktop#36](https://github.com/OctoSense-org/OctoSense-Desktop/pull/36) 起）。
Shell 会：

1. 在 `native-apps.lock.json` 中固定本仓库版本（ROM：`home/native-apps.lock.json`，
   检出到 `.sources/system-apps`；Desktop：同级目录 `../OctoSense-System-Apps`）。
2. 在 `system-apps.json` 中列出应用，并在 `.cargo/config.toml` 中让
   `OCTOSENSE_SYSTEM_APPS` 指向它。App Hub 的 Shell crate `octosense-app-hub-app`
   在构建时读取该文件，把每个 `apps/<name>/bundle/` 打包进二进制并填入摘要。
   `assets` 把额外目录挂载到应用的 `{{assets}}` 下（ROM 中的 Photos）：

   ```json
   {
     "schema": 1,
     "source": "../.sources/system-apps/apps",
     "apps": ["news", "photos", "maps", "camera", "mail", "ai-providers"],
     "assets": { "photos": { "photos": "apps/photos/resources/photos" } }
   }
   ```

3. 链接宿主服务 `octosense-mail-service` 和 `octosense-llm-service`（对固定检出的
   path 依赖）并在启动时注册：`llm` 服务使用 octos 内核的 core 目录以及 Shell 的
   二维码扫描器和图片选择器（见 [`llm` 服务](#llm-服务)）；Mail 服务：
   真实账户用 `register()`，Shell 的应用配置中 `mail_demo: true` 时用
   `register_demo()`。Shell 链接的 App Hub 版本与该服务为 `octosense-appstore` 引用的版本相同，因此只有一个宿主服务注册表。
4. 链接 `octosense-octos-core`（两个 Shell 的 feature `octos-core`，默认开启），
   在启动时配置内核；以内核的 core 目录注册 `llm` 服务（开启其 `octos-core`
   feature），这样修改服务商会重启内核。见 [octos 内核](#octos-内核)。
5. 可选（需显式开启 `app-appcard`）以 `default-features = false` 链接 AppCard
   的 `octos-app`，并通过其 `AppShell` 控件挂载（见 [AppCard 助手](#appcard-助手)）；
   它连接的是同一个内核。

本仓库的改动只有在 Shell 升级固定版本（在该 Shell 仓库中提 PR）之后才会到达设备。

## 仓库结构

```
apps/<name>/bundle/          隔离运行的脚本应用：manifest.json、main.splash、图片资源
apps/mail/host-service/      octosense-mail-service，`mail` 宿主服务（Rust）
crates/octos-core/            octosense-octos-core：Shell 的 octos 内核（每进程一个，共享）
apps/appcard/                原生 AppCard 助手
  app/                       Cargo workspace：octos-app 及 store/transport/render crate
  a2app/                     Splash 卡片记忆（需求规格、控件模式、lint 规则），编译进应用
  a2app-l0/                  L0 卡片框架、目录和各应用示例卡片，编译进应用
  personal-data/             octos 技能：对邮件和日历数据的只读搜索
  vendor/                    内置的第三方 crate（rustyline、mmap-rs；见 NOTICE）
  tools/                     setup-native.py、octos macOS/OpenHarmony 启动器、build-android.sh 等
  docs/                      架构、构建和评审笔记
  native-runtime.lock.json   AppCard 构建所用的 Octoscript-Makepad 版本
.github/workflows/appcard.yml   apps/appcard 的 CI
.github/workflows/octos-core.yml   crates/octos-core 的 CI
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
# 在 OctoSense-App-Hub 的检出中
cargo build --release -p octosense-card-host --bin card-host

card-host --bundle <System-Apps>/apps/news/bundle --system
card-host --bundle <System-Apps>/apps/photos/bundle --system --static photos=<原图目录>
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
# OctoSense-Desktop（仓库根目录），或 ROM Home（在 home/ 中）
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense
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
| `lib.rs` | 服务本体：`mail.accounts`、`add_account`（弹出登录面板）、`remove_account`、`folders`、`sync`、`list`、`message`、`mark_read`、`send`；`register()`、`register_demo()`、`register_with*()`；`Transport` trait |
| `imap.rs` | IMAP 客户端（文件夹、已读标记回写服务器） |
| `network.rs` | POP3 和 SMTP、MIME 解码；错误信息中从不包含凭据 |
| `html.rs` | 把邮件重建为 Mail 的 `Html` 视图能绘制的少量标签，不含任何远程内容 |
| `vault.rs` | 密码的存放位置：macOS/iOS 钥匙串；Android 上用 Android Keystore 密钥加密的文件；其他平台为仅所有者可读的文件；`OCTOSENSE_MAIL_VAULT=file` 强制使用文件存储，便于未签名的开发构建 |

账户元数据（不含密码）和已拉取的邮件存放在宿主自己的目录（`<host_dir>/mail`），
位于所有应用沙箱之外。每个账户只授权给添加它的应用。服务会先测试账户可用，再保存。

### `llm` 服务

`octosense-llm-service`（`apps/ai-providers/host-service`）是 AI providers 的 Rust
部分。它把 octos 内核的大模型服务商保存在内核的 profile
`<core_dir>/profiles/_main.json` 中（由 `octosense-llm-config` 合并 `config.llm` 和
`config.env_vars`，其他键保持不变），密钥则放在 octos 读取的位置：macOS 上是钥匙串
`octos` 服务（profile 中写 `keychain:` 标记），Linux 上是 `<core_dir>/secrets/`，
其他平台（Android）写在应用私有的 profile 中。密钥只在宿主面板上输入，二维码只在
宿主面板上显示和扫描；应用只能看到打码后的状态。开启其 `octos-core` feature（Shell
的默认设置）后，它写入 `octosense_octos_core::core_dir()`，并在每次更改后调用
`octosense_octos_core::restart()`，让正在运行的内核读取新的服务商。方法列表与注册
方式见其 [README（英文）](apps/ai-providers/host-service/README.md)。

## octos 内核

octos Agent 内核是 **Shell 服务**，不属于任何应用。
[`crates/octos-core`](crates/octos-core)（`octosense-octos-core`）就是这个服务；
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

测试：`cd crates/octos-core && cargo test`；有编译好的 `octos` 时，
`OCTOS_CORE_TEST_KERNEL=<octos> cargo test --test real_kernel` 会用
`octosense-llm-config` 写入的 profile 启动真实内核，并在修改服务商后重启它。
详见 [crates/octos-core/README.md](crates/octos-core/README.md)。

## AppCard 助手

即“Ask anything”磁贴。你输入一个请求；路由大脑（AMA）选择或组合一个应用 Agent；
该 Agent 生成一张实时卡片（Splash 或 webview），在渲染时绑定真实数据。它通过
octos UI Protocol v1 与 octos 通信。

- **代码**：`apps/appcard/app`，一个 Cargo workspace，包括 `octos-app`（路由、
  组合、多 Agent 调度、Splash 渲染与校验、L0 卡片生成、WebView 浮层）、
  `octos-app-store`（状态 reducer，不依赖 Makepad）、`octos-app-transport`
  （经由 Shell 内核、WebSocket 或 REST 的 octos UI Protocol 客户端）和
  `octos-app-render`（流式 markdown 渲染）。
- **octos**：所有 octos crate 都来自 git `octos-org/octos`，版本为
  `apps/appcard/app/Cargo.toml` 中唯一的 rev（目前是 octos `main` 上的
  `a6ea8505`）；`crates/octos-core` 在 OpenHarmony 上引用同一 rev。同样依赖 octos
  的 Shell 必须使用同一 rev。AppCard 不再自己启动内核，而是连接 Shell 的内核
  （见 [octos 内核](#octos-内核)）。
- **Makepad**：不内置。Makepad、Octoscript 和 Octoscript-Makepad 是与本仓库
  *同级*的检出，版本由 `apps/appcard/native-runtime.lock.json` 选定。

构建与测试（详见 [apps/appcard/README.zh-CN.md](apps/appcard/README.zh-CN.md)）：

```sh
cd apps/appcard
python3 tools/setup-native.py                       # 准备同级运行时
python3 tools/setup-native.py --check --cargo-manifest app/Cargo.toml
PYTHONPATH=tools python3 -m unittest core.test_native_runtime

cd app
cargo check
cargo test --workspace
cargo clippy -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
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
的应用根视图）的控件。`AppShell::ask` 像用户输入一样提交文本。在 ROM 和
Desktop Shell 中，它被包在实现了 Shell 的 `AppModule` trait 的 `AppCardModule`
里（分别位于这两个仓库的 `home/apps/appcard` 和 `apps/appcard`）。

**CI**：[.github/workflows/appcard.yml](.github/workflows/appcard.yml) 在
`apps/appcard/**` 有改动时运行（macOS）：准备锁定的运行时，运行运行时锁测试，
对四个 crate 运行 clippy（这一步会编译整个应用），并检查依赖图中只有一份 octos
和一份 Makepad。`apps/appcard/app/.github/workflows/` 是原仓库遗留的，在这里不会运行。

## 修改应用

1. 编辑 `apps/<name>/bundle/`。只使用 OctoScript-App-Design-Flow 的
   `docs/SCRIPT-API.md` 中有文档的 API，或本仓库其他应用已经在用的 API；
   用其他东西之前先查运行时源码。
2. 只申请应用实际用到的权限。新的网络主机写进 `network.hosts`；新的权限必须
   已存在于 App Hub 的 `KNOWN_CAPABILITIES` 中。
3. 绝不添加密码或验证码输入框。应用需要密钥时，由宿主服务及其面板处理。
4. 用 `card-host --system` 运行（Mail：在 Shell 中用演示邮箱）。在手机上通过
   ROM 的 Home 以独立测试包的方式测试，绝不替换设备上已安装的 Home。
5. 在本仓库提 PR。合并后，在每个 Shell 仓库提 PR 升级固定版本（`native-apps.lock.json`）。

**新增**系统应用即新建一个 `apps/<name>/bundle/`，id 为 `os.<name>`，并在每个
Shell 的 `system-apps.json` 中加入它。

## 测试

| 对象 | 方法 |
| --- | --- |
| Mail 服务 | 在链接了它的 Shell workspace 中：`cargo test -p octosense-mail-service`（ROM：在 `home/` 中）。钥匙串测试默认忽略：`cargo test -p octosense-mail-service -- --ignored keychain` |
| octos 内核服务 | `cd crates/octos-core && cargo test`（替身内核）；`OCTOS_CORE_TEST_KERNEL=<octos> cargo test --test real_kernel`（真实内核）；CI 见 `octos-core.yml` |
| `llm` 服务 | 在 `apps/ai-providers` 中：`cargo test --workspace --features octosense-llm-service/octos-core` |
| AppCard | 上文的命令；CI 见 `appcard.yml` |
| 脚本 bundle | 在 `card-host` 和 Shell 中手动测试，通过 `MAKEPAD_REMOTE` 操控。本仓库暂无自动化 UI 测试 |

## 相关仓库

| 仓库 | 作用 |
| --- | --- |
| [OctoSense-ROM](https://github.com/OctoSense-org/OctoSense-ROM/blob/main/README.zh-CN.md) | 手机 Shell（`home/`）：独立启动器或烧录进 ROM 镜像 |
| [OctoSense-Desktop](https://github.com/OctoSense-org/OctoSense-Desktop) | 桌面 Shell |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | 目录、准入检查（`hub stamp`、`check`、`scan`、`sign-manifest`、`publish`）、`card-host`、Card runner 与宿主服务注册表，以及每个 Shell 都链接的 `octosense-app-hub-app` |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | 如何设计、构建、检查和发布应用 |
| [OctoScript](https://github.com/OctoSense-org/OctoScript)、[OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad)、[makepad](https://github.com/OctoSense-org/makepad) | 语言与运行时 |
| [octos](https://github.com/octos-org/octos) | Agent 内核：由 `crates/octos-core` 作为 Shell 服务运行，由 AI providers 配置，供 AppCard 等使用方使用（只用一个版本 `a6ea8505`） |

## 参与贡献

- 向 `main` 提 PR；绝不强推 `main`。
- 改动保持小，并在 Shell 中测试。遵循 [AGENTS.md](AGENTS.md)。
- AppCard 的改动必须通过 `appcard.yml`。

## 历史与许可

这些 bundle 和 Mail 服务最初写在 OctoSense-mobile（已归档）和
OctoScript-App-Design-Flow（原名 Octoscript-AppCard）中，历史记录保留在那里。
AppCard 来自 OctoSense-org/OctoSense-AppCard（`d0a836b8`），它是从
OctoScript-App-Design-Flow 的 `app/` 在 `cbbda4da` 拆分出来的。

Apache-2.0（[LICENSE](LICENSE)）。第三方组件见 [NOTICE](NOTICE)。
