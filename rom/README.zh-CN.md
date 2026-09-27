# OctoSense ROM

[English](README.md) | 简体中文

OctoSense ROM 是面向 OnePlus 6（一加 6，`enchilada`）的 LineageOS 22.2（Android 15），预装 OctoSense Home 应用以及具有系统权限的 agent、Quickstep 和 SystemUI，让 Agent 进入系统层。本目录只包含镜像本身：产品层、补丁、构建/签名/刷写/更新镜像的脚本，以及网页安装器。它原是 OctoSense-ROM 仓库（其中的 `home/` 现为 [`../phone/`](../phone/README.zh-CN.md)）；见 [ADR 0001（英文）](../docs/adr/0001-one-octosense-repository.md)。

Home 应用本身（也可作为普通 Home 应用安装在任意 Android 手机上）从 [`../phone/`](../phone/README.zh-CN.md) 构建。镜像使用这个以平台密钥签名的 APK，并加上具有系统权限的系统侧组件。

> **要开发 OctoSense 应用？** 不需要 ROM。请从 [OctoSense-org 主页](https://github.com/OctoSense-org)的阅读列表开始（先读 OctoScript-App-Design-Flow 的 `AGENTS.md`，再读 `docs/QUICKSTART.md`）。目前还不支持把自己的应用包安装到手机上；想在发布前在 Shell 中看到它，请用指向本地目录的桌面端 Shell（见[桌面端 README](../desktop/README.zh-CN.md#发布前试用自己的应用)）。

## 目录结构

| 路径 | 内容 |
| --- | --- |
| `vendor/octosense/` | ROM 产品层：makefile、权限、overlay、sepolicy、设置后端、具有系统权限的 agent（及其 OTA 更新器） |
| `patches/` | LineageOS 与内核补丁。经审查的 Makepad 运行时补丁位于仓库根目录的 [`../tools/runtime-patches/`](../tools/runtime-patches) |
| `scripts/` | Home APK 构建（`build-home.sh`、`build-home.py`、`build-home-ohos.py`）、ROM 暂存、构建、刷写、发布和手机检查 |
| `web-installer/` | 面向 OnePlus 6 的 WebUSB 安装器（本地开发预览版） |
| `docs/` | ROM 决策记录（[docs/adr/](docs/adr/README.zh-CN.md)），构建、刷写、更新和验证记录 |
| `tests/` | 构建、暂存、清单和安装器脚本的 Python 测试 |
| `PLAN.md` | ROM 计划（英文） |
| `out/` | 已忽略。构建产物与构建回执（`out/home/<variant>/`） |

系统桥的手机侧（AIDL 接口、System Bridge APK、Quickstep 与 SystemUI 工程、平台构建暂存脚本）与 Home 一起位于 [`../phone/android/`](../phone/android/README.zh-CN.md)。

## 前提条件

- Home APK 所需的一切（[phone/README.zh-CN.md](../phone/README.zh-CN.md#构建与运行)）：已准备好的框架源码（在仓库根目录运行 `python3 tools/setup.py`）、固定版本的 `cargo-makepad`、Android SDK/NDK、完整的 JDK 17+ 和 Gradle 8.11.1。
- 面向 `enchilada` 的 LineageOS 22.2 源码树、OnePlus 厂商二进制文件、内核源码和 ROM 签名密钥，均在本仓库之外。
- 用于系统构建的 Linux 构建主机（Ubuntu 24.04 chroot）。

## 构建并刷写镜像

在仓库根目录构建用平台密钥签名的 ROM 版 Home APK 组合，再把它和各 fork 暂存到 LineageOS 源码树：

```sh
rom/scripts/build-home.sh --variant rom --sdk ... --android-sdk ... \
  --gradle-home ... --java-home ... --packager ... \
  --sign-key /private/rom-keys/platform.pk8 \
  --sign-cert /private/rom-keys/platform.x509.pem
python3 rom/scripts/stage-home.py                 # verify the receipt, copy the APKs to vendor/octosense/prebuilt/
rom/scripts/stage-forks.sh /path/to/lineage-tree  # apply vendor/octosense and stage the Quickstep and SystemUI forks
```

然后在主机上，`scripts/run-rom-rootfs.sh` 进入 chroot（位于 `OCTOSENSE_BUILD_ROOT` 下，默认 `/home/ubuntu/octosense-adr0001`），并运行 `build-rom.sh preflight`、`bacon`（完整签名构建）或 `module <name>`。它要求 `build-rom.sh` 位于构建根目录下的 `exports/build-rom.sh`，并由 systemd 启动；该 unit 和主机配置不在本仓库中。`scripts/make-keys.sh` 一次性生成签名密钥；`scripts/release.sh <build-tag>` 从 `~/.config/octosense/build.env` 中指定的主机取回构建完成的镜像。签名、构建回执和 ROM 版本：[docs/home-build.md（英文）](docs/home-build.md)。

刷写方式：

- **浏览器**：[网页安装器](web-installer/README.zh-CN.md#在浏览器中刷入-rom)，目前是本地开发预览版。全新安装会清除手机数据。在 [ROM ADR 0001（英文）](docs/adr/0001-public-web-installer.md) 完成前，公开网站上的网页刷写保持关闭。
- **命令行**：`scripts/flash.sh <build dir> [serial]`，或 [docs/flashing.md（英文）](docs/flashing.md) 中的 recovery sideload 方式，其中也记录了首次刷写的经验。
- **刷写之后**：`scripts/verify-phone.sh <build-tag> [serial]` 等待开机，并运行 `scripts/checklist.sh` 和 `scripts/agent-test.sh`。

已刷写的手机通过 GitHub Releases 进行 OTA 更新（[docs/updates.md（英文）](docs/updates.md)）；`scripts/ota-push.sh` 可从 Mac 推送一次更新。目前已构建镜像中的更新器读取的是已归档的 OctoSense-ROM 仓库（`OctoSense-org/octosense-rom`）的 releases；改为读取本仓库的 `rom-v*` releases 还需修改代码：`vendor/octosense/agent/.../Updater.java`、`scripts/publish-release.sh` 和 `scripts/ota-push.sh`。

## 镜像在 Home 之外增加的内容

| 组件 | 位置 |
| --- | --- |
| Home 与 System Bridge，作为平台签名的特权应用 | 从 [`../phone/`](../phone/README.zh-CN.md) 构建，暂存到 `vendor/octosense/prebuilt/` |
| 具有系统权限的 agent 服务 | `vendor/octosense/agent/`（[docs/agent-service.md（英文）](docs/agent-service.md)） |
| Quickstep 与 SystemUI fork、PermissionController 挂钩 | 由 `scripts/stage-forks.sh` 从 `../phone/android/platform-build/` 暂存 |
| 特权权限、overlay、sepolicy、设置后端 | `vendor/octosense/` |
| 共享手机主题 | [ROM ADR 0002（英文）](docs/adr/0002-rom-themes.md) |

应用、octos 内核服务和 App Hub 都属于 Home，在独立 APK 和镜像中完全相同（[phone/README.zh-CN.md](../phone/README.zh-CN.md)）。

## 测试与验证

CI（仓库根目录的 `.github/workflows/rom.yml`）在 `rom/` 和手机的 Android 源码变更时运行，工作目录为本目录：

```sh
python3 -m unittest discover -s tests -v
python3 scripts/generate-agent-aidl.py --check --sdk "$ANDROID_HOME"
bash -n scripts/stage-forks.sh scripts/publish-release.sh scripts/apply-to-tree.sh scripts/build-rom.sh
(cd web-installer && npm ci --ignore-scripts && npm test && npm run test:browser)
```

镜像构建本身不在 CI 中。源码和构建检查不能证明 ROM 能开机，也不能证明射频、通知、最近任务、紧急呼叫和 OTA 恢复可用：更换证书或发布前，请运行设备检查（`scripts/checklist.sh`、`scripts/agent-test.sh`、`scripts/verify-phone.sh`）。记录：[docs/home-device-validation.md（英文）](docs/home-device-validation.md)、[phone/docs/validation.md（英文）](../phone/docs/validation.md)、[phone/docs/android/](../phone/docs/android/README.zh-CN.md)。

## 文档

- ROM 决策：[docs/adr/](docs/adr/README.zh-CN.md)。Home 决策（包括 Home ADR 0004，系统应用作为隔离运行的脚本应用）和整个仓库的决策：[../docs/adr/](../docs/adr/README.zh-CN.md)。
- 构建与交付：[docs/home-build.md（英文）](docs/home-build.md)、[docs/flashing.md（英文）](docs/flashing.md)、[docs/updates.md（英文）](docs/updates.md)、[web-installer/README.zh-CN.md](web-installer/README.zh-CN.md)。
- ROM 平台：[docs/agent-service.md（英文）](docs/agent-service.md)、[phone/android/README.zh-CN.md](../phone/android/README.zh-CN.md)、[PLAN.md（英文）](PLAN.md)。
- 历史：[docs/home-migration.md（英文）](docs/home-migration.md)（Home 如何迁入 ROM 仓库，之后两者又一起并入本仓库）。

## 参与贡献

在分支上工作，并向 `main` 提交 pull request；见仓库的 [AGENTS.md（英文）](../AGENTS.md)。不要把签名密钥、keystore 和个人路径放进仓库（`.gitignore` 和 `tests/test_no_local_paths.py` 会检查）。不要刷写不是为本任务分配给你的手机。

## 许可证

Apache License 2.0（[LICENSE](LICENSE)、[NOTICE](NOTICE)）。第三方许可证位于 [LICENSES/](LICENSES)。
