# ADR 0014：以 WebAssembly 组件运行应用自带的 Rust crate

[English](0014-app-components-in-webassembly.md) | 简体中文

状态：已接受；实现与发布验收分别追踪。第 1 至第 3 阶段于 2026 年 10 月 10 日合并到 OctoSense（#436、#451）、
App Hub（#186、#188、#189、#190）和 App Flow（#180、#181）。第 1 阶段（运行时验证原型）位于
`crates/wasm-host`（`src/component.rs`、`tests/component.rs`）。第 2 阶段的运行时和服务
部分位于 `crates/wasm-host/src/component/files.rs` 和 `crates/shell/src/wasm_service.rs`，
测量结果见下文。第 3 阶段的部分是 `component/net.rs`（`wasi:http`）、`component/host.rs` 与
`wit/octosense-host.wit`（`octosense:host`）、`Runtime::precompile` 与 `wasm_service::warm`
（安装时编译），以及 OpenHarmony 上和测试中的 Pulley；Windows 运行这个服务，其测试在 CI 中运行。
第 4 阶段中 App Hub 的部分是 App Hub #190（目录、审核和商店中的共享组件），OctoSense 的部分是 `wasm`
服务加载应用固定的共享组件（`wasm_service::shared_components`）。App Flow 的部分是 SDK 和 `tools/octo wasm`；iOS 暂不计划。
本 ADR 扩展 [ADR 0011](0011-apps-own-functions-in-webassembly.zh-CN.md)：核心模块照旧可用。
桌面 rc.2 和 Home beta.2 仅包含核心模块。
[共享组件验收](../../tools/fixtures/wasm-phone-lab/README.zh-CN.md#真实-github-共享组件验收)
现已记录真实发布者证明、带管理员证明的私有试运行目录，以及 macOS 和 OnePlus 6 上的实际商店安装和 Splash 工具执行。
较早的 `c5f0c5c1` 回执记录两端原生断言均为 28/28 通过，驱动分别为 12/12 和 13/13。
最终源码 `8b09e05d`（以相同树合并为 `40ca21da`）的完整 macOS 桌面通过 11/11 项
App Hub 检查；OnePlus 6 通过 28/28 项组件断言和 13/13 项驱动检查。独立 Host API Lab
在 macOS 和 OnePlus 6 上均通过 31/31 项原生检查，手机驱动全部 45 项也通过。详见
[最终组件证据](../../tools/fixtures/wasm-phone-lab/README.zh-CN.md#最终源码验收)和
[Host API 证据](../../tools/fixtures/host-api-lab/README.zh-CN.md#最终源码验收)。
公开目录和已安装的 Home 保持不变。[桌面 RC4](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4)
已从 `9266b008` 发布，Mac 归档包通过 11/11 项检查，
[验收已绑定最终文件](../../tools/fixtures/wasm-phone-lab/README.zh-CN.md#rc4-发布证据)。开发二进制和独立手机测试不证明发行 Home 升级、真实模型行为、性能或
OpenHarmony 设备执行。后续 [Windows 验证](../../tools/fixtures/wasm-phone-lab/README.zh-CN.md#windows-缓存验证)
在 `a8e170d4` 通过 42/42 项测试及 Shell 编译检查，以 `af205d9c` 合并。
该修复仅改测试，生产源码不变；它不属于 RC4 tag，也不证明 RC4 归档包验收。
当前行为见 [OctoSense 中的 WebAssembly](../wasm.zh-CN.md)及
[应用能力与执行边界](../capabilities.zh-CN.md)。

## 背景

ADR 0011 允许应用以 WebAssembly 核心模块的形式携带 Rust 函数。它们由 Cranelift
编译，速度很快，但与外界隔绝：
- 唯一的导入是 `octo.log`，因此无法访问时钟、随机数、文件或网络；
- 每次调用都使用全新实例，调用之间不保留状态；
- 输入输出都是字节或 JSON，经由手写的 ABI 传递；
- 开发者要从 OctoSense 复制一份 guest crate 才能编写。

crates.io 上的大多数 crate 要么需要上述缺失能力中的某些（时钟、随机数、文件），
要么需要在调用之间保留状态，例如已解析的文档、缓存或模型。因此能直接使用的 crate 很少。

浏览器解决过同样的问题：其中的 WebAssembly 经 JavaScript 导入平台 API，保留自身状态，
并由 `wasm-bindgen` 生成带类型的胶水代码。浏览器之外的标准做法是 WebAssembly
**组件模型**：带类型的接口（WIT）与 **WASI 0.2** 系统接口。Rust 自 1.82 起可直接构建组件
（`--target wasm32-wasip2`，Tier 2），我们已在发行版中链接的 Wasmtime 也实现了这两者
（`wasmtime-wasi` 49）。

我们希望开发者拿一个普通的 crate，用一条命令构建，就能在应用脚本中调用其函数，
无需胶水代码，也不引入新的信任：组件能访问的，只是其应用本来就能访问的东西。

## 决定

1. **组件与模块并存。** 应用包的 `fns/*.wasm` 可以是组件，也可以是核心模块。运行时按文件头区分
   （`component::is_component`）。模块沿用 ADR 0011 的约定。组件用普通的
   `cargo build --target wasm32-wasip2` 构建。
2. **带类型的函数，以 JSON 调用。** 组件导出的每个函数都能在应用脚本中直接调用，无需胶水代码：
   - world 自身的函数以 `wasm.<function>` 调用，导出接口中的函数以
     `wasm.<interface>.<function>` 调用。
   - `wasm.functions` 列出每个函数及其 WIT 签名。
   - 参数可以是按参数名作键的对象、按参数顺序排列的数组；只有一个参数时也可以直接传值。
   - WIT 类型与 JSON 的对应：
     - 记录（record）是对象，元组是数组；
     - 变体是 `"case"` 或 `{"case": 值}`，枚举是其名称，标志（flags）是名称列表；
     - 可选值是 `null` 或该值，`char` 是单个字符，数字会检查范围，`list<u8>`
       是 base64 文本（也接受数字数组）。
   - 函数返回 `result<T, E>` 时，结果为其值，或以 `E` 作为调用的错误。
   - 接收或返回资源（resource）的函数不能从脚本调用，会被列为已跳过并附原因。
3. **实例保留状态。** 每个组件一个实例，在应用的 worker 存活期间一直存在，因此组件可以保存文档、
   缓存或模型。持有实例的 worker 等待下一次调用一分钟，而不是五秒。陷阱（trap）或超时会使实例
   作废，下一次调用会得到新实例。应用更新、授权变更或撤回时实例会被丢弃，与模块相同。
4. **WASI 的范围由应用资源和账户决定。**
   - **始终提供：** `wasi:clocks`、`wasi:random`、`wasi:io` 和 `wasi:cli`。stdout 与 stderr
     成为应用的日志行（有上限）。环境变量、参数和 stdin 都为空。
   - **`wasi:filesystem`：** 使用应用可用的存储目录；按账户隔离的应用需要当前已连接账户。
     每次调用按引擎文件夹的规则决定。存储文件夹以读写方式预打开为 `/`，主机文件系统的
     其余部分不可见。没有可用目录时不预打开。`storage` 是使用披露声明。
   - **存储配额逐次写入计量。** 一次调用可以写入的量，是调用开始时应用配额的剩余部分。运行时把使文件
     变大的 WASI 调用（`write`、`set-size`，以及 `write-via-stream` 与 `append-via-stream` 返回
     的流）替换为计量增长的版本，把释放字节的调用（截断的 `open-at`、`unlink-file-at`）替换为
     归还字节的版本。超出预算的写入在组件内部失败，调用的错误会说明原因。我们没有采用"每次调用后
     检查"：一次调用在检查之前就可能写入数 GB，而且每次调用要遍历文件夹两次。
   - **`wasi:http` 出站（第 3 阶段）：** 可以访问任何主机。应用声明 `net`，让安装界面说明它会使用
     网络，但运行时既不强制 `net`，也不强制 `network.hosts`：2026 年 10 月 8 日的裁定取消了按应用的
     运行时闸门，边界是操作系统和宿主的 API 表面。把组件限制在应用的 `network.hosts` 内、后来又限制
     为公共 HTTPS 主机的做法，都已按该裁定否决。请求在客体之外等待，epoch 检查无法结束它，因此
     请求的超时被限制在调用的截止时间内；导入 `wasi:http` 的组件每次调用有 10 秒。
   - **`octosense:host`（第 3 阶段）：** `request(service, args)` 像应用脚本的 `host.request` 一样调用
     宿主服务，在 UI 线程上分派；自 makepad#118（OctoSense #450）起不检查清单声明的服务族。
     各服务保留实际的应用／账户、同意、审阅与资源检查，但不打开面板、不询问用户（只能调用后台界面可以调用的方法），并且绝不调用 `wasm.*`。
   - **从不提供：** `wasi:sockets`，以及上述包以外的任何导入。请求这些导入的组件在加载时被拒绝
     （`LoadError::Import`），App Hub 的审核闸门也会拒绝。
5. **无需编写 WIT。**
   - **Guest SDK（第 2 阶段）：** guest crate `octosense-component` 提供
     `#[octosense_component::export]` 属性，从普通的 Rust 签名推导 WIT world。支持的类型包括数字、
     `String`、`Vec<T>`、`Vec<u8>`、`Option`、`Result<T, String>`，以及标记为导出的结构体和枚举。
   - **工具（第 2 阶段）：** App Flow 的 `octo` 新增：
     - `octo wasm new`：生成 crate 模板；
     - `octo wasm build`：构建 crate、对照清单检查其导入，并写入 `fns/`；
     - `octo wasm doctor`：指出无法为 `wasm32-wasip2` 构建的依赖（`openssl-sys`、`libsqlite3-sys`
       等 C 库、线程、多线程运行时）并建议替代方案。
   - **分发：** SDK 发布到 crates.io（需维护者批准）；发布前使用 git 依赖。
6. **审核能看到组件的访问范围。** App Hub 的闸门读取每个组件的导入：
   - 拒绝允许集合以外的导入；
   - 记录文件系统、HTTP 和宿主服务导入供披露，不把匹配的能力声明作为执行许可；
   - 告诉审核者组件能访问什么，例如“其应用文件夹中的文件；网络”。
   组件需要新的应用合约版本。

## 阶段

| 阶段 | 范围 |
| --- | --- |
| 1. 运行时验证原型（本 PR） | `crates/wasm-host::component`：加载、检查导入、列出导出及其 WIT 签名、长期存活的实例、JSON 调用、上述 WASI 子集与存储预打开、超时、内存上限和日志。测试运行一个用普通 cargo 构建的、未作修改的 crate（`pulldown-cmark`），并拒绝导入 `wasi:sockets` 的组件。 |
| 2. 开发者可用 | shell 的 `wasm` 服务：从 `fns/` 加载组件、`wasm.<function>` 调用、按应用的实例、应用可用的存储目录，以及组件写入的存储配额计量。提高组件的输入上限。guest SDK 及其宏；`octo wasm new/build/doctor`；App Hub 闸门检查与合约版本；文档与示例应用。 |
| 3. 访问能力与平台 | 可访问任何主机的 `wasi:http` 出站（以 `net` 声明，不在运行时强制）；`octosense:host` 导入，以与 `host.request` 相同的检查调用主机服务；安装时编译，让手机跳过首次编译；OpenHarmony 在其 JIT 策略明确前使用 Pulley（Wasmtime 的解释器）；Windows 运行时测试在 CI 中运行。iOS 不在当前计划内。 |
| 4. 共享组件 | App Hub 目录中经过审核、带版本的组件，应用可以像 npm 包一样依赖它们。安装器负责校验，每个应用仍有自己的实例、账户和存储边界。 |

## 考虑过的替代方案

- **继续使用核心模块与 JSON（维持 ADR 0011 现状）：** 保持安全，但能直接运行的 crate 很少，
  所有类型都要手工编码。
- **自定义的 WASI 0.1（preview 1）模块 ABI：** 能提供文件和时钟，但没有带类型的接口，也用不上组件工具链。
  Rust 的 `wasm32-wasip2` 默认构建组件。
- **把原生 Rust 加载进 shell**（动态库，或 makepad 的 Relax 这类 JIT 编译的 Rust）：速度快、功能完整，
  但代码在 shell 进程中以其权限运行。Relax 作为实验跟进，第三方代码需要先有隔离。
- **自研 Wasm 运行时，或 wasmi：** Wasmtime 已经链接在内，以 Cranelift 编译，并实现了 WASI 0.2 与组件模型。

## 影响

- **shell 的依赖图变大。** 新增 `wasmtime-wasi` 49.0.2（仅 `p2`）和 `component-model` 特性，
  链接在 `wasm-host` 已链接的地方（特性 `wasm-functions`）。
- **组件使用所属应用的资源和身份：** 可用且有配额的应用私有存储、出站 HTTP，以及通过应用／账户和
  后台调用检查的可用宿主服务。`storage`、`net` 和 `network.hosts` 用于披露用途，省略它们不会拒绝执行。
  时钟和随机数对 Wasm 是新增的，但每个脚本本来就能使用。
- **常驻实例会更长时间占用内存。** 每应用 worker 上限（`MAX_WORKERS`）与内存上限约束其用量，
  空闲的 worker 照旧退出。
- **验证原型的大小：** `notes`（含 `pulldown-cmark` 与 `getrandom`）为 313 KiB，sockets 探测组件为 120 KiB。
- **磁盘已满看起来像 I/O 错误。** 无论宿主给出什么错误，wasi-libc 都把失败的流写入报告为 `EIO`，
  因此大多数被拒绝的写入在客体中表现为 I/O 错误，而不是 `ENOSPC`。运行时会把原因加到调用的错误
  和应用的日志中。

## 测量

在 Apple M5 Max、macOS 26.6.2 上运行
`cargo run --release -p octosense-wasm-host --example measure_component`（2026 年 10 月 9 日）。
除非该行另有说明，每个数字都是 200 次调用的中位数。模块是 Wasm Lab 的（ADR 0011），运行同一个
`pulldown-cmark`。

| | 组件 | 模块 | 原生 |
| --- | --- | --- | --- |
| Cranelift 编译（首次加载） | 37.5 毫秒（313 KiB） | | |
| 从缓存加载 | 3.6 毫秒 | | |
| 实例化（带存储文件夹） | 0.09 毫秒（0.10 毫秒） | | |
| Markdown 转 HTML，32 KiB | 468 微秒 | 400 微秒 | 183 微秒 |
| 渲染并写入一个 49 KiB 的新文件 | 678 微秒 | | 156 微秒（仅写入） |
| 写入一个 12 字节的文件 | 143 微秒 | | |
| 读回 49 KiB | 59 微秒 | | |
| 1 MiB 的 `list<u8>`，以 base64 往返 | 18.4 毫秒 | | |
| 最小的调用 | 0.2 微秒 | 0.3 微秒 | |

在 Pulley 中（`OCTOSENSE_WASM_PULLEY=1`、`--features pulley`，同一台机器），Markdown 函数作为组件
耗时 15.1 毫秒，作为模块耗时 14.1 毫秒，约为 Cranelift 代码的 32 倍；最小的组件调用为 0.4 微秒。
在 OpenHarmony 的代码生成策略明确之前，OpenHarmony 以这种方式运行。

计量配额需要知道调用开始时文件夹里有多少数据：遍历一次文件夹，与引擎的调用相同。10 个文件耗时
0.12 毫秒，1,000 个 2.0 毫秒，10,000 个 23.8 毫秒（50 次的中位数）。在 APFS 上反复改写同一个
文件很慢，原生代码也一样（每次改写的中位数约 6 毫秒），因此示例每次都写入新文件。

## 待定问题

第 3 阶段已定：组件绝不调用会打开面板或询问用户的宿主服务，这类调用属于脚本。

- 异步函数（WASI 0.3）和流式内容：第 3 阶段之后。
- iOS 不在当前计划内。任何后续支持方案都需要独立的 Home 构建与设备验收；Pulley 的测试结果不能证明 iOS 可用。
