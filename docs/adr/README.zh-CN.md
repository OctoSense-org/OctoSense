# 架构决策记录

[English](README.md) | 简体中文

OctoSense 仓库的决策：Shell、Shell 服务、系统应用，以及桌面、手机和 ROM 三种打包形态。新的 ADR 放在这里，编号接在下表最后一条之后。

这些决策在 `main` 的代码中如何组合在一起、哪些部分仍在规划中：[OctoSense 架构](../architecture.zh-CN.md)。

| ADR | 标题 | 状态 |
| --- | --- | --- |
| [0001](0001-one-octosense-repository.md)（英文） | 用一个 OctoSense 仓库承载 Shell、Shell 服务、系统应用和两种打包形态 | 已接受 |
| [0002](0002-event-driven-app-agents.md)（英文） | 事件驱动的应用智能体：应用按自己的触发条件思考，并把卡片发布到一览屏 | 提议中 |
| [0003](0003-shared-octos-client-access.md)（英文） | Talk to Octos：原生与外部客户端共用一个内核（需手动开启） | 已实现；Android 未验证 |
| [0004](0004-native-apps-hosting-and-peers.md)（英文） | 原生应用、应用智能体与跨应用协作：一份清单、按目标平台托管、每个应用都有智能体、由本人批准 | 已接受 |

## Home（手机 Shell）的决策，2026-09-16 至 2026-09-25

这些记录在仓库合并前写于 OctoSense-ROM（已停用，并入本仓库）的 `home/docs/adr/`，现原样作为历史保存在 [`home/`](home/) 下。它们保留原编号，引用时写作“Home ADR 0004”。文中出现 `home/src/`、`home/apps/` 等路径时，对应现在的 `crates/shell/src/`（Shell；设置应用在 `phone/src/`）和 `apps/`（见 ADR 0001）。状态为当时记录的状态。

| Home ADR | 标题 | 日期 | 状态 |
| --- | --- | --- | --- |
| [0001](home/0001-hybrid-android-launcher-and-system-bridge.md)（英文） | 混合式 Android 启动器与系统桥接 | 2026-09-16 | 已接受 |
| [0002](home/0002-agentic-app-security-model.md)（英文） | Agent 应用安全模型 | 2026-09-19 | 提议中 |
| [0003](home/0003-app-hub-and-store.md)（英文） | App Hub、签名与商店应用 | 2026-09-19 | 提议中 |
| [0004](home/0004-system-apps-are-contained-script-apps.md)（英文） | 系统应用作为隔离运行的脚本应用发布 | 2026-09-25 | 提议中 |
| [0005](home/0005-settings-octoscript-controller.md)（英文） | 用 Octoscript 实现设置应用逻辑 | 2026-09-25 | 源码已实现；模拟器验收待完成 |
| [0006](home/0006-builtin-settings.md)（英文） | 内置 OctoSense 设置 | 2026-09-24 | 已接受；完整替换进行中 |

## ROM 镜像的决策

OnePlus 6 镜像及其交付方式的记录位于 [`rom/docs/adr/`](../../rom/docs/adr/README.zh-CN.md)：0001 公开的浏览器安装器，0002 共享的手机主题。

## 其他位置

- 应用与 Agent 之间的代理（`crates/app-peers`）遵循 Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md)（由宿主持有的 octos 应用 peer）。
- App Hub、目录与准入检查：[OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub)。
