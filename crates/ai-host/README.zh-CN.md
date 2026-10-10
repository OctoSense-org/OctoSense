# octosense-ai-host：Shell 的 AI 服务

[English](README.md) | 简体中文

这个 crate 是桌面和 Home 共用的 AI 接入层。Shell 负责内核、供应商配置、应用身份和授权；应用通过受限服务使用 AI，不直接连接内核。完整关系见[架构文档](../../docs/architecture.md)和 [ADR 0004](../../docs/adr/0004-native-apps-hosting-and-peers.md)。

## 入口和职责

- `start(Host::platform(data_dir))` 配置内核和服务；首个消费者连接时才启动内核。一个进程共用一个内核，`shutdown()` 结束它。
- `handle_event`、`handle_drop`、`qr_image_result` 处理供应商设置界面的扫码、选图和拖放。
- `llm` 是 AI providers 系统应用的配置服务；密钥只在宿主界面中输入，普通应用得不到密钥。
- `model.complete` 是一次性的结构化模型调用。宿主选择供应商并执行预算、输出格式及大小限制；它不会建立持久的应用 Agent。
- `contained` 提供脚本应用的 `octos` 服务；`offer` 在原生模块创建期间注入受限的助手接口。模块关闭时释放该实例的上下文，不停止其他应用的内核。
- `module_peer` 连接原生模块的 Makepad `OctosPeer` 与 Shell 的 peer link，使模块和独立进程使用同一协议。

`Host` 配置数据目录、内核来源、扫码方式和 `Policy`。Android 使用 APK 内的内核，OpenHarmony 使用进程内内核，桌面使用明确指定或随包发布且通过版本校验的内核；没有可用内核时不会创建助手。`octos-core`、`llm`、`toolbox-peers` 分别控制相应组件；具体平台依赖见 [Cargo.toml](Cargo.toml)。

## 声明、身份和用户同意

`capabilities` 描述应用计划使用的 API，不是宿主服务的权限开关。省略 `model` 或某个 `octos.*` 名称，本身不会拒绝调用。但以下边界仍然有效：

| 边界 | 实现 |
| --- | --- |
| 已准入的应用及其宿主目录 | Shell 检查当前 bundle、应用 ID 和精确的 `<apps root>/.host`；缺少检查回调时拒绝 |
| Agent 是否存在 | `agent` 对象、已准入的工具或受支持的助手声明表示主动启用；普通应用不会因为 API 可用就自动获得 Agent |
| 用户同意 | 首次使用提示，Settings 中可关闭；关闭后立即释放现有 peer |
| 当前账号 | 无账号功能的应用使用设备范围；有账号功能的应用未登录时返回 `SIGN_IN`，不能回退到设备范围 |
| 方法和输入范围 | 只接受公开方法和允许的参数；应用不能指定内核 session、profile、其他账号或任意 RPC |
| 跨应用工具 | 工具共享授权、审批和资源范围独立检查，公开助手接口不会自动授予这些工具 |

原生应用的 `native-apps.json` 中 `agent.octos` 用来识别助手接入意图。应用启用且用户同意后，接口包含全部四个公开方法，不按声明列表拆分权限。

## 脚本应用的 Agent

每个应用和账号有一个宿主持有的 peer，脚本应用的 ID 前缀为 `card.<app id>`。其所有者是系统 Agent。Shell 在用户允许后调用 `prepare` 准备 peer，因此后台事件和系统 Agent 不必等待应用第一次调用 `octos`。

公开方法为：

- `octos.session.open`：打开应用对话。
- `octos.session.history`：读取人类和系统两条对话通道的合并历史。
- `octos.turn.start`：提交文本及触发来源；文本最多 32 KiB。
- `octos.turn.interrupt`：停止当前工作。

回复最多 2 MiB。人类对话使用 peer 的独立 request context，与系统 Agent 的 `peer/input` 通道并行；它们共享可读的最近历史，保留发言者和触发来源。审批由 Shell 处理，脚本无法自行批准。

`set_declared` 为兼容保留原名：返回 `None` 表示没有启用 Agent，返回空集合仍可表示有 Agent。它不再按集合裁剪方法。`set_caller_admitted` 验证应用和宿主目录，`set_account_of` 提供当前账号。

`Policy::shipped()` 默认使用 `ContainedGate::Consent`。开发覆盖 `OCTOSENSE_CONTAINED_APPS=1` 仅跳过首次同意，不跳过应用准入或 Agent 启用；`0` 关闭脚本助手服务。关闭时服务仍注册，以返回明确错误。

## 系统工具箱

启用 `toolbox-peers` 时，工具箱通过 Shell 的工具中继服务应用 Agent。系统应用和已准入的商店应用遵循相同规则，没有额外的 `os.*` 限制。

| 显式选择且允许的工具能力 | 提供的工具 |
| --- | --- |
| `research` | `workflow.run`、`workflow.fork`、`toolbox.search`、`toolbox.web_read` |
| `crawl`，且范围中的 `max_depth`、`max_pages` 大于 0 | `toolbox.deep_crawl` |

这里的选择描述实际共享工具和资源范围，不能因宿主 API 声明改为描述性就自动开放。调用方必须提供经过准入和摘要校验的 manifest。中继仍检查 Agent 同意、工具授权和需要的审批；执行器再次核对工具授权。取消的调用不返回结果。

模板模型调用与 `model.complete` 共用供应商和预算。结果写入宿主持有的 `<apps root>/.host/toolbox/<app id>`，供 Glance 的 `sys.digest` 读取。手机默认启用工具箱；桌面可通过同名 feature 启用。手机的隐藏系统 WebView 可辅助读取需要渲染的网页。

## 宿主提供的 Agent 指导

`contained::set_guidance(app_id, account, TrustedGuidance)` 在 peer 准备前或两次调用之间设置指令和技能文本。最多 16 KiB、16 个技能；下一次调用使用新快照，不删除历史。切换账号或撤销 Agent 会清除内存指导。

这些内容是宿主提供的文本，不是内核技能安装机制，也不是独立的模型 system-message 角色。应用请求和指导分别序列化；工具授权和原始触发来源仍是安全边界。

## 验证

从仓库根目录准备依赖后运行：

```sh
cargo test --locked -p octosense-ai-host --features octos-core,llm
cargo test --locked -p octosense-ai-host --features toolbox-peers --test toolbox_peers
```

声明相关测试覆盖未声明或只声明部分方法、未启用 Agent、错误宿主目录、未同意、账号切换和未登录。工具箱测试使用脚本内核与假供应商；这些结果不代表真实模型或手机验收。真实内核测试需要显式设置 `OCTOS_APP_PEERS_TEST_KERNEL`，否则可选测试不会验证真实内核。
