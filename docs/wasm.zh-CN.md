# OctoSense 中的 WebAssembly

[English](wasm.md) | 简体中文

OctoSense 中有四处用到 WebAssembly。其中一处专为运行应用自己的代码而设计：应用自带的
函数，默认关闭，任何发布版本都不包含。`photo` 服务背后的 photocraft 插件也会运行调用方
交给它的代码，只是目前还没有调用方。另外两处是 makepad 的内部实现，以及一个无法编译的
浏览器构建。本页逐一说明：运行的是谁的模块、它能接触什么、哪些构建包含它、如何检查过。
内容对应 `main` 的 a790f21a（2026 年 10 月 8 日）。

| 位置 | 谁的模块 | 运行时 | 构建 | 状态 |
| --- | --- | --- | --- | --- |
| 应用自带的函数：`wasm` 服务（[ADR 0011](adr/0011-apps-own-functions-in-webassembly.zh-CN.md)） | 应用自己的应用包，`fns/*.wasm`，需要 `wasm` 能力 | Wasmtime 49，由 Cranelift 编译（`crates/wasm-host`） | 仅限启用 `wasm-lab` 特性的构建：默认关闭，不在任何发布版本中 | 提议中。测试在 macOS 和 Linux 上通过；一部 OnePlus 6 通过了手机验收 |
| photocraft 插件：`photo` 服务（[ADR 0013](adr/0013-craft-engines-as-pinned-services.zh-CN.md)） | 调用方传给引擎 `plugin.install` 命令的任何内容 | wasmi 2，解释器 | 所有带 App Hub 的构建：默认桌面版，以及启用 `mobile-apps` 的 Home | 只有系统应用能调用；已发布的内容都没有调用它 |
| Splash 的数学编译器（makepad） | makepad 根据 Splash 代码生成 | makepad-stitch，解释器 | 链接进所有构建 | 未使用：OctoSense 没有链接任何调用它的代码 |
| 浏览器中的外壳 | 外壳本身，为 `wasm32-unknown-unknown` 构建 | 浏览器 | 无 | 无法构建 |

## 应用自带的函数：`wasm` 服务

[ADR 0011](adr/0011-apps-own-functions-in-webassembly.zh-CN.md)（提议中）允许应用在
应用包里携带编译成 WebAssembly 的 Rust 函数，由外壳只为这个应用运行。它们用于脚本旁边
的轻量计算：解析、排序、排程、差异比较、格式转换。数值计算仍然交给 Splash 的计算内核。

### 模块

模块是放在 `fns/<name>.wasm` 的核心 WebAssembly，每个应用包最多 8 个。不使用组件模型，
也不使用 WASI。

- 它导出 `memory`、`octo_alloc(len) -> ptr` 和 `octo_free(ptr, len)`。
- 其余类型为 `(i32, i32) -> i64` 的导出都是函数：在 `(ptr, len)` 处取输入，返回
  `(out_ptr << 32) | out_len`。输出的第一个字节是状态，`0` 表示成功，`1` 表示出错，
  后面跟着错误文本。
- 唯一允许的导入是 `octo.log(ptr, len)`。导入其他任何东西的模块在加载时就被拒绝。

客体 crate
[`apps/wasmlab/guest/octosense-guest`](../apps/wasmlab/guest/octosense-guest/src/lib.rs)
生成这一切：`export!` 用于 `fn(&[u8]) -> Result<Vec<u8>, String>`，`export_json!`
用于 serde 类型，另有 `log`。把 crate 构建成面向 `wasm32-unknown-unknown` 的
`cdylib`。应用开发者请从 Design Flow 的
[Run your own Rust code](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/RUST.md)
开始。

### 模块如何到达设备

