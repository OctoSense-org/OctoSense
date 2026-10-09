# OctoSense 中的 WebAssembly

[English](wasm.md) | 简体中文

OctoSense 中有四处用到 WebAssembly。其中一处专为运行应用自己的代码而设计：应用自带的
函数，标准构建在 macOS、Linux 和 Android 上运行它们，属于有限支持。两个引擎的插件已经关闭：
`photo` 背后的 photocraft 和 `vector` 背后的 vectorcraft，这两个服务都拒绝所有插件命令。另外两处是 makepad 的内部实现，以及
一个无法编译的浏览器构建。本页逐一说明：运行的是谁的模块、它能接触什么、哪些构建包含它、
如何检查过。内容对应 #400 之后的 `main`（2026 年 10 月 8 日）。

| 位置 | 谁的模块 | 运行时 | 构建 | 状态 |
| --- | --- | --- | --- | --- |
| 应用自带的函数：`wasm` 服务（[ADR 0011](adr/0011-apps-own-functions-in-webassembly.zh-CN.md)） | 应用自己的应用包，`fns/*.wasm`，需要 `wasm` 能力 | Wasmtime 49，由 Cranelift 编译（`crates/wasm-host`）；组件的 WASI 0.2 来自 `wasmtime-wasi` | macOS、Windows、Linux、Android 和 OpenHarmony 上的每个标准桌面版和 Home 构建（特性 `wasm-functions`；OpenHarmony 上在 Wasmtime 的解释器 Pulley 中运行）；iOS 不包含 | 已接受，有限支持。测试在 macOS 和 Linux 上通过；一部 OnePlus 6 通过了手机验收。尚无发布版本包含它 |
| 引擎插件：`photo` 背后的 photocraft、`vector` 背后的 vectorcraft（[ADR 0013](adr/0013-craft-engines-as-pinned-services.zh-CN.md)） | 无：两个服务都拒绝所有 `plugin.*` 命令（#398、#405） | wasmi 2，解释器 | 链接进所有带 App Hub 的构建 | 已关闭：任何调用方都不能安装或运行插件 |
| Splash 的数学编译器（makepad） | makepad 根据 Splash 代码生成 | makepad-stitch，解释器 | 链接进所有构建 | 未使用：OctoSense 没有链接任何调用它的代码 |
| 浏览器中的外壳 | 外壳本身，为 `wasm32-unknown-unknown` 构建 | 浏览器 | 无 | 无法构建 |

## 应用自带的函数：`wasm` 服务

[ADR 0011](adr/0011-apps-own-functions-in-webassembly.zh-CN.md)（已接受，有限支持）允许
应用在应用包里携带编译成 WebAssembly 的 Rust 函数，由外壳只为这个应用运行。它们用于脚本旁边
的轻量计算：解析、排序、排程、差异比较、格式转换。数值计算仍然交给 Splash 的计算内核。

### 模块

