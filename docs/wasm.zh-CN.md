# OctoSense 中的 WebAssembly

[English](wasm.md) | 简体中文

OctoSense 中有四处用到 WebAssembly。其中一处专为运行应用自己的代码而设计：应用自带的
函数，标准构建在 macOS、Windows、Linux、Android 和 OpenHarmony 上运行它们，属于有限支持。两个引擎的插件已经关闭：
`photo` 背后的 photocraft 和 `vector` 背后的 vectorcraft，这两个服务都拒绝所有插件命令。另外两处是 makepad 的内部实现，以及
一个无法编译的浏览器构建。本页逐一说明：运行的是谁的模块、它能接触什么、哪些构建包含它、
如何检查过。本页描述当前源码。桌面版 0.1.0-rc.2 包含 ADR 0011 核心模块，
不包含 ADR 0014 组件及下文的新声明策略。源码、测试与发行状态分别记录。
参见[应用能力与执行边界](capabilities.zh-CN.md)。

| 位置 | 谁的模块 | 运行时 | 构建 | 状态 |
| --- | --- | --- | --- | --- |
| 应用自带的函数：`wasm` 服务（[ADR 0011](adr/0011-apps-own-functions-in-webassembly.zh-CN.md)） | 已准入应用的 `fns/*.wasm` 或精确固定的共享组件 | Wasmtime 49，由 Cranelift 编译（`crates/wasm-host`）；组件的 WASI 0.2 来自 `wasmtime-wasi` | macOS、Windows、Linux、Android 和 OpenHarmony 上的每个标准桌面版和 Home 构建（特性 `wasm-functions`；OpenHarmony 上在 Wasmtime 的解释器 Pulley 中运行）；iOS 不包含 | 核心模块已进入桌面 rc.2；历史 OnePlus 验收仅覆盖核心模块。组件和新策略仍需新验收及发行版 |
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

[ADR 0014](adr/0014-app-components-in-webassembly.zh-CN.md)（已接受；发布尚未完成）增加了第二种
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
- **文件。** 应用可用的存储文件夹，即其脚本的 `fs.*` 看到的那个，就是组件的 `/`。
  按账户隔离的应用需要当前已连接账户。设备上的其他路径不可见；存储不可用或所需账户已退出时
  不预打开目录。省略 `storage` 声明不会取消存储。
- **存储配额。** 一次调用可以写入的量，是调用开始时应用配额的剩余部分。超出的写入会在组件
  内部失败：`ftruncate` 报告磁盘已满，普通写入报告 I/O 错误，因为 wasi-libc 把任何失败的
  流写入都报告为 I/O 错误。这时请求的错误以 "a write was refused: the storage budget is
  used up" 结尾。改写、截断和删除会归还字节，因此组件可以自己腾出空间。
- **网络。** 组件的 `wasi:http` 请求可以访问任何主机，与设备本身能访问的范围相同：应用的网络
  声明（`net`、`network.hosts`）在安装时展示，运行时不强制。2026 年 10 月 8 日的裁定取消了按应用
  的运行时闸门，边界是操作系统和宿主的 API 表面。固定版本的 Splash 运行时也不再强制执行
  清单中的主机列表。导入 `wasi:http` 的组件每次调用有 10 秒，而不是 2 秒；
  每个请求的连接、首字节和字节间超时都随调用结束。
- **宿主服务。** 通过 `octosense:host`
  （[`crates/wasm-host/wit/octosense-host.wit`](../crates/wasm-host/wit/octosense-host.wit)），
  组件通过 `request(service, args)` 以已准入应用的身份在 UI 线程上分派调用。
  声明列表不作为执行许可；各服务仍检查账户、同意、审阅和资源边界。组件不能获得面板、权限提示
  或可信用户输入，只能调用允许后台使用的方法。`wasm.*`
