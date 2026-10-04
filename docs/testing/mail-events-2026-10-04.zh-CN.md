# OnePlus 6 新邮件测试 — 2026-10-04

[English](mail-events-2026-10-04.md) | 简体中文

[卡片按钮后续试验](mail-card-actions-2026-10-04.zh-CN.md)另行验证本轮初始生成卡片
缺少的内部交互。

AgentMail 实际邮件到达已连接的 Gmail 后，在独立 Android 测试包
`dev.makepad.octosense.studio` 中自动启动 Mail Agent。系统 Agent 配置策略，后续也通过
配置提供审查反馈。没有逐封聊天提示、手动派发 peer 回合，也没有由测试人员重写模型生成的
卡片。测试人员发送虚构邮件、检查宿主记录，并操作手机界面。

试验仅处理主题以 `[OctoSense simulation]` 开头的邮件，其他新邮件不读正文便跳过。
首次同步的 25 封邮件用于建立基线，没有变成事件。Gmail 凭据由用户在宿主登录面板输入；
证据中不包含密码、模型密钥、真实邮件正文或个人账户标识。

## 实际结果

| 模型及策略阶段 | 物流 | 预约 | 普通简报 |
| --- | --- | --- | --- |
| DeepSeek V4 Flash，初始 | 修复通知摘要过长后发布；卡片太啰嗦 | 发布；手机及原生渲染发现裁切 | 明确 `no_action`，未发布 |
| DeepSeek，紧凑布局反馈 | 自行修复两次语法错误后发布；事实完整可读 | 首次发布成功，事实完整可读 | 沿用前次对照结果 |
| MiniMax M3.1 Flash Preview，相同紧凑策略 | 自行修复两次语法错误后发布，但事实显示为占位符 | 发布，但事实显示为占位符 | 明确 `no_action`，未发布 |
| MiniMax，数据绑定反馈及宿主校验 | 自行修复两次语法错误后发布；事实完整，但仍啰嗦 | 发件账户验证后首次发布成功；事实完整可读 | 沿用前次对照结果 |

十封测试邮件已接收并处理，产生八次发布和两次明确跳过。发布成功不等于卡片可用：
MiniMax 第一组 L0 语法有效，但数据放在顶层，而非 `data.note`，因此事实缺失。
这些失败结果仍保留。宿主现在会拒绝缺失的数据集对象/字段、未声明字段列表及循环依赖，
让模型收到可自行修复的错误。

[工具结果](mail-events-2026-10-04/tool-results.json) 同时保留失败和成功。
[模型归属](mail-events-2026-10-04/provider-attribution.json) 来自各已完成回合的内核
ledger 元数据，不依赖邮件主题或模型自述。[生成参数](mail-events-2026-10-04/cards/)
保留模型的原始源码字符串和数据值，没有人工修改。

## 手机与原生 Makepad 审查

手机 Glance 显示了改进后的 DeepSeek 卡片及修正后的 MiniMax 物流卡片。
点击后者的 Mail 通知打开了对应完整卡片。最终 MiniMax 预约在 `2026100406` 上也通过了
“通知→完整卡片→Android 返回键→Glance”，测试应用始终保持前台。较早构建的返回键失败，
原因是固定版本 Makepad 按 APK 包名查找扩展类。给独立包增加转发扩展后问题修复，
没有替换 Home 或修改 Makepad 版本。卡片关闭按钮另有单元测试，未实测其触摸点击。

| 截图 | 证明范围 |
| --- | --- |
| [DeepSeek Glance](mail-events-2026-10-04/screenshots/deepseek-compact-glance.png) | 完整预约事实、尚未预约状态及下一步 |
| [MiniMax 通知](mail-events-2026-10-04/screenshots/minimax-shipping-notification.png) | Mail 归属及物流通知 |
| [MiniMax Glance](mail-events-2026-10-04/screenshots/minimax-shipping-glance.png) | 修正后的数据实际显示 |
| [MiniMax 完整卡片](mail-events-2026-10-04/screenshots/minimax-shipping-full-card.png) | 点击通知后打开对应物流卡片 |
| [最终预约](mail-events-2026-10-04/screenshots/minimax-appointment-full-card.png) | 最终构建的新邮件打开完整卡片 |
| [Android 返回键](mail-events-2026-10-04/screenshots/minimax-appointment-back.png) | 返回 Glance，测试应用仍在前台 |

原始卡片还通过固定版本 Makepad 的 `card-host`/`card-studio` 隐藏窗口及 instrument API
渲染。按手机卡片内部 344 点宽度，紧凑 DeepSeek 物流文字终点为 266 点，预约为 228 点，
修正后 MiniMax 物流为 308 点；最终 MiniMax 预约为 214 点，全部七个被引用字段可见。生产卡片按内容适应高度，最大 440 点；240 点是设计目标，
不是运行时上限，因此超出 240 不能直接证明手机裁切。MiniMax 第一组各有七处缺失绑定；
修正后的物流卡片显示了全部八个被引用字段。

原生工具的配色及字体与手机 shell 不同。自动 `fits` 可能遗漏部分文字被裁切的情况，
因此还比较了同宽高画布的 instrument 文字矩形，并查看实际手机截图。
[原生测量](mail-events-2026-10-04/native-review.json) 记录了这些差异。
这是少量迭代案例，不能据此宣称通用速度或质量排名。本轮 DeepSeek 更符合紧凑信息层次；
MiniMax 修正后的物流仍重复模拟免责声明，并用额外行分开标签和值。

## 构建与验证范围

测试 APK 升级保留了 Gmail 账户，没有替换生产 Home。DeepSeek 使用 `2026100402`；
MiniMax 第一组及通知路径使用 `2026100403`；其修正物流使用 `2026100404`。
`2026100405` 增加字段列表/依赖严格校验及更明确的已接纳技能说明。
最终 `2026100406` 还加载了独立包的 Android 扩展；新 MiniMax 预约邮件、完整卡片打开
及实体返回键测试均通过。之前 AgentMail 未验证账户每日上限的 HTTP 429，已由用户提供
验证码完成验证后解决，不属于 Gmail 或 Agent 故障。

[构建记录](mail-events-2026-10-04/build-receipt.json) 绑定最终 APK、基础提交、修改源码哈希及设备结果。

已执行：

- 手机共享 shell：**862 项通过**。收紧数据校验后，两项聚焦测试再次通过，覆盖扁平、
  缺失、空值、嵌套数据，省略/空字段列表及循环依赖。
- Mail 服务：**28 项通过，2 项忽略**。忽略项依赖可选真实 Gmail/平台钥匙串，不算通过。
- Broker：**62 项通过**；AI host：**39 项通过**。另有指令及账户隔离聚焦测试。
  脚本连接器测试与上面的真实模型回合分别计证。
- Desktop 默认及 `mobile-apps` 检查、phone `mobile-apps` 检查、两种 shell graph、
  源码及版本固定检查、原生应用目录检查、Android contracts 导出、APK 构建和个人路径检查均通过。

该流程依赖 OctoSense 进程存活，没有 Android 后台任务/服务或系统原生通知投递。
待处理事件及决策持久保存，但可见 Glance 卡片仅在内存中，进程重启后消失。
若发布后、保存记录前崩溃，通知仍可能重复。账户工具路径及限制见
[实现 walkthrough](../mail-agent-events.zh-CN.md)。
