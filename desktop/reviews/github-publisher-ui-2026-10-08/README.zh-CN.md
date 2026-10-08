# 无独立密钥的开发者发布 UI 验收 — 2026-10-08

[English](README.md)

最终源码 `5e1a8414cba4c9c3ab041b5555b6d90bcbad03bb` 的 macOS 打包 Shell 使用原生 Makepad instrument 和全新数据，重新通过 **8/8 项检查**。[最终收据](macos-final-5e1.json)用 SHA-256 绑定可执行文件和真实 GitHub 发布包。先前 `5607e90b` 的[收据](macos-5607.json)及下方截图保持不变；重建后可执行文件字节相同，但最终包仍实际重跑了一遍。这是针对发布、安装和更新的验收，不是长时间 UX 浸泡测试，也不是正式发行公告。

测试拒绝了 App Hub 的可选 Agent，搜索 Publishing Test Notes，安装并打开 0.1.0，添加笔记，重启后更新到 0.1.1，确认原笔记保留，添加第二条并删除第一条，再次重启并从应用库打开。剩余笔记仍然存在。测试启动的 Shell 进程均已退出。四张原始截图均已审视，不包含个人账户、凭证或私有路径。

| 截图状态 | 证据 |
| --- | --- |
| 搜索结果 | [搜索](01-search.png) |
| 安装后的应用及第一条笔记 | [0.1.0](02-installed-note.png) |
| 更新后修改笔记 | [0.1.1](03-updated-notes.png) |
| 重新打开后数据保留 | [从应用库打开](04-retained-note.png) |

## 发布证明与目录边界

真实开发者证明来自测试仓库的 [0.1.0 工作流](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37736273522)和 [0.1.1 工作流](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37736765473)。没有使用开发者签名密钥或仓库签名 secret。Shell 安装的是附有证明的发布原始字节，不是重新盖章的源码检出。

准入使用**隔离的本地旧格式测试目录**，其临时 Hub 专用密钥在两个目录快照生成后已删除。测试显式设置 `OCTOSENSE_HUB_CATALOG=legacy`，并使用全新的 Shell、应用和 core 数据目录。没有修改公开目录，也没有向 App Hub 提交测试应用。这与生产 GitHub 证明目录 sequence 11 的验证是两项独立证据。正式开发者通过 [App Hub 提交 issue](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/new?template=submit-app.yml)申请发布；GitHub release 成功本身并不构成申请或批准。

## Instrument 限制

此前尝试暴露了驱动未处理首次 Agent 同意框、安装确认按钮滚动以及启动快照尚未就绪的情况。还遇到过输入已生效但 `wait=1` 帧确认失败的瞬态错误。成功的驱动使用 `wait=0` 排队原生输入，随后验证真实 UI 和存储状态，没有重放结果不明确的写操作。只读截图请求的重试限定为八秒。收据保留这些限制和此前结果，没有将每次尝试都记为成功。

自绘的可选 Agent 同意按钮没有出现在控件快照中。驱动根据已审视的原生画面选择 **Don't allow**，并验证自身测试配置中的拒绝记录。被测应用不需要 Agent 或模型调用。

## Android 测试包准备

原来的两个包只声明 macOS。新生成的真实 [0.1.2](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37742612574)和 [0.1.3](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37742627927) 发布同时声明 macOS 与 Android，使用已合并的 Design Flow `6f4ce207` 生成器和 App Hub `655114c4943cd2490daaefa2173e7b5aaa20669f`。工作流、原生证明验证、平台声明与哈希均通过，详见[准备收据](android-fixture-releases.json)。该收据**不代表**手机 UI 已通过验收。旧包及证据保持原样。

## 最终打包版本与 OnePlus 6

最终 Mac 测试重跑了全部八项检查，并审视原始[安装后](final-01-installed-note.png)和[更新后](final-02-updated-notes.png)截图。[截图哈希](captures-sha256.json)绑定本仓库保存的原图。

OnePlus 6 真机使用源码 `8afcf35f03649f31b939b822302c132f18feb86e`，通过 **8/8 项功能检查**：搜索、安装并打开 0.1.2，添加笔记，更新到 0.1.3 后完整证明及笔记保留，修改笔记列表，重启后从应用库打开仍保留最终笔记。详见[真机收据](android-8af.json)及[重新打开的原始画面](android-24-library-opened.png)。其余截图哈希保留在收据中；不提交原始日志或私有配置。

**手机仍有视觉缺陷：**“Publishing Test Notes”标题在右侧被截断。正文可换行，被测操作仍可触达。这是功能通过，不是完整的移动 UX 通过。问题来自初始模板标题，另有窄窗口修复；该修复没有改写这些不可变发布包，也未宣称通过手机复测。

手机使用隔离的 debug Home 包、限定包范围的 ADB 输入和临时本地旧格式目录；没有替换日常 Home，也未使用个人账户或模型。手机源码早于最终打包专用修改，收据保留实际源码和 APK 哈希。Mac 与手机测试进程均已结束。这些是合成发布测试应用的结果，不是另行提交的 ymote Notes、Inbox、Calendar 或 Camera 应用的验收。
