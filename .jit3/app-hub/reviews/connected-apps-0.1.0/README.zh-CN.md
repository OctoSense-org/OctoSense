# 连接账户的应用预览版准入

[English](README.md) | 简体中文

0.1.1 版另有记录：[connected-apps-0.1.1](../../docs/admissions/connected-apps-0.1.1/README.zh-CN.md)。

目录序列 **7** 收录了 ymote 发布的全部三款可独立安装的 macOS 开发者预览应用。[`admission.json`](admission.json) 记录了它们分别在序列 5、6、7 中准入，每个序列一款。App Hub 维护者指示完成运行时交付和目录验收，以此授权了发布。几个独立的 Agent 审核了发布者的应用包、能力、隐私说明，以及下文所述的 `hub publish` 兼容性修复。本记录记载 Agent 的审核结果和维护者的发布授权，不代表已有独立的人工审核人员完成了真实提供方验收。

| 应用 | 发布者 commit | 提交 issue |
| --- | --- | --- |
| GitHub Notes | `5f0c4c6b13bddae87a4945b2b76ca2a416793f14` | [#121](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/121) |
| Inbox Assistant | `28dd24a39a7e9668b877dd84d9577d10b3f87363` | [#122](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/122) |
| Google Calendar | `c0783291a528689008d7213f1b1163c9c15ac874` | [#123](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/123) |

[`admission.json`](admission.json) 记录了应用包摘要、源码标签、公开发布文件的验证结果和目录标识。每个应用包都与发布者签名时逐字节一致，并通过了 `hub check`。随后，`hub publish` 再次执行准入检查，原样复制文件，并用现有的已认证工作密钥为目录签名。审核人员按 SHA-256 把每个工件文件与发布者源码逐一比对；`admission.json` 只记录每个应用包的摘要和文件数，没有记录这些逐文件哈希。索引条目是已准入条目的精确副本，`artifacts/` 中应用包目录旁边的 `.pack.json` 文件包含相同的应用包。本目录不包含签名密钥、提供方凭据、私人配置或原始运行日志。公共信任锚保持不变。

准入前，审核 Agent 就以下几点检查了发布者的扫描答复：可见功能；平台和分类；能力是否最小必要；误导性界面；藏在应用内容中的 Agent 指令；冒犯性或私人内容；工具风险和路由。它们审核通过了这些应用，范围限于已披露的 macOS 开发者预览。所有暴露提供方私人数据的工具默认都不可共享。GitHub Notes 不导出写入工具，Inbox Assistant 不导出发送工具，Google Calendar 不导出预约工具。账户管理和外部操作的确认仍由宿主负责。发布者的隐私政策和商店概要说明了提供方和模型产生的网络流量，只有一处遗漏：GitHub Notes 带了工具却没有 `agent` 字段，所以 OctoSense 桌面版 0.1.0-beta.2 的商店概要显示“Runs no assistant.”，而 Shell 实际提供“Ask GitHub Notes”。发布者修订后的隐私政策已经说明这个 Agent，基于 App Hub `main` 构建的商店也会在概要中说明。

兼容性审核发现，较旧的严格目录读取器不接受展示元数据中的 `host_method` 字段。修复后的 `hub publish` 只在目录的工具概要中省略这个字段。已签名的原始 `tools.json` 中的映射保持逐字节一致，宿主执行时加载的仍是它们。回归测试验证了旧字段结构的解析、规范签名字节和安装后映射不变，并验证篡改过的映射或禁止的路由会遭拒绝。这些测试只检查旧的数据格式，并没有运行旧版桌面程序。

这些记录证明了准入和完整性，但不证明以下各项：真实的 GitHub 或 Google 连接、提供方写入、亲手点按确认、Android 上的 Google 授权、Windows 或 Linux 上的体验，以及长时间运行没有内存增长。现有的原生、模拟提供方和模型证据保存在发布者仓库中，仍对应原始源码哈希。

## 官方商店验收

[公开 HTTP 检查](official-http.json) 请求了 21 个地址，均返回状态 200：`catalog.json`、三个索引条目、三个 `.pack.json` 文件，以及三个应用包共 28 个文件中的 14 个（各应用的清单、图标和截图）。它验证了默认信任锚，记录的应用包摘要与 `admission.json` 一致。

[macOS Apple silicon 预览版](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-beta.2) 由源码 `84e3438b7ee25b43ba945bfe063db594a71e61ba` 构建。该版本的打包应用按原样运行，在全新的隔离配置中从官方目录搜索、安装并打开了三款应用，没有替换目录或信任锚。[九项功能检查和六项原始像素视觉评审](native-store/receipt.json) 全部通过，包括进程完全重启后本地草稿的精确恢复。六项视觉评审均由同一个评审 Agent 完成，记录名为 `calendar_sample`。回执绑定了运行时 commit；可执行文件、内核和各应用包的哈希；以及该版本 `macos-aarch64-release-receipt.json` 的 SHA-256（记录为 `package_receipt_sha256`）。回执原样保留候选版本的记录；发布没有增加新的测试结果。

| 应用 | 恢复状态与原始截图 |
| --- | --- |
| GitHub Notes | [Unicode Markdown 与渲染预览](native-store/notes-restored-preview.png) |
| Inbox Assistant | [第 2 版虚构回复，在 Reply 和 Chat 标签页之间保持不变](native-store/inbox-restored-reply.png) |
| Google Calendar | [本地日程草稿](native-store/calendar-restored-draft.png) 和 [时区](native-store/calendar-restored-timezone.png) |

[商店](native-store/official-store.png) 和 [已安装应用库](native-store/installed-library.png) 的截图同样是未经编辑的原始像素。

本次运行记录了 **14 次只读截帧错误**，没有输入错误，也没有重放。运行中停止并恢复过一次，原因是测试驱动脚本在滚动尚未稳定时使用了旧坐标；修正后的测试驱动脚本会先观察到新的一帧，再计算下一个目标位置。这次运行证明了上述本地流程，但不是原生测试工具的无中断运行，也不是延迟测量或无故障的体验持续测试。本次运行没有使用 OAuth 连接、真实提供方数据、模型调用或远程写入，也没有对发送或保存进行亲手点按确认。
