# OctoSense Desktop

[English](README.md) | 简体中文

OctoSense-Desktop 是 [OctoSense](https://github.com/OctoSense-org)（运行在操作系统之上的 Agent 交互 Shell）的桌面端 Shell。它是一个 Makepad 窗口，这个窗口本身就是桌面：launcher、dock 和平铺窗格（tile）。系统应用和 App Hub 商店应用以隔离的脚本程序运行，受信任的原生模块在进程内运行，Makepad 开发者程序作为子进程运行。它获取应用的方式与手机 Shell（OctoSense-ROM 的 `home/`）完全相同。

## 在仓库体系中的位置

| 仓库 | 与本仓库的关系 |
| --- | --- |
| [OctoSense-ROM](https://github.com/OctoSense-org/OctoSense-ROM/blob/main/README.zh-CN.md) | 手机 Shell（`home/`）。相同的应用模型、相同的运行时补丁、相同的系统应用。 |
| [OctoSense-System-Apps](https://github.com/OctoSense-org/OctoSense-System-Apps) | 新闻、相册、地图、相机、邮件的应用包，邮件宿主服务，以及 AppCard 助手（`octos-app`）。以固定版本的同级目录检出。 |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | 签名目录、商店和 Card 运行器。以 Git crate `octosense-app-hub-app` 链接。 |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | 设计、构建应用并发布到 App Hub 的地方。 |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | 固定 Makepad 与 OctoScript 版本的运行时发行版。以固定版本的同级目录检出。 |
| [makepad（OctoSense 分支）](https://github.com/OctoSense-org/makepad) | 框架。以固定版本的同级目录检出，并打上经过评审的补丁。 |
| [octos](https://github.com/octos-org/octos) | AppCard 助手背后的 Agent 内核。Git 依赖，只有一个版本（`18fcd3f1`）。 |

## 仓库结构

| 路径 | 内容 |
| --- | --- |
| `src/` | Shell 本体（crate `octosense`）：平铺、launcher、dock、状态栏、应用托管（`host.rs`、`hub.rs`、`module_host.rs`）、应用注册表（`apps.rs`）。 |
| `src/shell/` | 状态栏、launcher、菜单、通知、AI 面板、gallery。 |
| `src/octosense/` | OctoSense 专有部分：应用目录加载、状态路径、样式、Makepad 源码定位。 |
| `apps/reference/` | `octosense-reference`，一个计数器加文本输入的小应用，既能作为托管进程运行，也能作为链接模块运行。 |
| `apps/appcard/` | `octosense-appcard`，在一个 tile 中挂载 AppCard 助手（`octos-app`）的模块。 |
| `config/apps.json` | 默认的开发者程序目录。`apps.makepad.json` 是供 `--apps` 使用的相同副本；`apps.overlay.json` 保存重新生成时应用的调整。 |
| `system-apps.json` | 本次构建打包哪些系统应用，以及从哪里取。 |
| `native-runtime.lock.json` | OctoScript-Makepad 发行版（并由它确定 Makepad 和 OctoScript）。 |
| `native-apps.lock.json` | OctoSense-System-Apps 的版本。 |
| `runtime-patches.lock.json`、`patches/runtime/` | 经过评审的 Makepad 补丁（[OctoSense-org/makepad#30](https://github.com/OctoSense-org/makepad/pull/30)）及其期望的源码树。 |
| `tools/setup-native.py` | 准备并检查固定版本的同级目录。 |
| `scripts/` | `upstream.py`（WM 来源记录与目录重新生成）、`smoke.py`（原生冒烟测试）、它们的 Python 测试，以及 `provision-appcard-llm.sh`（Android）。 |
| `upstream/makepad.json` | 每个从 Makepad `apps/wm` 引入的文件的来源记录。 |
| `resources/` | 主题、壁纸、图标、Android manifest 模板、启动脚本。 |
| `docs/` | [验证记录](docs/validation.md)、[上游同步](docs/upstream.md)、[本地 AI](docs/local-ai.md)、[Android AppCard 构建](docs/android-appcard-build.md)、按日期的计划文档。 |
| `KEYBINDINGS.md`、`BACKLOG.md` | 快捷键说明；待办事项。 |

## 前置条件

- 稳定版 Rust（`cargo` 位于 `~/.cargo/bin`）以及所在系统的原生工具链。macOS 需要 Xcode Command Line Tools（`xcode-select --install`）。
- Git，以及供 `tools/setup-native.py` 使用的 Python 3.9+。`scripts/upstream.py` 需要 Python 3.11+（它导入 `tomllib`）。
- 首次准备和构建时需要联网。

## 准备同级工作区

构建时，Makepad、OctoScript、OctoScript-Makepad 和 OctoSense-System-Apps 都作为本仓库的**同级目录**解析（`../makepad` 等，通过 `Cargo.toml` 中的 `[patch]` 条目）。把本仓库放进一个单独的工作区目录，再由准备脚本补齐其余部分：

```sh
mkdir octosense-ws && cd octosense-ws
git clone https://github.com/OctoSense-org/OctoSense-Desktop.git
cd OctoSense-Desktop
python3 tools/setup-native.py
```

结果：

```text
octosense-ws/
  OctoSense-Desktop/       this repository
  octoscript-makepad/      the release native-runtime.lock.json selects
  makepad/, octoscript/    the revisions that release's runtime.json pins; makepad carries the patch
  OctoSense-System-Apps/   the revision native-apps.lock.json pins
```

| 命令 | 作用 |
| --- | --- |
| `python3 tools/setup-native.py` | 按固定版本克隆缺失的同级仓库，并应用 Makepad 补丁。 |
| `python3 tools/setup-native.py --check` | 只校验同级目录，不做任何修改。 |
| `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml` | 同时检查锁定的 Cargo 依赖图：只有一个 Makepad、一个 octos、一个 App Hub。 |
| `python3 tools/setup-native.py --update` | 把干净的检出移动到锁定版本（在锁文件变更之后）。 |
| `--root DIR`、`--cache DIR` | 使用其他工作区目录；复用本地 Git 对象缓存。 |

同级目录中的本地修改会被保留；`--update` 只移动干净的检出。补丁所基于的提交（`runtime-patches.lock.json` 中的 `source_commit`）的干净检出，会被视为同一棵源码树。

## 构建与运行

准备完成后，在本目录执行：

```sh
cargo run --release
```

桌面启动时是空的。从 dock、左上角的 **Apps** 菜单或 **⌘Space**（菜单和搜索）启动 App Hub、系统应用或开发者程序。**System → Quit OctoSense** 关闭桌面及其托管的所有内容。

`config/apps.json` 中的开发者程序在首次启动时构建（进度显示在 tile 中）。如需预先构建整个工作区：

```sh
cargo build --release --workspace
```

| 平台 | 状态 |
| --- | --- |
| macOS | 已支持并验证（源码构建、进程托管、App Hub、系统应用）。 |
| Windows、Linux | 保留了上游的代码路径，但在本仓库未经验证。 |
| Android | `cargo makepad android run -p octosense --release`；见[手机](#手机)。 |
| iOS | 启动策略有测试覆盖，但完整构建目前在固定版本的 Makepad Metal 后端中失败（[验证记录](docs/validation.md)）。 |

目前不提供可重定位的 `.app`、安装包，也没有 Linux 会话合成器。

### Cargo features

| Feature | 默认 | 作用 |
| --- | --- | --- |
| `app-hub` | 开 | 链接 `octosense-app-hub-app`（商店 `apphub`、Card 运行器 `card`、系统应用）以及邮件宿主服务 `octosense-mail-service`。关闭后构建中没有 App Hub，也没有系统应用。 |
| `app-reference` | 关 | 把 Reference 作为模块链接。 |
| `app-sheets` | 关 | 把 Makepad 的 Sheets 作为模块链接。 |
| `app-photos` | 关 | 链接 Makepad 的原生 Photos 模块；它会替换同 id 的相册系统应用（用于对比）。 |
| `app-appcard` | 关 | 链接 AppCard 助手模块（`apps/appcard`）。 |
| `app-aichat` | 关 | 把 Makepad 的 AI chat 作为模块链接，不含模型引擎。 |
| `app-rinx` | 关 | 把 Matrix 客户端 [Rinx](https://github.com/upstreamlabs/Rinx) 作为模块链接。 |
| `mobile-apps` | 关 | `app-reference` + `app-sheets` + `app-appcard` + `app-hub`：手机构建链接的同一组模块，用于在桌面上测试。 |

已链接的模块用 `--module <id>` 打开（或在状态目录下的 `wm/apps.splash` 中写一行 `<id>: Module`）：

```sh
cargo run --release --features app-appcard -- --module appcard
cargo run --release --features mobile-apps -- --module reference --module sheets
cargo run --release --features app-rinx -- --module rinx
```

App Hub 的模块是例外：它们没有进程形态，总是在进程内打开。

### 命令行参数与环境变量

| 名称 | 作用 |
| --- | --- |
| `--apps <file>` | 使用指定的开发者程序目录。 |
| `--module <id>` | 在进程内托管一个已链接的模块。 |
| `--assistant`、`--prewarm` | 启动助手应用 / 预热应用（需要目录中有对应条目）。默认关闭。 |
| `--demo-home`、`--download-wallpapers` | 生成演示文件系统；下载 Omarchy 主题的完整壁纸集。 |
| `OCTOSENSE_HOME` | 状态目录（默认 `~/.octosense`；若不存在则沿用已有的 `~/.makeos`，以及 `MAKEOS_HOME`）。 |
| `OCTOSENSE_APP_DATA` | App Hub 保存已安装应用的位置（默认是平台数据目录下的 `apps/`）。 |
| `OCTOSENSE_HUB`、`OCTOSENSE_HUB_ANCHOR` | App Hub 目录来源（路径或 URL）和信任锚；默认是 App Hub 仓库的 `main`。 |
| `OCTOSENSE_SYSTEM_APPS` | 系统应用选择文件；`.cargo/config.toml` 将其设为 `system-apps.json`。 |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | 提供邮件的演示邮箱（见[演示](#演示)）。 |
| `OCTOSENSE_MAIL_VAULT=file` | 把邮件密码保存在权限为 0600 的文件中，而不是 macOS 钥匙串。 |
| `OCTOS_APP_CORE_BIN`、`OCTOS_APP_CORE_DIR` | 让 AppCard 助手使用本地的 octos 内核。 |
| `MAKEPAD_REMOTE`、`MAKEPAD_HIDE_WINDOWS` | 远程控制桥；隐藏窗口（见[演示](#演示)）。 |

## 应用模型

launcher 把四类应用列在一起：

| 类别 | 来源 | 运行方式 | Launcher id |
| --- | --- | --- | --- |
| **系统应用**：新闻、相册、地图、相机、邮件 | OctoSense-System-Apps 的 `apps/<name>/bundle`，由 `system-apps.json` 选择，打包进构建 | App Hub 的 Card 运行器中隔离运行的 Splash 程序，每个应用一个 isolate，只拥有其清单申请的能力 | `<name>`（清单 id `os.<name>`） |
| **商店应用** | 签名的 App Hub 目录，从商店（`apphub`）安装 | 同一个 Card 运行器。每次打开都会对照目录检查；更新会关闭旧实例。 | `hub:<manifest-id>` |
| **原生模块** | 链接进本二进制的 Rust crate | 进程内的 `AppModule`。只允许受信任的代码：App Hub、AppCard、Reference 以及各 `app-*` feature。 | 模块 id |
| **开发者程序** | `config/apps.json` | tile 中的独立进程，通过 Makepad 的 `--stdin-loop` 托管协议运行，首次启动时构建 | 目录 `id` |

优先级：已链接的原生模块优先于同 id 的系统应用，系统应用优先于同 id 的目录条目。因此 Makepad 示例中的 Mail 和 Photos 已从随附目录中移除（`config/apps.overlay.json` 中的 `drop`）。

### 隔离与权限

隔离运行的应用是一个包：`manifest.json`（id、版本、能力）加上 `main.splash`。Card 运行器只授予清单中列出的能力（邮件申请 `storage` 和 `mail`）。Makepad 补丁（[makepad#30](https://github.com/OctoSense-org/makepad/pull/30)）在 isolate 的每个出口执行这一约束：网络请求和 web socket 受应用的主机列表约束，原始 socket 和服务端被拒绝，文件访问限制在应用的存储沙箱内，密码和一次性验证码输入框在受约束的 isolate 中不起作用。

### 宿主服务与宿主自有面板

密钥属于宿主。需要账户的应用通过 `host.request` 调用**宿主服务**；服务在 Shell 中持有凭据运行，应用永远拿不到 socket，也拿不到密码。

邮件是完整的示例（`octosense-mail-service`，来自 `../OctoSense-System-Apps/apps/mail/host-service`）：

- `mail.add_account` 弹出宿主的**登录面板**，这是覆盖在应用之上的独立 isolate。只有这个面板的调用（`mail.sheet.submit`、`mail.sheet.cancel`）可以携带密码。
- 服务先测试账户，再把密码存入平台的密钥存储（macOS 钥匙串），并且只把账户授予添加它的应用。
- 邮件状态保存在宿主自己的目录中，位于所有应用的沙箱之外。

需要密码、PIN 或令牌的新功能，应放在宿主服务和宿主自有面板中，绝不放在应用自己的界面里。

### 商店应用（App Hub）

App Hub 默认开启。从 launcher 打开 **App Hub**，浏览签名目录并安装应用；安装后的应用无需重启就会出现在 launcher 中。目录来源默认是 App Hub 仓库，可以用 `OCTOSENSE_HUB` 指向其他位置。要构建和发布应用，从 [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) 开始。

### 选择与覆盖系统应用

`system-apps.json` 列出应用及其应用包所在位置：

```json
{
  "schema": 1,
  "source": "../OctoSense-System-Apps/apps",
  "apps": ["news", "photos", "maps", "camera", "mail"],
  "assets": {}
}
```

- 从 `apps` 中删除某个 id 即可不打包它；把 `OCTOSENSE_SYSTEM_APPS` 指向另一个文件即可换一套选择。没有这个变量时，构建不包含任何系统应用。
- 想试用修改过的应用包，就在 `../OctoSense-System-Apps` 检出中修改并重新构建；要正式发布，把修改合入 OctoSense-System-Apps，再更新 `native-apps.lock.json`。
- 桌面端没有挂载照片库，因此相册显示的是应用包自带的缩略图。要提供原尺寸照片，添加 `"assets": {"photos": {"photos": "<dir>"}}`。
- 同 id 的原生模块会覆盖系统应用（例如 `--features app-photos`）。

### 开发者程序与目录

`config/apps.json` 列出 Reference 和 Makepad 自带的应用（Browser、Files、Terminal、Sheets、Notes、Calendar、id 为 `studio` 的 Director 等）。Image、PDF 和 AI 辅助应用也会出现在 launcher 中，除非它们的 id（`image`、`pdf`、`aichat`）写在状态目录下的 `wm/launcher.hides` 中。

目录查找顺序：给了 `--apps <file>` 就用它；否则若存在 `~/.octosense/apps.json` 就用它；否则用 `config/apps.json`。目录是一个 JSON 数组，每个条目选择一种启动目标：

```json
[
  { "id": "notes", "label": "Notes", "manifest": "../notes/Cargo.toml", "package": "my-notes", "bin": "notes", "policy": "new", "args": [] },
  { "id": "installed-notes", "label": "Installed Notes", "executable": "/opt/my-apps/notes" },
  { "id": "browser", "label": "Browser", "source": "makepad", "package": "makepad-browser", "bin": "browser", "policy": "focus" }
]
```

- `"source": "makepad"` 通过 Cargo 解析到与宿主相同的 Makepad 检出；这类构建输出到 `~/.octosense/build/makepad`。
- 相对路径相对于目录文件所在目录解析。参数按原样传递，不经过 shell。
- `policy`：`"new"` 打开新实例；`"focus"`（默认）聚焦已运行的实例。
- 不要添加 `--stdin-loop` 或 Studio 相关变量，Shell 会自己添加。修改后需重启。
- 被托管的程序必须是基于同一 Makepad 版本构建的 Makepad 应用；托管协议在不同版本之间并不稳定。可以从 `apps/reference` 开始。

目录中的 Makepad 条目由固定版本上游的应用注册表生成：

```sh
python3 scripts/upstream.py catalog          # report drift
python3 scripts/upstream.py catalog --apply  # rewrite config/apps.json and apps.makepad.json
```

### AppCard 助手

`apps/appcard`（feature `app-appcard`，手机构建总是链接）在一个 tile 中托管完整的 AppCard 助手：来自 `../OctoSense-System-Apps/apps/appcard/app/app` 的 `octos-app`，构建时关闭其 `standalone` feature。路由、卡片、会话、输入框和内核 Agent 都在该 tile 的 isolate 中运行；`ask` 是该模块的 AI 总线工具。

```sh
cargo run --release --features app-appcard -- --module appcard
```

在桌面上，`OCTOS_APP_CORE_BIN` 和 `OCTOS_APP_CORE_DIR` 让它使用本地 octos 内核；没有内核时显示登录 / WebSocket 界面。所有 octos crate 都来自 octos-org/octos，且只有 `octos-app` 固定的那一个版本。

## 演示

### 无账户试用邮件

```sh
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release
```

打开 **Mail**，在宿主面板上用任意地址和密码 `demo` 登录。演示模式从文件保险库提供示例邮件：不联网，不用钥匙串。

在未签名的开发构建上使用真实账户时，每次重新构建后 macOS 都会再次请求钥匙串访问权限。开发时可以改用文件保存密码：

```sh
OCTOSENSE_MAIL_VAULT=file cargo run --release
```

### 远程控制桥

每个桌面端 Makepad 应用（包括本 Shell）都内置一个本机 HTTP 控制接口。用 `MAKEPAD_REMOTE=1`（临时端口）、`MAKEPAD_REMOTE=<port>` 或 `--remote[=PORT]` 启用：

```sh
MAKEPAD_REMOTE=8399 cargo run --release
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
MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=1 cargo run --release
```

Makepad 的 [`makepad_test`](https://github.com/OctoSense-org/makepad/tree/main/libs/makepad_test) crate（位于 `../makepad` 同级目录中）正是基于这两者：`#[makepad_test]` 测试以隐藏窗口启动应用，通过 `--remote` 用选择器和等待条件驱动它，最后用 `/gq` 关闭。本仓库目前还没有 `makepad_test` 测试套件。

## 手机

安装 Makepad Android 工具链并通过 ADB 连接设备后：

```sh
cargo makepad android run -p octosense --release
```

手机构建总是链接 Reference、Sheets 和 AppCard，并通过默认 feature 链接 App Hub 及系统应用。launcher 显示名为 **OctoSense**，应用 id 为 `dev.makepad.octosense`。AppCard 的 Java 功能（GPS、通知、分享、intent）以及打包的 `liboctos.so` 内核需要使用分支的 buildtool 和 `MAKEPAD_ANDROID_EXTRA_LIBS`；见 [docs/android-appcard-build.md](docs/android-appcard-build.md)。专门的手机 Shell 是 OctoSense-ROM 的 `home/`。

## 桌面样式与设置

- 共八种桌面样式。桌面构建默认使用 **OctoSense**，带 Liquid Glass 窗框，顶栏有 **Light / Dark** 切换；其余为 Omarchy、macOS、Windows、Windows 2000、NeXTSTEP、iOS 和 Android。主题源文件在 `resources/themes/`，壁纸来源见 [resources/wallpapers/README.md](resources/wallpapers/README.md)。
- 快捷键：**⌘Space** 菜单，**⌘W** 关闭 tile，**⌘F** tile 全屏，**⌘1…0** 切换工作区，**⌘Shift1…0** 移动 tile。**Learn → Keybindings** 列出全部快捷键；另见 [KEYBINDINGS.md](KEYBINDINGS.md)。
- 状态保存在 `~/.octosense`（`OCTOSENSE_HOME`）；托管应用通过 `MAKEPAD_HOME` 获得该路径。
- AI 面板（**F10**）的本地模型：[docs/local-ai.md](docs/local-ai.md)。没有模型时桌面照常工作。

## 固定与更新同级仓库

| 要更新的内容 | 修改 | 然后 |
| --- | --- | --- |
| Makepad / OctoScript | `native-runtime.lock.json`（新的 OctoScript-Makepad 发行版），以及 `Cargo.toml` 和 `apps/*/Cargo.toml` 中 Makepad Git 依赖的 `rev` | 必要时变基补丁，更新 `runtime-patches.lock.json`（base、sha256、tree） |
| 系统应用、邮件服务、AppCard | `native-apps.lock.json` 中的 `revision` | — |
| App Hub | `Cargo.toml` 中 `octosense-app-hub-app` 的 `rev` 以及三个 `[patch]` 条目 | — |

任何更改之后：执行 `python3 tools/setup-native.py --update`，按需执行 `cargo update`，再运行 `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml` 和下面的测试。

`scripts/upstream.py sync|status|diff|update` 跟踪从官方 Makepad 引入的 WM 文件（`upstream/makepad.json`，基线 `74b63be8`）；见 [docs/upstream.md](docs/upstream.md)。它需要用 `--source` 指向官方 Makepad 的完整克隆：`../makepad` 同级目录是分支的浅克隆，不包含基线提交。

## 测试

```sh
cargo test --locked --workspace
cargo test --locked --workspace --features mobile-apps
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/upstream.py catalog
python3 tools/setup-native.py --check --cargo-manifest Cargo.toml
```

原生冒烟测试会打开自己的窗口，把状态隔离在临时目录中，并通过远程控制桥驱动 Shell。需要 GUI 访问权限，并且先完成 release 构建：

```sh
cargo build --release --locked --workspace
python3 scripts/smoke.py --styles
python3 scripts/smoke.py --cargo-run --default-catalog
```

每次变更的结果记录在 [docs/validation.md](docs/validation.md)。

## CI

`.github/workflows/runtime.yml` 在每次 push 和 pull request 时于 macOS 14 上运行：`setup-native.py`、`cargo check --locked --workspace --features mobile-apps`，以及 `setup-native.py --check --cargo-manifest Cargo.toml`。它**不**运行 `cargo test`、Python 测试或冒烟测试；提交 PR 前请在本地运行。

## 已知不足

- 只有 macOS 经过验证。Windows 和 Linux 未测试；iOS 构建在固定版本的 Metal 后端中失败。
- 没有打包好的 `.app` 或安装包；源码构建从 `../makepad` 检出读取字体和资源，请保留该目录。
- 桌面端相册只有缩略图，除非挂载照片目录。
- 托管的 AppCard 助手尚未接入通知、分享和 WebView 浮层。
- 手机端 Sheets 还需要修复网格标签和工具栏（[BACKLOG.md](BACKLOG.md)）。
- 没有 `makepad_test` UI 测试套件；CI 只编译，不测试。

## 参与贡献

`main` 受保护：所有更改都必须通过 pull request（管理员也不例外），且禁止强制推送。从 `main` 创建分支，运行 `setup-native.py --check` 和上面的 Rust、Python 测试；涉及 UI 的更改还要做一次冒烟测试或远程驱动运行，并把原生检查记录到 `docs/validation.md`。保持“一个 Makepad、一个 octos、一个 App Hub”的规则：`setup-native.py --check --cargo-manifest Cargo.toml` 必须通过。

## 许可证

Apache License 2.0（[LICENSE](LICENSE)、[NOTICE](NOTICE)）。从 Makepad 复制的源码保留其 [MIT 声明](LICENSES/Makepad-MIT.txt)。各依赖保留各自的许可证。
