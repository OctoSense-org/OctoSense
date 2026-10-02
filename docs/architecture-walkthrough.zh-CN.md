# 代码导读：从应用窗口到 Agent 回合

[English](architecture-walkthrough.md) | 简体中文

本文面向了解 Rust 结构体、trait 和函数，但不熟悉 Agent 与 OctoSense 的开发者。阅读依据是 2026-10-01 检查的 OctoSense `c19da8d`，以及 Cargo 锁定的 octos `ae230ce0`。[架构参考](architecture.zh-CN.md)说明整体设计；本文沿着调用链、数据所有权与执行边界阅读实现。ADR 中的决定不代表所有步骤都已运行。

## 1. 先区分名字

| 名称 | 在这里的含义 |
| --- | --- |
| OctoSense | 桌面/Home Shell、应用和宿主服务；ROM 将 Home 与 Android 平台组件一起打包。 |
| Makepad | Rust UI、事件循环与渲染框架。 |
| Splash / Makepad Script | 受限 `main.splash` 应用使用的脚本 VM 和 UI 语言，不是独立的 Octoscript L0 解析器。 |
| Octoscript / Octoscript-Makepad | L0 卡片语言、检查/降级到运行时的工具和 Makepad 集成；`.card` 与 `main.splash` 的加载路径不同。 |
| octos | 负责模型调用、回合、工具、记录、记忆及 peer 协作的 Agent 内核，不是操作系统内核。 |
| Agent | 读取消息、选择已提供的工具、根据结果继续推理并回答的模型工作单元。模型输出本身不会执行应用操作。 |
| session / turn | session 标识会话及其状态；turn 是一次输入引起的执行，可包含多次模型和工具调用。 |
| peer / request context | peer 是持久协作身份；context 是该 peer 下的另一个会话，有独立记录、工作目录和子记忆命名空间。 |
| tool / host service | tool 是提供给模型的操作；host service 是处理应用或工具执行器请求的 Rust 实现。两者不会自动互相暴露。 |
| `AGENTS.md` / `AGENT.md` | 仓库贡献者规则 / 应用包的 Agent 指令文件。App Hub 可以校验后者，但 Shell 尚未将它装入 peer 提示词。 |