- **商店应用。** App Hub 的审核（应用契约 1.7.0）只接受放在 `fns/<name>.wasm` 的
  `.wasm` 文件（名称为 `[a-z0-9_-]`，最多 64 个字符），必须是核心模块，最多 8 个，并且
  应用必须声明 `wasm` 能力。声明了 `wasm` 能力却没有模块会得到一条警告。商店会告诉用户：
  "Runs its own functions in a sandbox on this device; they reach no files, network
  or other apps."（在本设备的沙箱中运行自带的函数，不接触文件、网络或其他应用。）规则见
  App Hub 的[发布参考](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md)
  （`functions` 检查项和 `wasm` 能力）。
- **系统应用。** 和其他系统应用一样，按摘要从打包方式的系统应用列表打包进外壳。
  `desktop/system-apps-wasm-lab.json` 和 `phone/system-apps-wasm-lab.json` 就是标准
  列表加上 `wasmlab`。

### 服务

`wasm` 宿主服务（`crates/shell/src/wasm_service.rs`）只存在于启用外壳 `wasm-lab`
特性的构建中，桌面版和 Home 的包会把这个特性传下去。它回答两类请求：

- `wasm.functions`：应用的模块导出了哪些函数，每个模块如何加载（编译，还是取自缓存，
  用了多久），以及每个函数的运行情况。
- `wasm.<function>`：一次调用。字符串参数按原文传入，其他参数按 JSON 传入。JSON 输出
  作为数据返回，其他输出返回为 `{"text": …}`。函数自己返回的错误、陷阱或超时都是这次
  请求的错误。

代码总是来自发起调用的应用自己的、已准入且摘要校验过的应用包，绝不来自参数；任何应用都
接触不到别的应用的函数。Card runner 的审核和工具执行器都要求 `wasm` 能力，服务在加载任何
东西之前还会再次检查已准入清单中的授权。

每次调用都使用全新的实例：一次调用的内存、全局变量、表和日志行都不会进入下一次调用，成功
调用或函数自己返回错误之后也一样。只有编译后的代码会被复用：保存在应用的工作线程里，以及
磁盘缓存中（宿主目录下的 `wasm-cache`，以模块的 SHA-256 和引擎兼容性哈希为键）。调用
运行之前、答复交付之前，服务都会再次检查准入。更新、授权变化或签名撤回会丢弃这次答复和
编译后的代码。编译过程无法中断，但已过期或已取消的请求不会继续运行。

| 限制 | 值 |
| --- | --- |
| 一次调用（包括启动函数） | 2 秒实际经过时间（每 10 毫秒检查一次） |
| 线性内存 | 256 MiB |
| 表元素 | 16,384 个 |
| wasm 栈 | 512 KiB |
| 模块 | 8 MiB |
| 一次调用的输入或输出 | 运行时 16 MiB；服务中序列化后的输入 1 MiB |
| 日志 | 每次调用 64 行，每行 1 KiB |
| 每个应用的模块 | 8 个 |
| 同时运行函数的应用 | 4 个工作线程，每个应用一个 |
| 排队的请求 | 每个应用 4 个 |
| 所有应用缓冲的输入 | 16 MiB |
| 一个请求（包括排队和加载） | 10 秒 |
| 空闲的工作线程 | 5 秒后退出 |

队列已满或没有空闲的工作线程时，请求会立即失败，不会等待。

### Agent 工具

应用的 `tools.json` 可以用 `"host_method": "wasm.<function>"` 把一个工具映射到自己的
某个函数。这需要 `wasm` 能力。与共享宿主方法不同，它没有最低风险等级，也不需要
`"private_data": true`，因为函数只能看到传给它的参数。Wasm Lab 把 `wasmlab.find_slots`、
`wasmlab.rank` 和 `wasmlab.diff` 映射到 `wasm.find_slots`、`wasm.fuzzy_rank` 和
`wasm.text_diff`，风险等级都是 `read`。

### 平台

