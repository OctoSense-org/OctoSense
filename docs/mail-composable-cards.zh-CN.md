# 组合 Mail 卡片：草稿、聊天与宿主审批

[English](mail-composable-cards.md) | 简体中文

ADR0007 的草稿、聊天与审核路径已在源码中实现。**双模型 OnePlus 6 完整流程，以及用户最终批准发送，仍未验证。** 本轮只有 Android 实体触摸屏输入可以取得发送批准；桌面与无障碍审批仍是暂缓实现的要求。

生成的卡片描述展示。Rust 宿主代码拥有账户、原邮件、已保存草稿、修订号和发送操作。编辑字段会更新持久化草稿；生成的 Review reply 按钮只打开宿主审核。模型和卡片本地的 `sent` 状态都不能授权 SMTP。

## 沿代码追踪一封回复

1. 传入事件分发器在已登录账户下启动 Mail 应用 agent。agent 读取邮件，然后调用 `mail.propose_reply`，提供 `message`、可选 `folder` 和建议的 `body`。[Mail 服务](../apps/mail/host-service/src/drafts.rs)推导收件人与回复邮件头，返回 `draft_id`、`revision` 和 `chat_thread`。`mail.draft` 读取已保存修订版；`mail.suggest_reply` 创建绑定修订版的建议，不覆盖编辑；`mail.propose_send` 准备不可变提议，不执行发送。
2. `mail.publish_card` 接收该 `draft_id` 及模型编写的 L0 源码／数据。[发布路径](../crates/shell/src/glance.rs)在生成数据之外附加可信 Mail 元数据，拒绝把已绑定卡片改指向另一个账户、邮件或草稿。普通 `glance.publish` 不能提供此绑定。
3. [卡片适配器](../crates/shell/src/mail_card.rs)从宿主存储回答 `sys.mail_draft(app, id, fields)`。可写源只声明 `to`、`subject`、`body` 中的一个；直接绑定的 `Field(text: draft.body, on_change: save)` 配合 `draft: set($value)` 携带 Field UserInput。适配器保存前校验显示中的修订号。冲突或失败的编辑仍作为可见的未保存输入保留，不会静默接受。
4. `sys.chat(app, thread, fields)` 使用宿主返回的线程。[聊天适配器](../crates/shell/src/glance_chat.rs)把绑定的邮件、持久化草稿修订版和有限线程历史交给 Mail 现有 peer。未保存文字不包含在内。过期 agent 建议不能覆盖后来的编辑。这是同一账户 peer 内的对话上下文，不是另一个内核或应用 agent。
5. `sys.mail_review(app, id, fields)` 的 `set` 请求审核，`clear` 取消审核。[展开卡片](../crates/shell/src/glance_sheet.rs)内的[宿主审核 UI](../crates/shell/src/mail_review.rs)显示准确账户、收件人、主题和正文。只有其可信 Approve & Send 手势，才把不透明的审核能力交给服务执行器。agent 没有 `mail.send` 或 `mail.approve`。完整应用的旧发送入口也会请求宿主审核；开发者模式不能绕过该检查。
6. 服务取得不可变操作的执行权，并在 SMTP 前记录身份。重复请求共享已有结果。`accepted` 表示 SMTP 接受，不等于收件人收到。不确定结果绝不自动重发；明确重试需要重新批准新的尝试，适用时还要确认重复发送风险。卡片／聊天发布失败，不能把本地状态变化变成投递回执。

完整决策与强制验收条件见 [ADR0007](adr/0007-composable-mail-action-cards.zh-CN.md)。初期回复仅支持一个经审核的 To 地址；Cc／Bcc、全部回复、附件、富文本和转发仍不支持。

草稿和尝试回执会持久化；审核能力刻意不持久化，重新打开后必须重新审核、批准。完整写信界面在活跃会话内保留 compose ID，但目前没有已保存草稿列表，无法在重启后任意选择草稿重开。宿主私有发布缓存只为当前已登录账户、且仍有权威已保存草稿的卡片恢复模型 Glance 源码／数据。它保留原始到期时间（digest 到期可能进一步缩短它）以及持久关闭／撤销状态，不再发送通知。5 个缓存测试和 27 个 Glance 测试已通过；实际进程重启与手机恢复仍未验证。

## 运行时契约与输入来源

[工具 schema](../apps/mail/bundle/tools.json)定义四个草稿／提议工具。目录契约固定在 Octoscript `9ca9545b6cba489ab72ec988dfd649fb7c13ce17`，Octoscript-Makepad 固定在 `a950f7fb7c5560eb0583d643ea1cf11d4558e6d8`。两个 Mail 源都要求应用身份为字面量；本宿主还要求草稿 ID 字面量与可信发布绑定一致。检查器将草稿 `to`、`subject`、`body` 和 `suggestion_body` 保守地视为模型文本：可以显示／编辑，不能直接复用为操作载荷或源选择参数。宿主批准授权准确的已保存邮件，不要求用户把未经修改的 AI 草稿重新输入一遍。

[Makepad 补丁](../tools/runtime-patches/makepad-trusted-user-input.patch)默认令 `trusted_user_input()` 为 false。Android JNI 检查正数且非虚拟的设备 ID、触摸屏来源、未遮挡标志及非取消输入；空对象或 Java 异常不会入队可信输入。作用域 guard 只在原生 TouchUpdate 处理器执行期间生效。远程／嵌套模拟分发、延后的 action 和脚本任务不能继承它。宿主审核要求相匹配的可信按下与释放。`with_untrusted_input` 只能移除信任。目前键盘／输入法、长按、桌面和无障碍输入都不能批准发送。受攻击的 OS／root 进程冒充硬件，不在这一应用级边界内；这不是硬件认证。

## 验证条件

| 条件 | 当前证据／状态 |
| --- | --- |
| L0 目录与来源规则 | Octoscript 315 项测试通过，包括五项 Mail 源测试。 |
| 可移植 UI 转换 | Octoscript-Makepad 86 项可移植测试通过，包含文档测试。 |
| 输入元数据与作用域 guard | 三项专项测试通过；它们不模拟 Android JNI，也不能证明设备兼容性。 |
| 集成服务、shell 与 Android 构建 | 验证中；结果需绑定最终源码修订。 |
| 双模型卡片：编辑、聊天、冲突、取消、重开／重启 | 完整功能尚未验证。早期取件码卡片只测试了本地视图。 |
| 实体输入批准、真实线程回复及 Glance／展开回执一致 | 未验证；自动输入不能代替用户批准。 |
| 桌面／无障碍审批 | 暂缓；当前来源边界拒绝此类输入。 |

这里不宣称新的启动或发信配方已验证。假传输层、原生 instrument 和真机结果必须分开记录。语法通过或评分达到 4.5／5，不能替代任何未通过的授权、持久化或投递条件。
