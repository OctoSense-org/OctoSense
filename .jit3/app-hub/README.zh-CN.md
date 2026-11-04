# OctoSense App Hub

[English](README.md) | 简体中文

App Hub 为 OctoSense 发布应用。本仓库包含：

- 每个 OctoSense 商店都会读取的签名目录；
- App Hub 为每个已准入应用包保存的副本；
- 准入检查（每个应用包都必须通过）、签名、商店和参考宿主的代码。

每个应用的源码留在发布者自己的仓库中。

| 想找 | 仓库 |
| --- | --- |
| 如何开发应用：快速上手、脚本 API、脚本应用模板、设计流程、示例 | [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)（原 Design Flow） |
| AppCard 助手运行时（Shell 中需 `--features app-appcard` 才启用） | [OctoSense `apps/appcard`](https://github.com/OctoSense-org/OctoSense/tree/main/apps/appcard) |
| 第一方系统应用（AI 提供方、日历、相机、邮件、地图、新闻、相册、YouTube）及其宿主服务（`llm`、`model`、`calendar`、`mail`、`news`） | [OctoSense `apps/`](https://github.com/OctoSense-org/OctoSense/tree/main/apps) |
| L0 解析器和检查器，以及 Makepad 转换层和渲染层 | [OctoScript](https://github.com/OctoSense-org/OctoScript) 与 [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) |
| 应用包格式、准入检查、签名、提交与商店 | 本仓库 |

**要开发应用？** 请从 [OctoSense-org 主页](https://github.com/OctoSense-org)上的“先读这些”列表开始：先读 App Flow 的 `AGENTS.md`，再读其 `docs/QUICKSTART.md`。接着按照[开发你的第一个 Hub 应用](docs/FIRST-APP.zh-CN.md)和[应用图标与随包素材](docs/ICONS.zh-CN.md)操作。把本仓库克隆到 App Flow 检出目录旁边，用来构建 `hub` 和 `card-host`。不要修改 `catalog.json`、`index/` 或 `artifacts/`。

**要提交应用？** 按照[向 App Hub 提交应用](docs/SUBMITTING.zh-CN.md)，在本仓库开 issue 请求发布，填写仓库、版本和所需能力。Release 还没准备好时也可以先开，之后再补充 tag、commit、截图和经过验证的 Release pack；只打 tag 或只创建 GitHub Release 都不算提交。该指南的四个阶段涵盖审核、批准和发布；每个命令由谁运行、何时运行，见其中的 [`hub` 命令](docs/SUBMITTING.zh-CN.md#hub-命令)一节。

签名目录中的[三个参考应用](docs/SUBMITTING.zh-CN.md#三个参考应用)（GitHub Notes、Inbox Assistant 和 Google Calendar）完整通过了准入，它们的仓库展示了一次完整的提交。规则、能力和字段请查阅[发布参考](docs/PUBLISHING.zh-CN.md)。

**要使用宿主 API？** [宿主 API 兼容性](docs/HOST-API.zh-CN.md)说明应用如何声明所需的宿主 API，以及如何查询宿主实现了哪些 API。

## 下载兼容宿主

[**桌面版 0.1.0-rc.4**](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4) 是兼容的预发布版，
源码为 `9266b008`。它包含契约 1.11、仅作披露的能力策略、Wasm 核心模块与组件，
以及共享组件目录和安装支持。已有宿主 API 和连接账户流程仍受平台、用户同意及
提供商限制约束。

| 平台 | 下载 |
| --- | --- |
| macOS Apple 芯片 | [DMG](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/OctoSense_0.1.0-rc.4_aarch64.dmg) 或 [应用 ZIP](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/OctoSense_0.1.0-rc.4_macos_aarch64.app.zip) |
| Windows x64 | [安装程序](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/octosense_0.1.0-rc.4_x64-setup.exe) |
| Linux x86_64 | [Debian 包](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/octosense_0.1.0-rc.4_amd64.deb) 或 [AppImage](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/octosense_0.1.0-rc.4_x86_64.AppImage) |

请用 [SHA256SUMS](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/SHA256SUMS) 核对下载，并阅读
[发布说明](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4)中的平台限制。
这些安装包**没有 Apple Developer ID 签名、公证或 Windows Authenticode 签名**。Mac 二进制只有链接器临时签名；三个平台的打包隐私扫描均已通过。
[发布文件](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4)包含各平台打包回执和确切校验和。
Mac app ZIP 在候选包首次验收中通过 **11/11 项 App Hub/组件检查**；最终发布 ZIP 与 DMG 的哈希和已测文件一致。这不代表 Windows/Linux 安装后的界面、真实模型行为或账户写入已验证。
自行构建请按[固定源码的环境准备指南](https://github.com/OctoSense-org/OctoSense/blob/9266b0083544d86bd7636543b5ff60c61b26460f/README.zh-CN.md#环境准备)操作。

下面四个示例请在 **Apple 芯片 Mac** 上运行。打开 **App Hub → Search**，
输入确切应用 ID，选择 **Get**，审阅用途披露后选择 **Install**，再点 **Open**。
**Library** 可重新打开已安装应用，并在有兼容新版本时提供 **Update**。
保留默认公开目录，不需要自定义来源、信任锚、开发者密钥或 OctoSense 云端账户。
旧桌面 beta.2 宿主无法读取新的发布者证明或公开 v2 目录：请换用兼容宿主，
不要修改应用证明或目录设置。

RC4 支持共享组件，但用户能够找到某个组件或应用之前，它仍须单独通过 Hub 审核
并发布到目录。安装此桌面版不会升级 Android Home；现有手机版与隔离的 OnePlus
验收测试包是不同的内容。

RC4 **没有附带公开的 GitHub/Google OAuth 客户端注册信息**。本地草稿和未连接
账户时的界面可用；提供商登录需要宿主发行方或运维人员提供注册信息
（[配置](https://github.com/OctoSense-org/OctoSense/blob/9266b0083544d86bd7636543b5ff60c61b26460f/crates/oauth-service/README.zh-CN.md)）。
普通用户不应被要求注册 Google 开发者项目。安装成功不代表登录、邮件投递、
GitHub commit 或日历写入已验证。连接账户示例只声明 macOS。Linux/Windows 已实现
外部浏览器后端登录和清单声明的后端读取。RC2 记录的 Windows 模拟后端登录结果
属于历史验证，不是新安装 RC4 包的验收。Linux 拒绝嵌入式后端登录；Windows 运行时
虽能拦截回调，但 RC4 的宿主嵌入式登录流程仍关闭，没有可启用的生产开关。两端都使用
外部浏览器登录；受保护的写操作仍不支持，会拒绝执行。Android Google 授权仍不可用。普通 WebReader 与登录是不同的流程；
Linux 需要 GTK 3/WebKitGTK 和 X11/XWayland，Windows 需要 WebView2。这些引擎不随包附带，
见[浏览器要求](https://github.com/OctoSense-org/OctoSense/blob/9266b0083544d86bd7636543b5ff60c61b26460f/docs/desktop-embedded-browser.zh-CN.md)。

## 仓库结构

| 路径 | 说明 |
| --- | --- |
| `catalog-v2.json` | 新兼容宿主默认选择的 GitHub 证明目录；商店使用前验证完整证明。 |
| `catalog.json` | 保持原样的旧格式签名目录，供旧宿主和显式选择旧格式的镜像使用。 |
| `index/<app>-<version>.json` | 每个应用版本对应一条已准入条目：清单、发布者、源码位置与状态。维护者在 `hub publish` 之后从目录导出。 |
| `artifacts/<app>-<version>.bundle/` | App Hub 保存的应用包副本，与审核时的字节完全一致。由 `hub publish` 生成。 |
| `artifacts/<app>-<version>.bundle.pack.json` | 打成单个文件的同一应用包，商店下载的就是它。 |
| `artifacts/<component>-<version>.wasm` 和 `index/components/<component>-<version>.json` | 供应用固定的共享组件版本：App Hub 保存的已审核文件副本及其已准入条目（[ADR 0003](docs/adr/0003-shared-components.zh-CN.md)）。目前尚未发布任何组件。 |
| `docs/FIRST-APP.md` | 第一个应用（卡片应用或脚本应用）的分步教程：创建、运行、截图与检查。 |
| `docs/SUBMITTING.md` | 提交流程的四个阶段、每个 `hub` 命令由谁运行，以及从仓库结构到发布的每一步。 |
| `docs/PUBLISHING.md` | 参考文档：准入规则；能力及其提供方；清单、商店信息和工具的字段；宿主服务；`hub` 命令；签名。 |
| `docs/GITHUB-PUBLISHING.md` | 管理员授权的 GitHub 签名、确切候选审核及 v2 迁移。 |
| `docs/HOST-API.md` | 声明应用需要的宿主 API、用 `runtime` 发现宿主 API，以及目前各宿主实现了哪些 API。 |
| `docs/ICONS.md` | 规范图标的归属、导出约束与视觉评审。 |
| `docs/DEVELOPMENT.md` | 指南导航、交付路径、`card-host` 及其远程控制路由，以及 `card-studio`。 |
| `docs/adr/` | 架构决策：通过 GitHub 管理员身份认证目录（[ADR 0001](docs/adr/0001-github-attested-catalog.zh-CN.md)）、通过 GitHub 证明发布者身份（[ADR 0002](docs/adr/0002-github-attested-publisher-identity.zh-CN.md)），以及共享组件（[ADR 0003](docs/adr/0003-shared-components.zh-CN.md)，提议中）。 |
| `templates/app/` | 卡片应用仓库脚手架，包含元数据、示例图标和链接好的 Agent 指引。 |
| `crates/app-contract` | 应用契约 `octosense-app-contract`（[OctoSense ADR 0005](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0005-app-contract.md)）：清单、应用获得的策略、应用包完整性校验与运行应用包所需的内容。在 `1.x` 内只做增量变更（[README](crates/app-contract/README.md)）。契约 1.11.0 已发布到 crates.io（[crates.io 上的版本](crates/app-contract/README.md#versions-on-cratesio)，英文）。 |
| `crates/app-policy` | 签名清单与商店信息、准入，以及解析为隔离环境设置和 Agent 会话配置（[OctoSense Home ADR 0002](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/home/0002-agentic-app-security-model.md)）；应用自带的 Agent（`tools.json`、`AGENT.md`、技能）加载为 `AgentBundle`；原生模块的工具清单共用同一套 `tools.json` 解析与检查（`ToolManifest::load`）。它还重新导出应用契约。 |
| `crates/app-hub` | 索引、签名目录、准入检查、Agent 扫描、设备端客户端和 `hub` 命令（[OctoSense Home ADR 0003](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/home/0003-app-hub-and-store.md)）。 |
| `crates/appstore` | 作为 OctoSense 模块的商店；把已安装应用作为独立客户端运行的 `card` 模块；系统应用（`os.` 前缀 id）；以及宿主服务及其面板。 |
| `crates/appstore-app` | 作为独立应用的商店（`appstore`）。 |
| `crates/card-host` | 参考的隔离宿主：运行单个应用包，可以是卡片应用，也可以是脚本应用（`card-host`）。 |
| `crates/card-studio` | 一个工具：在隐藏的 `card-host --remote` 中按速览卡片、手机和桌面尺寸渲染卡片，运行测量检查，并准备视觉评审（`card-studio`，[OctoSense ADR 0002 第 7 节](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0002-event-driven-app-agents.md#7-cards-l0-grounded-rendered-and-critiqued-before-publishing)）。 |
| `crates/app-host` | 单窗口宿主，可把任意 OctoSense `AppModule` 作为独立应用运行。 |
| `crates/app-hub-app` | 每个 OctoSense Shell 都会链接的集成：原生商店模块、`card` 运行模块、由 `OCTOSENSE_SYSTEM_APPS` 指定的系统应用、已安装应用和图标（[README](crates/app-hub-app/README.md)）。 |
| `skills/card-studio` | 基于 `card-studio` 的 octos 技能：`card_render`、`card_critique_payload`。 |

## 构建与测试

这些 crate 基于固定版本的 OctoSense Makepad 复刻、OctoScript-Makepad 和 OctoScript 构建，从同级检出目录（`../makepad`、`../octoscript-makepad`、`../octoscript`）解析依赖；[开发你的第一个 Hub 应用](docs/FIRST-APP.zh-CN.md#1-准备工具和应用仓库)会准备好这些检出目录。构建两个开发工具：

```sh
cargo build --release -p octosense-card-host -p octosense-app-hub
```

不做 release 构建时，可以直接从源码运行 `hub`：`cargo run -p octosense-app-hub --bin hub`。[运行合适的宿主](docs/CODE-WALKTHROUGH.md#2-run-the-right-host)（英文）列出了各个包的测试。[原生工具 CI](.github/workflows/native-tools.yml) 在 macOS、Windows 和 Linux 上运行这条构建命令和这些测试，所用的同级检出目录没有打 OctoSense 的运行时补丁。原生字体渲染检查在 macOS 图形会话中运行；其他任务覆盖构建和测试。

只构建你需要的包。如果一次构建包含按默认特性编译的商店相关包（例如 `cargo test --workspace`），`../makepad` 还必须打上 OctoSense 的运行时补丁。如果构建失败并出现 `no variant … TextInputStateQuery`，请参阅 [`card-host` 构建失败](docs/DEVELOPMENT.zh-CN.md#card-host-构建失败)。

商店界面在桌面和手机布局中使用同一套控件。要预览它，先构建它的示例，再为它指定一个单独的应用数据目录并运行：

```sh
cargo build --locked --release -p octosense-app-hub-app --example preview
OCTOSENSE_PREVIEW_SIZE=1200x860 OCTOSENSE_APP_DATA="$(mktemp -d)" target/release/examples/preview
```

`OCTOSENSE_PREVIEW_SIZE` 设定窗口尺寸：桌面用 `1200x860`，手机尺寸用默认的 `406x820`。手机尺寸的窗口并不等于在手机上测试。做自动化检查时，再设置 `MAKEPAD_HIDE_WINDOWS=1` 和 `MAKEPAD_REMOTE=<port>`。预览会按默认特性构建商店，因此同样需要上述运行时补丁。在预览中安装应用，仍要经过商店的同意和准入检查。

## 代码导读

先看[运行合适的宿主](docs/CODE-WALKTHROUGH.md#2-run-the-right-host)（英文）。导读接着追踪应用包如何进入 UI、宿主服务的请求如何回到回调函数，以及一次查询已保存笔记的应用 Agent 请求。仓库协作规则见 [AGENTS.md](AGENTS.md)。

## 宿主目前提供什么

准入检查校验用途声明；每次调用仍需要宿主实现并核对实际授权。当前源码把能力和目的地字段用作披露信息（见[策略](docs/PUBLISHING.zh-CN.md#当前源码策略声明与授权)）；RC4 在桌面版中包含这项策略、Wasm 组件和共享组件安装，并使用契约 1.11。应用与 ABI 准入、配额、用户同意、账户隔离及写入审阅仍保留。目录兼容不等于发布组件。下方清单保留较早的 RC2 基线。
完整参考见[能力](docs/PUBLISHING.zh-CN.md#能力)。[历史 RC2 发行版](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2)包含 RC1 的全部内容，以及标为 RC2 的新增项：

- 公开 GitHub v2 目录及 `publisher-github-v1` 验证，使用契约 1.10.0（RC1 为 1.8.0）。
- `runtime` 发现和 `script-tools-v1`：已准入的 `implemented_by: "app"` 工具
  调用已打开的完整应用中的 Splash 处理函数；应用关闭时返回 `app_not_running`。
- 已连接账户服务（`auth`、`github`、`gmail`、`gcalendar`），受提供商注册和平台
  限制约束。令牌保留在宿主中。
- macOS、Android、Linux 和 Windows 上由清单声明的后端读取；受保护的写操作需要
  受支持的原生批准流程。相机、麦克风和位置的逐应用授权支持 macOS/Android；
  `location.get` 仅在 Android 上提供，RC2 的 `location.sample` 在 macOS 和 Android 上读取新鲜位置。
- 支持平台上的 GitHub/Calendar 保存和 Gmail 发送需要亲手确认；Agent 发布速览
  卡片时拒绝可执行 Splash `script`；Calendar 同步限定日期范围。
- `model` 的媒体及文本方法，仍需已配置且有相应权益的提供商。方法已注册不代表
  已通过真实提供商执行验证。
- RC2：文档导入导出和照片选择（`files.*`），支持 macOS、Windows 和 Android，Linux 需安装对话框辅助程序；Android 上可分享文本；可向应用存储写入二进制文件（`fs.write_bytes`）。单文件上限 1 MiB。
- RC2：设备日历（`device_calendar.*`），macOS 上通过 EventKit 提供，Android Home 上通过其适配器提供，写入须亲手点按确认；与系统日历的实际交互仍待验收。
- RC2：任何获得 `mail` 授权的应用都可以保存邮件草稿（`mail.compose`、`mail.compose_status`）并请求原生发送审阅（`mail.review_send`）；审阅只在 macOS 和 Android 上可以批准。
- RC2：前台音频播放和录音（`audio.*`、`microphone.record_*`），支持 macOS 和 Android，文件位于应用存储内；硬件验收待完成。
- RC2：`Video` 控件的播放控制接口，以及 Windows 和 Linux 上 `LinkLabel` 的原生链接打开方式（浏览器启动未验证）。

`card-host` 仅提供 `runtime` 发现，不运行 Agent，并拒绝封存的 Release 和仅宿主
支持的要求标记。`calendar`、`llm`、`news` 仍为系统应用服务；`photos`/`youtube`
通知仅供各自的系统应用使用。Wasm **在 RC2 的 macOS 和 Linux 上已启用**：标准构建运行应用自带的 `fns/*.wasm`，属于有限支持；Windows、iOS 和 OpenHarmony 的构建不包含它，RC1 及更早的发行版都未启用。受支持的 Android Home 源码构建也运行它。声明能力不能添加原生代码或开启构建特性。见[宿主 API 兼容性](docs/HOST-API.zh-CN.md)。

## 信任锚

旧版商店信任这个锚，并沿着它的证书找到为 `catalog.json` 签名的工作密钥。轮换工作密钥不需要发布新版商店。

```text
6000284a069ba7cada2925094074e8e0baae07e25d1b7fc31f396c993f363e11
```

## 让商店读取其他 Hub

新兼容宿主默认选择 `github-v2`，从本 Hub 获取 `catalog-v2.json`。`OCTOSENSE_HUB` 可指定另一个目录或基础 URL，但更换来源不会更换可信 GitHub 身份或渠道。镜像必须提供同一份可验证的 v2 信封和产物。

使用**旧格式本地测试 Hub** 时，显式选择 `legacy`、提供测试信任锚，并使用新的应用数据目录：

```sh
OCTOSENSE_HUB_CATALOG=legacy OCTOSENSE_HUB=/path/to/legacy-mirror \
OCTOSENSE_HUB_ANCHOR="<test-anchor-hex>" OCTOSENSE_APP_DATA="<fresh-test-directory>" \
appstore
```

此示例仅按源码核对；运行前替换占位符。应用库只要已有 v2 缓存，即使损坏，也会拒绝降级到旧格式。旧缓存不会离线转换：第一次获取 v2 需要网络，或显式提供含有效 v2 信封的镜像。拉取或证明失败不会回退。较旧的宿主发布版继续读取 `catalog.json`；公开 v2 目录需要[兼容的 RC2 发行版](#下载兼容宿主)。

## Hub 如何发布应用

你开了提交 issue（[向 App Hub 提交应用](docs/SUBMITTING.zh-CN.md)）之后，审核人员会对 Release 中的原样字节运行准入检查，并在 issue 中反馈发现的问题。随后由 App Hub 管理员批准这次提交；未经批准，Hub 不会发布任何内容。管理员通过 [GitHub 管理员发布](docs/GITHUB-PUBLISHING.zh-CN.md)发布已批准的条目：GitHub Actions 用 Sigstore 为新的 `catalog-v2.json` 签名，因此管理员无需另外保管 Hub 私钥。RC1 和 RC2 发行版读取 `catalog-v2.json`；OctoSense 桌面版 0.1.0-beta.2 只读取 `hub publish` 生成、由信任锚签名的旧格式 `catalog.json`。

v2 的撤回通过相同的受保护工作流发布带理由的新候选，保留历史。商店在获取并验证新目录后执行撤回。`hub withdraw` / `hub remove` 仅修改旧格式目录，不能代替 v2 发布。

## 应用

已认证的公开**签名目录第 14 版**提供以下 GitHub 发布者身份。三个历史
`org.octosense.samples.*` 应用的 0.1.0 和 0.1.1 版本均已撤回。旧安装的数据保留，
新 ID 不会迁移这些数据。RC1 仍将撤回的应用显示为不可用；RC2 包含 [App Hub #170](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/170) 的发现列表修复，会在浏览及搜索中隐藏这些条目。

| 应用 | 确切应用 ID | 最新版本 | 运行平台 |
| --- | --- | --- | --- |
| [GitHub Notes](https://github.com/ymote/octosense-github-notes/releases/tag/v0.2.1) | `io.github.ymote.githubnotes` | 0.2.1 | macOS |
| [Inbox Assistant](https://github.com/ymote/octosense-inbox-assistant/releases/tag/v0.2.1) | `io.github.ymote.inboxassistant` | 0.2.1 | macOS |
| [Google Calendar](https://github.com/ymote/octosense-google-calendar/releases/tag/v0.2.1) | `io.github.ymote.googlecalendar` | 0.2.1 | macOS |
| [Camera Card Demo](https://github.com/ymote/camera-card/releases/tag/v1.1.1) | `io.github.ymote.cameracard` | 1.1.1 | macOS |

四个应用都是开发者预览。Camera Card Demo 是**静态 L0 界面**，不是拍照应用。
三个连接账户的应用保留本地草稿，访问提供商时需要宿主配置 OAuth；Calendar 的
Agent 提供建议，不会创建日程。所有应用都不需要 OctoSense 云端账户。

公开的[首次准入候选](catalog-candidates/ymote-github-samples-first/admission-review.json)及
[更新候选](catalog-candidates/ymote-github-samples-updates/independent-review.json)记录了经审核
的 Release。[撤回审核](catalog-candidates/retire-legacy-connected-samples/review.json)记录了
六次撤回。目录 14 保留之前的条目，包括这些 ID 的 0.2.0/1.1.0 版本。
历史 [0.1.1 准入](docs/admissions/connected-apps-0.1.1/README.zh-CN.md)及
[0.1.0 证据](reviews/connected-apps-0.1.0/README.zh-CN.md)保持原样，描述的是旧
Ed25519 身份，而不是新的发布路径。安装方法及当前账户/平台限制见
[下载兼容宿主](#下载兼容宿主)。
