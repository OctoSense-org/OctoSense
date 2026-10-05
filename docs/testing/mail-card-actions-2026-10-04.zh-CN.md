# OnePlus 6 邮件卡片按钮测试 — 2026-10-04

[English](mail-card-actions-2026-10-04.md) | 简体中文

**最终 DeepSeek 和 MiniMax 卡片均通过本地交互测试。** 新 AgentMail 邮件到达 Gmail 后，
自动触发 Mail Agent，由模型编写卡片。在指定 OnePlus 6 上，通知打开对应卡片，
Show code→Back→Details→Back 在完整面板及 Glance 上均可用。两个模型都展示新取件码
**9064**、正确取件事实及物品身份。这补齐了[较早新邮件试验](mail-events-2026-10-04.zh-CN.md)
缺少按钮的问题，不代表完整实现[邮件操作卡片计划](../../apps/mail/docs/2026-10-01-email-action-card-plan.md)。

## 最终配对结果

最后两封邮件正文逐字相同，采用同一配置版本；主题含各自模型标签，并使用对应模型路由。
虚构台灯取件邮件包含
取件码、地点、营业时间、截止时间/时区及带照片证件要求，没有追踪网址。
[发送记录](mail-card-actions-2026-10-04/repair-send-receipts.json)和
[策略记录](mail-card-actions-2026-10-04/policy-receipt-repair.json)记录了这一配对条件。
策略不含具体取件码。最终 9064 不同于前两轮的 4821、7392，追踪参考编号仍含 4821，
因此结果也检验了模型提取的是当前取件码。

| 最终手机视图 | DeepSeek V4 Flash | MiniMax M3.1 Flash Preview |
| --- | --- | --- |
| 摘要与按钮 | [完整卡片](mail-card-actions-2026-10-04/screenshots/deepseek-repair-full-brief.png) | [完整卡片](mail-card-actions-2026-10-04/screenshots/minimax-repair-full-brief.png) |
| 取件码 9064 与返回 | [取件码](mail-card-actions-2026-10-04/screenshots/deepseek-repair-full-code.png) | [取件码](mail-card-actions-2026-10-04/screenshots/minimax-repair-full-code.png) |
| 地址、营业时间、截止时间与返回 | [详情](mail-card-actions-2026-10-04/screenshots/deepseek-repair-full-details.png) | [详情](mail-card-actions-2026-10-04/screenshots/minimax-repair-full-details.png) |
| Glance 上的相同交互 | [取件码](mail-card-actions-2026-10-04/screenshots/deepseek-repair-glance-code.png) | [取件码](mail-card-actions-2026-10-04/screenshots/minimax-repair-glance-code.png) |

过滤后的 Android 日志分别记录 **八次已应用状态转换**：
[DeepSeek](mail-card-actions-2026-10-04/deepseek-repair-taps.log)、
[MiniMax](mail-card-actions-2026-10-04/minimax-repair-taps.log)，完整面板四次、Glance 四次。
[手机观察记录](mail-card-actions-2026-10-04/phone-review.json)记录对应检查。未修改的源码
在 **310 × 330** 和 **344 × 440** 逻辑点下也均通过四次原生点击：
[DeepSeek 原生记录](mail-card-actions-2026-10-04/repair-deepseek-native.json)、
[MiniMax 原生记录](mail-card-actions-2026-10-04/repair-minimax-native.json)。

这些是已测路径的功能与可读性通过，不代表无障碍或视觉打磨已完善。两个原生 Details
目标仍只有 **61 × 24 点**，未达到 44 点高度目标。DeepSeek 的营业时间、截止时间缺少
明确标签。MiniMax 标签更清楚，完整面板详情虽有滚动条，但事实和 Back 都可见。
这组少量迭代案例不能证明模型的普遍优劣。

## 保留之前的结果

三个共同策略阶段共收到六封新邮件，产生六张卡片。每张都由模型通过 `mail.publish_card`
编写，没有测试人员手写或修补最终源码/数据。审查人员通过系统 Agent 调整策略并操作 UI，
没有逐封聊天提示或手动派发 peer 回合。初始、中间记录与最终通过结果分别保留。
每个阶段的 APK 还带有修订后的内置 Mail 指引（`SKILL.md`，哈希见构建记录）；Git 中只保留最终 0409 版本。

