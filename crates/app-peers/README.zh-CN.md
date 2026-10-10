# octosense-app-peers：宿主持有的应用 peer

[English](README.md) | 简体中文

这个 crate 把应用 Agent 接到 Shell 的同一个 octos 内核。每个应用和账号有一个 peer，由系统 Agent 持有；应用获得受限接口，不获得内核令牌、供应商密钥或原始内核协议。参见[架构文档](../../docs/architecture.md)、[ADR 0004](../../docs/adr/0004-native-apps-hosting-and-peers.md)和 [Rinx ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md)。

## Agent、上下文和任务的区别

peer 表示应用和账号的身份及其持久记忆，不等于一个固定的 Rust 线程。Broker 管理其连接、上下文、请求和事件路由；运行中的异步任务执行具体请求。

同一个应用 Agent 有两条通道：

- 系统 Agent 通过 `peer/input` 使用 peer 的 session。
- 人类通过 `open_conversation` 打开独立 request context，使用 `share_history`。它与系统通道并行，每条消息保留通道和发言者。

此外，`open_context` 为某个客户端实例创建独立上下文，例如 Rinx 的 mini app。每个上下文都有租约，调用前和回复前都验证账号与上下文是否仍有效；过期回复被丢弃。

## 声明不再划分助手方法权限

原生模块通过公开的 `octos.*` 名称表示启用助手，宿主策略和用户同意决定是否提供服务。启用后提供全部四个公开方法：`octos.session.open`、`octos.session.history`、`octos.turn.start`、`octos.turn.interrupt`。声明一个方法不会把助手限制为这个方法。

脚本应用也可以通过 `agent` 对象或已准入的工具启用 Agent，而不声明 `octos.*`。Shell 验证其 bundle 和精确的宿主目录。没有启用 Agent 或没有宿主同意的应用不会创建 peer。

Broker 仍将宿主创建的上下文所请求的方法与公开接口相交。这是上下文租约边界，不是 manifest 权限。manifest 无法增加内核 RPC，公开助手接口也不会自动授予另一个应用的工具。

## crate 组成

| feature | 内容 |
| --- | --- |
| 默认 | `contract` 接口及 `injection` 的 offer/claim/withdraw；应用可以只依赖这一层 |
| `broker` | peer 绑定、上下文、租约校验、事件路由、取消和过期回复过滤 |
| `octos-core` | Shell 的 `CoreConnector`、宿主策略及 `hosted::launch` |
| `ws` | 明确指定远程 octos 服务的 `WsConnector` |

Shell 在模块创建期间 `offer` 接口，应用用自己的实例 scope `claim`；创建结束撤回公开入口，已持有的服务随实例存活。实例关闭调用 `release`。没有服务时应用不能自行启动另一个内核来绕开宿主策略。

Shell 的系统 session 是 `_main:api:octosense#system`。内核创建 peer 时发放的宿主令牌写入 `<core_dir>/../app-peers/<namespace>.peer`，目录模式 0700、文件模式 0600，位于应用范围外。记录同时保存工作目录，恢复时使用相同目录。

peer 的记忆命名空间为 `app/<id>/acct-<tag>`。账号标签是规范化账号的 FNV-1a 哈希，不是应用存储目录使用的 SHA-256 哈希。脚本 peer 使用 `card.<app id>`，不与原生模块共享身份或记忆。

## 生命周期和审批

- `confirm: host` 的工具调用交给 Shell 的审批路由；`confirm: app` 交给工具所属应用的确认界面。系统 Agent 不能代替人类批准。
- `release()` 关闭实例上下文、停止运行中的调用；最后一个实例关闭时撤销 peer 工具路由，系统 Agent 不能继续向已关闭应用发送工作。记忆保留用于下次启动。
- 删除账号或卸载应用时，宿主调用 purge，使用记录的 peer 身份和令牌删除对应记忆。忙碌时重试；未成功的记录保留。仅退出登录不会 purge。
- 有账号工作目录的应用，可以让人类对话通过 `read_parent` 只读访问该账号目录。普通客户端上下文仍限制在自己的目录中。
- 审批或问题默认十分钟到期，结果是拒绝或放弃，不是自动同意；宽限期后停止仍运行的调用。
- 人类对话的 Stop 可以中断 peer 工作；Shell 的某些界面使用单通道停止。完成的调用撤回尚未处理的工具审批和问题。

## 宿主提供的指导

Shell 通过 `ai-host::contained::set_guidance` 设置某应用和账号的指令及技能文本。Broker 在 request context、人类对话、系统 `peer/input` 每次开始时取快照；更新作用于下一次调用，不重建 peer 或删除历史。上限为 16 KiB 和 16 个技能，切换账号或撤销服务时清除。

这不是内核原生技能安装，也不会创建独立的 system-message 角色。原始请求数据、触发来源和工具授权保持不变，外部邮件内容不能替换宿主提供的结构化指导字段。

## 验证

在仓库根目录运行：

```sh
cargo test --locked -p octosense-app-peers --features octos-core,ws
```

这些单元和脚本连接器测试覆盖上下文与账号隔离、授权、取消、历史通道、指导更新，以及启用后的完整公开方法集合。它们不代表真实供应商或手机验收。

真实内核可选测试需要同一版本的内核和本地脚本模型：

```sh
OCTOS_APP_PEERS_TEST_KERNEL=/path/to/octos cargo test --locked -p octosense-app-peers --features octos-core --test real_kernel -- --nocapture
```

未提供内核时，可选测试跳过真实内核验证。Shell 不接受缺少 host-owned peer 协议的内核，也不会偷偷改用普通共享记忆 session。