相关仓库是 [Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)、[App Hub](https://github.com/OctoSense-org/OctoSense-App-Hub)、[Octoscript-Makepad](https://github.com/OctoSense-org/Octoscript-Makepad) 和 [octos](https://github.com/octos-org/octos)。分别读取各自的锁定版本，不要默认相邻 checkout 的 `main` 就是 Shell 使用的版本。

## 2. 从可执行入口找到应用宿主

先读 [desktop/src/main.rs](../desktop/src/main.rs)、[phone/src/main.rs](../phone/src/main.rs)，再读 [shell/src/lib.rs](../crates/shell/src/lib.rs)。Desktop 和 Home 是链接同一 Shell 的两个包；Home 还提供 Settings 与平台集成。ROM 打包 Home 和 Android 特权组件，并未另写一套 Rust Agent 架构。

应用启动选择来自 [native-apps.json](../native-apps.json)、[apps.rs](../crates/shell/src/apps.rs) 中的 `AppRegistry`，以及各产品的 `system-apps.json`：

| 应用形态 | 源码入口 | 实际执行方式 |
| --- | --- | --- |
| Rust 模块，如 Reference、Rinx | [module_host.rs](../crates/shell/src/module_host.rs) | 与 Shell 同进程；每个实例有脚本 isolate，但 Rust 内存仍共享。 |
| 原生进程应用，目前为受支持桌面上的 Terminal | [clients.rs](../crates/shell/src/clients.rs)、[hub.rs](../crates/shell/src/hub.rs)、[process-apps](../crates/process-apps/src/lib.rs) | 子进程通过经过认证的 Makepad hub 交换帧、输入和 AI-bus 消息。此连接不同于 octos 协议连接。 |
| 受限脚本应用 | App Hub 的 `CARD_MODULE`，从 `apps.rs` 接入 | 校验应用包后创建嵌套的受限 Splash VM；每个包不需要编译一个 Rust 可执行文件。 |
| L0 glance 卡片 | [glance.rs](../crates/shell/src/glance.rs)、[glance_chat.rs](../crates/shell/src/glance_chat.rs) | 运行时检查、转换并渲染卡片，宿主提供数据和聊天；卡片本身不是新 peer。 |

具体命令见[桌面](../desktop/README.zh-CN.md)、[Home](../phone/README.zh-CN.md)、[ROM](../rom/README.zh-CN.md)及[系统应用](../apps/README.zh-CN.md)。下面从仓库根目录运行的启动/构建命令已核对源码，但**本次未验证实际启动、GUI 或设备构建**：

```sh
python3 tools/setup.py --hub /path/to/existing-clones
python3 tools/setup.py --check --cargo
cargo run --release -p octosense
MAKEPAD_WM_TEST_APP=reference cargo run --release -p octosense --features app-reference -- --module reference
OCTOS_APP_CORE_BIN=/path/to/pinned/octos \
  cargo run --release -p octosense --features octos-core
```

内核二进制应来自 `Cargo.toml` 锁定的 octos 版本，`OCTOS_APP_CORE_BIN` 必须指向文件；这里不会查找 `PATH`。真实对话还要求用户在宿主拥有的 AI providers 面板配置提供方和模型。编译 Rust 不会自动配置凭据。自动化 UI 检查使用产品 README 的隐藏窗口方式。

脚本应用的 Design Flow `tools/octo` 是 Python CLI，包装 App Hub 的 `card-host` 和 `hub`，与 octos Agent 内核无关。`card-host` 可检查受限 UI/策略，但没有安装 Shell 的 Mail、Calendar、提供方或 peer 服务。这些集成必须在 Shell 中验证；`hub check` 通过不代表 Agent 回合成功。

## 3. 谁拥有内核

阅读 [ai-host/src/lib.rs](../crates/ai-host/src/lib.rs)，然后读 [kernel/src/lib.rs](../crates/kernel/src/lib.rs)、[launch.rs](../crates/kernel/src/launch.rs)、[kernel.rs](../crates/kernel/src/kernel.rs) 和 [router.rs](../crates/kernel/src/router.rs)。

1. Shell 调用 `ai_host::start` 注册服务、配置内核来源。`model.complete` 是单次模型请求，不会创建 peer 或工具循环。
2. 首个获准使用内核的消费者调用 `Core::connect`，得到逻辑 `Connection`；后续消费者共享同一代内核。
3. `launch::resolve` 选择桌面子进程、Android 包装为 `liboctos.so` 的可执行文件，或 OpenHarmony 内嵌服务；iOS 此处没有内核。
4. `kernel::supervise` 拥有运行中的进程/任务和消息泵；`Router` 在唯一物理连接上关联消费者请求 ID 与会话事件。
5. OUP 使用 JSON-RPC 请求和异步通知。默认通过 stdio 传输逐行 JSON；开启 Talk to Octos 后改用宿主管理的本地 WebSocket，不会为每个客户端另起一个内核。
6. 提供方变更会重启内核代次，消费者必须重连、重新绑定。丢弃一个 Rust `Connection` 不等于删除 peer 记忆。

ROM 的 Android 特权“agent”执行经过允许的平台操作，不运行系统 LLM 会话。这两个“agent”必须分清。

## 4. 跟随系统聊天的一条消息

读 [system_chat/mod.rs](../crates/shell/src/system_chat/mod.rs)、[session.rs](../crates/shell/src/system_chat/session.rs)、[link.rs](../crates/shell/src/system_chat/link.rs) 和 [system_tools.rs](../crates/kernel/src/system_tools.rs)。

助手面板发送 `Command` 给工作线程；`Driver` 打开 `_main:api:octosense#system`、读取历史并启动回合。事件更新聊天模型，`SignalToUI` 唤醒 Makepad 绘制快照。绘制线程不会同步等待模型完成。

系统 Agent 是 `_main` profile 下的会话。宿主通过 `session/tool_list/set` 缩小其内核工具集；`SYSTEM_AGENT_TOOLS` 包括 `peer_list`、`peer_send_input`、`peer_gather`、`peer_respond`。用户开启命令执行后，正常路径是 Shell 的 `terminal.run` 及其审批界面，而不是 octos 内置 `shell`。

[agents.rs](../crates/shell/src/agents.rs) 的 `agents.list` 发现应用 Agent 及其状态，`agents.ask` 请求首次同意。后者不是通用的“应用调用系统 Agent”聊天 API，模型也不能替用户同意。

## 5. 一个应用 peer，两条并行通道

先读 [contract.rs](../crates/app-peers/src/contract.rs)，再读 [broker.rs](../crates/app-peers/src/broker.rs)：`OctosAppService` 是应用拿到的受限助手句柄；`OctosContext` 是对话/请求句柄；`ContextSpec` 携带宿主认证的账号、实例和服务授权；`Broker(Arc<Inner>)` 实现连接、peer、context 与在途请求管理。

原生模块经 [hosted.rs](../crates/app-peers/src/hosted.rs) 和 [injection.rs](../crates/app-peers/src/injection.rs) 获得服务。脚本应用经 [contained.rs](../crates/ai-host/src/contained.rs) 接入同一 broker。manifest 的助手服务、`agent` 块或 `tools.json` 使应用具备 Agent 资格，但仍需内核、同意及有效账号。

`card.os.news` 是 broker 使用的应用身份，不一定是内核生成的 peer slug。`peer/prepare` 返回 slug、会话和宿主凭据，应使用返回值。peer 按应用与账号区分；无账号应用使用 `device`，Mail 使用宿主报告的已登录账号。`ensure_peer` 恢复记录、核验命名空间并注册工具，成功后才执行回合。

系统 Agent 调用 `peer_send_input` → octos 发出 `peer/input` → Shell broker 校验和排队 → broker 以 input ID 在 peer 会话调用 `turn/start`。因此是 Shell 驱动应用回合，并非内核收到消息就绕过 Shell 执行。

人类通过 `open_conversation` 打开带 `share_history` 的 request context，形成另一条通道。两条通道的 transcript 和回合状态独立；模型只读地看到另一条通道的有限近期文本。多个聊天界面可以打开同一 peer 的不同 conversation context。普通 `open_context` 用于 Rinx mini app 等客户端工作，不共享历史。context 有自己的内核会话，但不是另一个应用 peer。

`driver_of` 与 `take_over` 保证一个 peer 的系统输入队列只有一个 broker 驱动，避免每个应用窗口重复执行同一 `peer/input`。人类 context 的回合可以同时运行。

## 6. 人在哪里说话，回答回到哪里

| 入口 | 实现 |
| --- | --- |
| 系统助手面板 / F8 | `system_chat` 使用系统会话；系统委派任务引发的应用问题也显示在此。 |
| “Ask <app>” / Shift+F8 | [app_chat](../crates/shell/src/app_chat/mod.rs) 调用 `agents::conversation`，订阅两条通道并合并历史；Send 在人类 context 启动回合。 |
| 原生应用自己的聊天 | 注入的 `OctosAppService::open_conversation`，应用渲染事件。 |
| 脚本应用自己的聊天 | 通过 `host.request` 调用精确授权的四个 `octos.*` 服务。News/Mail/Calendar 自身未声明这些调用，但 Shell 仍可驱动其 Agent。 |
| 卡片中的聊天 | `sys.chat` → [l0-chat](../crates/l0-chat/src/lib.rs) 与 [glance_chat.rs](../crates/shell/src/glance_chat.rs)，校验发布者后绑定拥有该卡片的应用。 |

目前手机触摸导航还没有打开 Ask 面板的对应入口；应用自己的聊天或已发布卡片的聊天是不同入口。Ask 面板的 Stop 只停人类通道；另一个明确的按钮停系统任务。底层 conversation 的 `ContextOp::Interrupt` 范围更大，可中断两条通道，不能把所有 Stop 混为一谈。隐藏面板保留 context/订阅；切换应用或撤销权限才关闭。

Shell 将自己的输入动作标为 `TurnTrigger::Person`；脚本 `octos.turn.start` 可携带 `trigger`/`from`，但其 `trigger: "person"` 只成为 `AppSaysPerson`。聊天中的人物标签不证明宿主看到了真实用户操作，也不会解锁仅限用户主动发起的审批规则。

工具结果先回到发起它的应用回合，供模型继续处理。人类 context 的最终事件通过其 event sink 回到聊天界面；系统委派回合的结果进入 peer blackboard，系统 Agent 用 `peer_gather` 获取并向用户总结。

## 7. 将工具声明追到真正的 Rust 实现

读 [script_apps.rs](../crates/shell/src/host_tools/script_apps.rs)、[relay.rs](../crates/shell/src/host_tools/relay.rs) 和 [host_tools.rs](../crates/app-peers/src/host_tools.rs)。以 Calendar 的 `calendar.events` 为例：

1. [tools.json](../apps/calendar/bundle/tools.json) 声明 schema、风险、共享及实现方式；App Hub 校验摘要和规则。
2. `from_bundle` 读取已准入声明；`install` 安装 catalog 项和 `HostServiceExecutor`。
3. 驱动 peer 的 broker 经 `peer/tools/register` 注册精确工具集。
4. 模型请求调用后，octos 发出 `peer/tool/call`；broker 附上真实调用者、账号、context 并交给 Shell `ToolHost`。
5. `Relay::handle` 检查授权、同意、账号状态、输入 schema、大小及预算，需要时走 Shell 审批。
6. 执行器以工具所属应用身份调用 [Calendar 服务](../apps/calendar/host-service/src/lib.rs)。回复经队列回来，输出也经过校验，`ToolReply` 最多发送一次 `peer/tool/result`。
7. 模型读取 JSON 结果，回答或继续调用其他已授权工具。

声明不等于实现。`implemented_by: "host-service"` 需要存在对应服务且所属应用有权限；`implemented_by: "app"` 目前虽能准入，Card runner 尚未实现其脚本执行器，会返回不可用。复制 `tools.json` 或 gate 通过都不会补齐服务代码。

## 8. “读取应用数据”有几种完全不同的含义

| 数据 | 访问方式 |
| --- | --- |
| 应用账号目录 | [app_storage](../crates/shell/src/app_storage/mod.rs) 和 `agent_workspace` 将新 peer 的 cwd 绑定到获准目录；内核文件工具仍需授权。 |
| context 读取账号文本文件 | Shell 的 [files.list/read/search](../crates/shell/src/host_tools/files.rs)，目前仅 Unix、需同意和可用 workspace；有读取上限，拒绝符号链接并隐藏兄弟 context。这不是 SQLite 查询 API。 |
| 人类对话读取父账号目录 | `storage.agent_workspace: "account"` 时请求 `read_parent`，父目录只读、写入仍限自身 context；普通客户端 context 不自动拥有此能力。 |
| 宿主服务数据库或远端账号 | 必须经声明且实现的工具访问；workspace 不会挂载所有服务数据库或泄露凭据。 |
| 对话历史和 Agent 记忆 | octos session/context 及 `app/<broker-app-id>/acct-<tag>`，不同于应用业务数据。 |
| 机密 | 宿主的 secret store 和输入面板，不在脚本状态或 Agent workspace。 |

Calendar 服务在其宿主目录保存 `calendar/events.json` 并通过工具操作；News 暴露 `news.list/read`。Mail 当前 Agent **只声明 `mail.notify`**：Mail UI 能读信和发信不表示模型也能调用。旧 AppCard personal-data 导入器不会自动同步今天 Mail 的存储。

账号目录的 SHA-256 标签与 peer 记忆的 FNV 标签是不同的兼容标识，不能互换。登出暂停访问并保留数据；删除账号/卸载还请求 `peer/purge`，遇到忙 peer 会重试。恢复已有 peer 时不能静默更换已记录的 workspace。

## 9. 跨应用调用与请求帮助

**委派**：系统 Agent 用 `peer_send_input` 请求已有应用 peer 做事，再获取结果；Shell 驱动该回合，并未新建一个进程。

**直接调用另一个应用的工具**：`Catalog::owner_of` 解析 owner；`may_call` 要求工具确实存在、`shareable: true` 以及该调用者的授权（显式开发模式另有规则）。脚本 `agent.tools` 中带点名称和原生清单 grants 表达请求；relay 调用 owner 执行器，不一定经过 owner 的模型。当前 owner 解析覆盖原生、工具箱和 `os.<namespace>`，不是任意商店应用发现机制。还必须通过 App Hub 的 `HostLimits.offered_tools` 准入；默认不提供的 `mail.send` 等名字不能仅靠写入 manifest 获准。

**问题或系统设施**：应用 Agent 只有获得 `ask_user_question` 才能使用它。[questions/mod.rs](../crates/shell/src/questions/mod.rs) 按回合来源将问题放到系统聊天或应用聊天，最终由人在 Shell 界面回答。此路由不是获取系统 Agent 权限的方法。Shell 没有给每个受限应用通用 `ask_system_agent` API 或任意 peer 工具。[系统工具箱](../crates/toolbox/README.md) 是工具/工作流服务，调用它也不等于与系统 Agent 聊天。

系统 Agent 的 `peer_respond` 不能代替用户批准应用操作。审批与问题使用不同协议消息和宿主句柄；超时只会拒绝/谢绝，不会把沉默当成同意。

## 10. 映射到 Rust、线程和 Tokio

`async fn` 返回 future；poll 推进执行，直到等待 I/O。Tokio task 是被调度的 future，一个 OS 线程可以轮流 poll 很多 task。持久 session/peer 可以比当前处理它的所有 task 活得更久。

| 层 | 实际执行方式 | 源码 |
| --- | --- | --- |
| Makepad Shell | UI 事件循环，绘制、模块事件和 relay pump | `module_host.rs`、`host_tools/mod.rs` |
| 系统聊天 | 普通 `std::thread`；命令/快照，通过 waker/unpark 轮询接收 | `system_chat/mod.rs`、`link.rs` |
| Shell 内核服务 | 懒创建 Tokio runtime，**2 个 worker、8 MiB 栈**；一代 supervisor 拥有传输与生命周期 | `kernel/src/lib.rs`、`kernel.rs` |
| 应用 broker | **每次 `Broker::new` 创建 1-worker runtime**；连接、请求、重试与期限任务；多个 broker 可共享同一 peer | `app-peers/src/broker.rs` |
| OpenHarmony 内嵌内核 | 宿主 runtime 上 spawn `serve_io`，通过 `tokio::io::duplex` 通信 | `kernel.rs::start` |
| octos OUP 回合 | 准入后 spawn `run_standalone_turn`，内部模型处理和进度/心跳还有其他任务 | octos `crates/octos-cli/src/api/ui_protocol_transport.rs` |
| octos 输出 | WebSocket 为异步 writer task；stdio/内嵌为有界同步队列和普通 writer 线程 | 同上 |
| 宿主服务/文件执行器 | 随服务而异：UI pump、回调、回复队列、阻塞工作线程；不是统一每应用一个 Tokio task | `files.rs`、App Hub `services.rs`、各服务 |

`mpsc` 是多发送者邮箱，`oneshot` 是一次对应回复，`watch` 是最新生命周期/就绪状态。broker 用请求 ID 找到 `oneshot`，其连接循环用 `select!` 接收双向消息；supervisor 同时等待控制消息、内核输出与退出。`Arc` 共享所有权，`Weak` 避免让已关闭对象永久存活，generation/epoch 丢弃旧账号或旧连接的迟到回复。

同步 `bind`/`host_request` 会等待 channel，不能在绘制回调中阻塞调用。模型异步 I/O 可以并发，阻塞文件操作仍需要现有工作线程边界。**一个应用/账号一个 peer 是身份与隔离规则，不是一个 Agent 对应一个 Tokio task 或 OS 线程。**

## 11. 本次验证与读代码练习

已运行并通过：使用现有 clone hub 的 setup，以及 `python3 tools/setup.py --check --cargo`；`python3 tools/native_apps.py --check`；`python3 -m unittest discover -s rom/tests -p test_no_local_paths.py`；五个文档 worktree 的相对文件链接和 `git diff --check`。另外，`cargo test --locked -p octosense-kernel -p octosense-app-peers --features octosense-app-peers/octos-core,octosense-app-peers/ws` 的单元与脚本化连接测试也通过。真实内核测试未提供所需二进制环境变量，会提前返回，**不算真实内核集成验证**。

[broker 测试](../crates/app-peers/tests/broker.rs)可作为逐步练习：阅读 `a_persons_message_runs_while_the_system_agents_input_runs`、`a_lane_stop_leaves_the_other_lane_running`、`a_kernel_without_shared_history_is_refused_for_the_conversation` 和 `removing_an_account_purges_its_recorded_peer_and_drops_the_record`。它们不需要付费模型即可展示协议行为。[relay 场景测试](../crates/shell/src/host_tools/scenario_tests.rs)展示调用者和审批边界；本次仅阅读，未运行。

桌面 GUI、真实提供方回合、Android/OpenHarmony/iOS 构建、ROM 构建及刷机均**未验证**。`AGENT.md` 提示词加载、应用包自动技能/触发器、任意脚本实现的 Agent 工具及通用应用到系统 Agent RPC 仍有实现缺口。一个架构示例是否能实际执行，要同时检查声明、授权和执行器。