| 策略阶段 / APK / 取件码 | DeepSeek | MiniMax |
| --- | --- | --- |
| 初始 / `2026100407` / 4821 | 两次校验修复后发布。完整卡片按钮可用，但摘要重复、详情缺少物品身份，未达布局目标；未测 Glance 取件码。 | 两次校验修复后发布。本地按钮可用，但 Pickup details 横向被裁切。 |
| 布局反馈 / `2026100408` / 7392 | 首次发布成功。手机两个面板四次交互均可用，但冗长卡片未通过更紧的原生 310 × 400 目标。 | 首次发布成功。按钮不可见，详情编造截止后果并错误绑定追踪标签，审查失败。 |
| 紧凑修复 / `2026100409` / 9064 | 首次发布成功；最终手机、原生交互通过，仍有上述目标尺寸及标签不足。 | 修复一次不支持的 TextEyebrow width 后发布；最终手机、原生交互通过，仍有上述目标尺寸不足。 |

初始 MiniMax 的[手机摘要](mail-card-actions-2026-10-04/screenshots/initial-minimax-full-brief.png)
和[原生记录](mail-card-actions-2026-10-04/initial-minimax-native.json)显示宽度失败：
第一个按钮占 210 点，扣除间距后第二个只剩 86 点，增加高度无济于事。初始 DeepSeek 的
[摘要](mail-card-actions-2026-10-04/screenshots/initial-deepseek-full-brief.png)、
[详情](mail-card-actions-2026-10-04/screenshots/initial-deepseek-full-details.png)及
[原生记录](mail-card-actions-2026-10-04/initial-deepseek-native.json)保留其不同问题。

中间阶段，DeepSeek 的[原生记录](mail-card-actions-2026-10-04/final-deepseek-native.json)
显示 310 × 400 下 Details 只剩一逻辑点可见高度。这是紧凑高度目标失败，不能否定手机
动态高度面板的实际功能通过。MiniMax 的[手机卡片](mail-card-actions-2026-10-04/screenshots/minimax-final-full-brief.png)
和[原生复现](mail-card-actions-2026-10-04/final-minimax-native.json)显示即使 344 × 800
控件尺寸仍为零：已接纳的 `Chip(width: .fill)` 在 Fit 包装内收缩。
**这是渲染器限制，不是非法 L0 语法。** 最终指令省略 width 并缩短内容，未修复通用渲染器
缺陷。数据含 7392 并不能使此前不可到达的取件码视图成为成功测试。

## 支持行为与证据边界

按钮路径是 `Chip.on_tap`→声明的 L0 事件→本地 `mode` 变化→对应 `when` 视图，
Back 恢复摘要。状态是临时的，不修改邮箱或远端状态。原计划的外部 **Track** 需要真实
网址与已实现的执行器，本场景无网址，shell 也不执行 `sys.link` 写操作。真实 **Send**、
**Confirm**、**Mark read/Mark done**、预约及持久完成状态都不在本测试范围。
没有发送回复，也没有修改已读状态或预约。

隐藏窗口原生工具使用固定版本 Makepad 及与 shell 对应的 L0 导航/状态适配层，
并非 Android shell。四次点击检查记录本地转换，宿主写操作列表为空。手机点击与截图
另作 Android 证据。原生字体不同；较早不完整的字形截图经整帧重绘重新采集，没有归咎于模型。

构建记录将独立包 `dev.makepad.octosense.studio` 绑定到 APK 及载荷哈希：
[0407](mail-card-actions-2026-10-04/build-receipt.json)、
[0408](mail-card-actions-2026-10-04/build-receipt-final.json)、
[最终 0409](mail-card-actions-2026-10-04/build-receipt-repair.json)。未替换生产 Home。
[工具结果](mail-card-actions-2026-10-04/tool-results.json)保留五次失败发布尝试及六次成功；
[模型归属](mail-card-actions-2026-10-04/provider-attribution.json)标识每个回合。
[六份卡片参数](mail-card-actions-2026-10-04/cards/)原样复制，并用
[文件/源码哈希](mail-card-actions-2026-10-04/artifact-hashes.json)校验。
哈希证明复制保真，不等于独立重放作者过程。公开证据不包含 Gmail 地址、凭据、私有配置
或邮件传输标识。[实现指南](../mail-agent-events.zh-CN.md)中的进程存活及远端操作限制仍适用。
