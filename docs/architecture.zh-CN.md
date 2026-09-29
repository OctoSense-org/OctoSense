# OctoSense 架构

[English](architecture.md) | 简体中文

OctoSense 的各部分如何组合在一起：每个平台上运行哪些进程、Agent 在哪里、各部分如何通信、工具如何授权和审批、数据与机密存放在哪里，以及信任边界在哪里。本文描述 2026-09-28 的 `main`（OctoSense `ff40e9c`，它锁定 octos `5e7577f0`），内容均在代码中读过。

每条陈述都标明状态：

- **已在 main**：已合入，在本仓库代码中读过（给出路径）。
- **进行中**：有未合并的 pull request，附链接。
- **规划中**：已在 ADR 中决定（附链接和计划步骤），尚未实现。

背后的决策是 [ADR 0001](adr/0001-one-octosense-repository.md)（英文，一个仓库）、[ADR 0002](adr/0002-event-driven-app-agents.md)（英文，应用 Agent，Proposed，经 0004 修订）、[ADR 0003](adr/0003-shared-octos-client-access.md)（英文，Talk to Octos）和 [ADR 0004](adr/0004-native-apps-hosting-and-peers.md)（英文，原生应用、应用 Agent、跨应用协作与审批，已接受）。助手的各项服务、各类应用目前能调用什么、如何在本地运行，见 [ai-services.zh-CN.md](ai-services.zh-CN.md)；本文不再重复。

## 目录

