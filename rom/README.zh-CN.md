# OctoSense ROM

[English](README.md) | 简体中文

OctoSense 是运行在操作系统之上的 Agent 交互 Shell，基于 [Makepad](https://github.com/OctoSense-org/makepad) 构建。本仓库包含手机 Shell **OctoSense Home**（`home/`）及其全部交付方式：既可以作为普通桌面应用安装在任意 Android 手机上，也可以连同具有系统权限的 agent、Quickstep 和 SystemUI 一起预装进面向 OnePlus 6（一加 6，`enchilada`）的 LineageOS 22.2（Android 15）镜像。同一套 Home 还能在 macOS 上以窗口形式运行，方便开发，并可构建 OpenHarmony 应用和 iOS 模拟器版本。

## 本仓库在整体中的位置

| 仓库 | 作用 | 本仓库如何使用它 |
| --- | --- | --- |
| **OctoSense-ROM**（本仓库） | 手机 Shell、ROM 镜像、安装器 | |
| [OctoSense-Desktop](https://github.com/OctoSense-org/OctoSense-Desktop) | 桌面端 Shell | Home 于 2026 年 9 月 15 日从中拆分出来，两者仍共享大量源码（见 [home/README.zh-CN.md](home/README.zh-CN.md)）。 |
| [OctoSense-System-Apps](https://github.com/OctoSense-org/OctoSense-System-Apps) | 新闻、相册、地图、相机、邮件这五个隔离运行的脚本应用，邮件宿主服务，以及 AppCard 助手 | 版本固定在 `home/native-apps.lock.json`，检出到 `.sources/system-apps`。 |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | 签名目录、准入检查、`hub` 命令行、`card-host`，以及各 Shell 共用的 crate `octosense-app-hub-app` | 作为 git 依赖固定在 `home/Cargo.toml`。 |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | 如何开发并发布 OctoSense 应用 | 不参与构建。要为 Home 开发应用，从这里开始。 |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | 运行时发布版本：指定 Makepad 与 OctoScript 的版本 | 版本固定在 `home/native-runtime.lock.json`。 |
| [makepad](https://github.com/OctoSense-org/makepad)（OctoSense 分支） | UI 框架及打包工具 `cargo-makepad` | 按运行时指定的版本检出到 `.sources/makepad`，并叠加一个经过审查的补丁。 |
| [octos](https://github.com/octos-org/octos) | AppCard 背后的 Agent 内核 | 只用一个版本，即 OctoSense-System-Apps 中 `octos-app` 所固定的版本。 |

组织概览见 [OctoSense 组织主页](https://github.com/OctoSense-org/.github/blob/main/profile/README.zh-CN.md)。

## 仓库结构

| 路径 | 内容 |
| --- | --- |
| `home/` | Home 的 Rust 工作区（crate `octosense`），以及 Android、iOS、OpenHarmony 资源 |
| `home/src/` | Shell 源码：`mobile*.rs` 是手机 Shell，`apps.rs` 负责接入已链接的模块和系统应用 |
| `home/apps/` | 原生模块：`appcard`（AppCard 宿主）、`reference`，以及仅用于对比的 `news`、`photos`、`maps` |
| `home/android/` | Gradle 项目：AIDL 接口、System Bridge APK、Quickstep、SystemUI 预览和平台构建准备工具 |
| `home/tools/` | `setup-native.py`（运行时源码）、`build_app_icons.py`、`a11y-probe/` |
| `home/scripts/` | 桌面冒烟测试、Makepad 导入同步（`upstream.py`）、Android 帧率测量 |
| `home/docs/` | Home 的 ADR、Android 与性能记录、设计笔记 |
| `home/*.lock.json`、`home/system-apps.json` | 源码版本锁定和系统应用选择（见[版本固定与更新](#版本固定与更新)） |
| `vendor/octosense/` | ROM 产品层：makefile、权限、资源覆盖、sepolicy、系统 agent |
| `patches/` | LineageOS 与内核补丁；`patches/runtime/` 存放经过审查的 Makepad 补丁 |
| `scripts/` | Home 构建、ROM 产物准备、构建、刷机、发布和手机检查 |
| `web-installer/` | 面向 OnePlus 6 的 WebUSB 安装器（本地开发预览版） |
| `docs/` | ROM 的 ADR，以及构建、刷机、更新和验证记录 |
| `tests/` | 构建、产物准备、清单和安装器脚本的 Python 测试 |
| `.sources/` | Git 忽略。由 setup 生成的固定版本依赖检出 |
| `out/` | Git 忽略。构建产物和构建回执 |

## 快速开始

### 前置条件

- Git，Python 3.9 或更高版本（`home/scripts/upstream.py` 及其测试需要 3.11）。
- Rust stable。如果 `cargo` 只装在 `~/.cargo/bin` 下，先把它加入 `PATH`：`export PATH="$HOME/.cargo/bin:$PATH"`。
- 构建 Android APK：一个 `cargo-makepad` 的 Android SDK/NDK 目录、带 platform 35 和 build-tools 35.0.0 的 Android SDK、完整的 JDK 17+ 以及 Gradle 8.11.1。构建脚本不会安装这些工具。第一项可用 `cargo-makepad makepad android --sdk-path=<dir> install-toolchain` 准备。
- 构建 ROM 镜像：`enchilada` 的 LineageOS 22.2 源码树、OnePlus 厂商 blob、内核源码以及 ROM 签名密钥，全部位于本仓库之外。

### 准备固定版本的源码

在仓库根目录运行：

```sh
python3 scripts/setup-home.py              # check out every pin into .sources/
python3 scripts/setup-home.py --check --cargo
```

setup 会把 OctoSense-System-Apps、OctoScript-Makepad、Makepad 和 OctoScript 检出到锁定的版本，应用经过审查的 Makepad 补丁，并且不会改动存在本地修改的检出。`--update` 把干净的检出移动到新的锁定版本；`--check` 不做任何修改，只要有检出与锁定不符就失败；`--cargo` 还会拒绝依赖图中出现第二份核心 Makepad crate。

### 在桌面上运行 Home

```sh
cd home
cargo run --release --features mobile-only
```

这会在手机尺寸的窗口中运行手机 Shell，包含 App Hub 和五个系统应用。去掉 `mobile-only`，`cargo run --release` 启动的是通用桌面 Shell；它在本仓库中保持可构建，但桌面产品是 OctoSense-Desktop。CI 只在 macOS 上构建。

常用开关：

| 开关 | 作用 |
| --- | --- |
| `--features mobile-apps` | 同时链接原生模块（Reference、Sheets、AppCard，以及原生的新闻、相册、地图，它们会替换对应的脚本应用） |
| `-- --module <id>` | 在进程内以模块方式运行已链接的模块，而不是作为子进程 |
| `-- --test-action <name>` | 启动时触发一个 Shell 动作：`launch-<app id>`、`page:<n>`、`island:demo`、`capture:<path>`、`ask-appcard:<text>`、`taps:<x>,<y>@<s>` |
| `MAKEPAD_WM_TEST_APP=<app>[:<count>]` | Shell 启动后打开某个应用（可指定次数） |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | 邮件使用演示邮箱（密码 `demo`） |
| `OCTOSENSE_HOME=<dir>` | 把状态保存在 `~/.octosense` 以外的目录 |

### 构建 Android Home APK

`scripts/build-home.sh` 构建 Home APK 及配套的 System Bridge APK，用同一身份签名，并把 `OctoSenseHome.apk`、`OctoSenseBridge.apk` 和构建回执 `build.json` 写入 `out/home/<variant>/`。它不会安装 APK，也不会刷机。以下命令构建使用 Makepad 开发密钥签名的独立开发版：

```sh
scripts/build-home.sh --variant standalone --development \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk \
  --packager .sources/makepad/target/release/cargo-makepad
```

加上 `--dry-run` 可只打印构建计划。发布版请把 `--development` 换成 `--sign-key` 和 `--sign-cert`，指向保存在检出目录之外的现有签名密钥。

**已知问题**：不传 `--packager` 时，脚本会用 `cargo build --locked` 构建固定版本的 `cargo-makepad`，但 Makepad 检出中没有 `Cargo.lock`，因此会失败。请先自行构建打包工具，再像上面那样传入 `--packager`：

```sh
cargo build --release --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
```

请使用这个固定版本的打包工具，而不是上游的：它包含本应用所需的 Java activity（[home/docs/build-tool.md（英文）](home/docs/build-tool.md)）。

**包名**。Home 的应用 ID 是 `dev.makepad.octosense`，独立版和 ROM 版相同。运行 OctoSense ROM 的手机上已经装有这个 ID 的应用，并使用平台密钥签名，所以开发版无法覆盖安装。要把测试版装在它旁边，请直接调用打包工具并指定另一个包名（`build-home.sh` 没有提供这个选项）：

```sh
cd home
../.sources/makepad/target/release/cargo-makepad makepad android \
  --sdk-path=/path/to/makepad-android \
  --package-name=dev.makepad.octosense.scriptapps \
  build -p octosense --release
```

APK 输出到 `home/target/android/makepad-android-apk/octosense/apk/`；把 `build` 换成 `run` 会同时安装并启动。操作测试包时使用它自己的包名，例如 `adb shell am start -n dev.makepad.octosense.scriptapps/.MakepadApp`。

在你自己控制的手机上，把某个构建设为桌面应用：在 Android 的桌面选择器中选择 OctoSense，或者执行：

```sh
adb shell cmd package set-home-activity dev.makepad.octosense/.MakepadApp
```

签名、构建回执和 ROM 版见 [docs/home-build.md（英文）](docs/home-build.md)；Home 角色、手势和导航模式见 [home/README.zh-CN.md](home/README.zh-CN.md)。

### 构建并刷入 ROM 镜像（OnePlus 6）

系统构建在 Linux 构建主机的 Ubuntu 24.04 chroot 中进行，使用外部的 LineageOS 源码树。本仓库中的步骤：

```sh
scripts/build-home.sh --variant rom --sdk ... --android-sdk ... \
  --gradle-home ... --java-home ... --packager ... \
  --sign-key /private/rom-keys/platform.pk8 \
  --sign-cert /private/rom-keys/platform.x509.pem
python3 scripts/stage-home.py                 # verify the receipt, copy the APKs to vendor/octosense/prebuilt/
scripts/stage-forks.sh /path/to/lineage-tree  # apply vendor/octosense and stage the Quickstep and SystemUI forks
```

之后在构建主机上，`scripts/run-rom-rootfs.sh` 进入 chroot（位于 `OCTOSENSE_BUILD_ROOT` 下，默认 `/home/ubuntu/octosense-adr0001`），运行 `build-rom.sh preflight`、`bacon`（完整签名构建）或 `module <name>`。它要求 `build-rom.sh` 位于构建根目录下的 `exports/build-rom.sh`，并由 systemd 启动；对应的 systemd unit 和主机配置不在本仓库中。`scripts/make-keys.sh` 用于一次性生成签名密钥；`scripts/release.sh <build-tag>` 从 `~/.config/octosense/build.env` 中配置的主机下载构建好的产物。

刷机方式：

- **浏览器**：[网页安装器](web-installer/README.zh-CN.md#在浏览器中刷入-rom)，目前是本地开发预览版。全新安装会清除手机数据。在 [ADR 0001（英文）](docs/adr/0001-public-web-installer.md) 完成之前，公共网站刷机保持关闭。
- **命令行**：`scripts/flash.sh <build dir> [serial]`，或 [docs/flashing.md（英文）](docs/flashing.md) 中的 recovery 侧载流程；该文档也记录了首次刷机的经验。
- **刷机之后**：`scripts/verify-phone.sh <build-tag> [serial]` 等待开机，然后运行 `scripts/checklist.sh` 和 `scripts/agent-test.sh`。

已刷机的手机通过本仓库的 GitHub Releases 进行 OTA 更新（[docs/updates.md（英文）](docs/updates.md)）；`scripts/ota-push.sh` 可以从 Mac 推送一次更新。

### OpenHarmony 与 iOS

- **OpenHarmony**：`python3 scripts/build-home-ohos.py --deveco-home ... --packager ... --signing-config ...` 使用已有的 DevEco 签名配置构建一个普通的 OpenHarmony 应用。见 [docs/home-build.md（英文）](docs/home-build.md#openharmony-home)。
- **iOS 模拟器**：在 `home/` 下运行 `../.sources/makepad/target/release/cargo-makepad makepad apple ios --org=dev.makepad --app=octosense run-sim -p octosense --features mobile-only`。CI 不构建 iOS。

## 应用如何进入 Home

| 类型 | 来源 | 运行方式 |
| --- | --- | --- |
| 系统应用：新闻、相册、地图、相机、邮件 | OctoSense-System-Apps 的 `apps/<name>/bundle/`，由 `home/system-apps.json` 选择 | 隔离运行的脚本应用，打包进构建产物 |
| 商店应用 | App Hub 目录，运行时安装 | 隔离运行的脚本应用或卡片应用 |
| AppCard 助手 | OctoSense-System-Apps 的 `apps/appcard/app/app`（`octos-app`） | 原生模块，链接进 Home |
| 原生模块 | `home/apps/*`，以及来自 Makepad 的 Sheets | 链接的模块，由 feature 控制 |

**系统应用**（[ADR 0004（英文）](home/docs/adr/0004-system-apps-are-contained-script-apps.md)）。App Hub 共用 crate 的构建过程会打包 `home/system-apps.json` 中列出的每个应用；它通过 `OCTOSENSE_SYSTEM_APPS` 找到这个文件，该变量由 `home/.cargo/config.toml` 设置。未设置时不包含任何系统应用。每个应用在自己的 isolate 中按其清单的策略运行，在启动器中保留短 ID（`os.news` 显示为 `news`），且无法被商店替换，因为 `os.` 下的 ID 是保留的。相册的示例图库从 `home/apps/photos/resources/photos` 挂载，而不是打包进去。

**商店应用**。App Hub（`apphub`）浏览签名目录，校验并安装应用包；Card 运行器（`card`）按每个已安装应用的清单所申请的权限，在独立的 isolate 中打开它。已安装的应用以 `hub:<manifest-id>` 出现在启动器和最近任务中。两者都来自 `octosense-app-hub-app`，由默认的 `app-hub` feature 链接，所有移动端构建也都包含。如何开发和发布应用：[OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)。

**宿主服务与密钥**。应用不能自己持有的东西，通过宿主服务获取：`host.request("family.method", ...)`，需要清单授权。邮件是第一个宿主服务：`mail` 服务（OctoSense-System-Apps 的 `apps/mail/host-service`）保管账户和密码，密码存放在钥匙串中或由 Android Keystore 密钥保护，应用只能拿到文件夹、邮件和发送功能，永远拿不到 socket 或密码。密钥归宿主所有：任何脚本应用都不收集密码、PIN 或一次性验证码。用户只在宿主自有面板上输入这类信息；在隔离运行的应用中，运行时会让密码输入框失效；App Hub 的准入检查会拒绝声明了此类输入框的应用包。

**原生模块**。移动端构建总是链接 Reference、Sheets、AppCard 和 App Hub。桌面构建通过 feature 选择：

| Feature | 链接内容 |
| --- | --- |
| `app-hub`（默认） | App Hub、Card 运行器、系统应用和邮件服务 |
| `app-reference`、`app-sheets`、`app-appcard` | Reference、Makepad Sheets、AppCard |
| `app-news`、`app-photos`、`app-maps` | 原生的新闻、相册、地图，用于对比；各自会替换对应的脚本应用 |
| `app-aichat` | 以模块形式链接 Makepad 的 aichat 助手 |
| `mobile-apps` | 除 `app-aichat` 外的以上全部 |
| `mobile-only` | 独立的手机 Shell（Android 会自动开启） |

**AppCard**（“Ask anything”）是 `octos-app` 的 `AppShell`，由 `home/apps/appcard` 托管，它以路径依赖指向 `.sources/system-apps/apps/appcard/app/app`。其内核 `octos` 不是 Home 的 Cargo 依赖；要把它打进 APK，请为打包工具设置 `MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=<path>"`（[home/docs/android-appcard-build.md（英文）](home/docs/android-appcard-build.md)，其中记录的版本早于当前版本）。未打包内核时，AppCard 会退回到 WebSocket 传输和登录界面。

## 版本固定与更新

| 文件 | 固定内容 |
| --- | --- |
| `home/native-apps.lock.json` | OctoSense-System-Apps 的版本（`.sources/system-apps`） |
| `home/system-apps.json` | 包含哪些系统应用，以及 Home 为它们挂载的资源 |
| `home/native-runtime.lock.json` | OctoScript-Makepad 的版本；其 `runtime.json` 指定 Makepad 与 OctoScript |
| `home/runtime-patches.lock.json` | 经过审查的 Makepad 补丁：基础版本、来源提交、SHA-256、应用后的 tree |
| `home/Cargo.toml`、`home/Cargo.lock` | Makepad 的 `rev`（必须与运行时一致）、App Hub 的 `rev`、`nix` 使用的 octos `rev` |
| `home/upstream/makepad.json` | 从 Makepad 导入的窗口管理器源码的来源记录 |

Cargo 清单保证**每种依赖只有一个来源**：`[patch]` 把所有 Makepad crate（包括 App Hub 的 crate 和 `octos-app` 引用的那些）指向 `.sources/makepad`，把所有 App Hub crate 指向 Home 固定的 App Hub 版本；`nix` 取自 `octos-app` 所用的同一个 octos 版本。

- **System-Apps**。在 `home/native-apps.lock.json` 中写入新版本，运行 `python3 scripts/setup-home.py --update`；如果 `octos-app` 的依赖有变化，在 `home/` 下不带 `--locked` 构建一次，并提交 `home/Cargo.lock`。如果新的 System-Apps 固定了不同的 octos 版本，把 `home/Cargo.toml` 中 `nix` 的补丁也移到该版本。
- **Makepad / OctoScript**。更新 `home/native-runtime.lock.json` 中的 OctoScript-Makepad 版本，把其中的 Makepad 版本同步到 `home/Cargo.toml` 和 `home/apps/*/Cargo.toml` 的每个 `rev = "…"`，并对运行时补丁做 rebase 或删除。任何不一致都会让 `setup-home.py --check --cargo` 失败。完整流程见 [home/docs/makepad-fork.md（英文）](home/docs/makepad-fork.md#adopting-a-fork-revision)。
- **运行时补丁**。`patches/runtime/makepad-contained-apps.patch` 在 Makepad `1d3d383e` 之上携带 [makepad#30](https://github.com/OctoSense-org/makepad/pull/30)（隔离运行的脚本应用）。setup 应用补丁后将其保留为已暂存状态；`--check` 只接受记录中的那个 tree。makepad#30 合并且运行时版本越过它之后，从 `home/runtime-patches.lock.json` 中删除 `makepad` 条目，并删除补丁文件。
- **App Hub**。同时修改 `octosense-app-hub-app` 的 `rev`（两处依赖声明），以及 `[patch."https://github.com/OctoSense-org/OctoSense-App-Hub"]` 中四个 App Hub crate 的 `rev`。

## 测试与验证

CI（`.github/workflows/home.yml`）在 Ubuntu 上运行产品测试，在 macOS 上构建并测试 Home，均从仓库根目录开始：

```sh
python3 -m unittest discover -s tests -v
python3 scripts/setup-home.py
python3 scripts/setup-home.py --check --cargo
cd home
cargo check --locked --workspace --features mobile-apps
cargo test --locked --features mobile-apps -p octosense -p octosense-app-policy -p octosense-app-hub -p octosense-news -p octosense-appcard
cargo test --locked -p octosense-maps -- --skip view::tests --skip module::tests
cargo test --locked -p makepad-widgets splash_policy
cargo test --locked -p makepad-script-std gate::tests
```

地图的 view 和 module 测试需要 macOS 主线程，因此被跳过。`octosense-app-hub-app` 在 OctoSense-App-Hub 的 CI 中测试，不在这里。手机 Shell 自身的单元测试：`cargo test --features mobile-only mobile -- --test-threads=1`。

其他检查：

- **Home 脚本**：在 `home/scripts` 下运行 `python3 -m unittest test_smoke test_upstream`（`test_upstream` 需要 Python 3.11）。
- **网页安装器**：在 `web-installer/` 下运行 `npm ci --ignore-scripts`、`npm test`、`npm run test:browser`（`.github/workflows/web-installer.yml`）。
- **无人值守的界面检查**。每个 Makepad 应用都有一个本机控制接口：`MAKEPAD_REMOTE=<port>`（或 `--remote`）会打印端口，并通过 HTTP 提供截图、控件树和真实输入（`/help` 列出所有路由；Android 上不可用）。`MAKEPAD_HIDE_WINDOWS=1` 在 macOS 和 Windows 上让窗口不显示在屏幕上。可以与 `--test-action`、`MAKEPAD_WM_TEST_APP` 和 `--module` 组合使用。`python3 home/scripts/smoke.py` 就是这样驱动 release 构建的。固定版本的 Makepad 自带 `makepad_test` crate（默认隐藏窗口），但 Home 目前还没有 `makepad_test` 测试集。
- **真机**：已刷机的 ROM 使用 `scripts/checklist.sh`、`scripts/agent-test.sh` 和 `scripts/verify-phone.sh`；帧时间用 `home/scripts/measure_android_frames.py` 测量。记录见 [docs/home-device-validation.md（英文）](docs/home-device-validation.md)、[home/docs/validation.md（英文）](home/docs/validation.md)、[home/docs/android/](home/docs/android/README.zh-CN.md)。

源码和构建检查不能证明 ROM 能够开机，也不能证明无线、通知、最近任务、紧急呼叫和 OTA 恢复正常工作。在更换证书或发布之前，请完成真机检查。

## 文档

- ROM 决策：[docs/adr/](docs/adr/README.zh-CN.md)。Home 决策：[home/docs/adr/](home/docs/adr/README.zh-CN.md)，其中包括 ADR 0004（系统应用作为隔离运行的脚本应用）。
- 构建与交付：[docs/home-build.md（英文）](docs/home-build.md)、[docs/flashing.md（英文）](docs/flashing.md)、[docs/updates.md（英文）](docs/updates.md)、[web-installer/README.zh-CN.md](web-installer/README.zh-CN.md)。
- ROM 平台：[docs/agent-service.md（英文）](docs/agent-service.md)、[home/android/README.zh-CN.md](home/android/README.zh-CN.md)、[PLAN.md（英文）](PLAN.md)。
- Home：[home/README.zh-CN.md](home/README.zh-CN.md)、[home/docs/makepad-fork.md（英文）](home/docs/makepad-fork.md)、[home/docs/build-tool.md（英文）](home/docs/build-tool.md)。
- 历史：[docs/home-migration.md（英文）](docs/home-migration.md)（Home 如何迁入本仓库）。

## 参与贡献

`main` 分支受保护：请在分支上工作，并向 `main` 发起 pull request，CI 必须通过。不要把签名密钥、keystore 和个人路径提交进仓库（`.gitignore` 和 `tests/test_no_local_paths.py` 会检查）。如果改动需要依赖方配合，先在上游合入，再在这里更新版本锁定；不要把锁定指向一个会移动的分支。

## 许可证

Apache License 2.0（[LICENSE](LICENSE)、[NOTICE](NOTICE)）。第三方许可证见 [LICENSES/](LICENSES)。