模块是放在 `fns/<name>.wasm` 的核心 WebAssembly，每个应用包最多 8 个，不使用 WASI。
那里的文件也可以是[组件](#组件)，组件两者都有。

- 它导出 `memory`、`octo_alloc(len) -> ptr` 和 `octo_free(ptr, len)`。
- 其余类型为 `(i32, i32) -> i64` 的导出都是函数：在 `(ptr, len)` 处取输入，返回
  `(out_ptr << 32) | out_len`。输出的第一个字节是状态，`0` 表示成功，`1` 表示出错，
  后面跟着错误文本。
- 唯一允许的导入是 `octo.log(ptr, len)`。导入其他任何东西的模块在加载时就被拒绝。
- 它有一个内存，最多一个表。

客体 crate
[`apps/wasmlab/guest/octosense-guest`](../apps/wasmlab/guest/octosense-guest/src/lib.rs)
生成这一切：`export!` 用于 `fn(&[u8]) -> Result<Vec<u8>, String>`，`export_json!`
用于 serde 类型，另有 `log`。把 crate 构建成面向 `wasm32-unknown-unknown` 的
`cdylib`。面向 `wasm32-wasip1` 的构建和 wasm-bindgen 的输出会导入外壳不提供的函数，
因此无法加载；面向 `wasm32-wasip2` 的构建是[组件](#组件)，即服务运行的另一种文件。应用开发者请从 App Flow 的
[Run your own Rust code](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/RUST.md)
开始。

### 组件

[ADR 0014](adr/0014-app-components-in-webassembly.zh-CN.md)（提议）增加了第二种
`fns/<name>.wasm`：WebAssembly 组件，用 `cargo build --target wasm32-wasip2` 从普通的
Rust crate 构建。服务按文件头区分两者，一个应用包可以同时携带两种。

- **无需胶水代码。** 组件导出的每个函数都是 `wasm.<function>`，名称用 snake_case：
  `save-html` 即 `wasm.save_html`，两种写法都接受。参数可以是按参数名作键的对象、按参数
  顺序排列的数组；只有一个参数时也可以直接传值。记录返回为对象，枚举返回为其名称，
  `list<u8>` 返回为 base64，`result<T, E>` 返回为其值，或作为这次请求的错误。
  `wasm.functions` 列出每个导出及其 WIT 签名；接收或返回资源的函数会被列为已跳过，并附原因。
- **状态。** 每个组件的实例在调用之间留在应用的工作线程里，因此可以保存已解析的文档或缓存。
  陷阱或超时会使实例作废，下一次调用得到新实例；更新、授权变化、撤回或工作线程退出（一分钟
  没有调用）都会结束它。
- **能接触什么。** 时钟和随机数。它的 stdout 和 stderr 成为应用的日志行，没有环境变量、
  参数和 stdin。导入 `wasi:cli`、`wasi:clocks`、`wasi:filesystem`、`wasi:http`、`wasi:io`、
  `wasi:random` 和 `octosense:host` 以外任何东西（例如套接字）的组件在加载时被拒绝。
- **文件。** 应用有 `storage` 能力（有账户的应用还需已登录账户）时，应用自己的存储文件夹，
  即其脚本的 `fs.*` 看到的那个，就是组件的 `/`。设备上的其他东西一概不可见；没有这个能力
  时没有文件系统。
- **存储配额。** 一次调用可以写入的量，是调用开始时应用配额的剩余部分。超出的写入会在组件
  内部失败：`ftruncate` 报告磁盘已满，普通写入报告 I/O 错误，因为 wasi-libc 把任何失败的
  流写入都报告为 I/O 错误。这时请求的错误以 "a write was refused: the storage budget is
  used up" 结尾。改写、截断和删除会归还字节，因此组件可以自己腾出空间。
- **网络。** 应用有 `net` 能力时，组件的 `wasi:http` 请求按脚本的规则访问应用的
  `network.hosts`：主机必须完全列出，不区分大小写，端口不限。请求使用 HTTPS，只有访问设备本身
  （`localhost`、`127.0.0.1`、`[::1]`）时才允许普通 HTTP。可以访问网络的组件每次调用有 10 秒，
  而不是 2 秒；每个请求的连接、首字节和字节间超时都随调用结束。被拒绝的请求在组件内部失败
  （`HttpRequestDenied`），应用的日志会说明原因："a request to … was refused: it is not one
  of the app's network hosts"。
- **宿主服务。** 通过 `octosense:host`
  （[`crates/wasm-host/wit/octosense-host.wit`](../crates/wasm-host/wit/octosense-host.wit)），
  组件可以用 `request(service, args)` 调用应用已获授权的宿主服务，与应用的脚本相同：同样的服务族，
  在 UI 线程上分派，但绝不打开面板、不询问用户，因此只能调用后台界面可以调用的方法。`wasm.*`
  被拒绝，因为应用的工作线程正忙于这次调用；调用的截止时间限制等待时长。

ADR 0014 记录了开销。在 M 系列 Mac 上，313 KiB 的测试组件约 40 毫秒完成编译，从缓存加载
约 4 毫秒；它的 Markdown 函数比原生 Rust 慢约 2.6 倍，与同一段代码作为模块时接近。在 Pulley
中还要再慢约 32 倍。

### 模块如何到达设备

- **商店应用。** App Hub 的审核（应用契约 1.7.0）只接受放在 `fns/<name>.wasm` 的
  `.wasm` 文件（名称为 `[a-z0-9_-]`，最多 64 个字符），必须是核心模块（检查文件的前八个
  字节），最多 8 个，并且应用必须声明 `wasm` 能力。声明了 `wasm` 能力却没有模块会得到一条
  警告。审核不读取模块的导入和导出：外壳在加载模块时检查它们。商店会告诉用户：
  "Run its own sandboxed functions on this device"（在本设备上运行自带的沙箱函数）。规则见
  App Hub 的[发布参考](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md)
  （`functions` 检查项和 `wasm` 能力）。
- **系统应用。** 和其他系统应用一样，按摘要从打包方式的系统应用列表打包进外壳。
  `desktop/system-apps-wasm-lab.json` 和 `phone/system-apps-wasm-lab.json` 就是标准
  列表加上 `wasmlab`。

### 服务

`wasm` 宿主服务（`crates/shell/src/wasm_service.rs`）存在于 macOS、Linux 和 Android 上的
每个标准桌面版和 Home 构建中：即外壳的 `wasm-functions` 特性，两个包都默认开启（`wasm-lab`
是它以前的名字）。Windows、iOS 和 OpenHarmony 的构建不包含这个运行时
（`crates/shell/Cargo.toml`、`crates/shell/build.rs`），在那里调用会得到
`no service answers "wasm" on this device`。它回答两类请求：

- `wasm.functions`：应用的模块导出了哪些函数，每个模块如何加载（编译，还是取自缓存，
  用了多久），以及每个函数的运行情况。
- `wasm.<function>`：一次调用。对模块而言，字符串参数按原文传入，其他参数按 JSON 传入；
  JSON 输出作为数据返回，其他输出返回为 `{"text": …}`。组件的参数和结果带类型
  （见[组件](#组件)）。函数自己返回的错误、陷阱或超时都是这次请求的错误。

代码总是来自发起调用的应用自己的、已准入且摘要校验过的应用包，绝不来自参数；任何应用都
接触不到别的应用的函数。Card runner 的审核和工具执行器都要求 `wasm` 能力，服务在加载任何
东西之前还会再次检查已准入清单中的授权。

每次调用模块都使用全新的实例：一次调用的内存、全局变量、表和日志行都不会进入下一次调用，
成功调用或函数自己返回错误之后也一样。组件则在调用之间保留一个实例（见[组件](#组件)）。
只有编译后的代码会被复用：保存在应用的工作线程里，以及
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
| 一次调用的输入或输出 | 运行时 16 MiB；服务中序列化后的输入 8 MiB |
| 日志 | 每次调用 64 行，每行 1 KiB |
| 每个应用的模块 | 8 个 |
| 同时运行函数的应用 | 4 个工作线程，每个应用一个 |
| 排队的请求 | 每个应用 4 个 |
| 所有应用缓冲的输入 | 32 MiB |
| 一个请求（包括排队和加载） | 10 秒 |
| 空闲的工作线程 | 5 秒后退出；持有组件实例时 60 秒后退出 |

队列已满或没有空闲的工作线程时，请求会立即失败，不会等待。

App Hub 安装或更新应用时，外壳会在后台把它的函数编译进缓存（`wasm_service::warm`）：一次一个
应用，在单独的线程上，只编译已准入的应用包，并且只在应用有 `wasm` 授权时进行。这样应用的第一次
调用就从缓存加载，不必等待 Cranelift 编译。

### Agent 工具

应用的 `tools.json` 可以用 `"host_method": "wasm.<function>"` 把一个工具映射到自己的
某个函数。这需要 `wasm` 能力。与共享宿主方法不同，它没有最低风险等级，也不需要
`"private_data": true`：模块只能看到传给它的参数，组件还能看到自己应用的文件夹，与应用的
脚本相同。两者都接触不到共享数据。Wasm Lab 把 `wasmlab.find_slots`、
`wasmlab.rank` 和 `wasmlab.diff` 映射到 `wasm.find_slots`、`wasm.fuzzy_rank` 和
`wasm.text_diff`，风险等级都是 `read`。

### 平台

| 平台 | 状态 |
| --- | --- |
| macOS | 运行。已在隐藏的桌面外壳中验证（ADR 0011）。Wasmtime 在这里用 Mach 异常端口而不是信号捕获陷阱。 |
| Linux | 运行。运行时和服务的测试在 x86_64 上通过，陷阱由信号捕获；Wasm Lab 在无头桌面中运行过（2026 年 10 月 8 日）。 |
| Android | 在 Home 的默认构建中运行。需要 libc 0.2.190 或更高版本，`crates/wasm-host` 已要求这一点：在 0.2.189 下，每个陷阱都会结束进程。已在 Redmi Note 12 上验证（ADR 0011），并在一部 OnePlus 6 上通过[手机验收](#手机验收)。运行时增加约 7.5 MiB 代码；在 Snapdragon 685 上编译一个 433 KiB 的模块约需 0.4 秒，从缓存加载需 5–11 毫秒。 |
| Windows | 包含（ADR 0014）。[`wasm-windows.yml`](../.github/workflows/wasm-windows.yml) 在 GitHub 的 `windows-2022` 上运行运行时的测试，并编译带这个服务的外壳。在 Windows 上运行桌面版**未验证**。 |
| iOS | 不包含。那里的应用不能生成代码，所以运行时会像在 OpenHarmony 上一样使用 Pulley；但还没有编译过带这个服务的 iOS 构建。 |
| OpenHarmony | 包含，在 OpenHarmony 的代码生成策略明确之前于 Wasmtime 的解释器 Pulley 中运行：Cranelift 编译为 Pulley 字节码，没有任何东西以原生代码运行。Home 的 OpenHarmony 发布构建已带着这个服务编译通过（2026 年 10 月 9 日，`cargo-makepad makepad ohos … deveco -p octosense-home --release`）；还没有在设备上运行过（**未验证**）。Pulley 比 Cranelift 慢约 32 倍（ADR 0014）。 |

在受管理的 Mac 上，Microsoft Defender 会把新写入的缓存文件的第一次打开拦住约一秒；具体
测量见 ADR 0011。

### Wasm Lab

Wasm Lab（`os.wasmlab`，`apps/wasmlab/`）是演示这个服务的系统应用。它只是演示：标准构建
不包含它。它的函数（`apps/wasmlab/guest/functions`）包括 Markdown 转 HTML
（pulldown-cmark）、在忙碌时段之间找空闲时段、模糊排序（strsim）、按行比较差异（similar），
以及 `rogue`：按要求无限循环、无限分配内存、panic 或无限递归。它的界面通过 `host.request`
调用每个函数，并显示返回结果、往返耗时和 `wasm.functions`。

构建一个包含它的桌面版，然后以隐藏窗口启动并直接打开 Wasm Lab，通过远程控制接口操作
（`/help` 列出路由；用 `/quit` 结束）：

```sh
OCTOSENSE_SYSTEM_APPS=$PWD/desktop/system-apps-wasm-lab.json cargo build --locked -p octosense
MAKEPAD_HIDE_WINDOWS=1 target/debug/octosense --remote=47631 --test-action launch-wasmlab
```

在 Linux 构建主机上（2026 年 10 月 8 日；默认的调试构建，隐藏窗口，运行在无头 Weston 下），
模块第一次使用时编译用了 98 毫秒，之后从缓存加载用 4 毫秒，重启后也是如此。每种异常行为都
以错误结束，外壳一直在运行：无限循环在 2,039 毫秒后因截止时间结束，无限分配在 340 毫秒后
撞到内存上限，panic 和无限递归各在 37 毫秒后结束（均为脚本测得的时间）。

Wasm Lab 打开时先发出四个调用，等它们都有了答复再请求 `wasm.functions`：服务为每个应用
最多排队四个请求。`wasm.functions` 统计的是应用的工作线程启动以来的调用；工作线程空闲五秒
后退出，下一个工作线程重新开始计数。

已提交的模块可以逐字节重建：使用 rustc 1.97.1 时，`apps/wasmlab/guest/build.sh` 写出
完全相同的 `bundle/fns/wasmlab.wasm`（SHA-256 `f4a69c32…`）。首次需要安装目标：
`rustup target add wasm32-unknown-unknown`。

### 测试

```sh
cargo test --locked -p octosense-wasm-host
cd phone && cargo test --locked --features mobile-apps -p octosense-shell wasm_service::tests
```

- `crates/wasm-host/tests/runtime.rs` 运行一个手写的客体：输入和输出、客体错误、截止
  时间、内存、表和栈的上限、陷阱、导入规则、每个实例各自的状态、线程、大小上限、缓存及其
  并发写入、取消。
- `crates/wasm-host/tests/guest.rs` 运行 Wasm Lab 的模块，把每个函数的结果与同一段 Rust
  原生运行的结果比较，然后逐一运行 `rogue` 的各种模式。
- `crates/wasm-host/tests/component.rs` 运行四个由 `crates/wasm-host/tests/component-guest`
  构建的组件：以 JSON 调用未作修改的 crate（pulldown-cmark）、记录与 snake_case 名称、
  同一实例保留状态、时钟、随机数与字节、只能访问授予的文件夹（也包括只读）、存储预算、
  超时与内存上限；`fetch`，它的 `wasi:http` 只有在主机获准时才能访问本地服务器，向从不应答的
  服务器发出的请求在截止时间结束；`hostcall`，它通过嵌入方调用宿主服务；以及拒绝导入套接字的组件。
- 服务测试覆盖工具映射、输入上限、每次调用使用全新实例、更新或授权变化或撤回之后的撤销、
  取消和队列上限，以及应用自己的函数回答它的工具和脚本。对组件，测试覆盖带类型的调用、
  实例的状态、应用的存储文件夹及其配额、没有存储能力的应用、作为系统应用经 App Hub 准入发布的
  组件、只能访问应用主机的请求、只能到达已授权服务的宿主调用，以及已安装应用的第一次调用从缓存加载。
- `cargo run --release -p octosense-wasm-host --example measure_component` 打印组件
  相对于模块和原生 Rust 的耗时。
- `OCTOSENSE_WASM_PULLEY=1 cargo test --locked -p octosense-wasm-host --features pulley`
  在 Pulley 中运行同样的测试，与 OpenHarmony 的运行方式相同；`phone.yml` 也运行它。

测试组件可以逐字节重建：用 rustc 1.97.1，`crates/wasm-host/tests/component-guest/build.sh`
写出相同的 `notes.component.wasm`（SHA-256 `963c965b…`）、`netprobe.component.wasm`
（`98b2402a…`）、`fetch.component.wasm`（`6eaf6ac8…`）和 `hostcall.component.wasm`
（`d39f2a8e…`）。只需添加一次目标：`rustup target add wasm32-wasip2`。

`phone.yml` 运行第一条命令，服务的测试则作为 Home 测试的一部分运行（`mobile-apps` 包含
`wasm-functions`）；`tools/ci-local.sh --linux-host --offload` 在 Linux 构建主机上运行
两者。CI 中的每个桌面版和 Home 构建都包含这个服务，`tools/check-shell-graph.sh` 检查它的
运行时恰好在 macOS、Linux 和 Android 上链接。

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

- 还没有任何发布版本包含这个服务：#400 之后从 `main` 构建的第一个桌面版和 Home 发布版
  将会包含。
- iOS：带这个服务（在 Pulley 中）构建 Home。
- 应用之间的 CPU 公平调度，以及磁盘缓存的上限：目前没有任何东西会清理它。
- 提前编译系统应用的函数：已安装应用的函数在安装时编译，系统应用的函数在第一次调用时编译。
- 手机上的组件：Android 和 OpenHarmony 构建链接了运行时，但还没有组件在设备上运行过（**未验证**）。
- 经 JSON 传字节很慢：1 MiB 的 `list<u8>` 以 base64 往返约需 18 毫秒。

## 引擎插件：`photo` 和 `vector` 服务

ADR 0013 的两个引擎各自托管 WebAssembly 插件，运行在纯 Rust 解释器 wasmi 中，插件注册表
属于进程而不属于某次调用：

- photocraft，`photo` 服务（`apps/photo/host-service`）背后的光栅引擎：滤镜插件，没有导入，
  每个模块 32 MiB，每个实例 512 MiB 内存，每次调用 5,000 万条指令，每次运行 60 秒。
- vectorcraft，`vector` 服务（`apps/vector/host-service`）背后的矢量引擎：对象滤镜和实时
  效果，没有导入，有指令预算和内存上限，每次运行使用全新实例。

`effect` 背后的 effectcraft 也有 WebAssembly 插件，但位于一个可选特性之后；OctoSense 没有
链接它的插件 crate。vectorcraft 的桌面 UI crate 还有第二个安装命令 `ui.installPlugin`；
OctoSense 也没有链接这个 crate。

**自 #398 起关闭。** `photo.run` 在引擎看到命令之前就拒绝所有 `plugin.*` 命令，
`photo.commands` 也不再列出它们。在 #398 之前，调用方可以用 base64 `data` 安装插件；插件
进入 photocraft 的进程级注册表，在调用结束后仍然存在，并为之后的每个调用方服务，限制也远比
`wasm` 服务宽松。一个像脚本的 `host.request` 那样调用服务的临时程序在 Linux 构建主机上验证了
这两种情况（2026 年 10 月 8 日）。在 #398 之前，`os.photos` 安装了 photocraft 的示例插件，
下一次调用列出了它，另一个系统应用 `os.notes` 在文档上运行了它。有了 #398，这些调用都被拒绝：
``photo.run: `plugin.install` is not available through the photo service``。

**`vector` 中由 #405 关闭。** `vector.run` 原先拒绝文件、文档、`app.*` 命令组以及类似路径的
参数，但没有拒绝 `plugin.*`，而 vectorcraft 的引擎可以用 base64 `dataBase64` 安装插件。同一个
临时程序在 #405 之前的 `main` 上验证了这一点：`os.notes` 安装了 vectorcraft 的示例插件，下一次
调用列出了它，另一个系统应用 `os.maps` 也看得到它，而 `vector.commands` 还提供
`plugin.install`。现在 `vector.run` 在引擎看到命令之前就拒绝所有 `plugin.*` id
（``vector.run: `plugin.install` is not available through the vector service``），
`vector.commands` 也不再列出它们。

两个引擎仍然链接 wasmi，两个服务也仍然只回答系统应用（`os.*`）。

**UI 线程。** `photo` 服务和其他引擎服务一样，在请求之内完成工作。App Hub 在 UI 线程上
分派脚本的 `host.request`，并在持有服务注册表锁的同时调用服务（`services::dispatch`）。所以
耗时长的 `photo.run` 会卡住 UI 和所有其他应用的请求，直到它返回
（[#399](https://github.com/OctoSense-org/OctoSense/issues/399)）。

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
