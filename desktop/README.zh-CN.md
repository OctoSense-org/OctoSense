# OctoSense desktop

[English](README.md) | 简体中文

[OctoSense](https://github.com/OctoSense-org)（运行在操作系统之上的 Agent 交互 Shell）的桌面端 Shell，也是 OctoSense 仓库中的桌面端打包（原为 OctoSense-Desktop 仓库）。它是一个 Makepad 窗口，这个窗口本身就是桌面：launcher、dock 和平铺窗格（tile）。系统应用和 App Hub 商店应用以隔离的脚本程序运行，受信任的原生模块在进程内运行，Makepad 开发者程序作为子进程运行。它获取应用的方式与手机 Shell [Home](../phone/README.zh-CN.md) 完全相同。环境准备、仓库结构和 CI 见[根目录 README](../README.zh-CN.md)。

**要开发 OctoSense 应用？** 构建、检查和发布应用都不需要本仓库：请从 [OctoSense-org 主页](https://github.com/OctoSense-org)的阅读列表开始（先读 OctoScript-App-Design-Flow 的 `AGENTS.md`，再读 `docs/QUICKSTART.md`）。只有想在发布前在桌面 Shell 中看到自己的应用时，才需要构建本 Shell（见[发布前试用自己的应用](#发布前试用自己的应用)）。

## 在仓库体系中的位置

| 位置 | 对桌面端的作用 |
| --- | --- |
| [`../phone/`](../phone/README.zh-CN.md) | Home，手机 Shell。相同的应用模型、相同的运行时、相同的系统应用。 |
| [`../apps/`](../apps/README.zh-CN.md) | 新闻、相册、地图、相机、邮件、AI 提供商和 YouTube 的应用包，邮件与 `llm` 宿主服务，AppCard 助手（`octos-app`，需显式启用，默认不随产品发布），以及 Reference。 |
| [`../crates/`](../crates/) | Shell 本身（`crates/shell`，包名 `octosense-shell`，本包包装它）、它的 AI 服务（`crates/ai-host`）、octos 内核服务（`crates/kernel`，包名 `octosense-kernel`）和应用与 Agent 之间的代理（`crates/app-peers`）。 |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | 签名目录、商店和 Card 运行器。以 Git crate `octosense-app-hub-app` 链接。 |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | 设计、构建应用并发布到 App Hub 的地方。 |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | 固定 Makepad 与 OctoScript 版本的运行时发布。检出在 `.sources/` 中。 |
| [makepad（OctoSense 分支）](https://github.com/OctoSense-org/makepad) | 框架。检出在 `.sources/makepad` 中。 |
| [octos](https://github.com/octos-org/octos) | Agent 内核，一项 Shell 服务（`octos-core`，默认开启）：AI 提供商配置它，AppCard、Rinx 等使用方连接它。只有一个版本，固定在根目录 `Cargo.toml` 中；内核本身是单独的二进制（桌面：`OCTOS_APP_CORE_BIN`；Android：打包的 `liboctos.so`）。 |
| [Rinx](https://github.com/hagency-org/Rinx) | Matrix 聊天与小程序，作为模块链接（`app-rinx`，默认开启）。 |

## `desktop/` 的结构

| 路径 | 内容 |
| --- | --- |
| `src/main.rs` | 入口（包名 `octosense`）：在 Shell 的 `App` 上调用 `octosense_main!()`。Shell 本身（平铺、launcher、dock、顶栏、托管、应用注册表、`shell/`、`octosense/`）在 [`../crates/shell/src`](../crates/shell/src)。 |
| `config/apps.json` | 默认的开发者程序目录。`apps.makepad.json` 是供 `--apps` 使用的相同副本；`apps.overlay.json` 保存重新生成时应用的调整。 |
| `system-apps.json` | 本构建打包哪些系统应用，以及从哪里打包（`../apps`）。 |
| `scripts/` | `upstream.py`（WM 来源记录与目录重新生成）、`smoke.py`（原生冒烟测试）及它们的 Python 测试，`system_apps_remote.sh`、`ai_providers_remote.sh` 和 `glance_remote.sh`（以隐藏窗口 `--remote` 端到端运行系统应用、AI 提供商与一览屏），以及 `provision-appcard-llm.sh`（Android）。 |
| `upstream/makepad.json` | 从 Makepad `apps/wm` 导入的每个文件的来源记录。 |
| `resources/android/` | Android manifest 模板。主题、壁纸、图标和启动脚本属于 Shell，在 [`../crates/shell/resources`](../crates/shell/resources)。 |
| `docs/` | [验证记录](docs/validation.md)、[上游同步](docs/upstream.md)、[本地 AI](docs/local-ai.md)、[Android AppCard 构建](docs/android-appcard-build.md)、按日期的计划。 |
| `KEYBINDINGS.md`、`BACKLOG.md` | 快捷键说明；待办事项。 |

桌面端链接的模块位于仓库的其他位置：Reference 在 `../apps/reference`，AppCard 模块在 `../apps/appcard/module`（`octos-app` 在 `../apps/appcard/app/app`）。

## 前置条件

- 稳定版 Rust（`cargo` 位于 `~/.cargo/bin`）以及所在系统的原生工具链。macOS 上需要 Xcode Command Line Tools（`xcode-select --install`）。
- Git 和 Python 3.9+（用于 `tools/setup.py`）。`scripts/upstream.py` 需要 Python 3.11+（它导入 `tomllib`）。
- 首次准备和构建时需要联网。

## 环境准备

在仓库根目录准备一次固定版本的框架源码：

```sh
python3 tools/setup.py                  # makepad, octoscript, octoscript-makepad into .sources/
python3 tools/setup.py --check --cargo  # verify: one Makepad, App Hub, octos and Rinx in the graph
```

详细说明以及 `--update`、`--cache`：见[根目录 README](../README.zh-CN.md#环境准备)。

## 构建与运行

准备完成后，在仓库根目录或 `desktop/` 中运行：

```sh
cargo run --release -p octosense
```

桌面启动时是空的。可从 dock、左上角的 **Apps** 菜单或 **⌘Space**（菜单与搜索）启动 App Hub、系统应用或开发者程序。**System → Quit OctoSense** 会关闭桌面及其托管的一切。

`config/apps.json` 中的开发者程序在首次启动时构建（进度显示在 tile 中）。

| 平台 | 状态 |
| --- | --- |
| macOS | 已支持并验证（源码构建、进程托管、App Hub、系统应用）。 |
| Windows、Linux | 保留了上游的代码路径，但未在此验证。 |
| Android | `cargo makepad android run -p octosense --release`；见[手机](#手机)。 |
| iOS | 启动策略已测试，但完整构建目前在固定版本 Makepad 的 Metal 后端中失败（[验证记录](docs/validation.md)）。 |

不提供可移动的 `.app`、安装包或 Linux 会话合成器。

### Cargo features

原生应用的 feature（`app-hub`、`app-rinx`、`app-reference`、`app-sheets`、`app-terminal`、`app-appcard`）、默认集合和 `mobile-apps` 都来自 [`native-apps.json`](../native-apps.json)（ADR 0004 §1）：修改清单后运行 `python3 tools/native_apps.py`，不要手动修改各 `Cargo.toml` 中生成的区块。

| Feature | 默认 | 作用 |
| --- | --- | --- |
| `app-hub` | 开 | 链接 `octosense-app-hub-app`（商店 `apphub`、Card 运行器 `card`、系统应用）以及宿主服务 `octosense-mail-service`（邮件）和 `octosense-llm-service`（AI 提供商）。没有它，构建中既没有 App Hub 也没有系统应用。 |
| `octos-core` | 开 | octos 内核服务（`octosense-kernel`，来自 `../crates/kernel`）和应用与 Agent 之间的代理（`octosense-app-peers`）：AppCard、Rinx 等使用方共享的唯一内核，由 AI 提供商配置。Android 和 iOS 上始终开启。用 `--no-default-features --features app-hub`（再加上需要的其他 feature）可以去掉它。 |
| `app-rinx` | 开 | 以模块形式链接 Matrix 客户端 [Rinx](https://github.com/hagency-org/Rinx)；隐含 `octos-core`（它的助手就是 Shell 的助手）。 |
| `app-reference` | 关 | 以模块形式链接 Reference（`../apps/reference`）。 |
| `app-sheets` | 关 | 以模块形式链接 Makepad 的 Sheets。 |
| `app-terminal` | 开 | 以系统应用形式链接 Makepad 的 Terminal：在磁贴中运行的登录 Shell。在 macOS 和 Windows 上它作为独立进程运行（`terminal`，从固定版本的 Makepad 检出中用 `cargo run` 构建，否则使用 `octosense` 旁边的二进制文件），因此它崩溃不会影响 Shell；在 Linux 上只有 Vulkan 构建且处于 Wayland 会话时才如此。无法启动进程时（没有检出也没有二进制文件：发布包目前还不附带它，见 [#94](https://github.com/OctoSense-org/OctoSense/pull/94)），它在进程内打开，与手机上相同；在状态目录下的 `wm/apps.splash` 中写一行 `terminal: Module` 或 `terminal: Process` 可覆盖默认值。无论哪种托管方式，助手获得的工具都相同（ADR 0004 §10）：它可以读取（`read_screen`、`read_scrollback`），也可以输入命令（`run`），每条命令都要等待用户在助手的确认卡片上实时确认（`native-apps.json` 中为 `confirm: host`、`auto_approvable: false`）；确认卡片无法完整显示的过长命令会被拒绝。在 macOS 上进程内运行时，Shell 的 PTY 辅助程序就是 `octosense` 本身。 |
| `app-appcard` | 关 | 链接 AppCard 助手模块（`../apps/appcard/module`）；隐含 `octos-core`。在所有目标平台（包括手机）上都需显式启用；目前不随产品发布。 |
| `app-aichat` | 关 | 以模块形式链接 Makepad 的 AI chat，不含其模型引擎。 |
| `mobile-apps` | 关 | `app-rinx` + `app-reference` + `app-sheets` + `app-hub` + `octos-core`：手机构建所链接的集合，用于在桌面上测试。不含 AppCard。 |

已链接的原生应用按照 `native-apps.json` 中该平台的 `hosting` 托管：App Hub、Rinx、AppCard、Reference 和 Sheets 在所有平台上都在进程内运行，Terminal 在 macOS 和 Windows 上（以及 Vulkan 构建且处于 Wayland 会话的 Linux 上）作为独立进程运行。用 `--module <id>`（或在状态目录下的 `wm/apps.splash` 中写一行 `<id>: Module`）可改为在进程内打开：

```sh
cargo run --release -p octosense --features mobile-apps -- --module reference --module sheets
cargo run --release -p octosense -- --module terminal
```

App Hub 的模块没有进程形态，总是在进程内打开。

### 命令行参数与环境变量

| 名称 | 作用 |
| --- | --- |
| `--apps <file>` | 使用这个开发者程序目录。 |
| `--module <id>` | 在进程内托管一个已链接的模块。 |
| `--assistant`、`--prewarm` | 启动助手应用 / 预热应用（需要目录中有对应条目）。默认关闭。 |
| `--demo-home`、`--download-wallpapers` | 生成演示文件系统；下载 Omarchy 主题的完整壁纸集。 |
| `OCTOSENSE_HOME` | 状态目录（默认 `~/.octosense`；若已存在 `~/.makeos` 则回退到它，也支持 `MAKEOS_HOME`）。 |
| `OCTOSENSE_APP_DATA` | App Hub 保存已安装应用的位置（默认为平台数据目录下的 `apps/`）。 |
| `OCTOSENSE_HUB`、`OCTOSENSE_HUB_ANCHOR` | App Hub 目录来源（路径或 URL）和信任锚；默认是 App Hub 仓库的 `main`。 |
| `OCTOSENSE_SYSTEM_APPS` | 系统应用选择文件；根目录的 `.cargo/config.toml` 把它设为 `desktop/system-apps.json`。 |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | 提供邮件的演示邮箱（见[演示](#演示)）。 |
| `OCTOSENSE_MAIL_VAULT=file` | 把邮件密码保存在权限为 0600 的文件中，而不是 macOS 钥匙串。 |
| `OCTOSENSE_LLM_VAULT=file` | 把 AI 提供商的密钥保存在仅所有者可读的 octos profile 中，而不是 macOS 钥匙串。 |
| `OCTOS_APP_CORE_BIN`、`OCTOS_APP_CORE_DIR` | Shell 内核服务运行的 octos 内核二进制（未设置：此桌面上没有内核）及其 core 目录（默认 `~/octos-home/.octos`；AI 提供商的 profile 为 `<dir>/profiles/_main.json`）。 |
| `OCTOSENSE_GLANCE_DEMO=1` | 启动时以 `os.news` 身份向一览屏发布一张示例 L0 新闻摘要卡片：桌面风格下按 F9 查看，手机风格下在一览页查看。用于测试 `glance` 服务。 |
| `MAKEPAD_REMOTE`、`MAKEPAD_HIDE_WINDOWS` | 远程控制桥；隐藏窗口（见[演示](#演示)）。 |

## 应用模型

launcher 把四类应用列在一起：

| 类别 | 来源 | 运行方式 | Launcher id |
| --- | --- | --- | --- |
| **系统应用**：新闻、相册、地图、相机、邮件、AI 提供商、YouTube | `../apps/<name>/bundle`，由 `system-apps.json` 选择，打包进构建 | App Hub 的 Card 运行器中隔离运行的 Splash 程序，每个应用一个 isolate，只拥有其清单申请的能力 | `<name>`（清单 id `os.<name>`） |
| **商店应用** | 签名的 App Hub 目录，从商店（`apphub`）安装 | 同一个 Card 运行器。每次打开都会对照目录检查；更新会关闭旧实例。 | `hub:<manifest-id>` |
| **原生模块** | 链接进本二进制的 Rust crate | 进程内的 `AppModule`。只允许受信任的代码：App Hub、AppCard、Rinx、Reference 以及各 `app-*` feature。 | 模块 id |
| **开发者程序** | `config/apps.json` | tile 中的独立进程，通过 Makepad 的 `--stdin-loop` 托管协议运行，首次启动时构建 | 目录 `id` |

优先级：已链接的原生模块优先于同 id 的系统应用，系统应用优先于同 id 的目录条目。因此 Makepad 示例中的 Mail 和 Photos 已从随附目录中移除（`config/apps.overlay.json` 中的 `drop`）。

### 隔离与权限

隔离运行的应用是一个包：`manifest.json`（id、版本、能力）加上 `main.splash`。Card 运行器只授予清单中列出的能力（邮件申请 `storage` 和 `mail`）。固定的 Makepad（[makepad#30](https://github.com/OctoSense-org/makepad/pull/30)）在 isolate 的每个出口执行这一约束：网络请求和 web socket 受应用的主机列表约束，原始 socket 和服务端被拒绝，文件访问限制在应用的存储沙箱内，密码和一次性验证码输入框在受约束的 isolate 中不起作用。

### 宿主服务与宿主自有面板

密钥属于宿主。需要账户的应用通过 `host.request` 调用**宿主服务**；服务在 Shell 中持有凭据运行，应用永远拿不到 socket，也拿不到密码。

邮件是完整的示例（`octosense-mail-service`，来自 [`../apps/mail/host-service`](../apps/mail/host-service)）：

- `mail.add_account` 弹出宿主的**登录面板**，这是覆盖在应用之上的独立 isolate。只有这个面板的调用（`mail.sheet.submit`、`mail.sheet.cancel`）可以携带密码。
- 服务先测试账户，再把密码存入平台的密钥存储（macOS 钥匙串），并且只把账户授予添加它的应用。
- 邮件状态保存在宿主自己的目录中，位于所有应用的沙箱之外。

AI 提供商（`os.ai-providers`）通过 `llm` 服务（`octosense-llm-service`，来自 [`../apps/ai-providers/host-service`](../apps/ai-providers/host-service)）编辑 octos 内核的 LLM 提供商。密钥只在宿主面板上输入，保存到 octos 读取的 macOS 钥匙串条目；提供商写入 Shell 的 octos core 目录下内核的 profile（`<core 目录>/profiles/_main.json`；core 目录为 `OCTOS_APP_CORE_DIR`，否则为 `~/octos-home/.octos`）。手机上的提供商二维码可从图片导入：**Choose image** 打开文件面板，或把截图拖到导入面板上。**开始 → 设置 → AI providers** 可打开它。更改后服务会重启正在运行的内核，使用方（AppCard）会重新连接到新内核。

**octos 内核**是一项 Shell 服务，不属于任何应用：`octosense-kernel`（[`../crates/kernel`](../crates/kernel)，feature `octos-core`，默认开启）。Shell 在启动时通过其 AI 服务启动它（[`../crates/ai-host`](../crates/ai-host/README.md)，`octosense_ai_host::start`）；在有使用方连接之前不运行任何东西，之后每个进程只有一个内核（桌面上是 `<OCTOS_APP_CORE_BIN> serve --stdio --data-dir <core 目录>`，Android 上是 APK 中的 `liboctos.so`；iOS 以及未设置 `OCTOS_APP_CORE_BIN` 的桌面上没有内核）。AppCard 的 Agent 连接它；Rinx 通过应用与 Agent 之间的代理访问它。最后一个使用方离开或 Shell 退出时内核停止。

**Talk to Octos**（默认关闭）：在 **AI providers → Talk to Octos** 中打开后，本机会启动一个仅监听回环地址的服务，让 Web 客户端或终端界面与本设备的助手对话。开启期间内核以 `octos serve --host-managed` 代替 `--stdio` 运行，原生应用继续通过其 WebSocket 工作；外部客户端使用单独的令牌，只能打开 UI Protocol 套接字。Web 客户端通过一次性配对码或其链接的二维码配对；本用户的终端客户端读取私有连接文件。原生应用关闭后服务仍保持运行，直到关闭该功能或 Shell 退出。见 [ADR 0003（英文）](../docs/adr/0003-shared-octos-client-access.md) 和[内核指南](../crates/kernel/README.zh-CN.md)。

需要密码、PIN 或令牌的新功能，应放在宿主服务和宿主自有面板中，绝不放在应用自己的界面里。

### 商店应用（App Hub）

App Hub 默认开启。从 launcher 打开 **App Hub**，浏览签名目录并安装应用；安装后的应用无需重启就会出现在 launcher 中。目录来源默认是 App Hub 仓库，可以用 `OCTOSENSE_HUB` 指向其他位置。要构建和发布应用，从 [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) 开始。

#### 发布前试用自己的应用

用一次性信任锚把应用包发布到本地目录（命令见 OctoScript-App-Design-Flow 的 [PUBLISHING §4](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally)：`hub keygen`/`certify`/`publish`），再让本 Shell 指向它：

```sh
OCTOSENSE_HUB=<mirror dir> OCTOSENSE_HUB_ANCHOR=<anchor hex> \
  OCTOSENSE_HOME=/tmp/octosense-test OCTOSENSE_APP_DATA=/tmp/octosense-test-apps \
  cargo run --release -p octosense
```

打开 **App Hub**，选中应用，点 **Get**，向下滚动到 **Install**，然后点 **Open**：它会像商店应用一样，在 Card runner 中按其 manifest 运行。已于 2026-09-26 在 macOS 上用一个新的脚本应用验证（当时在仓库合并前的 OctoSense-Desktop 仓库中）。两个 `OCTOSENSE_*` 状态变量让测试不影响 `~/.octosense`。

### 选择与覆盖系统应用

`system-apps.json` 列出应用及其应用包所在位置（路径相对于该文件）：

```json
{
  "schema": 1,
  "source": "../apps",
  "apps": ["news", "photos", "maps", "mail", "ai-providers"],
  "assets": {}
}
```

- 从 `apps` 中删除某个 id 即可不打包它；把 `OCTOSENSE_SYSTEM_APPS` 指向另一个文件即可换一套选择。没有这个变量时，构建不包含任何系统应用。
- 修改应用包就在 `../apps/<name>/bundle` 中修改并重新构建；它会随下一次桌面构建发布，与 Shell 的修改放在同一个 pull request 中。
- 桌面端没有挂载照片库，因此相册显示的是应用包自带的缩略图。要提供原尺寸照片，添加 `"assets": {"photos": {"photos": "<dir>"}}`。
- 同 id 的已链接原生模块会覆盖系统应用；目前没有这样的模块（原生 News、Photos 和 Maps 模块已删除）。

### 开发者程序与目录

`config/apps.json` 列出 Reference 和 Makepad 自带的应用（Browser、Files、Terminal、Sheets、Notes、Calendar、id 为 `studio` 的 Director 等）。Terminal 同时以链接方式提供（`app-terminal`，默认开启）；它在 `config/apps.json` 中的条目是它在 macOS 和 Windows 上使用的独立进程形式，链接的模块是进程内形式。Image、PDF 和 AI 辅助应用也会出现在 launcher 中，除非它们的 id（`image`、`pdf`、`aichat`）写在状态目录下的 `wm/launcher.hides` 中。

目录查找顺序：给了 `--apps <file>` 就用它；否则若存在 `~/.octosense/apps.json` 就用它；否则用 `config/apps.json`。目录是一个 JSON 数组，每个条目选择一种启动目标：

```json
[
  { "id": "notes", "label": "Notes", "manifest": "../notes/Cargo.toml", "package": "my-notes", "bin": "notes", "policy": "new", "args": [] },
  { "id": "installed-notes", "label": "Installed Notes", "executable": "/opt/my-apps/notes" },
  { "id": "browser", "label": "Browser", "source": "makepad", "package": "makepad-browser", "bin": "browser", "policy": "focus" }
]
```

- `"source": "makepad"` 通过 Cargo 解析到与宿主相同的 Makepad 检出（`.sources/makepad`）；这类构建输出到 `~/.octosense/build/makepad`。
- 分步指南：[打开托管应用](docs/open-apps.md)（包括 `cargo metadata --offline` 失败导致目录为空时的修复方法），以及[打开完整的 Makepad 目录](docs/add-all-makepad-apps.md)。
- 相对路径相对于目录文件所在目录解析。参数按原样传递，不经过 shell。
- `policy`：`"new"` 打开新实例；`"focus"`（默认）聚焦已运行的实例。
- 不要添加 `--stdin-loop` 或 Studio 相关变量，Shell 会自己添加。修改后需重启。
- 被托管的程序必须是基于同一 Makepad 版本构建的 Makepad 应用；托管协议在不同版本之间并不稳定。可以从 `../apps/reference` 开始。

目录中的 Makepad 条目由固定版本上游的应用注册表生成：

```sh
python3 scripts/upstream.py catalog          # report drift
python3 scripts/upstream.py catalog --apply  # rewrite config/apps.json and apps.makepad.json
```

### AppCard 助手

AppCard **目前不随产品发布**：它会干扰其他应用，因此除非显式要求，任何构建都不链接它。默认构建、`mobile-apps`、Android 和 iOS 构建都不包含它的界面（octos 内核服务仍然存在），也没有它的磁贴、分组或启动器条目。使用 `--features app-appcard` 可在任意目标平台上恢复它（手机构建请把该 feature 传给 `cargo makepad`）。

`../apps/appcard/module`（包名 `octosense-appcard`，feature `app-appcard`，需显式启用）在一个 tile 中托管完整的 AppCard 助手：来自 `../apps/appcard/app/app` 的 `octos-app`，构建时关闭其 `standalone` feature。路由、卡片、会话、输入框和内核 Agent 都在该 tile 的 isolate 中运行；`ask` 是该模块的 AI 总线工具。

```sh
cargo run --release -p octosense --features app-appcard -- --module appcard
```

它不会自己启动内核，而是连接 Shell 的内核。在桌面上这需要 `OCTOS_APP_CORE_BIN`（`OCTOS_APP_CORE_DIR` 可选）；没有内核时显示登录 / WebSocket 界面。所有 octos crate 都来自 octos-org/octos，且只有根目录 `Cargo.toml` 固定的那一个版本。

## 演示

### 无账户试用邮件

```sh
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense
```

打开 **Mail**，在宿主面板上用任意地址和密码 `demo` 登录。演示模式从文件保险库提供示例邮件：不联网，不用钥匙串。

在未签名的开发构建上使用真实账户时，每次重新构建后 macOS 都会再次请求钥匙串访问权限。开发时可以改用文件保存密码：

```sh
OCTOSENSE_MAIL_VAULT=file cargo run --release -p octosense
```

### 远程控制桥

每个桌面端 Makepad 应用（包括本 Shell）都内置一个本机 HTTP 控制接口。用 `MAKEPAD_REMOTE=<port>`、`MAKEPAD_REMOTE=on`（临时端口；数字一律按端口解析，`1` 即端口 1，会绑定失败）或 `--remote[=PORT]` 启用：

```sh
MAKEPAD_REMOTE=8399 cargo run --release -p octosense
# prints: [makepad-remote] listening on 127.0.0.1:8399 pid=... app=... grabs=...
```

| 路由 | 作用 |
| --- | --- |
| `/` | 所有路由的速查表。 |
| `/s` | 窗口及其几何信息。 |
| `/snap?q=` | 可见控件及其矩形和文本，可按 id/类型/文本过滤。 |
| `/click?x=&y=` | 在窗口内布局坐标处点击。`/m`、`/k`、`/t` 分别对应鼠标、按键、文本。 |
| `/g` | 把窗口截图为 PNG。 |
| `/log?n=` | 查看日志末尾。 |
| `/gq` | 截取所有窗口后退出。凡是自己启动的会话，都用它（或 `/quit`）结束。 |

在输入类路由后加 `&wait=1`，会等下一帧绘制完成后再返回。该接口会注入真实输入并提供截图：只有在可信网络中才绑定非回环地址（`MAKEPAD_REMOTE=0.0.0.0:8399`）。

### 无界面 UI 检查

在 macOS 上，`MAKEPAD_HIDE_WINDOWS=1` 让窗口不显示在屏幕上但仍然渲染，这样远程驱动的运行不会占用屏幕：

```sh
MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=on cargo run --release -p octosense
```

Makepad 的 [`makepad_test`](https://github.com/OctoSense-org/makepad/tree/main/libs/makepad_test) crate（位于 `.sources/makepad` 中）正是基于这两者：`#[makepad_test]` 测试以隐藏窗口启动应用，通过 `--remote` 用选择器和等待条件驱动它，最后用 `/gq` 关闭。桌面端目前还没有 `makepad_test` 测试套件。

## 手机

安装好 Makepad Android 工具链并通过 ADB 连接设备后：

```sh
cargo makepad android run -p octosense --release
```

本包的手机构建始终链接 Reference、Sheets 和 octos 内核服务，并通过默认 feature 链接 App Hub 及系统应用；AppCard 只在 `--features app-appcard` 时链接。启动器名称为 **OctoSense**，应用 id 为 `dev.makepad.octosense`。APK 必须以 `liboctos.so` 的形式打包内核：（在 `desktop/` 中）`python3 ../tools/kernel-artifact.py --sdk <cargo-makepad Android SDK> -- cargo makepad android run -p octosense --release` 会按工作区 `Cargo.lock` 固定的版本交叉编译 `octos`，并以 `MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>` 运行打包器（预先构建好的内核用 `--kernel <path>`）。没有它，手机上不运行内核；AI 提供商设置仍会保存。AppCard 的 Java 功能（GPS、通知、分享、intent）需要分支版本的 buildtool；见 [docs/android-appcard-build.md](docs/android-appcard-build.md)。专门的手机 Shell 是 [Home](../phone/README.zh-CN.md)（包名 `octosense-home`）；这里的 Android 构建用于开发。

## 桌面样式与设置

- 八种桌面样式。桌面构建启动时使用 **OctoSense** 样式，带 Liquid Glass 窗框，顶栏有 **Light / Dark** 切换；其余样式为 Omarchy、macOS、Windows、Windows 2000、NeXTSTEP、iOS 和 Android。主题源文件在 `../crates/shell/resources/themes/`，壁纸来源见 [crates/shell/resources/wallpapers/README.md](../crates/shell/resources/wallpapers/README.md)。
- 快捷键：**⌘Space** 菜单，**⌘W** 关闭 tile，**⌘F** tile 全屏，**⌘1…0** 切换工作区，**⌘Shift1…0** 移动 tile。**Learn → Keybindings** 列出全部快捷键；见 [KEYBINDINGS.md](KEYBINDINGS.md)。
- 状态保存在 `~/.octosense`（`OCTOSENSE_HOME`）；被托管的应用通过 `MAKEPAD_HOME` 获得它。
- AI 面板（**F10**）的本地模型：[docs/local-ai.md](docs/local-ai.md)。没有模型桌面也能正常工作。

## 版本固定与上游同步

每个外部依赖在整个仓库中只固定一次：Makepad 和 OctoScript 通过 `native-runtime.lock.json`（以及 `runtime-patches.lock.json` 中经审查的补丁），App Hub、octos 和 Rinx 在根目录 `Cargo.toml` 的 `[workspace.dependencies]` 中。系统应用、宿主服务、内核服务和 AppCard 都在本仓库中，与 Shell 在同一个 pull request 中修改，无需固定版本。移动固定版本之后：运行 `python3 tools/setup.py --update`，按需运行 `cargo update`，再运行 `python3 tools/setup.py --check --cargo` 和下面的测试。

`scripts/upstream.py sync|status|diff|update` 跟踪从官方 Makepad 导入的 WM 文件（`upstream/makepad.json`，基线 `74b63be8`）；见 [docs/upstream.md](docs/upstream.md)。它需要用 `--source` 指向官方 Makepad 的完整克隆：`.sources/makepad` 是分支版本的检出，不包含基线提交。

## 测试

CI（`.github/workflows/desktop.yml`）在 `desktop/`、`crates/`、`apps/`、`tools/` 和工作区文件发生变化时，在 macOS 14 上运行：

```sh
python3 tools/setup.py
cd desktop
cargo check --locked -p octosense
cargo check --locked -p octosense --features mobile-apps
cargo check --locked -p octosense -p octosense-reference -p octosense-appcard --features mobile-apps,app-appcard
bash ../tools/check-shell-graph.sh -p octosense   # AI services linked, no AppCard UI without app-appcard, Rinx only as a module, one Makepad/App Hub/octos (host and aarch64-linux-android)
cd ..
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/setup.py --check --cargo
```

若某个 Shell 源文件同时出现在 `crates/shell/src`、`desktop/src`、`phone/src` 中的两处，CI 也会失败。

桌面端任务**不**运行 `cargo test`（Shell 的测试在 `phone.yml` 中运行）、`desktop/scripts` 的测试或冒烟测试；提交 pull request 前请在本地运行：

```sh
(cd phone && cargo test --locked --features mobile-apps -p octosense-shell)   # the shell's tests, as phone.yml runs them
python3 -m unittest discover -s desktop/scripts -p 'test_*.py'
python3 desktop/scripts/upstream.py catalog
```

原生冒烟测试会打开自己的窗口，把状态隔离在临时目录中，并通过远程控制桥驱动 Shell。它们需要图形界面访问权限和一次事先完成的 release 构建（迁入 `desktop/` 后**未验证**：`smoke.py` 仍在 `desktop/apps/reference` 查找 Reference）：

```sh
cargo build --release --locked -p octosense
python3 desktop/scripts/smoke.py --styles
python3 desktop/scripts/smoke.py --cargo-run --default-catalog
```

每次修改的结果记录在 [docs/validation.md](docs/validation.md) 中。

## 已知不足

- 只在 macOS 上验证过。Windows 和 Linux 未测试；iOS 构建在固定版本的 Metal 后端中失败。
- 没有打包好的 `.app` 或安装包；源码构建从 `.sources/makepad` 检出中读取字体和资源，所以请保留该检出。
- 桌面端的相册只有缩略图，除非挂载照片目录。
- 托管的 AppCard 助手尚未接通通知、分享和 WebView 覆盖层。
- 手机上的 Sheets 需要修复网格标签和工具栏（[BACKLOG.md](BACKLOG.md)）。
- 没有 `makepad_test` UI 测试套件；CI 只编译不测试。

## 参与贡献

`main` 受保护：所有修改都必须通过 pull request（管理员也不例外），并禁止强制推送。从 `main` 创建分支，运行 `python3 tools/setup.py --check --cargo`、上述 Rust 和 Python 测试，涉及 UI 的修改还要以隐藏窗口进行一次冒烟测试或远程驱动运行；原生检查结果记录在 `docs/validation.md` 中。遵守“一个 Makepad、一个 octos、一个 App Hub”的规则：`python3 tools/setup.py --check --cargo` 必须通过。面向人和编码 Agent 的规则：[AGENTS.md](../AGENTS.md)。

## 许可证

Apache License 2.0（[LICENSE](../LICENSE)、[NOTICE](../NOTICE)）。从 Makepad 复制的源码保留其 [MIT 声明](../LICENSES/Makepad-MIT.txt)。依赖项保留各自的许可证。
