# octosense-kernel：Shell 的 octos 内核

[English](README.md) | 简体中文

[octos](https://github.com/octos-org/octos) Agent 内核是一项 **Shell 服务**。
Shell（`phone/` 中的 Home、`desktop/` 中的桌面）拥有它；**AI providers** 系统应用
通过 `llm` 宿主服务配置它；**AppCard** 以及之后 Rinx 的原生小程序宿主连接它。
本 crate 就是这项服务：每个进程一个内核，按需启动、共享，服务商变化时重启。

它放在 `crates/` 而不是 `apps/`，因为它不是应用，而是 Shell、AppCard
（`apps/appcard/app`）和 `llm` 服务（`apps/ai-providers/host-service`，feature
`octos-core`）共同链接的运行时组件。

## 它做什么

| | |
|---|---|
| **Core 目录** | octos 的数据目录：`<core_dir>/profiles/_main.json` 是 AI providers 应用写入的 profile。解析顺序：Shell 的 `Options::core_dir`，否则 `$OCTOS_APP_CORE_DIR`，否则在 Android/OpenHarmony 上为 `<应用数据目录>/octos-home/.octos`（AppCard 一直使用的应用私有 octos home），否则 `$HOME/octos-home/.octos`（`octosense_llm_config::profile::default_core_dir()`）。 |
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
  `Options::program` 或 `$OCTOS_APP_CORE_BIN`。两者都没有时就没有内核：绝不会动开发者
  自己的 `octos serve`。
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

**Rinx 与其他使用方。** 原生小程序宿主使用自己的连接（`connect()`），用自己的 id 打开
会话（Rinx 使用 `<profile>:api:rinx-mini-…`），只收到自己会话的流量，不与 AppCard 的
连接或 UI 队列耦合。它必须通过重新连接来处理 `CloseReason::Restarted`。

其他函数：`core_dir()`、`home()`、`profile()`、`launch()` / `is_available()`（是否以及如何
启动内核）、`status()`，以及下面的 Talk to Octos 控制函数。

## Talk to Octos

Talk to Octos 让 Web 客户端或终端界面与本设备的助手对话。它**默认关闭**；此时内核是上文
的私有 stdio 子进程，不监听任何端口。**AI providers → Talk to Octos** 可将其开启
（`set_external_access(true)`），开启后：

- 内核以 `octos serve --host-managed` 重启（见 octos
  [`docs/HOST_MANAGED_SERVE.md`](https://github.com/octos-org/octos/blob/main/docs/HOST_MANAGED_SERVE.md)，英文）；
  原生使用方通过其 WebSocket 继续收发同样的帧，使用从不离开本进程的宿主令牌，并请求
  octos 的 stdio 功能集（`octos_core::ui_protocol::UI_PROTOCOL_STDIO_DEFAULT_FEATURES`）；
- 生成一个**外部令牌**。它只能打开 `/api/ui-protocol/ws`：不能访问 REST 或管理接口，
  不能调用 `server/shutdown`，不能回答应用助手（宿主拥有的应用 peer）的审批或提问，也不能管理这些 peer；
- 监听套接字保存在本进程中（Unix）并交给每一代内核，因此重启保持端口不变，中间也没有
  其他应用能占用它。其他平台上，端口空闲时重启沿用原端口，否则换到新端口并生成新的外部令牌；
- 原生使用方离开后服务仍保持运行，直到关闭该功能或 Shell 退出。内核的 stdin 是它的生命线：
  Shell 退出或崩溃时，内核读到 EOF 后停止。

客户端如何接入：

- **Web。** 面板上的 **Pair a web client** 启用 octos 的配对（`pairing()`）：一个 8 位
  配对码，五分钟内有效，只能使用一次，同时显示 Web 客户端链接
  （`<web origin>/?octos=<server>&pair=<code>`）的二维码。配对码只在该面板打开期间有效
  （面板关闭时调用 `end_pairing()`）。面板上保存的 Web origin 是服务唯一信任的浏览器
  来源：`https`，或仅限 localhost、127.0.0.1、[::1] 的 `http`。保存的 origin 格式错误时视为
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
