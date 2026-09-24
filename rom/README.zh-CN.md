# OctoSense ROM

[English](README.md) | [简体中文](README.zh-CN.md)

本仓库维护同一套 Android 产品的两种交付方式：

- **OctoSense Home**：可安装在普通 Android 手机上的桌面应用，包含内置应用、App Hub、通知和设备控制，通过公开 API 及用户授予的权限工作。
- **OctoSense ROM**：在系统中预装同一套 Home，并集成具有系统权限的 agent、Quickstep 和 SystemUI。当前目标机型为 OnePlus 6（一加 6，`enchilada`），基于 LineageOS 22.2 / Android 15。

Home 源码位于 `home/`，构建时不需要单独检出另一个桌面仓库。普通版 Home 与 ROM 版 Home 保留各自的签名方式，应用 ID 均为 `dev.makepad.octosense`，但生成的签名 APK 是不同产物。单独安装 Home 不会替换普通手机的全局 SystemUI。

```text
home/                        Home Rust 工作区、内置应用和 Android 桥接代码
home/android/platform-build/ Quickstep、SystemUI 源码及构建准备工具
vendor/octosense/             ROM 产品配置、权限、资源覆盖和系统 agent
scripts/                     Home/ROM 构建、产物准备和更新入口
patches/runtime/             App Hub 所需的运行时补丁
web-installer/               OnePlus ROM 浏览器刷机工具
```

先运行 `python3 scripts/setup-home.py`，将指定版本的依赖准备到 Git 忽略的 `.sources/` 目录。独立 APK 与 ROM 的构建命令、签名、SDK 要求和验证方式见 [Home 构建与 ROM 产物准备（英文）](docs/home-build.md)。AOSP/LineageOS、内核和厂商源码仍位于外部系统构建目录。

App Hub 的应用目录和发布流程位于 [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub)。[迁移记录（英文）](docs/home-migration.md) 说明了导入的源码历史、尚未处理的分支以及停用旧仓库前的检查项。[ROM 更新（英文）](docs/updates.md) 继续使用本仓库现有的发布源。密钥保存在仓库之外，源码整合不会改变签名密钥。

## 在浏览器中刷入 ROM

请按照 [中文网页刷机指南](web-installer/README.zh-CN.md#在浏览器中刷入-rom)，准备镜像、在电脑上启动安装页面、通过 USB 连接 OnePlus 6、校验并安装 ROM，最后检查手机是否正常启动。指南包含解锁步骤和常见问题处理。

当前安装器是面向 OnePlus 6（`enchilada`）的**本地开发预览版**。全新安装会清除手机数据。公共网站刷机尚未开放，准确识别机型的方法仍需真机验证；无法确认身份的手机只能读取信息，不能刷写。公开发布计划见 [ADR 0001（英文）](docs/adr/0001-public-web-installer.md)。

## 构建主机的 chroot 记录

当前构建在 Ubuntu 26.04 主机上的 Ubuntu 24.04 chroot（`~/octosense-adr0001/rootfs`）中运行。该 chroot 缺少 `gpgv`，apt 无法验证软件包归档；现有环境通过下载 24.04 的 `.deb` 文件并在 chroot 中运行 `dpkg -i` 添加软件包。

已添加 `libssl3t64` 和 `libssl-dev` 3.0.13-0ubuntu3.15，供 msm-4.9 内核的主机工具 `sign-file` 与 `extract-cert` 使用 OpenSSL 头文件。这是构建服务器的环境记录，使用网页安装器的手机不需要配置 chroot。

## 首次刷机记录（2026 年 9 月 18 日，OnePlus 6）

以下是首次部署时的调试记录；当前浏览器安装流程请以上面的中文指南为准。

- 当时这台 Mac 的 USB 链路上，`adb sideload` 和较长的 fastboot 传输会中断，boot、dtbo、vbmeta 等小文件传输正常。最终使用 `scripts/split-sparse.py` 将 `system.img` 拆成独立的 50 MB 稀疏镜像，逐个执行 `fastboot flash system_b part`，通过 `perl -e 'alarm ...'` 设置超时并单独重试。655 MB 的 vendor 镜像通过 `fastboot -S 64M` 写入。
- 卡住的 `fastboot` 进程会占用 USB 设备句柄，结束该进程后设备才重新可用。如果 bootloader 本身停止响应，当时通过手机上的 “Restart bootloader” 重启它。
- 分区只写入一部分时，启动可能进入 Qualcomm CrashDump。当时先按住电源键和音量下约 10 秒，再使用音量上、音量下和电源键的组合返回 bootloader。
- 当时 `fastboot -w` 只擦除了类型为 raw 的 userdata，Android 在首次启动时完成格式化。擦除后 USB 调试默认关闭；在该开发构建中，重新启用开发者选项和 USB 调试后，预授权的 `PRODUCT_ADB_KEYS` 密钥无需再次确认。
- agent 是持久系统应用，Android 会拒绝使用 `adb install` 更新它，报错为 `INSTALL_FAILED_INVALID_APK`，因此 agent 的修改通过 ROM 构建交付。
- 更新手机中的系统 APK 时，使用平台密钥签名后执行 `adb install -r`。`apksigner` 使用的 JDK 位于 `~/.local/share/octosense/android-tools/makepad-android/openjdk`。单独构建模块可使用 `run-rom-rootfs.sh module <Name>`，日志位于 `exports/rom-build/module.log`。
