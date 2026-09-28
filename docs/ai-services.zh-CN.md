# OctoSense 中的 AI 服务（octos）

[English](ai-services.md) | 简体中文

助手如何接入 OctoSense：Shell 运行的 [octos](https://github.com/octos-org/octos) Agent 内核、用户在哪里配置它、哪些应用可以使用以及如何使用，还有哪些在规划中。本文描述 2026-09-27 的 `main`（OctoSense `405139f`，它锁定 octos `a6ea8505` 和 OctoSense-App-Hub `46d67e51`）。标为**目前可用**的内容都在这份代码中读过；**进行中**表示有未合并的 pull request；**规划中**表示只有 ADR 或未关闭的 issue，尚无代码合入。

本文讨论的是 OctoSense *内部*的助手。开发应用不需要任何 AI 服务，也不需要特定的编程 Agent：应用开发工具集 [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) 适用于任何 Agent，也可以不用 Agent。它的 [AI-SERVICES](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/AI-SERVICES.zh-CN.md) 页面是本文面向应用开发者的简短版本。

它所处的整个系统（各平台的进程、Agent、协议、工具与授权、审批、存储和信任边界）见 [OctoSense 架构](architecture.zh-CN.md)。

## 目录

- [概览](#概览)
- [架构](#架构)
- [信任模型](#信任模型)
- [各类应用目前能用什么](#各类应用目前能用什么)
- [规划中：事件驱动的应用 Agent（ADR 0002）](#规划中事件驱动的应用-agentadr-0002)
- [本地运行与测试](#本地运行与测试)
- [源码位置](#源码位置)

## 概览

| 组成部分 | 状态 | 位置 |
| --- | --- | --- |
| 每个 Shell 进程一个 octos 内核，首次使用时启动，提供方变更后重启 | 目前可用（设置了 `OCTOS_APP_CORE_BIN` 的桌面端、Android、OpenHarmony；iOS 没有） | [`crates/kernel`](../crates/kernel/README.md) |
| AI providers：用户的模型提供方和密钥，密钥只在宿主面板上输入 | 目前可用 | [`apps/ai-providers`](../apps/ai-providers/host-service/README.md) |
| Shell 的 AI 入口（`start`、策略、按实例提供服务、QR 导入） | 目前可用 | [`crates/ai-host`](../crates/ai-host/README.md) |
| 宿主拥有的应用 peer：每个获授权的原生应用一个 octos peer，归系统 Agent 所有 | 目前可用，仅限原生模块（Rinx） | [`crates/app-peers`](../crates/app-peers/README.md)、[Rinx ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md) |
| 在 Shell 内核上运行的 AppCard（"Ask anything"） | 目前可用，需主动开启（`--features app-appcard`），不随产品发布 | [`apps/appcard`](../apps/appcard) |
| 隔离运行的脚本应用（系统应用或商店应用）向助手提问 | 在托管了内核的 Shell 中可用：`octos` 宿主服务为每个应用分配自己的 peer（`card.<应用 id>`）；在 Card runner 有审批面板之前，工具审批一律被拒绝。`llm` 仍是只供 `os.*` 应用使用的提供方管理服务 | [见下文](#隔离运行的脚本应用系统应用和商店应用) |
| 供隔离应用使用的一次性模型调用（`model`，`model.complete`） | 即将到来：权限已合入 App Hub（[App-Hub#24](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/24)）；OctoSense 的 `model` 服务尚未实现 | [见下文](#隔离运行的脚本应用系统应用和商店应用) |
| News 数据服务（`news`，不使用模型） | 已合入（[#69](https://github.com/OctoSense-org/OctoSense/pull/69)）；只响应 `os.*` 应用 | [`apps/news/host-service`](../apps/news/host-service/README.md) |
| `glance.publish`：glance 屏幕上的 L0 卡片 | 已合入（[#72](https://github.com/OctoSense-org/OctoSense/pull/72)）；隔离运行的应用暂时无法调用。进行中：`glance` 权限（[#86](https://github.com/OctoSense-org/OctoSense/pull/86)）和 `sys.digest` 数据源（[#87](https://github.com/OctoSense-org/OctoSense/pull/87)） | [`crates/shell/src/glance.rs`](../crates/shell/src/glance.rs) |
| 应用自己的 Agent：`tools.json`、`AGENT.md`、skills、模型需求、触发器 | 规划中（[ADR 0002](adr/0002-event-driven-app-agents.md)，Proposed）。App Hub 接受这些文件（[App-Hub#18](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/18)）；没有 Shell 运行它们；内核一侧尚未合并（[octos#2567](https://github.com/octos-org/octos/pull/2567)） | [见下文](#规划中事件驱动的应用-agentadr-0002) |

## 架构

```mermaid
flowchart TB
  person(["用户"])
  subgraph shell["一个 OctoSense Shell 进程：桌面端，或手机上的 Home"]
    providers["AI providers 应用（os.ai-providers）<br/>隔离运行的脚本应用"]
    sheet["宿主面板<br/>密钥、QR、PIN"]
    llm["llm 宿主服务<br/>写入提供方配置"]
    aihost["crates/ai-host<br/>start、策略、offer"]
    broker["crates/app-peers broker<br/>每个获授权的原生应用一个 peer"]
    runner["Card runner（App Hub）<br/>宿主服务：mail、news、glance、llm、octos"]
    rinx["Rinx（原生模块）"]
    appcard["AppCard（原生，需主动开启）"]
    scripts["脚本应用（系统和商店）"]
  end
  vault[("平台密钥存储<br/>或仅所有者可读的配置")]
  kernel[("octos 内核，每个 Shell 一个<br/>配置 _main，系统 Agent 会话")]
  person --> providers
  person --> sheet
  providers -- "llm.*" --> llm
  sheet -- "llm.sheet.*" --> llm
  llm --> vault
  llm -- "变更后重启" --> kernel
  aihost --> kernel
  aihost --> broker
  broker -- "peer/prepare、peer/context/open、turn/start" --> kernel
  rinx -- "受限的 OctosAppService" --> broker
  appcard -- "自己的连接" --> kernel
  scripts -- "host.request" --> runner
  runner -. "没有 octos.* 服务" .- kernel
```

### octos 内核：每个 Shell 一个

内核是一个 **Shell 服务**（[`crates/kernel`](../crates/kernel/README.md)，包名 `octosense-kernel`），不是应用。Shell 在启动时配置一次；在第一个使用者调用 `octosense_kernel::connect()` 之前什么都不运行，之后的使用者共享同一个进程。octos 对它的数据目录持有单写者锁，所以每个 core 目录只有一个内核。最后一个连接关闭时内核停止；`shutdown()` 随 Shell 一起停止它（最多 5 秒）。

各平台的运行方式（`crates/kernel/src/launch.rs`，`crates/ai-host` 中的 `KernelSource::platform()`）：

| 平台 | 内核 | Core 目录（octos home） |
| --- | --- | --- |
| 桌面端（macOS；Windows 和 Linux 未测试） | `$OCTOS_APP_CORE_BIN serve --stdio --data-dir <core 目录>`（存在 `<core 目录>/config.json` 时再加 `--config`）。**没有 `OCTOS_APP_CORE_BIN` 就没有内核**，开发者自己运行的 `octos serve` 永远不会被动到。把内核打包在桌面端二进制旁边的工作正在进行（[#85](https://github.com/OctoSense-org/OctoSense/pull/85)）。 | `$OCTOS_APP_CORE_DIR`，否则为 `<OctoSense 状态目录>/octos-home/.octos`（`~/.octosense/octos-home/.octos`）：OctoSense 自己的目录，不再是用户的 `~/octos-home/.octos`（只从中复制一次提供商设置） |
| Android（Home） | APK 中的 `liboctos.so serve --stdio`，由 [`tools/kernel-artifact.py`](../tools/kernel-artifact.py) 按根 `Cargo.toml` 锁定的 octos 版本构建 | `<应用数据目录>/octos-home/.octos` |
| OpenHarmony | 进程内运行（`octos_cli::embedded::serve_io`），因为 HAP 不能 exec | `<应用数据目录>/octos-home/.octos` |
| iOS | **没有。** 提供方仍会保存；没有应用能获得助手 | – |

使用者通过 `Connection` 使用 octos UI Protocol（JSON-RPC 帧，与 `octos serve --stdio` 相同）。每个使用者只收到自己请求的回复和自己会话的通知。提供方变更时内核重启，所有连接以 `CloseReason::Restarted` 结束；使用者重新连接并重新打开会话（AppCard 的传输层是参考实现）。

### AI providers 与 `llm` 宿主服务

**AI providers**（`os.ai-providers`，[`apps/ai-providers/bundle`](../apps/ai-providers/bundle)）是用户选择助手所用模型的地方：一个主模型和若干备用模型，都来自 octos 的模型目录，可以测试连接，并可通过有 PIN 保护的 `OCTOS1E` QR 码在设备之间迁移。桌面端从 **Start → Settings → AI providers** 打开，手机上从 **OctoSense Settings → Accounts → AI providers** 打开。

它和其他应用一样是隔离运行的脚本应用；有特权的一半是 **`llm` 宿主服务**（[`apps/ai-providers/host-service`](../apps/ai-providers/host-service/README.md)）：

- 它写入内核的配置 `<core 目录>/profiles/_main.json`（`config.llm` 和 `config.env_vars` 中的密钥变量），然后重启内核。
- 密钥存入 macOS 登录钥匙串（服务名 `octos`）；Linux 上存入 `<core 目录>/secrets/<ENV>`（0600）；Android 和 iOS 上存入应用私有的配置文件（0600），因为 octos 在那里没有密钥存储。开发时可用 `OCTOSENSE_LLM_VAULT=file` 把密钥留在配置文件中。
- 密钥、PIN 和 QR 码只在服务自己的**宿主面板**上输入、绘制和扫描。只有面板能调用 `llm.sheet.*`；应用只看到掩码后的状态（`"set ••••1234"`、`"missing"`）。
- 它**只服务 `os.*` 应用**（`"llm is for OctoSense's own apps."`）。它管理提供方；没有任何向模型发送提示词的方法。

### `crates/ai-host`：Shell 的统一入口

[`crates/ai-host`](../crates/ai-host/README.md)（`octosense-ai-host`）是两个 Shell 都调用的入口：启动时调用 `start(Host::platform(data_dir))`（配置内核、安装宿主策略、注册带平台 QR 导入能力的 `llm`），每个事件调用 `handle_event`，在原生模块 `create` 前后调用 `offer(module, scope)` / `finish()`，退出时调用 `shutdown()`。

宿主**策略**按精确的 `octos.*` 服务名决定哪些原生模块可以使用助手。`Policy::shipped()` 只把这四个服务授予 `rinx`；用户目前还没有按应用的开关。

### `crates/app-peers`：宿主拥有的应用 peer

[`crates/app-peers`](../crates/app-peers/README.md) 是应用和内核之间的中介（Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md)；内核一侧是 octos UPCR-2026-034）：

- 原生模块声明的 `octos.*` 服务获得策略授权后，该应用会在 Shell 的内核上得到**一个 octos peer**，通过 `peer/prepare` 创建或恢复。没有获得任何授权的模块不会分配 peer。
- peer 的**所有者**是 Shell 的系统 Agent 会话 `_main:api:octosense#system`。内核为 peer 生成宿主 token；Shell 把它保存在 `<core 目录>/../app-peers`（0600），任何应用都无法访问。
- peer 的**记忆命名空间**是 `app/<app>/acct-<hash>`：按应用和应用账号区分（hash 是账号 id 的非机密标签）。内核为它提供工作区。不支持这一约定的内核会被拒绝，绝不会退回到使用配置共享记忆的普通会话。
- 应用永远看不到内核协议。它为正在创建的实例拿到一个受限的 `OctosAppService`，绑定已登录的账号，并为每个客户端实例打开一个**请求上下文**（`peer/context/open`）。切换账号会撤销旧账号的所有上下文；迟到的事件会被丢弃。
- 操作包括 `Open`、`History`、`Turn { text }`、`Interrupt` 和 `Approval { id, approve }`，每个都受对应的精确服务名约束（`octos.session.open`、`octos.session.history`、`octos.turn.start`、`octos.turn.interrupt`；审批需要 `octos.turn.start`）。一个回合 180 秒后超时。

### 系统 Agent

目前系统 Agent 是所有应用 peer 的**所有者**：创建它们并能与之通信的会话。它不持有任何应用的工具，也无法回答应用 peer 的审批（octos [#2560](https://github.com/octos-org/octos/pull/2560)，已包含在锁定的 `a6ea8505` 中）。它在 ADR 0002 中更大的角色（监督应用 Agent、预算、紧急停止、对 glance 屏幕排序、改进应用 Agent）还在**规划中**；glance 屏幕目前按优先级和时间排序卡片。

### Shell 中的其他助手

- **AppCard**（[`apps/appcard`](../apps/appcard)），即 "Ask anything" 助手，是一个使用自己的内核连接和会话的原生模块。它需要主动开启（`--features app-appcard`），不随产品发布。
- 桌面端的 **AI 面板**承载 Makepad 自己的 `aichat` 应用，它通过窗口管理器的 AI 服务总线（`crates/shell/src/ai_bus.rs`）调用应用的类型化工具。Rinx 的助手工具目前就是这样被调用的（见下文）。它与 octos 应用 peer 是分开的。

## 信任模型

| 规则 | 目前如何保证 |
| --- | --- |
| **密钥留在宿主。** | 只有 `llm` 服务读写密钥；密钥保存在平台密钥存储或内核 core 目录下仅所有者可读的配置中，从不进入应用的 jail。app-peers 约定中不携带任何凭据（`ModelInfo` 中没有）。没有任何 `octos.*` 服务允许应用选择提供方或提交密钥。 |
| **机密归宿主所有。** | 应用不收集密码、PIN、密钥或一次性验证码，即使只是转交也不行。运行时让受管控隔离环境中的密码输入框失效，App Hub 的准入检查拒绝声明了此类输入框的应用包，服务在自己的面板上询问（`<family>.sheet.*` 只接受来自面板的调用）。 |
| **审批属于用户，在所属应用中进行。** | 应用上下文中发起的工具审批只能由该应用自己的界面通过 `ContextOp::Approval` 回答；系统 Agent 从不替应用审批（app-peers 策略，octos #2560）。Rinx 的 `send_message` 会在 Rinx 自己的面板上显示房间和确切文字，只有用户确认才发送。 |
| **系统 Agent 通过应用的 Agent 驱动应用。** | 它拥有 peer，但不持有应用工具；应用的工具在应用中运行（Rinx：`src/assistant`，“系统 Agent 自己从不持有这些工具，而是请求 Rinx 的 peer”）。 |
| **记忆按应用和账号私有。** | 每个 peer 的记忆命名空间是 `app/<app>/acct-<hash>`；一个账号的上下文从不在另一个账号下恢复。提升到共享记忆还在规划中（ADR 0002 §9）。 |
| **最小权限，按精确名称。** | 应用得到的是 `声明 ∩ 支持 ∩ 宿主策略` 的服务，按精确名称比较：`octos.` 或 `octos.admin` 不授予任何权限。 |

## 各类应用目前能用什么

### 原生模块（Rinx 这类）

原生模块是链接进 Shell 的受信任 Rust 代码。**Rinx**（[hagency-org/Rinx](https://github.com/hagency-org/Rinx)）是参考实现，也是 `Policy::shipped()` 下唯一获得助手授权的模块。

1. 模块在 `capabilities()` 中列出精确的服务名，例如 `octosense_app_peers::OCTOS_SERVICES` 中的全部四个。
2. Shell 在 `module.create` 之前调用 `ai_host::offer(module, &scope)`，之后调用 `offer.finish()`；返回的 `Assistant` 与实例同生命周期，被丢弃时释放。
3. 在 `create` 中，模块领取自己的服务；`None` 表示以无助手的方式托管，模块不得退回到自己启动的内核：

   ```rust
   // Option<Arc<dyn OctosAppService>>；None：此实例没有助手。
   let Some(service) = octosense_app_peers::injection::claim("rinx", &handles.scope.to_string()) else { return };
   service.set_account(Some(&user_id));
   let ctx = service.open_context(ContextSpec { account, instance, services })?;
   ctx.call(ContextOp::Turn { text }, sink)?;   // Data(..)*，然后 Complete(..)
   ctx.close();                                  // 实例关闭
   service.release();                            // 应用关闭
   ```

4. `availability()` 报告 `Unavailable`（没有内核、未授权、未登录）、`Idle`、`Ready` 或 `Failed`；在任何状态下应用的普通界面都照常工作。`settings_entry()` 为 `Host`：应用不提供自己的提供方表单，而是引导用户去 AI providers。

**工具。** Rinx 只定义一次它的助手工具（`status`、`list_rooms`、`open_room`、`draft_message`、`read_room`、`open_mini_app`、`send_message`；`src/assistant/mod.rs`）。目前 Shell 通过 AI 服务总线上 Rinx 的 `ServiceExecutor` 调用它们。`read_room` 会在 Rinx 的读取面板上请求按房间的授权；`send_message` 是 Destructive 且自行确认：Rinx 的发送面板就是唯一的一次确认。把这些工具注册到 Rinx 的 octos peer、让 peer 自己的模型能调用它们，要等内核支持宿主注册工具（[octos#2567](https://github.com/octos-org/octos/pull/2567)，尚未合并）。

**Rinx 迷你应用。** Rinx 托管经过审核的 OctoScript 迷你应用，并向它们提供同样的四个 `octos.*` 服务，每个运行中的实例使用 Rinx peer 的一个独立请求上下文（[示例](https://github.com/hagency-org/Rinx/tree/main/examples/miniapps/matrix-octos-script)）。这是 Rinx 自己的迷你应用宿主，适用于用户审核后导入 Rinx 的应用包；它不是 App Hub 的安装路径。

### 隔离运行的脚本应用（系统应用和商店应用）

脚本应用在 App Hub 的 Card runner 中运行，只能通过 `host.request("<family>.<method>", args, fn(r){…})` 访问 Shell，而且 family 必须是 manifest 已获授予的权限。Shell 目前注册的服务（`crates/shell/src/apps.rs`、`crates/ai-host`）：

| Family | 谁可以调用 | 是什么 |
| --- | --- | --- |
| `mail` | 获得 `mail` 授权的任何应用 | 用户在宿主面板上登录的邮件账号 |
| `llm` | 仅 `os.*` 应用 | 管理助手的提供方（`llm.providers`、`llm.add_provider`、`llm.test`、`llm.import_qr` 等）；**没有提示词或补全方法** |
| `news` | 仅 `os.*` 应用 | News 的数据服务（订阅源、已读记录、`news.list`、`news.read` 等），不使用模型。Shell 锁定的 App Hub 还没有 `news` 权限，所以 News 应用包仍自己抓取 |
| `glance` | 仅 `os.*` 应用，并且在 Shell 锁定的 App Hub 下没有 manifest 能申请 `glance` | 向 glance 屏幕发布 L0 卡片。目前只有 Shell 的 `OCTOSENSE_GLANCE_DEMO=1` 演示卡片会用到它；权限见 [#86](https://github.com/OctoSense-org/OctoSense/pull/86) |
| `model` | – | **尚未注册。** App Hub `main` 接受 `model` 权限（[App-Hub#24](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/24)），用于一次性的 `model.complete {task, input, schema, class}`（`class` 为 `fast` 或 `strong`；宿主从用户的提供方中挑选模型，按 schema 校验回复，按应用管理每日预算；没有工具和记忆）。OctoSense 服务尚未实现；调用返回 `no service answers "model" on this device`（在 App Hub `e8601b8` 的 `card-host` 中运行），而且 Shell 锁定的 App Hub 不认识这个名称 |
| `octos` | manifest 声明了确切 `octos.*` 名称的任何应用；前提是 Shell 托管了内核，且 `Policy::contained_apps` 为开（在首次使用同意功能落地前，出厂策略为关，见 ADR 0004 第 4 节；设置 `OCTOSENSE_CONTAINED_APPS=1` 可试用） | 助手：经应用自己的、由宿主拥有的 peer `card.<应用 id>`（`crates/ai-host/src/contained.rs`）。四个 `octos.*` 调用沿用 Rinx 的参数规则（`octos.turn.start` 只接受 `text`，非空白、最多 32 KiB；其余只接受 `{}`）。peer 发起的工具审批会被拒绝（Card runner 还没有审批面板），并列在回复的 `denied_approvals` 中；超过 2 MiB 的回复会被拒绝 |

因此，隔离运行的应用（商店应用或系统应用）只能通过 `octos.*` 使用助手，且需要 Shell 托管了内核。它会收到：

| 调用 | 返回 |
| --- | --- |
| manifest 未授予的 family | `r.error`：`this app was not granted "<family>", which "<service>" needs`（由隔离环境立即返回） |
| 已获授权的 `octos.turn.start {text}`，内核与提供方已配置 | `r.data`：`{turn_id, text}`，即应用 peer 的回复 |
| 参数超出规则的 `octos.*` | `r.error`：`Unsupported Octos arguments` 或 `Provide text (at most 32 KiB)` |
| `Policy::contained_apps` 为关时的 `octos.*` | `r.error`：`The assistant is turned off for apps on this device` |
| 在未设置 `OCTOS_APP_CORE_BIN` 的桌面端调用 `octos.*` | `r.error`：`no octos kernel: no kernel binary configured (OCTOS_APP_CORE_BIN)` |
| 在不链接内核的构建（iOS）中调用 `octos.*` | `r.error`：`no service answers "octos" on this device`（**未验证**） |
| 获得 `llm` 授权的商店应用调用 `llm.*` | `llm is for OctoSense's own apps.` |
| 在 App Hub 的 `card-host` 中调用任何服务 | `no service answers "<family>" on this device`（`card-host` 不注册任何服务） |

第一行、`llm` 一行和 `card-host` 一行已于 2026-09-27 在 `card-host`（App Hub `362d832`）中运行验证。回复、参数不合规的返回和缺少内核的返回已于 2026-09-28 在 macOS 上运行验证：release 版桌面端、隐藏窗口，经启动器打开一个声明了 `octos.session.open` 和 `octos.turn.start` 的系统应用；使用按当时锁定的 octos `7bec0918` 构建的内核和用户自己的提供方时，回复来自 peer `card.<应用 id>`，内核数据中出现了它的记忆命名空间 `app/card.<应用 id>/…`。文本长度和开关关闭的返回由 `cargo test -p octosense-ai-host` 覆盖，走的是同一个分发函数。OctoScript-App-Design-Flow 的 [AI-SERVICES](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/AI-SERVICES.zh-CN.md) 给出了示例应用，以及 Rinx 提供的四个 `octos.*` 调用的参数和返回形状。

manifest 的 `agent` 字段（权限档位、通用工具、迭代和 token 上限）会被 App Hub 接受并按上限裁剪，但 **Shell 中没有任何东西为它运行 Agent**，Rinx 也会拒绝导入声明了它的应用包。应用不能依赖它。

## 规划中：事件驱动的应用 Agent（ADR 0002）

[ADR 0002](adr/0002-event-driven-app-agents.md)（状态 **Proposed**）让每个申请的应用拥有**自己的 Agent**：在 Shell 唯一内核上的应用 peer，由应用自己的触发器唤醒，只通过应用自己的工具工作，把卡片发布到 glance 屏幕。原生模块和脚本应用遵循同一模型（§12）。各部分及其进展：

| 组成部分 | ADR | 状态 |
| --- | --- | --- |
| `tools.json`：名为 `<app>.<tool>` 的工具，JSON Schema 输入输出，`risk`（`read`、`act`、`destructive`），`confirm`（`host` 或 `app`），`background`、`shareable`、`implemented_by`（`host-service` 或 `app`） | §4、§12 | App Hub 接受并锁定（[App-Hub#18](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/18)，`crates/app-policy/src/agent.rs`）。目前没有 Shell 加载它。 |
| `AGENT.md`、只含数据的 skills、模型**需求**（从不写提供方或模型名）、`background`、`triggers`（cron、宿主服务事件） | §2、§3 | 同上：App Hub `main` 已接受。Shell 锁定的 App Hub `46d67e51` 早于这些字段；manifest 拒绝未知字段，所以在锁定版本更新之前，**目前的 Shell 会拒绝使用这些字段的 manifest**。 |
| 内核：按 peer 由宿主注册工具（`peer/tools/register`、`peer/tool/call`/`result`/`cancel`），强制工具列表和风险等级，审批只在宿主连接上进行，通用工具白名单 | §4、§13 | 尚未合并：[octos#2567](https://github.com/octos-org/octos/pull/2567)（UPCR-2026-035）。 |
| 风险决定监督方式：read 和应用内 act 无人值守运行；destructive 等待用户（`confirm: host`：内核的审批路径；`confirm: app`：用户在场时用应用自己的面板，不在场时在应用的对话中生成审批请求） | §4、§12 | 已在 App Hub（#18）和 octos#2567 中声明；OctoSense 尚未强制执行。Rinx 的 `send_message` 已经在自己的界面上按 `confirm: app` 的方式工作。 |
| News M1：`news` 数据服务（不使用模型） | 第一个切片 | 已合入（[#69](https://github.com/OctoSense-org/OctoSense/pull/69)）；issue [#60](https://github.com/OctoSense-org/OctoSense/issues/60)。 |
| News M2：`os.news` 获得 peer，并注册它的工具 | 第一个切片 | 规划中：[#61](https://github.com/OctoSense-org/OctoSense/issues/61)。 |
| News M3：触发器和无人值守运行 | 第一个切片 | 规划中：[#62](https://github.com/OctoSense-org/OctoSense/issues/62)。 |
| News M4：glance 卡片（`glance.publish`、glance 页面和桌面面板） | §7、§8 | `glance.publish` 已合入（[#72](https://github.com/OctoSense-org/OctoSense/pull/72)）；进行中：供隔离应用使用的 `glance` 权限（[#86](https://github.com/OctoSense-org/OctoSense/pull/86)，配合 [App-Hub#22](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/22)）和 `sys.digest` 数据源（[#87](https://github.com/OctoSense-org/OctoSense/pull/87)）；issue [#63](https://github.com/OctoSense-org/OctoSense/issues/63)。 |
| M5：系统工具箱（检索、抓取）作为可授予的工具 | §6 | 规划中：[#64](https://github.com/OctoSense-org/OctoSense/issues/64)；进行中：[#82](https://github.com/OctoSense-org/OctoSense/pull/82)（工作流模板）、[octos#2568](https://github.com/octos-org/octos/pull/2568)（检索引擎）。 |
| M6：渲染与评审（`card-studio`） | §7 | App Hub 的 `card-studio` crate 已合入（[App-Hub#19](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/19)）；对应 skill 规划中（[#65](https://github.com/OctoSense-org/OctoSense/issues/65)）。 |
| M7：每个应用的对话（审批、提问）、记忆 | §9、§10 | 规划中：[#66](https://github.com/OctoSense-org/OctoSense/issues/66)。 |
| M8：外层循环（叠加在 `AGENT.md` 上的 overlay） | §11 | 规划中：[#67](https://github.com/OctoSense-org/OctoSense/issues/67)。 |

跟踪 issue 是 [#68](https://github.com/OctoSense-org/OctoSense/issues/68)。文件格式见 App Hub 的 [PUBLISHING § The app's agent and tools](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md#the-apps-agent-and-tools)。

## 本地运行与测试

### 桌面端：使用临时的内核与配置

1. 在 [octos-org/octos](https://github.com/octos-org/octos) 的检出中，构建根 `Cargo.toml` 锁定的版本（`a6ea8505`）：

   ```sh
   git checkout a6ea8505170735f191a12bb47629a6728415d417
   cargo build --release -p octos-cli --bin octos --no-default-features --features api,git,ast
   ```

2. 用独立的状态目录、core 目录和文件密钥库运行桌面端，这样不会动到 `~/.octosense`、`~/octos-home` 和登录钥匙串：

   ```sh
   T=$(mktemp -d)
   OCTOS_APP_CORE_BIN=/path/to/octos/target/release/octos \
   OCTOS_APP_CORE_DIR=$T/octos-home/.octos \
   OCTOSENSE_HOME=$T/state OCTOSENSE_APP_DATA=$T/apps \
   OCTOSENSE_LLM_VAULT=file OCTOSENSE_MAIL_VAULT=file \
     cargo run --release -p octosense
   ```

   日志中会出现 `octos: kernel service ready (starts on first use), core dir …`。没有 `OCTOS_APP_CORE_BIN` 时日志会说明没有内核，AI providers 仍会保存提供方。

3. 打开 **Start → Settings → AI providers**，添加一个模型（family、模型、路由、密钥、**Test connection**、保存）。配置文件是 `$T/octos-home/.octos/profiles/_main.json`；使用文件密钥库时密钥就在其中，用完后请删除 `$T`。
4. 通过某个使用者来使用助手：Rinx（默认链接并在进程内运行；从启动器打开它；登录 Matrix 后使用它的助手），或 AppCard（`--features app-appcard`）。隔离运行的应用通过 `octos` 宿主服务访问它（[见上文](#隔离运行的脚本应用系统应用和商店应用)）。

**隐藏窗口。** 加上 `MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=<port>`，即可通过远程控制桥操作 Shell 而不占用屏幕（[桌面端 README § Remote-control bridge](../desktop/README.md#remote-control-bridge)）。`desktop/scripts/ai_providers_remote.sh` 以这种方式端到端运行 AI providers，使用假密钥并禁止出站 HTTPS；`desktop/scripts/glance_remote.sh` 对 glance 面板做同样的事。

**测试**（在仓库根目录）：

```sh
cargo test --locked -p octosense-kernel                              # 使用替身内核
cargo test --locked -p octosense-app-peers --features octos-core,ws  # broker，脚本化内核
cargo test -p octosense-ai-host --features octos-core,llm
# 真实内核（按第 1 步构建）：
OCTOS_CORE_TEST_KERNEL=/path/to/octos cargo test -p octosense-kernel --test real_kernel -- --nocapture
OCTOS_APP_PEERS_TEST_KERNEL=/path/to/octos cargo test -p octosense-app-peers --features octos-core --test real_kernel -- --nocapture
```

### 手机

- **Android（Home）：** Home APK 把内核打包为 `liboctos.so`；`cd phone && python3 ../rom/scripts/build-home.py` 构建这对 APK（见 [phone/README.md](../phone/README.md)），其中使用 [`tools/kernel-artifact.py`](../tools/kernel-artifact.py) 按锁定的 octos 版本构建内核。内核在首次使用时启动。
- 在 **OctoSense Settings → Accounts → AI providers** 中配置提供方，或者从桌面端迁移：在桌面端点 **Show QR for phone**，然后在手机上通过相机、图片或粘贴代码导入，并输入 PIN。
- **OpenHarmony** 在进程内运行内核。**iOS** 没有内核：提供方会被保存，但没有应用能获得助手。

## 源码位置

| 内容 | 位置 |
| --- | --- |
| 内核服务 | [`crates/kernel`](../crates/kernel/README.md)（`src/launch.rs`、`src/lib.rs`） |
| Shell 入口、策略、offer | [`crates/ai-host/src/lib.rs`](../crates/ai-host/src/lib.rs) |
| 应用 peer：约定、broker、Shell 一侧 | [`crates/app-peers/src`](../crates/app-peers/src)（`contract.rs`、`broker.rs`、`hosted.rs`） |
| `llm` 服务、密钥库、面板 | [`apps/ai-providers/host-service/src`](../apps/ai-providers/host-service/src) |
| Shell 注册的宿主服务 | `crates/shell/src/apps.rs`（`register_host_services`）、[`crates/shell/src/glance.rs`](../crates/shell/src/glance.rs) |
| 权限、`tools.json`、agent 字段 | OctoSense-App-Hub [`crates/app-policy/src`](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/crates/app-policy/src)（`manifest.rs`、`services.rs`、`agent.rs`） |
| Rinx 的助手和迷你应用适配器 | hagency-org/Rinx `src/assistant/`、`src/host/octos.rs` |
