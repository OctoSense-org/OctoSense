# octosense-kernel：Shell 的 octos 内核

[English](README.md) | 简体中文

> **在整个系统中的位置**。每个 Shell 一个 octos 内核：桌面端和 Android 上是子进程，OpenHarmony 上在进程内，iOS 上没有。Shell 持有宿主 token，是唯一的宿主连接；系统 Agent 和每个应用 Agent 都是这个内核中的会话，Talk to Octos 客户端只能用外部 token 使用系统对话。进程、应用 Agent 的两条通道以及一次带审批的工具调用的图示：[整体如何运作](../../README.zh-CN.md#整体如何运作)；详细说明：[docs/architecture.zh-CN.md](../../docs/architecture.zh-CN.md) 和 [ADR 0004（英文）](../../docs/adr/0004-native-apps-hosting-and-peers.md)。

[octos](https://github.com/octos-org/octos) Agent 内核是一项 **Shell 服务**。
Shell（`phone/` 中的 Home、`desktop/` 中的桌面）拥有它；**AI providers** 系统应用
通过 `llm` 宿主服务配置它；它的使用方连接它：Shell 的系统对话（系统 Agent 的面板，
`crates/shell/src/system_chat/link.rs`）、每个应用 Agent 的代理（`crates/app-peers`，
`CoreConnector`：Rinx 以及带 Agent 的脚本应用）和需显式启用的 AppCard。
本 crate 就是这项服务：每个进程一个内核，按需启动、共享，服务商变化时重启。

它放在 `crates/` 而不是 `apps/`，因为它不是应用，而是 Shell、AppCard
（`apps/appcard/app`）和 `llm` 服务（`apps/ai-providers/host-service`，feature
`octos-core`）共同链接的运行时组件。

## 它做什么

| | |
|---|---|
| **Core 目录** | octos 的数据目录：`<core_dir>/profiles/_main.json` 是 AI providers 应用写入的 profile。解析顺序：Shell 的 `Options::core_dir`，否则 `$OCTOS_APP_CORE_DIR`，否则 `<应用数据目录>/octos-home/.octos`：OctoSense 自己的 octos home（手机上为平台的应用数据目录，即 AppCard 一直使用的应用私有 home；桌面上为 OctoSense 的状态目录 `~/.octosense`），否则 `$HOME/octos-home/.octos`（`octosense_llm_config::profile::default_core_dir()`，供未指定任何目录的使用者）。桌面版 OctoSense 以前与用户独立的 octos 共用 `$HOME/octos-home/.octos`；现在首次使用时只从那里复制提供商和模型设置（`llm`、`env_vars`）到自己的 profile，绝不在那里写入、移动或删除任何内容。 |
| **一个内核，按需启动** | 第一次 `connect()` 启动它，之后的连接共享它。octos 对数据目录持有单写者锁，同一目录上本来也无法运行第二个内核。 |
| **按帧共享** | 一个 `Connection` 传递 UI Protocol（JSON-RPC）帧，与 `octos serve --stdio` 的格式完全相同。每个使用方使用自己的请求 id，只收到自己请求的回复和自己指定会话的通知（没有任何使用方指定的会话通知会发给所有使用方）。 |
| **重启** | `restart()` 停止正在运行的内核（没有运行时什么也不做）。连接随后以 `CloseReason::Restarted` 结束；使用方重新连接，会启动读取新 profile 的新内核。新内核要等旧内核退出并释放数据目录后才启动。 |
| **空闲停止** | 最后一个连接被丢弃时内核停止，与 AppCard 自己的子进程过去的行为一致（Talk to Octos 开启时除外，见下文）。 |
| **Talk to Octos** | 默认关闭。用户开启后，内核作为 octos 的宿主托管回环服务运行，外部客户端可以连接同一个内核（见下文）。 |
| **关闭** | `shutdown()` 停止内核并等待（最多 5 秒）。 |

各平台的启动方式（`src/launch.rs`）：

- **Android**：`<nativeLibraryDir>/liboctos.so serve --stdio`，`HOME=<core 目录的上级目录>`
  （octos home），使用 AppCard 的环境变量（`OCTOS_SKILLS_PATH`、
  `OCTOS_OMIT_WORKSPACE_HINT`、`RUST_LOG`、`makepad.OCTOS_PROXY` 代理）以及内核配置的
  内存预算。APK 必须打包内核：`MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>`
  （Shell 的构建脚本会这样做）。
- **OpenHarmony**：在进程内运行标准内核 `octos_cli::embedded::serve_io(<octos home>, ..)`，
  运行在本 crate 的运行时上，工作线程栈 8 MiB（HAP 原生库不能 exec）。
- **桌面**：`<program> serve --stdio --data-dir <core_dir>`（`<core_dir>/config.json`
  存在时再加 `--config <core_dir>/config.json`），`OCTOS_HOME=<core_dir>`；程序为 Shell 的
  `Options::program`、`$OCTOS_APP_CORE_BIN`，或 Shell 可执行文件旁随附的
  `octos-kernel[.exe]`（macOS `.app` 中为 `Contents/MacOS` 或 `Contents/Resources`），
  依次选用。随附内核只有在其收据 `octos-kernel.json`（在它旁边、`.app` 的
  `Contents/Resources` 或 Linux 包的 `usr/lib/octosense` 中）记录的版本与本次构建固定的
  octos 版本一致（`build.rs` 从 `Cargo.lock` 读取），且 SHA-256 相符时才运行；否则连接
  失败并给出原因。`tools/kernel-artifact.py --host --stage <目录>` 负责构建和放置；开发时
  `OCTOSENSE_KERNEL_ANY_REVISION=1` 可接受其他版本。不在 `PATH` 或工作目录中查找，绝不会
  动开发者自己的 `octos serve`。
- **iOS**：没有内核。
- **开启 Talk to Octos 时**（桌面和 Android）：同一命令，以 `--host 127.0.0.1 --host-managed`
  代替 `--stdio`，并把本进程保留的监听套接字作为描述符 3 传入（`--listen-fd 3`，Unix）。

内核和帧转发运行在本 crate 自己的 Tokio 运行时上，因此使用方可以使用任何运行时，或不用运行时。

## 使用方式

Shell 在启动时、第一个使用方之前调用一次：

```rust
octosense_kernel::configure(
    octosense_kernel::Options::default().app_data_dir(cx.get_data_dir()),
);
// The llm service (feature `octos-core`) writes under the same core dir
// and calls octosense_kernel::restart() after every change.
octosense_llm_service::register_with(
    octosense_llm_service::Options::default().core_dir(octosense_kernel::core_dir().unwrap()),
);
```

使用方：

```rust
let mut conn = octosense_kernel::connect()?;       // Err: no kernel here
conn.send(r#"{"jsonrpc":"2.0","id":"1","method":"session/open","params":{"session_id":"_main:api:x","profile_id":"_main"}}"#)?;
loop {
    match conn.recv().await {
        Ok(frame) => { /* a JSON-RPC frame for this consumer */ }
        Err(octosense_kernel::CloseReason::Restarted) => { /* connect again, re-open sessions */ break }
        Err(other) => { /* the kernel stopped or could not start: tell the person */ break }
    }
}
```

AppCard 的传输层（`apps/appcard/app/crates/octos-app-transport`，`kernel.rs`）是参考
使用方：收到 `Restarted` 时，它让仍在等待的请求失败，重新连接，并从各会话的回放游标重新
打开会话，应用得以继续。

**其他使用方。** 每个使用方都用自己的连接（`connect()`），只收到自己会话的流量：系统对话
打开 `_main:api:octosense#system`；app-peers 代理驱动其应用的 peer 和请求上下文（Rinx 的
小程序经 `OctosAppService` 使用 Rinx peer 的请求上下文；每个上下文有自己的内核会话和记录，但不是另一个应用 peer）。每个使用方
都必须通过重新连接来处理 `CloseReason::Restarted`。

其他函数：`core_dir()`、`home()`、`profile()`、`launch()` / `is_available()`（是否以及如何
启动内核）、`status()`，以及下面的 Talk to Octos 控制函数。

## Talk to Octos

Talk to Octos 让 Web 客户端或终端界面与本设备的助手对话。它**默认关闭**；此时内核是上文
的私有 stdio 子进程，不监听任何端口。**AI providers → Talk to Octos** 可将其开启
（`set_external_access(true)`），开启后：

- 内核以 `octos serve --host-managed` 重启（见 octos
  [`docs/HOST_MANAGED_SERVE.md`](https://github.com/octos-org/octos/blob/main/docs/HOST_MANAGED_SERVE.md)，英文）；
  原生使用方通过其 WebSocket 继续收发同样的帧，使用从不离开本进程的宿主令牌（内核从 stdin
  读取两个令牌，从不经过环境变量），并请求
  octos 的 stdio 功能集（`octos_core::ui_protocol::UI_PROTOCOL_STDIO_DEFAULT_FEATURES`）；
- 生成一个**外部令牌**。它只能打开 `/api/ui-protocol/ws`，且在其中只能调用白名单内的会话、
  轮次、回答和只读状态方法：不能修改配置（服务商、密钥、技能、快照），不能调用
  `server/shutdown`，不能操作应用助手（宿主拥有的应用 peer）的会话，只能在自己打开的会话中
  回答提问。它发起的轮次没有执行代码、管理 octos 或访问 peer 的工具（octos UPCR-2026-036）；
- 监听套接字保存在本进程中（Unix）并交给每一代内核，因此重启保持端口不变，中间也没有
  其他应用能占用它。其他平台上，端口空闲时重启沿用原端口，否则换到新端口并生成新的外部令牌；
- 原生使用方离开后服务仍保持运行，直到关闭该功能或 Shell 退出。内核的 stdin 是它的生命线：
  Shell 退出或崩溃时，内核读到 EOF 后停止。

客户端如何接入：

- **Web。** 面板上的 **Pair a web client** 启用 octos 的配对（`pairing()`）：一个 8 位
  配对码，五分钟内有效，只能使用一次，同时显示 Web 客户端链接
  （`<web origin>/?octos=<server>&pair=<code>`）的二维码。配对码只在该面板打开期间有效
  （面板关闭时调用 `end_pairing()`）。面板上保存的 Web origin 是服务唯一信任的浏览器
  来源：`https`，桌面上还可使用 localhost、127.0.0.1、[::1] 的 `http`（Android 只允许 https）。保存的 origin 格式错误时视为
  没有 origin，内核照常启动。
- **终端。** 连接文件 `connection_file(core_dir)`（`<core_dir>/client-connection.json`，
  权限 0600；Windows 上为 `%LOCALAPPDATA%\OctoSense\client-connection.json`，其默认 ACL
  只允许当前用户、SYSTEM 和管理员）保存端点和外部令牌，供当前用户的客户端使用。端口或
  令牌变化时重写，服务停止时删除。启动终端界面**未经验证**。
- **Revoke all clients**（`rotate_external_access()`）生成新的外部令牌并重启服务；
  **Turn off**（`set_external_access(false)`）停止服务，丢弃令牌和端口，并删除连接文件。

电脑需要通过保持端口号不变的隧道访问手机上的服务，因为服务只回应 `Host` 指向其自身端口的
请求（**未在设备上验证**）：

```sh
adb -s SERIAL forward tcp:PORT tcp:PORT
```

系统会话为 profile `_main` 中的 `_main:api:octosense#system`；其工作区保存在
`system-workspace.txt` 中，使原生打开与 Web 的限定会话保持一致。OpenHarmony（嵌入式内核）
和 iOS（没有内核）没有 Talk to Octos。威胁模型见
[ADR 0003（英文）](../../docs/adr/0003-shared-octos-client-access.md)。

## 系统智能体的工具

[ADR 0004](../../docs/adr/0004-native-apps-hosting-and-peers.md) §12：系统智能体的工具集就是它获得的授权。它默认的 octos
工具是 `system_tools::SYSTEM_AGENT_TOOLS`：监督（`peer_send_input`、`peer_gather`、`peer_list`、`peer_respond`；
不含 `peer_close`：octos 无法恢复已关闭的 peer，配置文件的 `tool_policy` 对所有智能体都禁用它）、其工作区内的文件工具（octos 将其限制在会话工作目录内）、记忆、`ask_user_question`、查看媒体（`view_image`、`view_video`）、octos 的
`web_search` / `web_fetch` 以及 `tool_search`。工具箱工具和跨应用工具按设计通过 `SystemAgentTools`（`grant_toolbox`、`grant_cross_app`）作为宿主工具加入，但目前除测试外没有任何地方授予它们，所以系统智能体还没有这些工具。系统对话总会在该会话上注册自己的两个宿主工具 `agents.list` 和 `agents.ask`（`crates/shell/src/agents.rs`：哪些应用有 Agent，以及允许某个 Agent 的首次使用面板；只有用户能回答）。命令执行作为授权**已完成**：用户在“设置 → 助手 → 命令执行”中的开关（默认关闭；开启需要用户输入确认语，确认语说明其风险；
`crates/shell/src/system_chat/grants.rs`）通过 `SystemAgentTools::grant_command_execution` 为系统代理授予宿主工具 `terminal.run`。
Shell 把授权交给本 crate（`system_tools::set_grants`）；每次内核启动时采用（`grants_at_start`、`system_agent_tools_in_effect()`），
因此更改在重启后生效，设置中提供重启按钮。每条命令都经过 Shell 的批准路由器，按 `auto_approvable: false` 处理，并在实时批准表单上显示完整命令（开发者模式仍可直接批准）。
开关开启期间，Shell 的系统对话在自己的连接上把该宿主工具注册到系统会话（octos#2567 的宿主会话目标，不带 `peer` 的 `peer/tools/register`；自 octos#2657 起也不需要任何应用 peer 的宿主 token，所以在任何应用的 Agent 启动之前就能提供），开关关闭时撤回；每个获批的调用由 Shell 输入到用户可见的 Terminal。

**内核执行的内容。** 每次启动都写入 `_main` profile 的 `tool_policy`（`system_tools::tool_policy`）：任何授权可给予的一切，
唯独去掉 octos 自己的 shell（`group:runtime`：`shell`、`bash`、`exec_command`、`write_stdin`）——这是 OctoSense 唯一从不提供的工具，
因为 §12 只以宿主工具的形式授予命令执行——以及 `peer_close`。octos 对该 profile 的每个回合（包括唤醒续接回合）都应用它，作为上限。因此：

- **系统智能体恰好得到它的清单**（§12，计划第 4 步）：每次内核启动时，在宿主自己的连接上、在任何使用方的帧到达内核之前，
  把系统会话的内核工具设为 `SystemAgentTools::kernel_tools`（octos `session/tool_list/set`，octos#2648）；`system_tools::set_grants`
  会在运行中的内核上再设一次。octos 持久保存该清单，并用它收窄该会话上的每个回合，无论由谁发起（Shell、Talk to Octos 客户端、唤醒续接）。
  它的宿主工具（命令执行的 `terminal.run`）由系统对话注册，不受清单过滤；`spawn` 系列工具从不在清单上（`SPAWN_FAMILY`）。
  真实内核测试：`a_system_agent_turn_is_offered_exactly_the_system_agent_tools`；
- 应用 peer 由其回合的 `generic_tools` 收窄到其授权（计划第 6 步）；
- Talk to Octos 外部回合保留 octos 自己的允许列表。

该策略只写入 OctoSense 自己的 core 目录，且只覆盖 OctoSense 写入的策略（`"owner": "octosense"`）：遇到外来策略或用户自己的
`$HOME/octos-home/.octos` 时拒绝并发出警告。

## 系统智能体的技能

[ADR 0013](../../docs/adr/0013-craft-engines-as-pinned-services.zh-CN.md)：系统智能体通过 octos 技能了解构建中链接的每个 craft 引擎，
请求用到某个引擎时才读取它的说明，而不必在每个回合都带上每个引擎方法的工具 schema。Shell 在启动时注册一次这些技能
（`skills::set_managed`，来自 `crates/shell/src/system_chat/skills.rs`）；每次内核启动前，`launch::prepare` 把它们同步
（`skills::sync`）到 octos 为 `_main` profile 读取的技能目录：

- **位置。** `<core_dir>/profiles/_main/data/skills/<name>/`：即该 profile 的数据目录；若 `_main.json` 指定了绝对路径的
  `data_dir`，则为 `<data_dir>/skills`，与 octos 的 `ProfileStore::resolve_data_dir` 一致。profile 运行时启动时，octos 把每个技能的
  名称、描述和 `SKILL.md` 位置列入该 profile 的系统提示词（`## Available Skills`），所以从这次启动起，`_main` 的每个会话（包括应用
  Agent 的会话）都能看到这份摘要。该目录也是每个会话文件工具的只读区域，因此必须在启动前就存在。
- **受管理，从不动用户的技能。** 每个受管理的技能都带有 `.octosense-managed`。同步时写入缺失或已变化的技能（在旁边构建，再用重命名换入），
  删除不再注册的受管理技能；没有该标记的技能，即使名称与注册的技能相同，也从不写入、移动或删除（那个注册的技能会被报告为未安装）。
  Shell 注册技能集之前什么都不做，也从不写入用户自己的 `$HOME/octos-home/.octos`。
- **失败时放行。** 技能是指引，不是边界：无法安装的技能会记入日志（`octos-core: managed skills in <dir>: ...`），内核照常启动。
  只有工具策略在失败时阻止启动。
- **校验。** `skills::validate` 拒绝 octos 会误读的技能：名称不在 `[a-z0-9-]` 之内、文件路径出了技能目录、`SKILL.md` 的
  frontmatter 写的是另一个技能、描述缺失、超过 200 字节、带引号，或含有 `#`（octos 会在此截断）或 `<`、`>`、`&`
  （其摘要是未转义的 XML），以及 `always: true`（会把整个正文加载进每个回合）。

## 测试

在仓库根目录：

```sh
cargo test --locked -p octosense-kernel   # unit tests + the core against a stand-in kernel (python3)
# The real kernel: a profile written by octosense-llm-config, session/open,
# profile/llm/list, a provider change and a restart; Talk to Octos on and off
# (what the external token must not reach, pairing, restart and rotation);
# native and web clients on one system conversation; and a host killed with
# SIGKILL taking its kernel with it. Build octos at the rev the root
# Cargo.toml pins, then:
OCTOS_CORE_TEST_KERNEL=/path/to/octos cargo test -p octosense-kernel --test real_kernel -- --nocapture
```

用根目录 `Cargo.toml` `[workspace.dependencies]` 中的 rev 从 octos-org/octos 构建该测试用的
内核（Android APK 用的内核再加 NDK 和 `--target aarch64-linux-android`）：

```sh
cargo build --release -p octos-cli --bin octos --no-default-features --features api,git,ast
```

`python3 tools/kernel-artifact.py --host --plan` 会打印在本机构建所锁定版本的同一构建步骤。

CI：`.github/workflows/apps.yml`（`services` 任务测试本 crate；`apps` 任务构建链接它的 AppCard）。

## 只有一个 octos

本 crate 从 git octos-org/octos 链接 `octos-core`（用于 stdio 功能集），在 OpenHarmony 上还
链接 `octos-cli`，二者都使用根目录 `Cargo.toml` 为所有 octos crate 锁定的同一个 rev。为
OpenHarmony 构建它的工作区还需要 `nix` 补丁（octos rev `18fcd3f1`，见根目录 `Cargo.toml`
的 `[patch.crates-io]`）。其他平台上，内核是用同一 rev 构建的独立二进制文件。

源码阅读补充：Rinx mini app 的 request context 有自己的内核会话和记录，但不是另一个应用 peer。线程、runtime 与回合任务的对应关系见[代码导读](../../docs/architecture-walkthrough.zh-CN.md#10-映射到-rust-的实际执行模型)。