| 平台 | 状态 |
| --- | --- |
| macOS | 已在隐藏的桌面外壳中验证（ADR 0011）。Wasmtime 在这里用 Mach 异常端口而不是信号捕获陷阱。 |
| Linux | 运行时和服务的测试在 x86_64 上通过，陷阱由信号捕获（2026 年 10 月 8 日）。 |
| Android | 需要 libc 0.2.190 或更高版本，`crates/wasm-host` 已要求这一点：在 0.2.189 下，每个陷阱都会结束进程。已在 Redmi Note 12 上验证（ADR 0011），并在一部 OnePlus 6 上通过[手机验收](#手机验收)。运行时增加约 7.5 MiB 代码；在 Snapdragon 685 上编译一个 433 KiB 的模块约需 0.4 秒，从缓存加载需 5–11 毫秒。 |
| iOS | 未实现。那里的应用不允许 JIT；可选方案是 Wasmtime 的解释器 Pulley，比 Cranelift 慢约 17 倍。 |
| OpenHarmony | 尚未尝试。其 JIT 策略**未验证**。 |

在受管理的 Mac 上，Microsoft Defender 会把新写入的缓存文件的第一次打开拦住约一秒；具体
测量见 ADR 0011。

### Wasm Lab

Wasm Lab（`os.wasmlab`，`apps/wasmlab/`）是演示这个服务的系统应用。它的函数
（`apps/wasmlab/guest/functions`）包括 Markdown 转 HTML（pulldown-cmark）、在忙碌时段
之间找空闲时段、模糊排序（strsim）、按行比较差异（similar），以及 `rogue`：按要求无限
循环、无限分配内存、panic 或无限递归。它的界面通过 `host.request` 调用每个函数，并显示
返回结果、往返耗时和 `wasm.functions`。

构建一个包含它的桌面版，然后以隐藏窗口启动并直接打开 Wasm Lab，通过远程控制接口操作
（`/help` 列出路由；用 `/quit` 结束）：

```sh
OCTOSENSE_SYSTEM_APPS=$PWD/desktop/system-apps-wasm-lab.json cargo build --locked -p octosense --features wasm-lab
MAKEPAD_HIDE_WINDOWS=1 target/debug/octosense --remote=47631 --test-action launch-wasmlab
```

在 Linux 构建主机上（2026 年 10 月 8 日，调试构建，隐藏窗口，运行在无头 Weston 下）：
模块第一次使用时编译用了 107 毫秒，之后从缓存加载用 4.0–4.2 毫秒，重启后也是如此。每种
异常行为都以错误结束，外壳一直在运行：无限循环在 2,031 毫秒后因截止时间结束，无限分配在
340 毫秒后撞到内存上限，panic 在 34 毫秒后结束，无限递归在 34 毫秒后结束（均为脚本测得的
时间）。

看界面时需要知道两点：

- Wasm Lab 打开时同时发出五个请求，而服务为每个应用最多排队四个请求，所以 Markdown 卡片
  会显示 "wasm app queue is full; try again later"。
- `wasm.functions` 统计的是应用的工作线程启动以来的调用。工作线程空闲五秒后退出，下一个
  工作线程重新开始计数。

已提交的模块可以逐字节重建：使用 rustc 1.97.1 时，`apps/wasmlab/guest/build.sh` 写出
完全相同的 `bundle/fns/wasmlab.wasm`（SHA-256 `f4a69c32…`）。首次需要安装目标：
`rustup target add wasm32-unknown-unknown`。

### 测试

```sh
cargo test --locked -p octosense-wasm-host
cd phone && cargo test --locked --features mobile-apps,wasm-lab -p octosense-shell wasm_service::tests
```

- `crates/wasm-host/tests/runtime.rs` 运行一个手写的客体：输入和输出、客体错误、截止
  时间、内存、表和栈的上限、陷阱、导入规则、每个实例各自的状态、线程、大小上限、缓存及其
  并发写入、取消。
- `crates/wasm-host/tests/guest.rs` 运行 Wasm Lab 的模块，把每个函数的结果与同一段 Rust
  原生运行的结果比较，然后逐一运行 `rogue` 的各种模式。
- 服务测试覆盖工具映射、输入上限、每次调用使用全新实例、更新或授权变化或撤回之后的撤销、
  取消和队列上限，以及应用自己的函数回答它的工具和脚本。

`phone.yml` 运行这两条命令，`tools/ci-local.sh --linux-host --offload` 在 Linux 构建
主机上运行它们。没有 CI 任务会构建启用 `wasm-lab` 的桌面版或 Home。

### 手机验收

[`tools/fixtures/wasm-phone-lab`](../tools/fixtures/wasm-phone-lab/README.zh-CN.md)
里有一个合成的已签名商店应用 `org.octosense.samples.wasmprobe`，它的模块（`state.wasm`，
由 `state.wat` 生成）在实例被复用时会记住上一次的输入。`crates/wasm-phone-smoke` 把它
打包进一个测试 APK，通过正常的签名商店流程安装它，并通过真实的脚本工具调用它；
`tools/test-wasm-phone.py` 在指定的设备上驱动这一过程。它的
[验收记录](../tools/fixtures/wasm-phone-lab/acceptance-oneplus6.json)显示，在 Android 15
的 OnePlus 6 上 22 项检查全部通过，包括成功、客体错误、陷阱和超时之后都使用全新实例。它的
源码 e67ce63e 与 `main` 的 wasm 服务和运行时相同。验收不包括实时模型、性能和发布版 APK。
用该夹具 README 中的 `encode_phone_fixture` 命令可以逐字节重建 `state.wasm`。

### 尚未完成

- 没有任何发布版本或默认构建包含这个服务，ADR 0011 仍是提议中。
- Wasm Lab 打开时的请求会超出每个应用的队列（见上文）。
- 应用之间的 CPU 公平调度，以及磁盘缓存的上限：目前没有任何东西会清理它。
- 在安装时（商店应用）或构建时（系统应用）编译，省掉手机上的第一次编译。
- iOS（Pulley）和 OpenHarmony。
- 用类型化接口（组件模型和 WIT）代替 JSON。`octo.log` 以外的任何导入，例如时钟或随机数，
  都将是新的能力。

## photocraft 插件：`photo` 服务

`photo` 宿主服务（`apps/photo/host-service`，ADR 0013）运行 photocraft，一个固定在
ymote/photocraft 某个版本的光栅引擎。photocraft 运行用 WebAssembly 写的滤镜插件，运行在
纯 Rust 解释器 wasmi 中。它的沙箱限制来自 photocraft 自己的插件参考：

| 限制 | 值 |
| --- | --- |
| 导入 | 无：没有 WASI、文件、网络、时钟或随机数 |
| 模块 | 32 MiB |
| 线性内存 | 每个实例 512 MiB |
| 指令 | 每次调用 5,000 万条，处理像素时（`pc_filter`）每个采样再加 4,000 条 |
| 实际经过时间 | 原生构建中每次运行 60 秒 |
| 调用深度 | 1,024 帧 |
| 状态 | 每一带像素使用一个全新实例 |

**插件如何进入。** `photo.run` 接受一个文档路径和一组按 id 指定的引擎命令，并在一个绑定
到宿主目录下该服务专属文件夹的会话中运行它们。photocraft 对这种会话的策略拒绝从路径执行
`plugin.install` 以及 `plugin.reload`，因为它们会读取文件系统；但接受以 base64 `data`
传入模块的 `plugin.install`，以及 `plugin.list`、`plugin.run` 和 `plugin.remove`。已安装
的插件属于进程而不属于某次调用：之后来自任何调用方的每次调用都能看到它。

一个像脚本的 `host.request` 那样调用真实服务的临时程序在 Linux 构建主机上确认了这一点
（2026 年 10 月 8 日）：`os.photos` 用 base64 `data` 安装了 photocraft 的示例插件；下一次
调用列出了它；另一个系统应用 `os.notes` 的调用在文档上运行了它；商店应用被拒绝；从路径执行
`plugin.install` 被拒绝。

**谁能调用。** `photo` 服务只回答系统应用（`os.*`）。在 `main` 上，没有系统应用的脚本
调用 `photo.*`，也没有 Agent 工具映射到它，所以在运行中的外壳里还没有任何东西走到这条
路径。第一个调用 `photo.run` 的东西，无论是系统应用、Agent 工具，还是 ADR 0013 留待以后
设计的商店能力，都可以用这种方式安装代码。

**与 `wasm` 服务的区别。** 没有能力或商店审核覆盖这些代码；模块来自请求参数，而不是已准入
的应用包；限制更宽（60 秒和 512 MiB，对比 2 秒和 256 MiB）；工作在请求之内完成。App Hub
在 UI 线程上分派脚本的 `host.request`，并在持有服务注册表锁的同时调用服务
（`services::dispatch`）。所以耗时长的 `photo.run`，无论是否运行插件，都会卡住 UI 和所有
其他应用的请求，直到它返回（[#399](https://github.com/OctoSense-org/OctoSense/issues/399)）。

## Splash 数学与计算内核

makepad 的脚本引擎可以把纯数学的 Splash 函数（例如有符号距离场）编译成它自己生成的
WebAssembly 模块，并在它自己的解释器 makepad-stitch 中运行，作为与 Splash 解释器逐位一致
的参考实现（`.sources/makepad/platform/script/src/math_aot/`）。没有应用会提供这个模块。
在 OctoSense 中只有 makepad 的 CSG 库会调用它，而 OctoSense 没有链接这个库，所以这段代码
存在但不被使用。ADR 0011 曾把 stitch 作为应用函数的运行时测量过（CoreMark 2,942，
Wasmtime 为 44,534），没有选用它。

makepad 的计算内核也有一个 WebAssembly 后端，但只有在 makepad 本身编译到 `wasm32`
（浏览器）时才会运行。原生构建使用原生后端。

## 浏览器中的外壳

外壳保留了 makepad 窗口管理器的 `wasm32` 代码路径，也就是它的"web superbuild"：没有子
进程（`crates/shell/src/host.rs`），所有原生应用都在进程内托管（`tools/native_apps.py`
在 `crates/shell/src/native_apps.rs` 中为每个应用给出 `module` 或 `none` 的 `wasm`
托管方式，并拒绝 `process`），助手通过进程内的服务连接（`crates/shell/src/pane_links.rs`）。
App Hub 把这种宿主的平台报告为 `web`。

没有任何构建、CI 任务或发布版本产出它，而且它无法编译。下面这条针对浏览器目标的检查会失败：

```sh
cargo check --locked --keep-going --target wasm32-unknown-unknown -p octosense-shell --no-default-features
```

getrandom 0.2 和 0.3、uuid、fs2、ring 以及 aws-lc-sys 都无法为该目标构建。即使不启用
默认特性，外壳也会链接 `octosense-ai-host` 和 App Hub 的 crate，它们带进了其中大部分
（凭据加密、sigstore 校验、TLS、文件锁）；外壳自己的 `uuid` 在该目标上缺少随机数来源。
请把这些代码路径视为无人维护。

## 另见

- [ADR 0011](adr/0011-apps-own-functions-in-webassembly.zh-CN.md)：决定、测量过的运行时
  以及手机上的测量。
- [ADR 0012](adr/0012-app-host-api-discovery.zh-CN.md)：宿主 API 发现没有增加任何
  WebAssembly 加载。
- [ADR 0013](adr/0013-craft-engines-as-pinned-services.zh-CN.md)：`photo` 及其他 craft
  服务背后的引擎。
- App Hub 的[发布参考](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md)：
  `wasm` 能力、`functions` 检查项和 `wasm.<function>` 工具。
