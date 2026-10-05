# octosense-l0-chat：L0 卡片内聊天的宿主实现

[English](README.md) | 简体中文

卡片用 `sys.chat` 声明与发布应用的 Agent 对话。这个 crate 负责发布者检查、消息来源、限额和聊天记录；共享 Shell 与 AppCard 都使用它。应用 Agent 仍是原来的应用/账户 peer，卡片不会启动新内核或新 Agent。

```text
source convo sys.chat(app: "os.mail", thread: "ana-contract", fields: [entries, id, role, text])
event send { convo: append($value), draft: clear }
for m in convo.entries key m.id { ChatEntry(text: m.text, role: m.role) }
```

这只是声明片段；完整卡片还需要输入框、状态和根视图。

| 边界 | 实现 |
| --- | --- |
| 应用所有权 | `check_publisher` 在发布时检查；`seed`、`perform` 再次检查。卡片不能访问另一应用的聊天。 |
| 聊天记录 | 宿主覆盖生成数据中同名的聊天来源。卡片不能伪造宿主或模型的消息。 |
| 消息角色 | 只有 `ValueOrigin::UserInput` 可以追加用户消息；Agent 回答记录为 `model`，宿主错误记录为 `host`。消息中写入角色字段不会改变其身份。 |
| 限额 | 用户消息清理后最多 4 KiB，每线程两秒一次；正在回答时拒绝新消息。保留最近 200 条记录，回答最多约 16 KiB；应用和线程 ID 最长 64 字节。 |
| 存储 | `ChatStore::with_folder` 在宿主指定的应用账户目录存储线程 JSON；测试可使用内存存储。晚到的回答仍写入原来的文件。AppCard 使用其 device 账户目录。 |
| UI 更新 | 每次变化增加 `generation` 并触发宿主回调，卡片重新读取聊天记录。 |

Shell 的 `AgentResponder` 在得到应用助手许可后，通过应用 peer 的会话发送 `TurnTrigger::AppSaysPerson`。没有 Agent 时显示宿主提示。Mail 演示模式使用固定回答，不调用模型；AppCard 当前使用 `NoAgent`。

显式未绑定的 `sys.chat` 卡片保留原来的单条问题适配器。声明了 agent 却没有聊天源的发布（包括 Splash 卡片），由 shell 提供宿主拥有的会话。`ContextKind::Card` 绑定原发布账户和卡片线程，包含有大小限制的发布数据及当前 L0 状态，并明确区分本地选择和外部操作；它不签发 Mail 编辑凭证，也不新增工具。应用 agent 必须先经过现有许可面板。生成源码保持不变。

`ContextKind::Mail` 单独处理邮件。绑定 Mail 卡片使用宿主创建的 `ContextBinding`，包含原账户、邮件上下文、带版本号的持久草稿快照及线程 ID。`seed_bound`、`perform_bound` 使用明确指定的账户目录，拒绝线程不匹配。账户切换或助手权限撤销后，Shell 拒绝操作，不会将旧卡片转到新账户。晚到的回答仍属于原聊天记录。

`Request::agent_text` 将绑定信息（最多 32 KiB）、最近最多 20 条且合计不超过 16 KiB 的历史消息和当前问题序列化为数据。邮件、草稿和历史内容明确标为不可信数据，不授予工具权限或发送许可。会话仍在同一个应用 peer 上；这个接口本身不发布组合卡片，也不授权发送邮件。

L0 的 `model-copy` 仍是带 AI 标记的纯文本，不能成为动作目标或授权。具体限制由 Octoscript 检查器执行，本 crate 只增加聊天的宿主处理。

在仓库根目录运行：

```sh
cargo test --locked -p octosense-l0-chat
cargo clippy --locked -p octosense-l0-chat --all-targets --no-deps -- -D warnings
```

本次改动已运行这两项检查；11 个单元测试通过。`.github/workflows/apps.yml` 的 `services` 任务也运行这些检查。手机上的组合卡片聊天与真实发送验收另行记录，不能由这些单元测试推断。