- [全局](#全局)
- [1. 各平台的进程](#1-各平台的进程)
- [2. Agent](#2-agent)
- [3. 通信](#3-通信)
- [4. 工具与授权](#4-工具与授权)
- [5. 审批](#5-审批)
- [6. 存储与机密](#6-存储与机密)
- [7. 信任边界与隔离](#7-信任边界与隔离)
- [8. 完整示例：用邮件发送会议邀请](#8-完整示例用邮件发送会议邀请)
- [代码与 ADR 不一致之处](#代码与-adr-不一致之处)
- [源码位置](#源码位置)

## 全局

![OctoSense AI 服务：进程与传输](images/ai-services-processes.png)

![OctoSense AI 服务：一次已授权的跨应用调用](images/ai-services-cross-app-call.png)

*示意图（生成）。实线表示已在 main 上；虚线表示计划中或进行中（进程应用的 peer link；`peer/input` 和宿主工具自锁定 octos 665209e5 起已在 main，octos#2567）。图与下文不一致时以文字为准：例如 `peer_send_input` 把消息放入 peer 的收件箱，peer 在下一轮处理它。*

```mermaid
flowchart LR
  person(["用户"])
  subgraph shellp["OctoSense Shell 进程（桌面端，或手机上的 Home）"]
    wm["窗口管理器、启动器、面板<br/>crates/shell"]
    mods["进程内原生模块<br/>App Hub、Rinx、（AppCard）"]
    runner["App Hub Card runner<br/>脚本应用各在隔离环境中"]
    router["审批路由<br/>crates/shell/src/approvals"]
    aihost["ai-host + app-peers broker<br/>到内核的宿主连接"]
    bus["AI 服务总线<br/>crates/shell/src/ai_bus.rs"]
  end
  term["进程应用（桌面端）<br/>Terminal"]
  subgraph kern["octos 内核（每个 Shell 一个）"]
    sys["系统 Agent 会话<br/>（profile _main）"]
    peers["应用 peer<br/>每个（应用，账号）一个"]
  end
  ext["Talk to Octos 客户端<br/>网页、终端（需手动开启）"]
  person --> wm
  wm --- mods
  wm --- runner
  term <-->|"hub（回环 WebSocket）"| wm
  mods -- "注入的 OctosAppService" --> aihost
  runner -- "host.request(octos.*)" --> aihost
  aihost <-->|"OUP over stdio<br/>（或宿主 token 的 WebSocket）"| kern
  sys -- "peer_send_input / 黑板" --> peers
  ext -. "外部 token、允许列表" .-> sys
  term -- "类型化工具" --> bus
  mods -- "类型化工具" --> bus
  bus --> router
```

```
+--------------------------- OctoSense Shell 进程 -------------------------------+
|  窗口管理器 / 启动器 / 面板               审批路由            AI 服务总线       |
|  进程内模块：App Hub（+ 运行脚本应用的 Card runner）、Rinx、（AppCard）         |
|  ai-host + app-peers broker  == 宿主连接 ==+                                    |
+--------------------------------------------|-----------------------------------+
        ^ hub：回环 WebSocket                | OUP：stdio（默认）或
        |                                    | 带宿主 token 的 WebSocket
+-------+---------+           +--------------v--------------------------+
| 进程应用        |           | octos 内核（子进程；OHOS 上在进程内）   |
| Terminal 等     |           |   系统 Agent 会话                       |
+-----------------+           |   应用 peer：每个（应用，账号）一个     |
                              |   工具以短时子进程运行                  |
   Talk to Octos 客户端 ----> +-----------------------------------------+
   （外部 token、允许列表、需手动开启）
```

## 1. 各平台的进程

### Shell

每台设备一个 Makepad 进程：桌面端（`desktop/`，包 `octosense`）或手机上的 Home（`phone/`，包 `octosense-home`），两者都链接同一个 Shell crate `crates/shell`（[ADR 0001](adr/0001-one-octosense-repository.md)）。Shell 拥有窗口管理器、启动器、宿主面板、审批路由、应用存储以及内核的宿主连接。

### octos 内核

每个 Shell 最多运行一个 [octos](https://github.com/octos-org/octos) 内核，它是 [`crates/kernel`](../crates/kernel/README.zh-CN.md)（包 `octosense-kernel`）中的一项 Shell 服务，通过 [`crates/ai-host`](../crates/ai-host/README.md) 访问。**已在 main。** 运行方式（`crates/kernel/src/launch.rs`，`launch::resolve`）：

| 平台 | 内核 | 如何启动 |
| --- | --- | --- |
| 桌面端（macOS；Windows 和 Linux 未经测试） | 子进程：`OCTOS_APP_CORE_BIN`（或嵌入方的 `Options::program`）指定的 `octos` 二进制 | `serve --stdio --data-dir <core 目录>`；没有二进制就没有内核（不在 `PATH` 中查找）。随桌面端一起发布该二进制正在进行中（[#85](https://github.com/OctoSense-org/OctoSense/pull/85)）。 |
| Android（Home） | 子进程：APK 中打包的 `liboctos.so`，由 [`tools/kernel-artifact.py`](../tools/kernel-artifact.py) 按锁定的 octos 版本构建 | `serve --stdio`，在应用的原生库目录中、`libmakepad.so` 旁边找到 |
| OpenHarmony | 进程内：`octos_cli::embedded::serve_io` 作为一个任务运行在内存中的双工流上（HAP 不允许 exec） | 同样的协议，没有子进程 |
| iOS | 无 | 提供方配置会保存；没有应用能用上助手 |

生命周期（`crates/kernel/src/lib.rs`、`kernel.rs`）：

- **按需启动，共享使用。** 第一个使用方调用 `connect()` 之前什么都不运行；之后的使用方连接到同一代内核。新一代内核会等上一代退出后再启动，从而先释放 octos 对数据目录的单写者锁。
- **提供方变化时重启。** `llm` 宿主服务重写配置后调用 `restart()`：所有连接以 `CloseReason::Restarted` 结束，使用方重新连接到新的内核。
- **空闲停止。** Talk to Octos 关闭时，最后一个连接关闭后内核即停止。开启时，内核一直运行，直到用户关闭它或 Shell 退出。
- **随 Shell 退出。** Shell 在子进程整个生命周期内持有其 stdin；Shell 退出、崩溃或被杀死时，内核读到 EOF 并停止。在 `--host-managed` 模式下，octos 在 Linux 和 Android 上还会请求在父进程死亡时收到 SIGTERM（octos `crates/octos-cli/src/api/host_managed.rs` 中的 `bind_to_parent()`）；子进程也以 `kill_on_drop` 方式创建。
- **崩溃**会以 `CloseReason::Exited`（附最后几行 stderr）结束所有连接；不会自动重启，下一次 `connect()` 启动新一代。见[内核崩溃意味着什么](#内核崩溃意味着什么)。

### 原生应用：进程内还是独立进程

原生应用是第一方 Rust crate，只在 [`native-apps.json`](../native-apps.json) 中声明；`tools/native_apps.py` 据此生成 `crates/shell/src/native_apps.rs` 和各处 Cargo 配置块，并在 CI 中检查（ADR 0004 §1，计划步骤 1）。**已在 main。** 目前的清单：

| 应用 | macOS / Windows 上的托管 | Linux | Android、iOS、OpenHarmony | 桌面端 / 手机 | 助手 |
| --- | --- | --- | --- | --- | --- |
| App Hub（`apphub`：商店和 Card runner） | 进程内 | 进程内 | 进程内 | 默认 / 默认 | – |
| Rinx | 进程内 | 进程内 | 进程内 | 默认 / 默认 | 获授权四个 `octos.*` 服务 |
| Terminal | **独立进程** | Vulkan 构建且在 Wayland 会话中时为独立进程，否则进程内 | 进程内 | 默认 / 关闭 | –（其 `run` 工具为 `confirm: host`、`auto_approvable: false`） |
| Sheets、Reference | 进程内 | 进程内 | 进程内 | 可选 / `mobile-apps` | – |
| AppCard | 进程内 | 进程内 | 进程内 | 可选 / 可选 | 自己的内核连接 |

因此**只有 Terminal 是进程应用**，而且只在桌面端；`tools/native_apps.py` 拒绝 Linux 上的普通 `process`（只允许 `process-if-vulkan`），所以没有 Vulkan 和 Wayland 的 Linux 上所有应用都在进程内运行。App Hub 留在进程内，因为它承载所有脚本应用运行所在的 Card runner；Rinx 留在进程内，直到 peer link 和进程沙箱就绪（ADR 0004 §2）。

Shell 在运行时如何决定（`crates/shell/src/apps.rs`，`AppRegistry::hosting`）：如果目标平台不能运行进程（在 wasm 和原生移动平台上 `host::processes_available()` 为 false），所有应用都作为模块；App Hub 和 Settings 始终是模块；其他情况下原生应用按清单条目托管，并且只有存在进程形态（可以 `cargo run` 的源码检出，或同目录下的二进制）时才作为进程运行。发布包目前还不附带进程应用的二进制（[#94](https://github.com/OctoSense-org/OctoSense/pull/94)，进行中），因此在发布包中会回退到进程内。按应用的覆盖设置（`~/.makepad/wm/apps.splash`、`--module <id>`）可以把模块切换为进程。

**进程托管**使用 Makepad 窗口管理器的托管机制（`crates/shell/src/clients.rs`、`hub.rs`）：

- Shell 以 `--stdin-loop` 启动应用子进程（在源码检出中为 `cargo run … -- --stdin-loop`，否则为同目录下的二进制），放在独立的进程组中，并设置 `STUDIO_HOST=http://127.0.0.1:<hub 端口>` 和其客户端 id。
- 子进程回连 Shell 的 **hub**：Shell 在回环地址 8765–8785 中第一个空闲端口上绑定的 HTTP/WebSocket 服务（`WmHub::start`），使用 Makepad 的 studio 协议（`AppToStudio` / `StudioToApp`）。
- 在操作系统允许时，画面以零拷贝方式到达合成器（在 Makepad `platform/src/os` 中实现）：macOS 上用 IOSurface，Windows 上用 D3D11 共享句柄，Linux 在 Vulkan 加 Wayland 下用 DMA_BUF；Linux 的 OpenGL 构建则每帧经 CPU 回读。
- 进程应用崩溃只影响它自己：Shell 移除其客户端并显示 “App stopped”。（目前只有进程内模块有 Restart 界面；见[不一致之处](#代码与-adr-不一致之处)。）

**进程内托管**（`crates/shell/src/module_host.rs`）：模块的每个实例都有自己的 splash 隔离环境（`alloc_splash_vm_with_network`）和存储命名空间。隔离环境只隔离脚本堆；模块的 Rust 代码与 Shell 共享内存。每次调用模块都在 `catch_unwind` 下进行（`contain`、`contain_outside`）：panic 会把该模块标记为失败，把它进行中的工具调用答复为“结果未知”，关闭它的额外窗口并显示 Restart 界面，而 Shell 继续运行（[#114](https://github.com/OctoSense-org/OctoSense/pull/114)，ADR 0004 步骤 9）。**已在 main。**

### 脚本应用

脚本应用（系统应用 News、Photos、Maps、仅手机上的 Camera、Mail、AI providers 和 YouTube，来自 `desktop/system-apps.json` 和 `phone/system-apps.json`，以及商店应用）是 OctoScript 应用包。它们都在进程内、在 App Hub 的 **Card runner**（`octosense-app-hub-app` 的 `CARD_MODULE`）中运行：每个应用实例一个嵌套隔离环境，去掉了 `mod.res` 和 `mod.run`，有 jail 和配额。应用只能通过 `host.request("<family>.<method>", …)` 调用其清单获授权的服务族来访问 Shell。脚本的错误只会在它自己的隔离环境中失败。**已在 main。**

```mermaid
flowchart TB
  subgraph desktop["桌面端（macOS）"]
    ds["OctoSense 进程<br/>Shell + App Hub + Card runner + Rinx"]
    dk["octos 子进程<br/>OCTOS_APP_CORE_BIN serve --stdio"]
    dt["Terminal 子进程<br/>--stdin-loop"]
    ds -- "stdin/stdout" --> dk
    dt -- "hub WebSocket、IOSurface 画面" --> ds
  end
  subgraph android["Android（Home）"]
    as["Home 进程<br/>Shell + App Hub + Card runner + Rinx（全部进程内）"]
    ak["liboctos.so 子进程<br/>serve --stdio"]
    as -- "stdin/stdout" --> ak
  end
  subgraph ohos["OpenHarmony"]
    os["Home 进程<br/>Shell + 内嵌 octos 任务"]
  end
  subgraph ios["iOS"]
    is["Home 进程<br/>无内核"]
  end
```

## 2. Agent

**每个 Shell 一个内核；Agent 是会话，不是进程。** 在这一个内核中，每个 Agent 都是一个以 tokio 任务运行的 octos 会话。文件编辑之类的工具在内核中运行；octos 的命令类工具在会话的工作区中以短时子进程运行（OctoSense 不让系统 Agent 使用它们，见[第 4 节](#4-工具与授权)）。

| Agent | 是什么 | 状态 |
| --- | --- | --- |
| **系统 Agent** | `_main` profile 上的会话 `_main:api:octosense#system`（`crates/kernel/src/network.rs` 中的 `SYSTEM_SESSION`）。它拥有并监督所有应用 peer。目前用户通过 Talk to Octos 客户端与它对话；Shell 还没有为它绘制对话界面 | 已在 main |
| **应用 Agent** | 每个（应用，账号）一个由宿主拥有的 octos **peer**，归系统 Agent 所有（octos UPCR-2026-034，Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md)） | Rinx（原生）已在 main；脚本应用在开关后可用；见下文 |

每个应用 peer 都独立拥有：

- **工作区**：peer 会话所绑定的目录。自锁定 octos 665209e5 起新建的 peer 以账号目录（`apps/<app id>/accounts/<account hash>/`，ADR 0004 §11）作为 `peer/prepare` 的 `cwd`（`host_tools::agent_workspace`），并记录在其宿主令牌旁边，恢复时使用同一目录；之前创建的 peer 保留内核分配的目录，因为 octos 不允许在另一个工作区下恢复 peer（见[不一致之处](#代码与-adr-不一致之处)）；
- **记忆命名空间** `app/<app>/acct-<hash>`（`crates/app-peers/src/broker.rs` 中的 `app_namespace()`）；不返回它的内核会被拒绝；
- 自己的**对话记录**和**模型**通道（宿主通过 `peer/prepare` / `peer/model/set` 设置模型）；
- 自己的**工具列表**：内核对 peer 安全的默认工具，加上 Shell 注册的工具（octos [#2567](https://github.com/octos-org/octos/pull/2567)，UPCR-2026-035）：每次 `peer/prepare` 和重连之后，broker 在驱动该 peer 回合的连接上注册应用声明的工具，以及授予它的其他应用可共享工具，每个都标明所属应用（`crates/shell/src/host_tools/`），不传 `generic_tools`。`main` 上还没有应用声明 `tools.json`，因此每个 peer 注册一个空集合，这仍然让它的回合获得应用的记忆和上下文（注册失败的 peer 不运行任何回合）；
- **请求上下文**（`peer/context/open`）：每个客户端实例一个（一个 Rinx 小程序），各有自己的对话记录、peer 工作区内的目录 `contexts/<id>/` 和子记忆命名空间。上下文读不到旁边的文件。ADR 0004 §6（2026-09-29 决定）只把它们留给按客户端划分的工作：用户与应用 Agent 的对话（来自应用界面或其卡片）改到 peer 自己的会话上，与系统 Agent 共享（见下文）。main 上用户的回合仍在请求上下文中运行。

目前谁有 peer（`crates/ai-host/src/lib.rs`，`Policy::shipped()`；`crates/app-peers/src/hosted.rs`，`effective_services` = 声明 ∩ 支持 ∩ 策略）：

- **Rinx**，唯一获授权使用助手的原生应用（四个 `octos.*` 服务），需首次使用时的同意。
- 声明了 `octos.*` 的**脚本应用**：每个应用一个 peer `card.<app id>`，账号为 `device`（`crates/ai-host/src/contained.rs`，[#106](https://github.com/OctoSense-org/OctoSense/pull/106)），前提是 `Policy::contained_apps` 开启（发布策略中默认关闭；`OCTOSENSE_CONTAINED_APPS=1` 可开启）且用户在首次使用时同意。
- **AppCard**（可选）使用自己的内核连接和会话，而不是 peer。
- 进程应用目前没有 Agent：只有 `ModuleHost::create` 中才会提供服务（peer link 规划中，步骤 8）。

peer 会被保留而不是丢弃：broker 用其宿主 token（存放在 `<core 目录>/../app-peers`，0600）恢复 peer，登出时也从不调用 `peer_close`，因为 octos 无法恢复已关闭的 peer，也无法为同一个（应用，账号）创建替代者（ADR 0004 §11）。

### 内核崩溃意味着什么

所有 Agent 都在同一个内核中，因此内核崩溃会同时停止**全部** Agent：系统 Agent 和每个应用 peer，包括正在进行的回合。Shell 和应用继续运行；`availability()` 报告助手失败，应用的常规界面照常工作，进行中的回合随连接一起结束。octos 已持久化的内容不会丢失：会话、黑板、记忆命名空间和 peer 绑定都在内核的数据目录中，因此下一次 `connect()` 启动新一代内核，使用方用保存的宿主 token 重新打开并恢复各自的 peer 和上下文。结果未知的工具调用未经用户同意绝不重试（ADR 0004 §7）。

## 3. 通信

```mermaid
flowchart LR
  subgraph shell["Shell（宿主连接）"]
    broker["app-peers broker"]
    relay["宿主工具中继<br/>（crates/shell/src/host_tools）"]
  end
  subgraph kernel["octos 内核"]
    sys["系统 Agent"]
    p1["Calendar peer"]
    p2["Mail peer"]
    bb[("黑板<br/>peers/&lt;slug&gt;/result.md、turns.txt")]
  end
  rinx["Rinx（模块）"] -- "OctosAppService" --> broker
  card["脚本应用"] -- "host.request(octos.*)" --> broker
  proc["进程应用"] -. "peer link（规划中）" .-> broker
  broker -- "OUP：peer/prepare、peer/context/open、turn/start" --> kernel
  sys -- "peer_send_input" --> p1
  p1 -- "写入" --> bb
  sys -- "peer_gather / peer_list" --> bb
  kernel -- "peer/input、peer/tool/call、host_tool 审批" --> relay
```

### 内核与客户端之间的 OUP

内核使用 **octos UI 协议**（OUP；`octos-ui/v1alpha1`，JSON-RPC 2.0 帧；octos [`api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md`](https://github.com/octos-org/octos/blob/main/api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md)）。**已在 main。**

- **默认：stdio。** Shell 是唯一的客户端，经子进程的 stdin 和 stdout 通信（按行分隔的 JSON；octos UPCR-2026-016）。在 Shell 内部，`crates/kernel/src/router.rs` 在各原生使用方之间复用同一个帧流：每个请求获得内核范围内唯一的 id，其回复只发回该使用方；通知发给声明了该会话的使用方。
- **Talk to Octos：宿主管理的 WebSocket**（[ADR 0003](adr/0003-shared-octos-client-access.md)，[#98](https://github.com/OctoSense-org/OctoSense/pull/98)；octos [`docs/HOST_MANAGED_SERVE.md`](https://github.com/octos-org/octos/blob/main/docs/HOST_MANAGED_SERVE.md)，UPCR-2026-036）。用户在 AI providers 中开启后（`<core 目录>/external-access.json`），Shell 以 `octos serve --host-managed --host 127.0.0.1` 重启内核，使用 Shell 只绑定一次并传下去的监听套接字（Unix 上为 `--listen-fd`）。两个 token 写在内核 stdin 的前两行，从不放入环境变量：

  | Token | 持有者 | 可以做什么 |
  | --- | --- | --- |
  | **宿主 token** | 只有 Shell 进程；每个 Shell 生命周期生成一次，从不记录日志 | 一切（管理员）；原生使用方用它连接 `/api/ui-protocol/ws` |
  | **外部 token** | 已配对的网页客户端（可信面板上的 8 位配对码，5 分钟有效，只能领取一次）或本用户的终端客户端（0600 的 `client-connection.json`） | 只能访问 `/api/ui-protocol/ws`，身份为 `_main`，且只能调用允许列表中的方法：打开并读取系统对话，启动、引导或中断**自己的**回合，回答自己回合的审批（一次性）和问题 |

  外部客户端不能：调用任何 `peer/*` 方法或指定应用 peer 的会话（`peer-…`、`peerctx-…`）；设置 `cwd`、`topic` 或 `sandbox`；触及提供方、密钥、模型、skills、快照或 `server/shutdown`；使用 REST 或管理路由。它们的回合只有固定的工具集（文件工具、`web_search`、`web_fetch`、记忆读取、`ask_user_question`、查看媒体；`crates/kernel/src/system_tools.rs` 中的 `EXTERNAL_TURN_TOOLS` 与之对应）：没有 shell、没有 peer 工具、没有宿主路由的工具（octos [#2601](https://github.com/octos-org/octos/pull/2601)）。OpenHarmony 和 iOS 上没有；Android 未验证。

### 内核中的系统 Agent 与应用 Agent

使用 octos 的 peer 机制，深度为 1（peer 不能创建、引导或关闭 peer）。octos 中**已在 main**；这些工具位于 octos `crates/octos-agent/src/tools/`。

- **系统 Agent → 应用 Agent。** `peer_send_input`（只有发起者可用，最多 64 KB）经 peer 的收件箱把文字作为 peer 的下一个用户回合送达（在 serve 中是持久队列，每隔几秒处理一次，至少送达一次）。
- **应用 Agent → 系统 Agent：黑板。** peer 的每个回合写入 `peers/<slug>/result.md`（以及 `result-<n>.md`），并在 `turns.txt` 中追加一行；系统 Agent 用 `peer_gather` 和 `peer_list` 读取（`awaiting_input` 表示 peer 正在等待回答问题）。这是 peer 之间唯一的通道。
- **提问。** peer 用 `ask_user_question` 提问；系统 Agent 用 `peer_respond` 回答。`peer_respond` 从不回答审批（octos 会拒绝）。宿主驱动的回合没有 `ask_user_question`，因此 ADR 0004 §6 增加了 Shell 工具 **`host.ask`**，由 Shell 转给系统 Agent 或应用自己的对话。**规划中**（步骤 6）。
- **每个应用 Agent 一个共享对话。** ADR 0004 §6（2026-09-29 决定）：用户（来自应用界面或其卡片）和系统 Agent 与同一个 peer 会话对话，每个回合标明说话者 `origin`（`person`、`system_agent`、`app`；宿主不能改写系统 Agent 回合的标记），因此应用的对话能看到系统 Agent 的往来，黑板也反映用户的回合。**规划中**：需要 octos 改动（octos PR 待提交）；main 上用户的回合在请求上下文中运行，双方看不到彼此的往来。

### 宿主拥有的路径：`peer/input`

对宿主拥有的 peer，普通的 `peer_send_input` 会作为内核续跑执行，没有应用的工具或记忆。ADR 0004 §6 解决了这个问题：octos 把系统 Agent 的输入以 **`peer/input {peer, session_id, input_id, turn_id, text}`** 的形式送到宿主的驱动连接，由 Shell 自己启动回合（用内核的 `turn_id` 调用 `turn/start`），这样回合就带有应用的工具、记忆和上下文，其审批也出现在应用中。如果没有宿主连接持有该 peer，octos 会告诉系统 Agent 该应用未连接。**已在 main**：自锁定 665209e5 起 octos 会发送它（octos [#2567](https://github.com/octos-org/octos/pull/2567)）；broker（`crates/app-peers/src/broker.rs`）用内核的 `turn_id` 在自己的连接上启动回合，丢弃重复的 `input_id`，peer 回合运行时把输入排队，对已登出的账号或用户未允许的应用不启动任何回合（`host_tools::ShellToolHost::admit_input`）。被拒绝的输入只记日志：octos 没有让宿主告诉系统 Agent 的途径，系统 Agent 听到的是 “queued”。Shell 的各个内核使用方共用一个内核连接（`crates/kernel/src/router.rs`），因此每个 broker 和系统对话对 octos 来说是同一个宿主连接；各自只回答自己的 peer 或会话的调用。

### 应用与它自己的 Agent

| 托管方式 | 通道 | 状态 |
| --- | --- | --- |
| 进程内原生模块（Rinx） | **注入的服务**：`create` 之前调用 `ai_host::offer`，在其中调用 `octosense_app_peers::injection::claim`，得到受限的 `OctosAppService`（`Open`、`History`、`Turn`、`Interrupt`、`Approval`）；模块永远看不到协议 | 已在 main |
| 脚本应用 | 向 `octos` 宿主服务（`crates/ai-host/src/contained.rs`）调用 `host.request("octos.session.open" / "octos.session.history" / "octos.turn.start" / "octos.turn.interrupt", …)`；受清单、`Policy::contained_apps` 和首次使用同意（`consent_for_contained`）约束；其 peer 发起的工具审批一律被拒绝，并列在 `denied_approvals` 中 | 已在 main（[#106](https://github.com/OctoSense-org/OctoSense/pull/106)，同意机制来自 [#120](https://github.com/OctoSense-org/OctoSense/pull/120)） |
| 独立进程的原生应用 | **peer link**：应用 hub 连接上的独立通道（`PeerRequest`、`PeerReply`，以及由 Shell 盖上身份和调用方的 `PeerToolCall`），从不注册到 AI 总线；客户端 API 在 Makepad 的 `makepad-ai-services` 中 | 规划中（ADR 0004 §5，步骤 8）；`hub.rs` 中尚无代码 |

### Makepad 的 AI 服务总线与 OctoSense 的应用 Agent

两种模型并存（ADR 0004，“Two AI models”）：

| | Makepad AI 服务总线 | OctoSense 应用 Agent |
| --- | --- | --- |
| 位置 | 窗口管理器一侧在 `crates/shell/src/ai_bus.rs`；上游 `libs/ai/services` | `crates/ai-host`、`crates/app-peers`、octos 内核 |
| 形态 | **一个中心对话**（桌面端的 AI 面板，即 Makepad 的 `aichat`）调用应用以风险等级（`Read`、`Act`、`Destructive`）注册的类型化工具 | **每个应用一个 Agent**，拥有应用的完整上下文（工作区、记忆、历史、工具），由系统 Agent 监督 |
| 路由 | Shell 给每个上行帧盖上发送方的端点，把注册转给面板（面板重连时重放），把面板的调用路由到应用的套接字，并自己回答 `os` 服务（list、launch、focus、close、open） | broker 直接用 OUP 与内核通信 |
| 用途 | 桌面端的 AI 面板；目前 Rinx 的助手工具；Terminal 的 `run`。其 `confirm: host` 调用经过 Shell 的审批路由（[#120](https://github.com/OctoSense-org/OctoSense/pull/120)） | 应用自己的助手；系统 Agent 的委派 |

**为什么应用 Agent 的流量不走总线。** 总线是通向一个掌握全部上下文的中心 Agent 的窄而单向的 API；应用 Agent 需要应用的完整上下文和一个私有、受监督的会话，而系统 Agent 必须通过内核的 peer 机制与它通信。总线也不携带账号、请求上下文或调用方，无法按 ADR 0004 §5 的要求在每次工具调用上盖上身份，而且其注册对面板可见。因此在 OctoSense 中，总线不是系统 Agent 通向应用的通道；它留给上游 Makepad 应用（ADR 0004 §6），进程应用的 peer link 也有意设计为独立通道。`crates/ai-host` 和 `crates/app-peers` 都不使用总线。

## 4. 工具与授权

**清单声明，用户在安装时授权，Shell 在每次调用时执行检查**（ADR 0004 §12，ADR 0002 §4）。脚本应用在 `manifest.json` 中声明能力，在 `tools.json` 中声明工具（由 App Hub 准入并锁定）；原生应用在经过评审的 `native-apps.json` 条目中声明（`agent.octos`、`agent.tools`、`agent.tool_policy`）。

Agent 的工具来源：

| 来源 | 示例 | 在哪里运行 | 状态 |
| --- | --- | --- | --- |
| 应用自己的工具（`tools.json`：名称 `<app>.<tool>`、schema、`risk`、`confirm: host` 或 `app`、`shareable`） | `mail.send`、`rinx.message.send` | 应用的宿主服务、模块或进程，由 Shell 调用 | 已在 main 上注册和路由（`crates/shell/src/host_tools/`）：按（所属应用，工具）和调用方授权，盖上账号、上下文和客户端，路由到进程应用的 peer link 或进程内模块的执行器（`OctosAppService::set_tool_executor`）。还没有应用声明 `tools.json`，所以尚未注册任何工具；Rinx 的工具经总线到达 AI 面板 |
| 系统工具箱，按能力授予（`research`、`crawl`、`model`） | `toolbox.search`、`toolbox.web_read`、`toolbox.deep_crawl`、`workflow.run` | 宿主 | 进行中（[#108](https://github.com/OctoSense-org/OctoSense/pull/108)）；`model` 尚未注册 |
| octos 的通用工具 | 限定在工作区内的文件读取、记忆、`web_search`、`deep_search` | octos | 作为 octos 对 peer 安全的默认工具已在 main；按应用设置的 `generic_tools` 列表规划中（步骤 6） |
| 其他应用可共享的工具，由 Shell 路由 | Calendar 的 Agent 调用 `mail.send` | 所属应用，经 Shell | 已在 main 上注册、逐次检查并路由（`relay::Catalog`）；还没有任何授权（App Hub 安装时授权和 `native-apps.json` 授权属于步骤 6） |
| 命令执行 | `terminal.run`（Terminal 的可共享工具：`confirm: host`、`auto_approvable: false`） | 宿主工具，在用户可见的终端中 | 系统 Agent 已在 main：Setup › Assistant › Command execution 开启时注册在其会话上，每次调用经路由作为命令实时批准，输入到正在运行的 Terminal（macOS 和 Windows 上是进程应用；进程内 Terminal 只提供读取工具）。授予应用 Agent 属于步骤 11 |

**系统 Agent 的工具集**（`crates/kernel/src/system_tools.rs`，[#117](https://github.com/OctoSense-org/OctoSense/pull/117)）。**已在 main**，部分生效：

- 默认列表 `SYSTEM_AGENT_TOOLS`：监督（`peer_send_input`、`peer_gather`、`peer_list`、`peer_respond`、`peer_close`；不含 `peer_handoff`）、其工作区的文件工具、`ask_user_question` 和查看媒体、记忆、`web_search` / `web_fetch`、`tool_search`。授权在此基础上增加（`SystemAgentTools`：工具箱工具、其他应用的可共享工具、命令执行）。
- **从不提供 octos 自己的 shell。** 每次启动内核前，`enforce` 写入 `_main` profile 的 `tool_policy`，拒绝 `group:runtime`（`shell`、`bash`、`exec_command`、`write_stdin`）。它只替换 OctoSense 自己写的策略，并拒绝用户自己的 octos 主目录。
- **还不是精确列表。** octos#2567 的宿主会话集合可以用 `generic_tools` 收窄会话，但 Shell 注册时不传它（按 #2567 的约定：该列表会收窄该会话上所有客户端的回合），因此系统 Agent 仍会得到 octos 注册的其余所有工具；精确列表的测试已保留但被忽略（持久的宿主专用列表是 octos#2605）。
- **从 Settings 开启命令执行**（默认关闭，每条命令实时批准）：Setup › Assistant › Command execution 设置授权；开启期间，系统对话在自己的连接上把 `terminal.run` 注册到系统会话（不带 `peer` 的 `peer/tools/register`），关闭时立即撤回。octos 以某个应用 peer 的宿主令牌作为凭据，因此在某个应用的 Agent 启动之前，对话会说明暂时无法提供。

**外部客户端**只有 octos 的固定允许列表（[第 3 节](#内核与客户端之间的-oup)）；宿主路由的工具、开发者授权和 `dev.run` 永远到不了它们。

## 5. 审批

**授权不等于审批。** 授权表示 Agent 可以*拥有*某个工具；审批表示*这一次*调用、带着这些确切参数，可以执行。只读和应用内操作类工具授权后即可运行；对外或破坏性的调用（发送、发布、分享、购买、删除、运行命令）需要用户实时批准或由常设规则批准。只有用户能批准；系统 Agent 从不批准，Agent 自己输出的文字也从不作为审批界面（ADR 0004 §8）。

**Agent 的工具调用，而不是用户自己的操作。** 审批路由和 Shell 的面板管的是 Agent 的工具调用（`peer/tool/call`）。用户在应用自己的界面中所做的操作，包括它在速览屏和通知背后的卡片，都是应用自己的操作：在应用的策略下，经 Card runner 的服务关口和各宿主服务自己的检查运行，Shell 不再另加审批（ADR 0004 §4 和 §8，2026-09-29 决定；卡片操作的安全加固推迟）。应用可以画出貌似审批卡片的界面，但只有 Shell 的宿主连接能回答 `approval/respond`。**进行中**：main 上速览磁贴仍是受限的（空策略、不接受输入、整个磁贴是 Shell 的一个点按目标；`crates/shell/src/glance_card.rs`）；可交互卡片在 PR 分支 `feat/interactive-glance-cards`。

**审批路由**（`crates/shell/src/approvals/router.rs`，[#120](https://github.com/OctoSense-org/OctoSense/pull/120)）是 Shell 中唯一回答审批请求的地方，依据的是确切参数。**已在 main。** 对每个请求依次：

```mermaid
flowchart TB
  req["审批请求<br/>（所属应用、工具、确切参数、调用方、上下文）"]
  dev{"1. 开发者模式<br/>覆盖该应用？"}
  capp{"2. confirm: app？"}
  always{"3. auto_approvable: false、<br/>结果未知，<br/>或外部客户端？"}
  rule{"4. 有针对（所属应用，工具）<br/>的常设规则匹配？"}
  sheet["5. Shell 绘制的面板<br/>（每个请求一个，或在系统对话中<br/>合并为一个）"]
  ok(["批准，已审计"])
  appsheet["所属应用自己的面板，<br/>显示调用方<br/>（等待，超时后明确拒绝）"]
  person["用户"]
  req --> dev
  dev -- 是 --> ok
  dev -- 否 --> capp
  capp -- 是 --> appsheet --> person
  capp -- 否 --> always
  always -- 是 --> sheet
  always -- 否 --> rule
  rule -- 是 --> ok
  rule -- 否 --> sheet
  sheet --> person
```

1. **开发者模式**（`dev_hooks.rs`、`crates/shell/src/dev_mode.rs`，[#118](https://github.com/OctoSense-org/OctoSense/pull/118)）批准它所覆盖应用的一切，包括 `auto_approvable: false` 和 `confirm: app`，但从不替外部客户端批准。只有用户能开启它：开发构建中用 `OCTOSENSE_DEV_MODE=all`（或应用列表），发布构建只能用 `--dev-grant-all`，或在桌面端的 Developer options 中输入确认短语；商店构建永远不能。开启时显示横幅，审计每次调用，不在开发者 profile 中时 8 小时后或重启时自动结束。`dev.run` 尚未注册（步骤 11）。
2. **`confirm: app`** 工具交给所属应用自己的面板（应用通过 `register_app_confirm` 注册的 `AppConfirm`），并附上调用方；规则不回答它们。未注册面板的应用有 120 秒（`app_wait_s`），之后调用被明确拒绝。进程内应用通过自己的服务注册面板（`OctosAppService::set_confirm_sheet`，由 `host_tools::SheetBridge` 接入）；所锁定标签的 Rinx 还没有这样做，因此它的发送面板仍在 Rinx 内部作答。
3. **`auto_approvable: false`**、**结果未知**和**外部客户端**的调用总是交给用户。
4. **常设规则**（`rules.rs`），以**（所属应用，工具）**为键，不论谁调用；可以对确切参数设条件（收件人在联系人中或在该会话中、无附件、由用户触发、次数或金额上限；调用缺少所需信息时条件不成立），有每日上限（工具规则默认 20 次），范围最宽的规则（“这个应用接下来一小时的所有请求”）最多 60 分钟（没有“一切、永久”的规则），还有一个“关闭所有规则”。联系人来自 Mail 宿主服务的数据（用户自己的账户，以及用户发过邮件的地址；`contacts.rs`），并且只有在用户于设置中打开“在审批规则中使用我的联系人”（默认关闭）之后才会使用；在此之前“收件人在联系人中”从不匹配。系统通讯录是后续工作。由收到的内容触发的运行会被跳过，除非规则明确包含。用户在 Settings → Assistant → Approvals（`settings_page.rs`）中或从面板上创建规则；系统 Agent 只能建议。
5. 否则显示 **Shell 绘制的面板**（`sheet.rs`、`view.rs`）：每一行显示所属应用、工具、确切参数，跨应用调用时还显示调用方应用；系统 Agent 同一请求的 `confirm: host` 审批可以合并到它对话中的一个面板。

每个决定都只向中继发送一次，并写入**审计**（`audit.rs`：`<octosense home>/logs/approvals-audit.jsonl` 中每个决定一行 JSON，仅所有者可读，记录参数的摘要而不是参数本身；开发者模式在 `logs/dev-audit.jsonl` 中另有完整审计）；每个自动决定也会作为通知告知用户。规则存放在 `approvals/rules.json`，同意记录在 `approvals/consent.json`，都在 OctoSense 主目录下。

**首次使用同意**（`consent.rs`）：应用第一次请求它的 Agent 时，Shell 显示该 Agent 可以读取和使用什么，以及模型在哪里运行；Settings 列出每个应用的 Agent 并提供关闭开关（ADR 0004 §4）。原生模块（`module_host.rs` 中在提供服务之前调用 `consent_for_module`）和脚本应用（`consent_for_contained`）**已在 main**。

**谁在向路由提交请求。** AI 服务总线：面板对 `native-apps.json` 中列出的 `confirm: host` 工具（目前是 Terminal 的 `run`）的调用会被挂起（`bus:<endpoint>:<call id>`），直到路由作答。以及 octos，经宿主工具中继（`host_tools::init` 在启动时用 `set_relay` 安装它）：每个 `host_tool` 审批（有门控的 `confirm: host` 应用工具，只发给宿主连接），附所属应用、确切参数和调用方；以及每个 `confirm: app` 调用，先确认收到，再交给所属应用的面板；系统对话也以同样方式把其会话的审批交给路由。不是宿主工具、在应用上下文中发起的 octos 审批仍交给该应用：脚本应用的在 `contained.rs` 中被拒绝，Rinx 的由用户在 Rinx 中回答，开发者模式下由 `crates/app-peers/src/host_approvals.rs` 为其覆盖的应用作答（有审计）。

## 6. 存储与机密

所有应用使用同一种由宿主拥有的布局，由其清单（`storage` 块）声明，且只由 Shell 计算（`crates/shell/src/app_storage/`，[#115](https://github.com/OctoSense-org/OctoSense/pull/115)，ADR 0004 §11）。作为原生模块在 `create` 时获得的 API **已在 main**（`octosense_app_peers::storage`，与助手服务的提供方式相同）：

```
<octosense home>/apps/<app id>/            应用的 jail（App Hub 的 jail 根目录；原生应用的沙箱根目录）
    accounts/<account hash>/               每个账号一个（应用没有账号时为 "device"）：
                                            该账号的数据 = 该账号 Agent 的工作区
    common/                                与账号无关的应用数据
    cache/                                 可清除，不备份
<octosense home>/secrets/<app id>/         宿主拥有：token、密钥、密码、加密存储
```

- **OctoSense 主目录**在手机上是平台的应用数据目录，否则为 `~/.octosense`（`OCTOSENSE_HOME` 可覆盖；`crates/shell/src/octosense/paths.rs`）。应用根目录可用 `OCTOSENSE_APP_DATA` 移动；机密根目录始终是 `<home>/secrets`，两者从不重叠。目录权限为 0700，拒绝路径中的符号链接。
- **账号 hash** 是对规范化账号 id 做带域分隔的 SHA-256 后取 128 位（`account_hash`）。
- **按 ADR，Agent 的工作区就是账号目录。** 目前目录已存在，模块可以通过 `AppStorage::agent_workspace` 获得它，但 broker 还没有把它作为 peer 的 `cwd` 传给 `peer/prepare`：peer 在内核分配的工作区中工作（规划中，步骤 10）。
- **机密**（`secrets.rs`，`AppStorage::secrets`）：macOS 和 iOS 上用钥匙串（每个 profile、应用和键一项），其他平台是 `secrets/<app id>/` 中每个键一个仅所有者可读（0600）的明文文件；测试、无界面运行和 `OCTOSENSE_SECRETS=file` 也使用文件存储。Windows、Linux 和 Android 的系统密钥库是待办项。机密从不放在 `apps/` 下。
- **启动检查**（`check.rs`）：任何 Agent 工作区都不能包含或通向 `secrets/`（工作区或 jail 本身是符号链接、指向机密的符号链接或硬链接、机密根目录位于其中）。被标记的工作区会被拒绝（`agent_workspace` 对该账号或所有账号返回 `Refused`），直到之后某次启动时检查通过；启动照常进行，不删除任何内容。
- **登出**会暂停该账号的 Agent，而不是关闭它：`Storage::sign_out` 让工作区返回 `SignedOut`。Shell 还没有账号事件（模块通过 `OctosAppService::set_account` 绑定账号）；`Storage::sign_out` 标记的状态由 broker 和中继遵守：该账号的工具调用返回 `signed_out`，不启动 `peer/input` 回合，已暂停账号的 peer 不会被准备。
- **内核自己的数据**是分开的：core 目录 `<数据目录>/octos-home/.octos`（桌面端为 `~/.octosense/octos-home/.octos`，`OCTOS_APP_CORE_DIR` 可覆盖；`crates/kernel/src/dirs.rs`），存放 `_main` profile、会话、黑板和记忆命名空间。提供方密钥归 `llm` 服务管理（macOS 钥匙串，其他平台为仅所有者可读的文件；见 [ai-services.zh-CN.md](ai-services.zh-CN.md#ai-providers-与-llm-宿主服务)）。peer 的宿主 token 在 `<core 目录>/../app-peers`（0600）。
- **脚本应用**保留 App Hub 的 jail 和配额；它们的机密由宿主服务和宿主面板保管（“机密归宿主所有”，AGENTS.md 规则 3）。在数据迁入这一布局之前，Rinx 仍使用自己的数据目录（[hagency-org/Rinx#37](https://github.com/hagency-org/Rinx/issues/37)）；显式设置了 `OCTOSENSE_HOME` 时，Shell 把它指向 `<home>/apps/rinx/data`（`RINX_DATA_DIR`，在 `crates/shell/src/octosense/paths.rs` 中设置；显式设置的 `RINX_DATA_DIR` 优先）。

## 7. 信任边界与隔离

```
 用户 ── 宿主面板（密钥、PIN、审批）──┐
                                      v
 +------------------------ Shell 进程（可信）-----------------------------+
 |  持有：宿主 token、peer 宿主 token、提供方密钥（经 llm）、机密          |
 |  每次调用都检查：授权、同意、审批、审计                                 |
 |   +------------------+   +------------------------------------------+  |
 |   | 原生模块         |   | Card runner：脚本应用各在隔离环境中      |  |
 |   | 经评审，共享     |   | （jail、配额、按授权 host.request）      |  |
 |   | 内存：受信任     |   +------------------------------------------+  |
 |   +------------------+                                                 |
 +-------|------------------------------------------|---------------------+
         | hub（回环）                              | OUP，宿主 token
 +-------v----------+                      +--------v--------------------+
 | 进程应用         |                      | octos 内核                  |
 | （操作系统沙箱： |                      |  每个 peer 的工作区围栏     |
 |  规划中）        |                      |  外部客户端：允许列表       |
 +------------------+                      +-----------------------------+
```

| 边界 | 由什么保证 | 状态 |
| --- | --- | --- |
| 脚本应用 ↔ Shell | App Hub 的嵌套隔离环境：没有 `mod.res` / `mod.run`，有 jail 和配额，只能对获授权的服务族使用 `host.request`；密码输入框失效；机密在宿主面板上输入 | 已在 main |
| 原生模块 ↔ Shell | 内存上没有边界：只接受经评审的第一方代码（`native-apps.json`）；模块边界上的 **panic 隔离**（`catch_unwind`，[#114](https://github.com/OctoSense-org/OctoSense/pull/114)）；局限：展开过程中再次 panic、`panic = "abort"`、FFI | 已在 main |
| 进程应用 ↔ Shell | 独立的地址空间；依据清单的 `sandbox` 和 `storage` 生成的**操作系统沙箱**（macOS 沙箱配置、Linux Landlock 和 seccomp、Windows AppContainer） | 沙箱规划中（ADR 0004 §3，步骤 8）；目前进程应用以用户权限运行 |
| 应用 ↔ 内核 | 应用从不使用内核协议，**永远看不到宿主 token**：模块得到受限的 `OctosAppService`，脚本应用得到宿主服务，进程应用（规划中）使用 peer link | 已在 main |
| peer ↔ peer | octos：独立的工作区（拒绝重叠的工作区）、记忆命名空间、对话记录；请求上下文被限定在 `contexts/<id>/` 内 | 已在 main（octos UPCR-2026-034） |
| Agent ↔ 机密 | 机密位于所有 jail 和工作区之外；启动检查 | 已在 main |
| 外部客户端 ↔ 内核 | 外部 token、方法和工具允许列表、`Host` 和 origin 检查、无法访问 peer | 已在 main（[ADR 0003](adr/0003-shared-octos-client-access.md)） |

**Shell 在每次调用时检查什么**（ADR 0004 §3）：授权和同意；预算、速率限制和后台策略；每次工具调用的名称和参数是否符合声明的 `tools.json`；每个结果是否符合其 schema 和大小；自己的审计；崩溃清理。目前 main 上已有：同意；按精确名称授予 `octos.*` 服务（声明 ∩ 支持 ∩ 策略）；脚本应用的参数规则和大小上限（文字最多 32 KiB，回复最多 2 MiB）；总线调用的审批路由和审计。对应用工具调用（`crates/shell/src/host_tools/`）：按（所属应用，工具）和调用方的授权、同意、已登出的账号、确认、每次出现只执行一次、取消后不再执行。参数和结果的 schema 检查以及 octos 之外的预算尚未实现。用户在应用卡片中的操作不经过这个中继：它们走应用自己的路径（ADR 0004 §4），因此这些检查从不覆盖它们。

**谁都无法检查的**：原生应用的代码在它自己的工具里做了什么，或它为什么发起一个回合。控制手段是评审，以及进程应用的沙箱。

## 8. 完整示例：用邮件发送会议邀请

“让系统 Agent 发一封会议邀请邮件。”这是 ADR 0004 中的 Calendar 与 Mail 示例。**其中几乎全部都在规划中**：`main` 上没有 Calendar 应用，Mail 还没有声明 `tools.json` 或 `octos.*`，内核一侧（`peer/input`、宿主注册的工具）是 octos#2567。状态一栏说明哪些部分已经存在。

```mermaid
sequenceDiagram
  actor P as 用户
  participant S as 系统 Agent
  participant SH as Shell（宿主连接）
  participant C as Calendar Agent（peer）
  participant R as 审批路由
  participant M as Mail（宿主服务）
  P->>S: "邀请 Ana、Bo 和 Edward 周二下午 3 点开会"
  S->>C: peer_send_input（任务说明）
  Note over S,SH: octos 把 peer/input 送到宿主
  SH->>C: turn/start（应用的工具、记忆、上下文）
  C->>SH: peer/tool/call calendar.create_event
  SH-->>C: 结果
  C->>SH: peer/tool/call mail.send x3（调用方：Calendar）
  SH->>SH: 授权检查：Calendar 是否获授权 mail.send？
  SH->>R: 审批（Mail、mail.send、确切参数、调用方 Calendar）
  alt 针对（Mail, mail.send）的常设规则匹配
    R-->>SH: 批准（通知并审计）
  else 没有规则
    R->>P: 一个合并面板，列出三封邀请
    P-->>R: 批准
  end
  SH->>M: 执行 mail.send x3
  M-->>SH: 结果
  SH-->>C: peer/tool/result
  C->>C: octos 写入 peers/(slug)/result.md
  S->>S: peer_gather 读取结果
  S->>P: "已预订周二下午 3 点；已向 3 人发送邀请"
```

| 步骤 | 发生什么 | 状态 |
| --- | --- | --- |
| 1 | 用户在系统对话 `_main:api:octosense#system` 中向系统 Agent 提出请求。 | 目前只能从已配对的 Talk to Octos 客户端访问；Shell 还没有自己的系统 Agent 对话界面（桌面端的 AI 面板是总线上的 Makepad `aichat`，不是系统 Agent） |
| 2 | 系统 Agent 做计划。有歧义就提问，不去猜（“两个 Edward？”）；有限的读取可以直接调用获授权的工具。需要 Calendar 自己判断的工作用 `peer_send_input` 交给 Calendar 的 Agent。 | `peer_send_input` 已在 main；直接的跨应用授权规划中（步骤 6） |
| 3 | octos 把输入以 `peer/input` 送到 Shell；Shell 在 Calendar 的 peer 上用 `turn/start` 启动回合，使其带有 Calendar 的工具、记忆和账号上下文。如果 Calendar 的 peer 没有宿主连接，系统 Agent 会被告知该应用未连接。 | 已在 main（锁定版本含 [octos#2567](https://github.com/octos-org/octos/pull/2567)；由 broker 启动回合） |
| 4 | Calendar 的 Agent 调用 `calendar.create_event`（自己的工具），并为每位受邀者调用一次 Mail 可共享的 `mail.send`。每次调用都以 `peer/tool/call` 到达 Shell；Shell 盖上调用方（Calendar 的 Agent）、账号和上下文。 | 中继已在 main（`host_tools`）；Calendar 和 Mail 的工具规划中 |
| 5 | Shell 检查 Calendar 的清单是否获授权 `mail.send`（脚本应用在安装时授权）；不需要第二道 Agent 级别的同意。 | 检查已在 main（`relay::Catalog`）；安装时授权规划中（步骤 6） |
| 6 | `mail.send` 是对外的 `confirm: host` 工具，由审批路由处理：开发者模式未开启；不是 `confirm: app`；不是 `auto_approvable: false`；如果有针对（Mail，`mail.send`）的常设规则（例如“发给我的联系人”），就由规则批准（通知并审计；只有用户打开了“在审批规则中使用我的联系人”，联系人条件才会成立），否则一个合并的 Shell 面板列出每封邀请：所属应用 Mail、工具 `mail.send`、调用方应用 Calendar 以及确切参数。 | 路由、规则、面板和审计已在 main（[#120](https://github.com/OctoSense-org/OctoSense/pull/120)）；经中继由内核的 `host_tool` 审批提交请求 |
| 7 | 批准后，Shell 把每次调用交给 Mail 的宿主服务，它用用户在 Mail 宿主面板上登录的账号发送（密码永远不会到达 Agent），并把结果以 `peer/tool/result` 返回内核。 | Mail 的宿主服务已在 main（`apps/mail/host-service`）；把它的工具作为 Agent 工具还在规划中 |
| 8 | Calendar 的回合结束，octos 写入 `peers/<slug>/result.md` 和 `turns.txt`；系统 Agent 用 `peer_gather` 读取。失败的邀请会被点名，结果未知的邀请未经用户同意绝不重试。 | 黑板已在 main（octos） |
| 9 | 系统 Agent 宣布“已预订周二下午 3 点；已向 3 人发送邀请”。Calendar 和 Mail 自己的界面会显示变化，因为它们的数据变了。 | 规划中 |

## 代码与 ADR 不一致之处

撰写本文时（2026-09-28）发现；本次改动只涉及文档，这里都没有修复。

1. **Terminal 之外的进程应用。** 已修复：可选的 Sheets 和 Reference 在所有目标上都是 `module`，生成器拒绝 Linux 上的普通 `process`（ADR 0004 §2）。
2. **崩溃进程应用的重启。** ADR 0004 §2 说它的磁贴会显示已关闭并提供重启。Shell 实际上移除客户端并发出 “App stopped” 通知；Restart 界面只用于进程内模块（`module_view.rs`）。
3. **Agent 工作区 = 账号目录。** 部分修复：自锁定 octos 665209e5 起新建的 peer 以账号目录作为 `peer/prepare` 的 `cwd`；之前创建的 peer 保留内核分配的工作区，因为 octos 只允许在创建时的工作区下恢复 peer（迁移需要 octos 尚未提供的支持）。账号目录名（`account_hash`，SHA-256）和记忆命名空间标签（`broker.rs` 中的 FNV-1a）仍是对账号的两种不同 hash；改动标签会让每个应用的记忆重新计算键。
4. **ADR 0003 的 “What the profile runs”** 说 OctoSense 既不配置工具集也不配置沙箱。自 [#117](https://github.com/OctoSense-org/OctoSense/pull/117) 起，Shell 每次启动前都向 `_main` profile 写入拒绝 `group:runtime` 的 `tool_policy`，因此宿主自己的回合也没有 octos shell。
5. **过时的背景描述。** 已修复：ADR 0004 的背景表格和各 README 的目录结构表不再列出 ADR 0004 步骤 1 删除的原生 News、Maps 和 Photos 模块（#113）。[ai-services.zh-CN.md](ai-services.zh-CN.md) 现已描述 2026-09-28 的 `main`（#106 的 Card runner `octos` 服务、#120 的同意机制；`Policy::contained_apps` 开关默认仍关闭）。
6. **“在 Settings 中开启”命令执行。** 已修复：Setup › Assistant › Command execution 设置它（#132），开启期间系统对话把 `terminal.run` 注册到系统会话。
7. **`host::processes_available()` 的测试**只检查 `wasm32`，而函数本身还排除了原生移动平台。
8. **审批，ADR 0004 §8。** 每次应用工具调用都通过 `peer/tool/call` 到达 Shell，每个有门控的调用都到达路由（见上文）。还没有应用注册自己的 `confirm: app` 面板（Rinx 需要通过 `OctosAppService::set_confirm_sheet` 交出它的发送面板），因此这类调用会等待后被拒绝。审计记录的是参数摘要而不是参数。发送队列和撤销窗口尚未实现。
9. **存储，ADR 0004 §11。** 机密只在 macOS 和 iOS 上使用系统钥匙串（其他平台为 0600 明文文件）。启动检查拒绝通过链接或包含关系通向机密的工作区，而不是查找 `secrets/` 路径，并且不会中止启动。（已修复：app storage 和同意面板中 `storage.accounts` 都默认为 `false`，同意面板现在读取 `StorageSpec`。）
10. **开发者模式，ADR 0004 §13。** `dev.run` 尚未注册；Settings 只能为所有应用开启（选定应用只能通过 `OCTOSENSE_DEV_MODE`）；手机上没有开启手势；进程内模块仍会显示自己的确认面板。
11. **应用 Agent，来自 2026-09-29 的代码审查**（octos acffad3b、`main` ecb3583；列在 ADR 0004 的后续事项中）：`peer/input` 回合上内核工具的审批被 broker 对 peer 会话的事件过滤丢弃（`broker.rs`），无人能回答；受限脚本应用的卡片没有流式输出、所有审批都被拒绝、每个应用共用一个上下文并使用固定账号 `device`（`crates/ai-host/src/contained.rs`）；卡片没有回答 `user_question/requested` 的操作（进行中）；`peer/input` 的宿主 `turn/start` 失败只记日志；`peer/input` 路由归最后注册该 peer 工具的连接所有（多实例未测试）。

## 源码位置

| 内容 | 位置 |
| --- | --- |
| 原生应用清单、生成器、生成的表 | [`native-apps.json`](../native-apps.json)、[`tools/native_apps.py`](../tools/native_apps.py)、[`crates/shell/src/native_apps.rs`](../crates/shell/src/native_apps.rs) |
| 托管决策、宿主服务 | [`crates/shell/src/apps.rs`](../crates/shell/src/apps.rs)（`AppRegistry::hosting`、`register_host_services`） |
| 进程应用、hub | [`crates/shell/src/clients.rs`](../crates/shell/src/clients.rs)、[`crates/shell/src/hub.rs`](../crates/shell/src/hub.rs)、[`crates/shell/src/host.rs`](../crates/shell/src/host.rs) |
| 进程内模块、panic 隔离 | [`crates/shell/src/module_host.rs`](../crates/shell/src/module_host.rs)、[`crates/shell/src/module_panic_tests.rs`](../crates/shell/src/module_panic_tests.rs) |
| AI 服务总线（Shell 一侧） | [`crates/shell/src/ai_bus.rs`](../crates/shell/src/ai_bus.rs) |
| 审批、同意、审计、Settings 页面 | [`crates/shell/src/approvals/`](../crates/shell/src/approvals/mod.rs) |
| 开发者模式 | [`crates/shell/src/dev_mode.rs`](../crates/shell/src/dev_mode.rs) |
| 应用存储、机密、启动检查 | [`crates/shell/src/app_storage/`](../crates/shell/src/app_storage/mod.rs) |
| 内核服务：启动、目录、Talk to Octos、帧路由、系统 Agent 工具 | [`crates/kernel/src/`](../crates/kernel/README.zh-CN.md)（`launch.rs`、`dirs.rs`、`network.rs`、`router.rs`、`system_tools.rs`） |
| Shell 的 AI 入口、策略、提供服务；脚本应用的 `octos` 服务 | [`crates/ai-host/src/lib.rs`](../crates/ai-host/src/lib.rs)、[`crates/ai-host/src/contained.rs`](../crates/ai-host/src/contained.rs) |
| 应用 peer：broker、托管启动、存储约定、注入 | [`crates/app-peers/src/`](../crates/app-peers/README.md)（`broker.rs`、`hosted.rs`、`storage.rs`、`injection.rs`、`host_approvals.rs`） |
| 各 Shell 的脚本系统应用 | [`desktop/system-apps.json`](../desktop/system-apps.json)、[`phone/system-apps.json`](../phone/system-apps.json) |
| octos：宿主管理的 serve、UPCR、peer 工具 | [`docs/HOST_MANAGED_SERVE.md`](https://github.com/octos-org/octos/blob/main/docs/HOST_MANAGED_SERVE.md)、[UPCR-2026-034](https://github.com/octos-org/octos/blob/main/docs/OCTOS_UI_PROTOCOL_CHANGE_REQUEST_UPCR_2026_034_HOST_APP_PEERS.md)、[UPCR-2026-035](https://github.com/octos-org/octos/blob/main/docs/OCTOS_UI_PROTOCOL_CHANGE_REQUEST_UPCR_2026_035_PEER_HOST_TOOLS.md)（[octos#2567](https://github.com/octos-org/octos/pull/2567)）、[UPCR-2026-036](https://github.com/octos-org/octos/blob/main/docs/OCTOS_UI_PROTOCOL_CHANGE_REQUEST_UPCR_2026_036_HOST_MANAGED_SERVE.md)、[`crates/octos-agent/src/tools/`](https://github.com/octos-org/octos/tree/main/crates/octos-agent/src/tools) |
