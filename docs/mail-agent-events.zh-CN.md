# 邮件事件与应用 Agent

[English](mail-agent-events.md) | 简体中文

Mail 可以处理 Inbox 新邮件，不需要用户逐封发送聊天请求。用户先在宿主登录面板中连接账户，
允许 Mail Agent，再让系统 Agent 配置自动处理。系统 Agent 用 `agents.provision`
设置指令、具名技能文本、启用状态和轮询间隔；`agents.status` 返回配置与处理状态。
模型密钥和邮箱密码留在宿主中；Agent 读取的邮件文本会发送给所配置的模型。

流程是：宿主同步 Inbox → 原子保存邮件、同步游标和待处理事件 → Mail Agent 收到带邮件 ID
的 incoming 事件 → 用 `mail.peek` 读取正文 → 自行判断是否需要卡片 → 用
`mail.publish_card` 发布模型编写的 L0 卡片和通知 → 回合成功且宿主有发布记录后确认事件。
无需卡片时必须调用 `mail.skip_event`，原因是 `no_action`、`duplicate` 或 `outside_policy`；
宿主保存决策，回合成功后才确认。工具失败后的普通模型回答不能满足确认条件。每封邮件不需要系统 Agent 或测试人员另发提示词。

轮询只作用于当前已获用户许可、且配置已启用的账户。首次同步或服务器 UID 重置只建立基线，
不会将历史邮件转成一批通知。待处理事件持久保存；失败或缺少宿主决策记录的回合保留并退避重试。

按以下顺序阅读源码：

1. [`incoming.rs`](../apps/mail/host-service/src/incoming.rs) 串行处理 UI 和后台同步。
   邮件、游标和事件队列一次原子保存；队列满时不会推进游标丢失事件。
2. [`agent_events.rs`](../crates/shell/src/agent_events.rs) 在应用工作区之外保存绑定账户的
   配置，启动 incoming 回合，并在等待期间检查取消、授权撤销和账户变化。
   状态读取不会等待同步锁；队列繁忙时返回 `pending: null`、`queue_busy: true`。
   完成处理在同一次非阻塞服务锁中检查已保存的决策并确认事件；繁忙时重试，不冻结 UI。
3. [`script_apps.rs`](../crates/shell/src/host_tools/script_apps.rs) 加载已接纳的
   `AGENT.md`/技能文本，将 Mail 工具调用绑定到 broker 提供的账户。模型不能用参数选择
   另一个账户。`mail.folders`、`mail.list` 读缓存；`mail.sync` 分批刷新指定文件夹；
   `mail.peek` 读取纯文本且不标记已读。Peer 工作区不会挂载密码保险库或邮箱数据库。
4. [`guidance.rs`](../crates/app-peers/src/guidance.rs) 为每轮快照宿主指令与技能**文本**，
   已存在的 peer 下一轮也会用新配置。指令和不可信请求放在两个独立序列化文本块中。
   这没有增加内核 system 消息角色，也没有安装原生技能；工具授权和账户检查才是可强制执行的边界。
5. [`glance.rs`](../crates/shell/src/glance.rs) 校验 L0 源码及具名数据集对象，以 Mail 身份发布。
   每个数据集必须声明非空字段列表并提供各字段的值；循环依赖会被拒绝。
   这能发现缺失绑定，但不能证明事实正确或布局好看。
   模型用稳定事件 ID 作为 `card_id`；已记录的发布结果阻止重复发布。若在发布后、记录前崩溃，
   通知仍可能重复，但相同卡片 ID 会替换卡片而非新增另一张。Glance 卡片本身目前只保存在内存，
   这些记录不会在进程重启后恢复可见卡片。
6. [`mobile_app.rs`](../crates/shell/src/mobile_app.rs) 保存每条通知对应的准确卡片键。
   点击通知会在 Glance 上打开该完整卡片；返回键或关闭按钮回到 Glance。
   卡片已移除时安全回退。卡片中独立的打开应用操作仍进入 Mail。

需要用户处理的邮件，生成卡片内部也应有可用控件。Mail 已接纳的技能现在要求使用声明式
L0 `Chip` 按钮、具名事件和本地视图状态，例如“配送详情”或“预约详情”，以及“返回”。
按钮必须展示该邮件中的事实，名称必须准确说明实际效果。正文里的建议、shell 的打开
Mail 控件，或名为“已发送”的本地状态，都不等于实现了邮件操作。
[物流](../crates/shell/resources/glance/mail-shipping.card)和
[请求](../crates/shell/resources/glance/mail-request.card)示例展示交互语法；其中模拟的
发送、追踪和完成状态不能冒充真实远端操作。从生成卡片发送邮件、预约或修改邮箱仍未实现。
下述较早设备试验验证了发布和打开，没有验证卡片内按钮。后续
[配对按钮试验](testing/mail-card-actions-2026-10-04.zh-CN.md)保留较早布局失败，
并实际验证两个模型在完整卡片及 Glance 上的取件码→返回→详情→返回。

该共同场景是有取件码、无追踪网址的邮件：“显示取件码”和“取件详情”必须展示
邮件提供的码、地点、截止时间及携带带照片证件的要求，两种视图都能返回。这些本地控件
实现了[邮件操作卡片计划](../apps/mail/docs/2026-10-01-email-action-card-plan.md)中
当前可支持的部分。计划中的外部 Track 操作仍需另做宿主集成：L0 虽然收录了 `sys.link`，
本 shell 并不执行其写操作。没有网址时不能编造链接或显示虚假的追踪成功页面。

当前 shell 有渲染限制：已通过校验的 `Chip(width: .fill)` 可能在 Fit 包装内收缩到不可见。
因此已接纳技能要求省略 width，使用自然宽度、纵向排列及短标签。这是渲染器限制，
不是非法 L0 token。[按钮后续试验](testing/mail-card-actions-2026-10-04.zh-CN.md)
分别记录布局失败、实际手机点击及尚未实现的远端操作范围。

Mail 窗口可以关闭，但 OctoSense 进程必须存活。当前没有 Android JobScheduler/WorkManager、
前台服务或 Android NotificationManager 集成。通知显示在 OctoSense 自己的通知栏，
卡片显示在 Glance。不能承诺 Android 挂起或终止进程后仍会送达。

这里只实现 Mail 的 `mail.messages.new`。通用 cron、任意应用事件路由、每应用模型选择、
内核原生技能发现，以及 ADR 0002 剩余预算和 Settings 界面仍是后续工作。系统 Agent 的配置
补充应用已接纳的指令；邮件内容不能配置 Agent 或扩大工具权限。关闭配置或撤销 Agent
访问权会停止新回合并取消调度器的活动上下文。

验证：指定 OnePlus 6 收到 AgentMail 经 Gmail 投递的邮件后，确实自动触发了 DeepSeek
和 MiniMax 的 Mail 回合，没有逐封人工提示。两个模型均明确跳过普通简报。
DeepSeek 改进后的物流、预约卡片可读；MiniMax 第一组虽然发布成功，但数据绑定缺失，
这促成了生成数据校验和模型反馈。其物流重试已经完整显示事实，并可由通知打开；
发件账户验证后，新的 MiniMax 预约卡片首次发布成功，通知、完整卡片及 Android 返回键实测均通过。构建范围、原始生成源码和限制见
[测试记录](testing/mail-events-2026-10-04.zh-CN.md)。试验策略仅处理主题以
`[OctoSense simulation]` 开头的邮件；其他新邮件不读正文，直接跳过。
