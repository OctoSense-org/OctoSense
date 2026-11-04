# App Hub 发布参考

[English](PUBLISHING.md) | 简体中文

| 任务 | 参阅 |
| --- | --- |
| 分步提交应用 | [向 App Hub 提交应用](SUBMITTING.zh-CN.md) |
| 开发并检查第一个应用 | [开发你的第一个 Hub 应用](FIRST-APP.zh-CN.md) |
| 搭建工作区，构建 `hub` 和 `card-host`，运行应用并截图 | 应用开发工具集 OctoSense App Flow（原 Design Flow）的[快速上手](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md) |
| 在脚本中调用能力或宿主服务 | App Flow 的[能力](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/CAPABILITIES.zh-CN.md)和[脚本 API](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/SCRIPT-API.md) |
| 查询哪个 Shell 提供哪个宿主服务 | App Flow 的[宿主服务](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.zh-CN.md) |
| 准备图标 | [应用图标与随包素材](ICONS.zh-CN.md) |

准入检查决定应用包能否进入 Hub。用 `hub check` 自行运行准入检查：每违反一条规则，它都会报告一条拒绝或警告。通过准入检查，并不能说明应用能正常渲染、图标在小尺寸下依然清晰，或商店信息属实。这些由审核人员检查（[审核检查什么](SUBMITTING.zh-CN.md#8-审核检查什么)）。

## 当前源码策略：声明与授权

`capabilities` 和 `network.hosts` 用于向用户和审核人员说明应用预计使用的
API 与网络目的地。遗漏某个服务族或主机，不会阻止调用宿主已支持的公开 API。
这项策略已包含在[桌面 RC4](../README.zh-CN.md#下载兼容宿主)中。RC2 的历史行为另行记录，不代表当前的授权规则。

每个准入应用都有独立的存储沙箱、已解析的配额和网络模块。设备访问仍需逐应用
同意和系统权限；连接账户仍按应用、账户和提供商 scope 隔离。外部写入保留
原生审阅，Agent 保留用户启用和工具审核，访问其他应用的数据或工具仍需共享
授权。宿主私有用户资料和不带调用方身份的 `agent.notify` 不是公开 API。

`requires` 和 `host_api.required` 仍用于检查兼容性，不是能力授权。组件的
ABI 与导入验证、确切摘要、发布者证明、目录撤回、配额和平台支持检查继续生效。
遗漏使用声明可以产生审核警告。`AppPolicy::allows` 与 `allows_host` 是历史
声明查询接口；宿主不能把它们当作执行授权。请用 `runtime.list` 和
`runtime.describe` 查询实际方法及其平台、同意要求。

## 应用是什么

应用包是一个由文本和素材组成的目录。运行应用包的是**宿主**：OctoSense Shell（桌面版或手机版应用）或 `card-host`。每个应用都运行在自己的**隔离环境**（一个独立的沙箱化脚本运行环境）中，并受其**策略**约束：遵守独立存储边界、资源配额和宿主的实际同意要求。应用包不含原生代码。需要新原生代码的应用，要改为随 Shell 版本发布（[选择合适的交付路径](DEVELOPMENT.zh-CN.md#选择合适的交付路径)）。

入口文件决定应用的类型：

| 类型 | 入口 | 运行形式 |
| --- | --- | --- |
| 脚本应用 | 应用包根目录下的 `main.splash` | 用 Splash（Makepad 的 UI 脚本语言）编写的程序，有自己的状态、处理函数、存储和请求。 |
| 卡片应用 | 应用包根目录下的 `page.card` | 用 L0（OctoSense 的声明式卡片语言）编写的卡片，由宿主渲染为控件。卡片不含任何逻辑。 |

同时含有两个入口文件的应用包算作脚本应用。

脚本应用的应用包中有这些文件：

```text
my-app/
  manifest.json      应用是什么、可以做什么（必需）
  listing.json       商店展示的应用信息（必需）
  main.splash        程序本身（必需）
  assets/            图标和其他本地素材（图标必需）
  screenshots/       商店信息引用的截图，至少一个 PNG 或 SVG 文件（必需）
```

以 App Flow 的 [`templates/script-app/`](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/templates/script-app) 为起点，按照它的[脚本应用流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/script-app/FLOW.md)和[脚本 API](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/SCRIPT-API.md) 开发脚本应用。

引用应用包自带的素材时，使用 `{{assets}}` 占位符，例如 `http_resource("{{assets}}/assets/logo.png")`。宿主会把占位符替换为一个本地回环地址，该地址只提供这个应用包的内容。

第一方[系统应用](https://github.com/OctoSense-org/OctoSense/tree/main/apps)（`apps/<name>/bundle/`）是完整的示例。它们的 `os.` ID 是保留的，所以复制出来的应用需要换用自己的 ID。

卡片应用的应用包中有这些文件：

```text
my-app/
  manifest.json      应用是什么、可以做什么（必需）
  listing.json       商店展示的应用信息（必需）
  page.card          L0 卡片，即应用的界面（必需）
  page.data.json     绑定到卡片中的数据（可选）
  kit/               渲染卡片的控件 kit（必需）
  assets/            图标和其他本地素材（图标必需）
  screenshots/       商店信息引用的截图，至少一个 PNG 或 SVG 文件（必需）
```

用 App Flow 的[图像到卡片流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/image-to-card/FLOW.md)生成卡片、卡片数据和 kit。[L0 规范](https://github.com/OctoSense-org/OctoSense/blob/main/apps/appcard/a2app-l0/framework/l0.md)定义了这门卡片语言。

两类应用都可以在 `manifest.json` 旁附带自己的 Agent：`tools.json`、`AGENT.md` 和 `skills/<name>/`（[应用的 Agent 与工具](#应用的-agent-与工具)）。

提交的只有应用包。开发者说明、源码与工具、密钥、测试数据和审核包都要放在应用包之外（[安排仓库结构](SUBMITTING.zh-CN.md#1-安排仓库结构)）。

## 准入检查的规则

`hub check` 分两个阶段运行：先检查结构限制，再检查其余所有规则。

### 结构限制

违反结构限制的应用包不会得到报告。`hub check` 只输出一行 `hub: <reason>`，退出码为 1。加上 `--json` 时，它把同一原因报告为一条拒绝，检查项名称为 `bundle-invalid`。

| 限制 | 输出的原因 |
| --- | --- |
| 文件总大小最多 8 MiB（8,388,608 字节），不计 `manifest.json` | `the bundle exceeds the size limit` |
| 文件和目录最多 2048 个 | `the bundle exceeds the file count limit` |
| 目录嵌套最多 32 层 | `the bundle exceeds the directory depth limit` |
| 应用包根目录下有 `manifest.json`，最大 64 KiB | `<bundle>/manifest.json: No such file or directory (os error 2)`、`manifest.json exceeds the size limit` |
| 只能有普通文件和目录，不能有符号链接 | `<path>: only regular files and directories are allowed` |
| 路径可移植：名称为 UTF-8，不含 `\` 或 `:`，也没有空段、`.` 段或 `..` 段 | `not a portable bundle path: "<path>"` |
| 清单可以解析：没有未知字段或取值，`schema` 为 1，`requires` 中的每个特性都是已知的 | `manifest is not valid: …`、`manifest schema <n> is not 1`、`app <id> needs a newer host: …` |

### 检查结果

其余规则每违反一条，报告中就多一行**检查结果**，注明所属的检查项：`[refused] <check> (<path>): <detail>` 或 `[warning] <check>: <detail>`。只有问题出在单个文件或属性上时，才会出现 `(<path>)`。拒绝会终止准入，警告不会。

| 检查项 | 拒绝条件 | 警告条件 |
| --- | --- | --- |
| `digest` | `integrity.bundle_blake3` 与应用包不符。修改后为可编辑源码重新写入摘要；已封存的 GitHub Release 需要新的证明。 | |
| `publisher-signature` | 声明的 GitHub 证明验证失败（即使加了 `--allow-unsigned`）；清单未签名，且没有加 `--allow-unsigned`；或者旧格式的 Ed25519 签名验证失败，或指向未知的密钥（`publisher key "<id>" is not registered with this hub`）。 | 清单未签名，且加了 `--allow-unsigned`。 |
| `identity` | ID 以 `os.` 开头；ID 本身或其最后一段是保留名称（[ID 与保留名称](#id-与保留名称)）。 | |
| `contents` | 文件的扩展名不是 `.card`、`.json`、`.l0`、`.octoscript`、`.splash`、`.svg`、`.png`、`.jpg`、`.jpeg`、`.webp`、`.ttf`、`.otf`、`.txt` 或 `.md` 之一，也不是函数模块（`.wasm`，见 `functions`）。准入检查同样拒绝没有扩展名的文件（例如 `.DS_Store` 和 `LICENSE`）。 | |
| `functions` | 应用包的 `fns/` 中带了 8 个以上的 `.wasm` 文件。既不在 `fns/<name>.wasm`（名称为 `[a-z0-9_-]`，最多 64 个字符），也不在 `components/<blake3>.wasm`（以自己的摘要命名，见 `components`）的 `.wasm` 文件，或既不是 WebAssembly 核心模块（版本 1 的 8 字节文件头）也不是有效组件的文件，以 `contents-invalid` 拒绝；导入了 `wasi:cli`、`wasi:clocks`、`wasi:filesystem`、`wasi:http`、`wasi:io`、`wasi:random`、`wasi:sockets` 和 `octosense:host` 以外任何内容的组件也是如此。清单没有在 `requires` 中声明 `wasm-components-v1`，或导入套接字却未要求 `wasm.sockets@1` 时，组件在这一项被拒绝（[WebAssembly 组件](#webassembly-组件)）。除此之外，准入检查不检查核心模块内部；宿主在加载模块时检查它的导入和导出。 | 应用声明了 `wasm`，却没有带 `fns/*.wasm`，也没有指定共享组件。清单声明了 `wasm-components-v1`，`fns/` 中却没有组件。每个准入的组件都有一行说明它能访问什么。随包函数未披露 `wasm`，或组件导入文件/HTTP/套接字接口却未披露 `storage`/`net` 时，只产生警告，不会拒绝。 |
| `components` | 清单指定的某个共享组件不在 `--catalog` 给出的签名目录中、已在其中被撤回或摘要不同，或导入了宿主不支持的接口，或应用遗漏套接字组件所需的 `wasm.sockets@1` ABI。商店应用的应用包在 `components/` 中带有 `.wasm` 文件。系统应用的应用包（`--system-app`）缺少它固定的某个组件的 `components/<blake3>.wasm`，或带有它没有固定的组件（[共享组件](#共享组件)）。 | 每个解析成功的组件都有一行说明它能访问什么。不使用 `--catalog` 时，每个组件都会被报告为未解析。清单声明了 `wasm-shared-components-v1`，却没有指定任何组件。 |
| `contents-invalid`（文本和图片） | 文本文件（`.splash`、`.card`、`.json`、`.l0`、`.octoscript`、`.txt` 或 `.md`）超过 1 MiB 或不是 UTF-8；JSON 无法解析；PNG、JPEG 或 WebP 无法解码，或单边超过 4096 像素；商店信息中的图标不是正方形，或者是超过 1 MiB 或单边超过 1024 像素的位图。 | |
| `contents-invalid`（SVG） | SVG 无法解析；既没有数值形式的 `width` 和 `height`，也没有 `viewBox`；单边超过 4096 像素；或含有脚本、`foreignObject` 或 `on…` 事件属性。SVG 的样式（`style` 属性或 `<style>` 块）导入样式表、使用转义或注释，或让 `url()` 指向同一文件内 `#fragment` 以外的任何位置。 | |
| `entry` | 应用包中既没有 `main.splash` 也没有 `page.card`；`page.card` 不是有效的 L0；`page.data.json` 不是 JSON；应用包中既没有 `kit/native/<theme>/kit.json`，卡片所需的 OctoScript kit 模块也不齐全。 | |
| `resource-invalid` | 卡片对图片或字体的引用，或 SVG 的 `href`，指向应用包中没有的文件。检查结果会给出对应的 JSON 指针。见[字体](#字体)。 | |
| `assets` | 除 `manifest.json`、`listing.json` 和 Agent 文件外，某个 `.card`、`.json`、`.l0` 或 `.octoscript` 文件含有 `http://`、`https://`、`file://` 或 `../`。普通文档中的链接不属于素材加载。`.splash` 文件或 Agent 文件含有 `http://`、`file://` 或 `../`。遗漏 `network.hosts` 条目不会导致拒绝。 | |
| `secrets` | `.card`、`.l0`、`.octoscript` 或 `.splash` 文件声明了 `is_password: true`，或把 `TextInputContentType` 设为 `Password`、`NewPassword` 或 `OneTimeCode`。 | |
| `storage` | | 没有披露 `storage` 用途时，某个 `.splash` 文件调用了 `fs.*`，或应用请求了 `camera`。`declarations:` 行仍显示带配额的独立存储。 |
| `listing` | 缺少 `listing.json`，或它违反了[商店信息](#商店信息)中的规则；商店信息未指定截图或未指定图标；指定的截图或图标不在应用包中。 | |
| `tools`、`agent`、`skills` | `tools.json`、`AGENT.md` 或某个技能不符合 [Agent 文件的规则](#agent-文件的规则)。 | 某个工具是破坏性工具（风险为 `destructive`）或对外工具（带有 `outward`），每次调用都要等待批准；某个工具写了 `confirm: "app"`；应用声明了工具，但 `agent.model.needs` 中没有 `tool_calling`；后台 Agent 带有破坏性工具。 |
| `policy` | 某项能力未知；ID 违反了 [ID 与保留名称](#id-与保留名称)中的规则；版本为空；某个主机不是纯主机名（[网络主机](#网络主机)）；`research` 范围或某个 `agent` 字段违反了相应规则；`storage.cache_max_bytes` 为 0。 | |
| `version`（使用 `--catalog` 时） | 签名目录中已有该应用的这个版本。 | |
| `continuity`（使用 `--catalog` 时） | 违反了[发布者连续性](#发布者连续性)中的规则，或签名目录的历史记录自相矛盾。 | |

### 字体

卡片用 `font_src` 指定字体，写在 `page.data.json` 的布局项（placements）中，或原生 kit 的组件样式中。准入检查接受应用包中的字体文件，以及锁定运行时提供的以下内置资源名称：

```text
makepad_widgets:resources/Inter.ttf
makepad_widgets:resources/LXGWWenKaiRegular.ttf
makepad_widgets:resources/LXGWWenKaiBold.ttf
```

打包的 `.ttf` 或 `.otf` 字体用相对于应用包的路径指定，例如 `"font_src": "assets/Body.ttf"`。已安装应用的卡片运行器和 `card-host` 先对 kit 样式和字体 token 求值，再解析路径，并从该应用包的宿主素材服务加载文件。不要在应用包中填写 HTTP 地址；加载打包字体既不需要网络权限，也不需要外部字体 URL。

在原生 kit 中，`font_src` 也可以是一个 token 引用 `{"$token": "<name>"}`，只要 kit 的 `tokens` 中这个 token 的 `value` 是这样的路径。准入检查核对的是 token 解析后的路径；其他对象或数组一律拒绝，报 `font_src must be a bundled font path, a supported built-in font, or one token resolving to a string`。

准入检查会拒绝其他内置名称和 crate 路径。如果需要上述资源以外的字体，请在许可允许的范围内随包提供字体子集。

请随包保留字体所要求的许可证和来源说明。普通 `.txt` 和 `.md` 文档中的链接不属于素材加载，也不会授予网络权限。Agent 指令、技能和脚本仍拒绝不安全的引用（`http://`、`file://`、`../`），但它们的 HTTPS 链接不必匹配 `network.hosts` 条目。卡片数据和 SVG 素材保留更严格的资源检查。

打包字体计入 8 MiB 上限，所以较大的 CJK 字体请只打包所需的子集。准入检查只核对字体路径，不解码字体文件，因此请在 `card-host` 中测试完整的应用包。用 Makepad 的 International 字体集构建的宿主（例如 `card-host`）还会用内置的 CJK 后备字体显示中文，这个字体在首次用到时才加载。

OctoSense 桌面版 0.1.0-beta.2 发布时还没有加载打包字体的功能：它会安装带打包字体的应用，但卡片不会加载这些字体。要在该版本上显示中文，请用角色 kit 组合卡片，并且不设 `font_src`。角色 kit 是 `Surface`、`TextTitle`、`TextBody` 等组件背后的 OctoScript 模块集合：把 OctoScript-Makepad 的 `components/l0/` 中的 `_kit.octoscript`、`_derive.octoscript`、`_derive_color.octoscript`、`_palette_light.octoscript` 和 `_palette_dark.octoscript` 复制到应用包的 `kit/` 中。它会用 Makepad 内置的 CJK 字体 LXGW WenKai（霞鹜文楷）显示中文。

默认情况下，卡片字体缺少的字形，Makepad 会改用操作系统的字体来绘制，所以在 macOS 上，字体没有加载也可能看不出来。检查卡片时，请关闭这项回退：

```sh
MAKEPAD_SYSTEM_FONTS=0 tools/octo run ~/apps/my-app/bundle --port 8141 --detach
```

这时，卡片字体和渲染器后备字体都不包含的字形会显示成方框。

脚本应用可以附带字体。在文字样式的 `FontMember` 中通过 `{{assets}}` 占位符指定字体文件；该文件计入 8 MiB 上限：

```splash
Label{text: "Hello" draw_text.text_style: TextStyle{font_family: FontFamily{latin := FontMember{res: http_resource("{{assets}}/fonts/<file>.ttf") asc: 0.0 desc: 0.0}} font_size: 20}}
```

未验证：OctoSense Shell 如何绘制这些字体。

### WebAssembly 组件

`fns/` 中的文件也可以是 WebAssembly **组件**，而不是核心模块：一个用 `cargo build --target wasm32-wasip2` 构建的普通 Rust crate（[OctoSense ADR 0014](https://github.com/OctoSense-org/OctoSense/blob/9266b0083544d86bd7636543b5ff60c61b26460f/docs/adr/0014-app-components-in-webassembly.zh-CN.md)，已接受）。它的函数接收和返回带类型的值，在调用之间保留状态，并可以使用 WASI 的一部分。OctoSense #453 已合并组件加载以及 HTTP/宿主服务适配器，[桌面 RC4](../README.zh-CN.md#下载兼容宿主)包含这些支持；每个提交的应用仍须单独验收。历史 RC2 只运行核心模块。

准入检查在以下条件都满足时接受组件：

- 清单在 `requires` 中要求 `wasm-components-v1` ABI；`wasm` 只是使用披露。`wasm` 服务只能加载核心模块的宿主不认识这个特性，因此会拒绝这个应用，而不是等到第一次调用时才失败；
- 组件能通过验证，且只从 `wasi:cli`、`wasi:clocks`、`wasi:filesystem`、`wasi:http`、`wasi:io`、`wasi:random`、`wasi:sockets` 和 `octosense:host` 导入接口。组件自己定义的类型（例如某个函数返回的记录）不算导入。其他包的导入以 `contents-invalid` 拒绝；
- 组件可以导入 `wasi:filesystem`，但只能访问本应用带配额的独立存储。按 ADR 0014，宿主只给它应用自己的存储文件夹；
- `wasi:http` 使用宿主支持的网络传输。`wasi:sockets` 导入（普通 TCP、UDP 和 DNS，并非需要特权的原始 IP 套接字）还要求 `host_api.required: {"wasm.sockets": 1}` 以及 `requires: ["host-api-v1"]`；`net` 和 `network.hosts` 用于披露用途，不是运行时目的地白名单；
- `octosense:host` 以所属应用的身份路由，保留实际同意、账户和共享检查；组件不能打开前台授权面板。

套接字准入是**尚未发布的源码支持**，桌面 RC5 不支持。保留组件特性，并添加已有的可扩展宿主 API 声明：

```json
"requires": ["wasm-components-v1", "host-api-v1"],
"host_api": {"required": {"wasm.sockets": 1}},
"capabilities": ["wasm", "net"]
```

即使导入列表只有 DNS 或套接字类型接口，也必须声明这项要求。可选声明或不同 ABI 版本不能满足要求。没有 `wasm.sockets@1` 的宿主仍能读取签名目录，但会在安装前拒绝这个应用，并在启动时再次检查。不要为套接字新增 `requires` 特性：RC5 会拒绝包含未知特性的整个目录。套接字配额、调用超时、取消和清理由宿主负责；声明不会授予系统权限。运行套接字代码前，请查询实际宿主的运行时清单。

组件遵守与模块相同的名称和大小规则，并计入 8 个函数文件的上限。每个准入的组件都有一条警告，告诉审核人员它能访问什么：

```text
[warning] functions (fns/notes.wasm): fns/notes.wasm is a component that reaches the clock, random numbers and files in its app folder, but no network or other app
```

导入 `wasi:http` 的组件（这里是 App Hub 的 `fetch` 测试组件，所在应用有 `net` 能力，没有
`network.hosts`）会得到：

```text
[warning] functions (fns/fetch.wasm): fns/fetch.wasm is a component that reaches the clock and the network, but no files or other app
```

没有 `net` 使用声明时，准入检查提出披露警告：

```text
[warning] functions: fns/fetch.wasm imports wasi:http; disclose net usage
```

导入 `octosense:host` 的组件（App Hub 的 `hostcall` 测试组件，所在应用只有 `wasm` 能力）会得到：

```text
[warning] functions (fns/hostcall.wasm): fns/hostcall.wasm is a component that reaches the clock and its app's host services, but no files, network or other app
```

### 组件列出的 crate

组件可以在名为 `octosense-crates` 的自定义段中说明它由哪些 crate 构建：JSON
`{"schema": 1, "crates": [{"name", "version", "source", "checksum"}]}`，其中 `source` 是
`crates.io`、`git+<url>#<commit>` 或 `path`。App Flow 的 `tools/octo wasm build` 会写入它；App Flow
的发布工作流也会写入：它从打了标签的提交构建组件，并从其 `Cargo.lock` 取得列表，因此 GitHub 为两者出具证明。
列表只包含二进制文件实际链接的内容：构建脚本和过程宏用到的 crate 不在其中。准入检查无法把列表与代码核对，
只把它展示给审核人员。下面是带有 App Flow 为它写入的列表的 notes 组件：

```text
[warning] functions (fns/notes.wasm): fns/notes.wasm is built from 8 crates: bitflags 2.13.2, cfg-if 1.0.5, getrandom 0.4.3, memchr 2.8.3, pulldown-cmark 0.13.4, pulldown-cmark-escape 0.11.0, unicase 2.10.0 and wit-bindgen 0.62.0
```

没有这个段的组件也会被接受，并得到：

```text
[warning] functions (fns/hostcall.wasm): fns/hostcall.wasm does not list the crates it is built from (its octosense-crates section); tools/octo wasm build and App Flow's release workflow add it
```

有两个这样的段，或者段的内容不是 crate 列表时，以 `contents-invalid` 拒绝。`hub check <bundle> --advisory-db <dir>`
（`<dir>` 是 [RustSec 安全公告数据库](https://github.com/rustsec/advisory-db)的检出）还会报告每个受公告影响的
crates.io crate。下面几行来自一个列出两个旧 crate 的组件，对照的是 2026 年 10 月 9 日的数据库：

```text
[warning] advisories (fns/notes.wasm): fns/notes.wasm includes smallvec 0.6.9, which RUSTSEC-2019-0009 reports as a vulnerability: Double-free and use-after-free in SmallVec::grow(); fixed in >= 0.6.10
[warning] advisories (fns/notes.wasm): fns/notes.wasm includes time 0.1.43, which RUSTSEC-2020-0071 reports as a vulnerability: Potential segfault in the time crate; fixed in >= 0.2.23
```

信息性公告（不再维护、不健全）会按其性质报告，已撤回的公告不报告。准入检查不会自己下载数据库。

`hub component-info <file.wasm>` 输出函数文件的类型（`component` 或 `module`）、它的导入，以及它导出的函数，每个函数都附上 WIT 写法的参数和结果。下面在一个应用包中运行它，包中的 `fns/notes.wasm` 就是 App Hub 测试中的 notes 组件（`crates/app-hub/tests/fixtures/notes.component.wasm`），省略处以 `…` 标出：

```console
$ hub component-info fns/notes.wasm
{
  "kind": "component",
  "imports": [
    "wasi:io/poll@0.2.9",
    "wasi:clocks/monotonic-clock@0.2.9",
    …
    "wasi:filesystem/preopens@0.2.9",
    "wasi:random/insecure-seed@0.2.9"
  ],
  "exports": [
    {
      "name": "analyze",
      "params": [
        [
          "markdown",
          "string"
        ]
      ],
      "result": "record { words: u32, lines: u32, headings: list<string> }"
    },
    …
  ]
}
```

对核心模块，它输出 `"kind": "module"`，导入写作 `module.name`，导出的函数附上核心类型，参数依次命名为 `p0`、`p1` 等。

`wasm.<function>` 工具的 [`host_method` 规则](#把工具映射到共享服务host_method)是为核心模块制定的，核心模块的函数只看得到自己的输入。目前这些规则原样适用于组件，尽管组件可以读取有配额的应用私有文件。

## 共享组件

**共享组件**是 App Hub 在签名目录中单独发布的 WebAssembly 组件，可供多个应用使用，就像共享 npm 包一样。与 npm 依赖不同的是，它被精确固定：应用指定一个确切版本和文件的 BLAKE3 摘要，所以已安装应用的代码只会随应用更新而改变（[ADR 0003](adr/0003-shared-components.zh-CN.md)，即 [OctoSense ADR 0014](https://github.com/OctoSense-org/OctoSense/pull/436) 的第 4 阶段）。App Hub 像审核应用一样审核共享组件，并像验证应用自己的组件一样验证它（[WebAssembly 组件](#webassembly-组件)）。它能访问的，只有使用它的应用可以访问的内容：每个应用都以自己的身份、资源上限和实际授权运行自己的实例。

共享组件需要兼容宿主，并在其经过验证的目录中发布。[桌面 RC4](../README.zh-CN.md#下载兼容宿主)实现共享组件的安装与执行；发布该宿主不会自动把组件或应用发布到公开目录，它们仍须通过 Hub 审核并获得发布批准。较旧宿主（包括桌面 RC1 和 RC2）会拒绝包含组件、或包含固定了组件的应用的整个目录；维护者规划目录迁移时仍须考虑这些客户端。

### 使用共享组件

在清单的 `components` 中列出每个组件，并在 `requires` 中声明 `wasm-shared-components-v1`，它标识所需的宿主 ABI：

```json
"capabilities": ["wasm", "storage"],
"requires": ["wasm-shared-components-v1"],
"components": [
  {
    "as": "markdown",
    "id": "org.example.markdown",
    "version": "1.0.0",
    "blake3": "c30d6e3125ff31676cad3c06735186d82cd0c017802afd344e3d2db876ff976c"
  }
]
```

| 字段 | 规则 |
| --- | --- |
| `as` | 应用给组件起的名字：`[a-z][a-z0-9_]{0,31}`，在清单中不能重复。 |
| `id` | 组件在签名目录中的 ID。 |
| `version` | 单个确切的语义版本，例如 `1.0.0` 或 `1.0.0-rc.1`。`^1.0.0` 这样的版本范围会被拒绝。 |
| `blake3` | 组件文件的 BLAKE3 摘要，64 个小写十六进制字符，即签名目录条目中的 `wasm_blake3`。 |

一个应用最多指定 8 个组件。没有该特性却写了 `components`（`components requires wasm-shared-components-v1`），或某个条目违反上述规则时，清单解析器会拒绝。不认识该特性的宿主会拒绝这个应用。只使用共享组件的应用不需要 `fns/`。

使用 `--catalog` 时，准入检查在该签名目录中解析每个组件。组件缺失（`is not in the catalog`）、已被撤回（`was withdrawn: <reason>`）或摘要不同（`pins blake3 …, but the catalog's file hashes to …`）时，`components` 检查项会拒绝这个应用。遗漏 `storage`、`net` 或 `wasm` 使用声明不会导致组件被拒绝。与随包组件相同，导入 `wasi:sockets` 的组件要求应用在 `host_api.required` 中包含 `"wasm.sockets": 1`，并在 `requires` 中包含 `host-api-v1`。准入检查根据目录中审核过的导入列表检查此要求。商店在下载或复用缓存组件前检查它，并在下载前及每次启动时检查宿主 API 是否可用。随后，每个组件都有一行说明它在这个应用中能访问什么。不使用 `--catalog` 时，准入检查会警告它无法解析这些组件。

下面的演练在本地开发 Hub 上运行（见[用开发 Hub 演练](#用开发-hub-演练)）。它的签名目录提供 `org.example.markdown` 1.0.0，即 App Hub 测试中的 notes 组件（`crates/app-hub/tests/fixtures/notes.component.wasm`）。应用 `writer` 固定了它，并披露 `wasm` 和 `storage` 用途：

```console
$ hub check writer --allow-unsigned --catalog dev-catalog.json --anchor "$(cat anchor.pub)"
org.example.writer 1.0.0 — PASSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [warning] components: component markdown (org.example.markdown 1.0.0) reaches the clock, random numbers and files in its app folder, but no network or other app
  declarations: capabilities {"storage", "wasm"}, hosts {}, storage 16777216 bytes, agent none
```

没有 `storage` 使用声明时，同一个应用仍可准入，独立存储和配额保持生效：

```text
  [warning] components: component markdown reaches files in its app folder; storage remains bounded
```

不使用 `--catalog` 时：

```text
  [warning] components: component markdown (org.example.markdown 1.0.0) is not resolved: check with --catalog to see that it is offered, matches its digest and what it reaches
```

商店应用的应用包绝不携带组件：准入检查拒绝 `components/` 中的 `.wasm` 文件，因为商店会从签名目录获取组件。随构建发布的系统应用没有签名目录，它把固定的每个组件放在应用包的 `components/<blake3>.wasm`。`hub check --system-app` 检查其中每个文件：它必须是有效组件、只有允许的导入、以自己的摘要命名，且只包含支持的导入。清单没有固定的文件会被拒绝。

对于这样的应用，商店的权限说明会多一行：`Runs shared components App Hub reviewed, with only this app's permissions: org.example.markdown 1.0.0`。

### 发布共享组件

组件 Release 由两个文件组成：用 `cargo build --target wasm32-wasip2` 构建的组件，以及描述它的 `<id>-<version>.component.json`。你只需写一份草稿，填写下面第一个表中的字段；其余字段由 `hub component-prepare` 从文件中得出：

```json
{
  "component": {
    "schema": 1,
    "id": "org.example.markdown",
    "version": "1.0.0",
    "name": "Markdown",
    "publisher": {
      "name": "Example Org",
      "support": "https://example.org/support",
      "privacy_policy_url": "https://example.org/privacy"
    },
    "license": "MIT OR Apache-2.0"
  },
  "listing": {
    "subtitle": "Markdown to HTML, with word counts",
    "description": "Renders CommonMark to HTML with pulldown-cmark and reports words, lines and headings.",
    "keywords": ["markdown", "html"]
  }
}
```

| 由你填写 | 规则 |
| --- | --- |
| `component.schema` | `1`。 |
| `component.id` | 应用 ID 的规则（[ID 与保留名称](#id-与保留名称)）；绝不以 `os.` 开头，也不能是签名目录中某个应用的 ID。 |
| `component.version` | 单个确切的语义版本。每个 Release 都是一个新版本，并且高于上一个已发布的版本。 |
| `component.name` | 1 到 64 个字符。 |
| `component.publisher` | 与商店信息中的 `publisher` 相同（[商店信息](#商店信息)）。 |
| `component.license` | SPDX 许可证表达式，例如 `MIT OR Apache-2.0`。 |
| `listing` | `subtitle`（最多 80 个字符）、`description`（必填，最多 4000 个字符）和 `keywords`（最多 10 个）。 |

| `hub component-prepare` 写入 | 来源 |
| --- | --- |
| `component.wasm_blake3`、`component.bytes` | 文件的 BLAKE3 摘要和大小。 |
| `component.imports`、`component.exports` | 文件本身，与 `hub component-info` 列出的相同。 |
| `component.integrity.github` | GitHub 身份参数，与应用相同（[GitHub 发布者来源证明](#github-发布者来源证明)）。不提供这些参数时，得到的是未签名的开发 Release。 |

```console
$ hub component-prepare markdown.wasm --draft component.json --out markdown.component.json
{"bytes":310601,"id":"org.example.markdown","schema":1,"status":"unsigned-development-release","version":"1.0.0","wasm_blake3":"c30d6e3125ff31676cad3c06735186d82cd0c017802afd344e3d2db876ff976c"}
```

与应用一样，Release 的 GitHub 工作流在公开仓库中、由推送 `v<version>` 标签触发，并运行：

```sh
hub component-prepare target/wasm32-wasip2/release/markdown.wasm --draft component.json \
  --repository OWNER/REPO --repository-id REPOSITORY_ID --owner-id OWNER_ID \
  --workflow .github/workflows/publish-component.yml \
  --tag v1.0.0 --commit IMMUTABLE_GIT_SHA \
  --out build/octosense-component.json
# actions/attest 只为 build/octosense-component.json 生成证明。
hub component-pack build/octosense-component.json \
  --wasm target/wasm32-wasip2/release/markdown.wasm \
  --attestation build/component-attestation.sigstore.json --out build/release
```

`build/octosense-component.json` 是不含证明的规范化 JSON 形式的 Release，也就是证明所覆盖的字节。它带有文件的摘要，所以证明绑定了文件。用示例身份准备时：

```console
$ hub component-prepare markdown.wasm --draft component.json --repository example-org/markdown --repository-id 123456789 --owner-id 987654321 --workflow .github/workflows/publish-component.yml --tag v1.0.0 --commit 0123456789abcdef0123456789abcdef01234567 --out build/octosense-component.json
{"bytes":310601,"id":"org.example.markdown","schema":1,"sha256":"ae2ff83c70842a4f8e2aa20de538a1ba94f3de76a85a5152e1bf7e716c0c5c34","status":"awaiting-github-attestation","subject":"octosense-component.json","version":"1.0.0","wasm_blake3":"c30d6e3125ff31676cad3c06735186d82cd0c017802afd344e3d2db876ff976c"}
```

`hub component-pack` 附加证明、运行准入检查，并在一个新目录中写出 `<id>-<version>.component.json` 和 `<id>-<version>.wasm`。没有能通过验证的证明时，它什么也不写：

```console
$ hub component-pack build/octosense-component.json --wasm markdown.wasm --out release
org.example.markdown 1.0.0 (component c30d6e3125ff31676cad3c06735186d82cd0c017802afd344e3d2db876ff976c) — REFUSED
  [refused] publisher-signature: publisher attestation is missing
  [warning] functions (org.example.markdown-1.0.0.wasm): org.example.markdown 1.0.0 is a component that reaches the clock, random numbers and files in its app folder, but no network or other app
hub: the component was refused
```

App Flow 现已包含组件发布工作流。上面的 CLI 记录不是 Release 证明：审核者必须验证实际工作流对确切组件描述符和 Wasm 字节生成的证明。

像提交应用一样提交组件（[向 App Hub 提交应用](SUBMITTING.zh-CN.md)）：开一个 issue，写明仓库、标签、commit，并附上两个 Release 文件。审核人员运行 `hub component-check`，即针对 Release 及其文件的准入检查：

```console
$ hub component-check markdown.component.json --wasm markdown.wasm
org.example.markdown 1.0.0 (component c30d6e3125ff31676cad3c06735186d82cd0c017802afd344e3d2db876ff976c) — REFUSED
  [refused] publisher-signature: App Hub accepts only GitHub-attested component releases
  [warning] functions (org.example.markdown-1.0.0.wasm): org.example.markdown 1.0.0 is a component that reaches the clock, random numbers and files in its app folder, but no network or other app
hub: the component was refused
```

| 检查项 | 拒绝的情况 |
| --- | --- |
| `component` | 上面两个表中的某个字段违反了规则，或 Release 的导入或导出与文件不一致。 |
| `digest` | 文件的 BLAKE3 摘要或大小与 Release 不一致。 |
| `size` | 文件超过 8 MiB。 |
| `contents-invalid` | 文件不是有效组件，或导入了 `wasi:cli`、`wasi:clocks`、`wasi:filesystem`、`wasi:http`、`wasi:io`、`wasi:random`、`wasi:sockets` 和 `octosense:host` 以外的任何内容。 |
| `publisher-signature` | Release 没有 GitHub 来源证明（使用 `--allow-unsigned` 时为警告），或证明验证失败。 |
| `identity`（使用 `--catalog` 时） | 签名目录中有应用使用了这个组件的 ID。 |
| `version`（使用 `--catalog` 时） | 签名目录中已有这个版本。 |
| `continuity`（使用 `--catalog` 时） | 仓库、所有者或工作流与早期版本不同，版本不高于最新版本，或历史记录自相矛盾。 |

`functions` 警告告诉审核人员组件导入了哪些接口。准入检查验证导入兼容性和精确固定的版本；运行时组件继续受调用方的应用身份、隔离目录、配额和实际授权约束。

App Hub 管理员用 `hub component-entry <release.json> --wasm <file.wasm> --catalog <authenticated catalog> --out <index.json>` 把已批准的 Release 变成签名目录候选内容，这需要 GitHub 来源证明。候选内容新增 `artifacts/<id>-<version>.wasm` 和 `index/components/<id>-<version>.json`（[通过 GitHub 发布目录](GITHUB-PUBLISHING.zh-CN.md#准备待审核候选)）。撤回时，与应用一样，把条目标记为已撤回并写明原因。

### 用开发 Hub 演练

`hub component-publish` 把组件加入一个用旧版密钥签名的签名目录，供用一次性密钥进行的本地演练使用。App Hub 自己的签名目录只通过 GitHub 管理员流程发布：

```console
$ hub keygen anchor.key > anchor.pub
$ hub keygen working.key > working.pub
$ hub certify --anchor anchor.key --working working.key > working.cert
$ hub component-publish markdown.component.json --wasm markdown.wasm --catalog dev-catalog.json --key working.key --anchor-cert "$(cat working.cert)" --publisher dev:example-org --out dev-hub --allow-unsigned
org.example.markdown 1.0.0 (component c30d6e3125ff31676cad3c06735186d82cd0c017802afd344e3d2db876ff976c) — PASSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [warning] functions (org.example.markdown-1.0.0.wasm): org.example.markdown 1.0.0 is a component that reaches the clock, random numbers and files in its app folder, but no network or other app
published component org.example.markdown 1.0.0 (catalog sequence 1)
```

`hub withdraw` 撤回组件的方式与撤回应用相同；之后，固定了它的应用会被拒绝：

```console
$ hub withdraw org.example.markdown --version 1.0.0 --reason "Renders raw HTML it should escape" --catalog dev-catalog.json --key working.key --anchor-cert "$(cat working.cert)"
withdrew org.example.markdown 1.0.0: Renders raw HTML it should escape (catalog sequence 2)
$ hub check writer --allow-unsigned --catalog dev-catalog.json --anchor "$(cat anchor.pub)"
org.example.writer 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] components: component markdown (org.example.markdown 1.0.0) was withdrawn: Renders raw HTML it should escape
  declarations: capabilities {"storage", "wasm"}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

### 在设备上

- **安装与更新**：安装或更新应用之前，商店从同一份已验证的签名目录中解析每个组件。它像获取应用包一样从 Hub 获取组件的 `artifacts/<id>-<version>.wasm`，并检查大小、摘要和有效性。每个摘要只保存一份、只读，放在 `<apps root>/.components/<blake3>.wasm`；应用绝不会在缺少组件的情况下被安装。
- **启动**：每次启动都会把应用的组件与应用包一起检查。组件缺失、被改动或被撤回，应用就不能运行，直到用户更新或重新安装它。
- **卸载**：通过商店删除应用（`Store::remove`），以及每次通过商店安装或更新，都会删除已没有任何已安装应用固定的组件。
- **宿主**：宿主的 `wasm` 服务用 `octosense_appstore::components::resolved(app_id)` 加载应用的组件，它返回每个组件的名字、ID、版本、摘要和已验证的文件。每次调用都会验证每个文件。系统应用的组件来自它自己的应用包。

**验收边界**：本指南中的准入与目录夹具不证明最终发布宿主能安装和运行某个确切的组件应用；须单独验证其构建、平台和发布字节。

## 清单

```json
{
  "schema": 1,
  "id": "com.example.forecast",
  "version": "1.0.0",
  "name": "Forecast",
  "integrity": { "bundle_blake3": "<written by hub stamp>" },
  "capabilities": ["storage", "net"],
  "network": { "hosts": ["api.open-meteo.com"] },
  "storage": { "max_bytes": 1048576 },
  "compute": { "instruction_budget": 5000000, "memory_bytes": 33554432 }
}
```

| 字段 | 含义 | 规则 |
| --- | --- | --- |
| `schema` | 清单的语法版本。 | `1`。 |
| `id` | 应用的身份。应用的存储文件夹以它命名，它的最后一段是应用工具的命名空间。 | 见 [ID 与保留名称](#id-与保留名称)。每个版本都保持不变。 |
| `version` | 本次发布的版本。 | 不能为空。每次发布都使用新值；Hub 从不替换已发布的版本。 |
| `name` | 用户看到的名称。 | |
| `integrity.bundle_blake3` | 应用包摘要。 | 由 `hub stamp` 写入。不要手工修改。 |
| `integrity.github` | GitHub 仓库/所有者 ID、工作流、标签、commit 和附加的 Sigstore 证明。 | 需要 `publisher-github-v1`，由 `publisher-prepare` 和 `publisher-attach` 生成。 |
| `integrity.signature` | `{key_id, value}`，即旧格式的 Ed25519 清单签名。 | 省略此字段：App Hub 只接受带 GitHub 证明的 Release（[签名](#签名)）。在 [App Hub #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168) 落地之前，准入检查仍会让用密钥签名的清单通过；审核人员会拒绝它。 |
| `capabilities` | 应用预计使用的能力。 | 封闭列表中的名称（[能力](#能力)）。 |
| `network.hosts` | 披露预计访问的网络目的地。 | 纯主机名，即使没有声明 `net` 也会校验格式（[网络主机](#网络主机)）。 |
| `storage` | `max_bytes`、`accounts`、`agent_workspace`、`cache_max_bytes`。 | 见[存储与配额](#存储与配额)。 |
| `compute` | `instruction_budget`、`memory_bytes`。 | 会限制在宿主的上限以内。 |
| `agent` | 应用自己的 Agent。 | 可选（[清单中的 `agent`](#清单中的-agent)）。 |
| `research` | `research` 和 `crawl` 的范围。 | 声明 `research` 或 `crawl` 时必需；即使未声明这两项，也接受有效的范围（[research 范围](#research-范围)）。 |
| `requires` | 应用需要的宿主特性。 | 每一项都必须是宿主已知的特性：`palpo-admin-v1`、`host-api-v1`、`backend-api-v1`、`script-tools-v1`、`publisher-github-v1`、`wasm-components-v1` 或 `wasm-shared-components-v1`。GitHub 发布需要真实来源证明验证器；三个 API 标记还需要宿主实现它们对应的 API（[宿主 API 兼容性](HOST-API.zh-CN.md)）。`wasm-components-v1` 和 `wasm-shared-components-v1` 表示所需的组件 ABI（[WebAssembly 组件](#webassembly-组件)、[共享组件](#共享组件)）。 |
| `components` | 应用的函数所用的共享组件，每个都固定到一个确切版本和摘要。 | 可选；需要 `wasm-shared-components-v1`；最多 8 个（[共享组件](#共享组件)）。 |
| `host_api` | 应用需要（`required`）或可以使用（`optional`）的宿主 API 方法，每个方法都写明 ABI 主版本。 | 可选；须在 `requires` 中声明 `host-api-v1`。商店在安装时和每次启动时检查 `required` 中的方法（[声明应用需要什么](HOST-API.zh-CN.md#声明应用需要什么)）。 |
| `backend` | 应用自己的后端：公开的登录端点，以及应用可以调用的操作。 | 可选；须在 `requires` 中声明 `backend-api-v1`，并设置 `storage.accounts: true`。不含任何凭据（[登录自己的后端](#登录自己的后端)）。 |
| `window` | 应用希望桌面窗口打开时的尺寸，单位为点：`{"width": 1536, "height": 1024}`。 | 可选；需要 `"schema_minor": 1`；宽度 320 到 4096，高度 240 到 4096。只是提示，不授予任何权限（[窗口尺寸](#窗口尺寸)）。 |
| `schema_minor` | 清单用到了 schema 1 的哪些新增内容。 | 省略此字段；但清单设置了 `window` 时，须写 `1`。 |

其他字段一律拒绝。运行 `hub publisher-prepare` 之后，你省略的可选字段也会出现在清单中，值为 `null`，例如 `"agent": null`。这些字段不改变任何行为。

请准确披露预计用途。商店展示的是使用声明；遗漏声明不代表应用不存储数据或不能联网。

### 能力

准入检查能识别 120 个能力名称：下表中的 42 个，以及[精确的服务名](#精确的服务名octosmatrixpalpo)一节中的 78 个。其他名称一律拒绝：

```text
[refused] policy: app com.example.forecast requests unknown capability "model.image"
```

能力声明说明预计用途，既不授权请求，也不提供响应这些请求的服务。响应请求的是**宿主服务**：OctoSense Shell 中的代码，负责完成应用自己无权做的事。**目前由谁提供**一列说明[桌面 RC2](../README.zh-CN.md#下载兼容宿主)在其平台和提供商限制内由谁响应；RC1 和历史 beta 的区别会明确注明。

| 能力 | 描述的用途 | 商店显示的文字 | 目前由谁提供 |
| --- | --- | --- | --- |
| `storage` | 应用自己的存储文件夹：`fs.*`、相机拍摄的内容，以及控件读取的本地文件。即使没有这项声明，应用仍有自己的隔离目录和配额。 | Keep its own data on this device | 运行时，所有宿主都提供 |
| `files` | 导入或导出用户在宿主原生对话框中选择的文件。导入和导出使用应用自己的隔离目录；应用得到的是自身存储内的相对路径，不是任意文件系统访问权限。应用应声明所需的具体 `files.*` 方法。 | Import and export files you choose in the system file dialog | 桌面 RC2 在 macOS、Windows 以及安装了对话框辅助程序（zenity、qarma、matedialog 或 kdialog）的 Linux 上提供，兼容的 Android 构建也提供；RC1 不提供。单文件上限 1 MiB；`files.share` 仅限 Android，且只确认已交给系统分享选择器 |
| `net` | 直接联网；`network.hosts` 披露预计访问的目的地。 | Declared destinations: *主机列表* | 运行时，所有宿主都提供 |
| `images` | 显示任何公开 https 主机上的图片，不限于 `network.hosts`。 | Show pictures from any website | 运行时 |
| `web` | 在系统网页视图中打开任何公开 https 页面；网页视图没有任何回到应用的通道。 | Open web pages in a browser view | 运行时在支持的平台上提供，自 RC1 起包括 Windows/WebView2 和 Linux X11/XWayland/WebKitGTK；原生 Wayland 内嵌仍不支持（[运行条件](../README.zh-CN.md#下载兼容宿主)）。 |
| `location` | 设备的位置。自桌面 RC1 起的 macOS 构建及兼容 Android 源码构建中：声明了 `host-api-v1` 的应用须先用 `location.permission.request` 请求权限，之后可以在 macOS 和 Android 上用 `location.sample` 读取新鲜位置（RC2），或仅在 Android 上用 `location.get` 读取上次已知的位置（[宿主 API 兼容性](HOST-API.zh-CN.md)）。 | Use your location | 运行时，限具备该功能的设备 |
| `camera` | 相机。拍摄的内容保存在应用带配额的私有存储中，即使没有声明 `storage` 也是如此。自桌面 RC1 起的 macOS 构建及兼容 Android 源码构建中：声明了 `host-api-v1` 的应用须先用 `camera.permission.request` 请求权限。 | Use the camera | 运行时，限具备该功能的设备 |
| `microphone` | 获得应用及系统授权后录音。前台 `microphone.record_*` 会话随桌面 RC2 在 macOS 和兼容的 Android 构建上提供（30 秒以内的录音写入应用存储；硬件验收待完成）；较早的宿主以及 Windows 和 Linux 只有权限方法，没有录音方法。请声明确切所需的方法，先调用 `microphone.permission.request`。 | 使用麦克风 | 运行时，取决于设备是否支持 |
| `audio` | 应用活跃时播放应用内音频，不授予麦克风访问或后台录音权限。 | 应用活跃时播放自己的音频文件 | 桌面 RC2 在 macOS 和兼容的 Android 构建上提供，还需要 `storage`（文件最大 1 MiB、最长 60 秒）；Windows、Linux 和 RC1 不提供；硬件验收待完成。 |
| `library` | 把拍摄的内容提供给系统相册，其他应用也能看到。 | Save to your photo library, where other apps can see it | 运行时，限具备该功能的设备 |
| `clipboard` | 剪贴板。 | Use the clipboard | 尚不支持：没有 API 使用它 |
| `prompt` | 应用向用户提出的问题。 | Ask you questions | 尚不支持：没有宿主读取它。应用 Agent 用 `ask_user_question` 提问。 |
| `ledger.read` | 读取共享账本。 | Read your shared data | 尚不支持：没有 `ledger` 服务 |
| `mail` | 通过宿主的 `mail` 服务收发邮件，使用用户在宿主[面板](#面板应用从不收集机密信息)上登录的账户。 | Read and send mail from accounts you sign in to on the device | OctoSense。自桌面 RC2 起，商店应用还可以保存草稿（`mail.compose`）并请求原生发送审阅（`mail.review_send`），审阅只在 macOS 和 Android 上可以批准（[宿主 API 兼容性](HOST-API.zh-CN.md#邮件草稿与发送审阅桌面-rc2)） |
| `auth` | 连接应用自己的 GitHub 或 Google 账户。自 OctoSense 桌面 RC1 起，还能登录应用自己的后端，并调用清单声明的操作（[登录自己的后端](#登录自己的后端)）。 | Connect its own GitHub or Google accounts, or sign in to its developer’s backend, through the host | OctoSense，需要宿主上有 OAuth 客户端注册信息（[已连接账户](#已连接账户)） |
| `github` | 读取仓库；每次 Markdown commit 都要等用户确认。 | Read authorized repositories and ask you to review Markdown commits | 同 `auth` |
| `gcalendar` | 读取 Google 日历；每次修改日程都要等用户确认。 | Read authorized Google calendars and ask you to review event changes | 同 `auth` |
| `gmail` | 读取 Gmail 并保存回复草稿；每次发送都要等用户确认。与 `mail` 相互独立。 | Read authorized Gmail messages, keep reply drafts and request native send review | 同 `auth` |
| `calendar` | 日历应用的本地日程存储和界面。不是 Google 日历。 | Read and manage local events through the device's Calendar service | 仅系统应用 `os.calendar`；商店应用请用 `gcalendar` |
| `device_calendar` | 读取用户选定的原生日历，并请求确认日程修改。需要分别获得应用授权、操作系统权限和日历选择授权。 | Read device calendars you choose and ask you to review event changes | 桌面 RC2 在 macOS（EventKit）和带适配器的 Android Home 构建上提供；Windows、Linux、RC1 和 Home beta.1 不提供。写入须亲手点按确认；与系统日历的实际交互待验收。请声明所需方法的确切版本。 |
| `llm` | 通过 `llm` 服务管理设备的 AI 提供商。 | Manage the assistant's AI providers, whose keys stay with the device | 仅系统应用 |
| `news` | 设备从订阅源和主题订阅源收集的条目。 | Read news the device collects from its feeds and topics | 仅系统应用 |
| `photos` | 相册应用自己的图库和合集。 | Read Photos's own library and publish collections | 仅系统应用：OctoSense 只向 `os.photos` 提供 `photos.notify` |
| `youtube` | YouTube 搜索和音乐推荐。 | Search YouTube and manage music recommendations | 仅系统应用：OctoSense 只向 `os.youtube` 提供 `youtube.notify` |
| `glance` | 向速览栏发布速览卡片（`glance.publish`、`glance.withdraw`、`glance.list`）。宿主会检查卡片，为卡片设置上限和有效期；卡片只能打开它自己的应用。 | Show cards on your glance screen | OctoSense |
| `model` | `model.complete` 和 `model.budget`，受每个应用的每日预算限制。`model.complete` 接受一个模型类别（`fast` 或 `strong`）和一个 JSON Schema。图片、音频、视频和向量方法在 [OctoSense #368](https://github.com/OctoSense-org/OctoSense/pull/368) 中实现，自桌面 RC1 起已包含。 | Send what you give it to the AI provider you configured, within a daily budget | OctoSense；媒体方法需要新实现及已配置的兼容供应商 |
| `research` | 通过系统工具箱搜索，不超出清单的 research 范围（[research 范围](#research-范围)）。每次搜索都由宿主执行。 | Search *范围允许的内容* | 仅系统应用，且只在手机版构建中 |
| `crawl` | 通过系统工具箱抓取网站，深度和页数不超过范围中的 `max_depth` 和 `max_pages`，并遵守其中的域名列表。覆盖面比 `research` 更广。 | Crawl websites, *范围的限制*, which reaches more than searching | 同 `research` |
| `runtime` | 用 `runtime.list` 和 `runtime.describe` 查询宿主实现了哪些 API（[宿主 API 兼容性](HOST-API.zh-CN.md)）。它不授予所列的任何 API。 | Inspect available host APIs without gaining access to their data or permissions | OctoSense 桌面版 0.1.0-beta.2 不提供，它的商店会拒绝这个名称。在基于 App Hub `main` 构建的每个宿主中（包括 `card-host`），由 App Hub 的请求分派器响应。 |
| `wasm` | 应用自带的函数：应用包 `fns/` 中的 WebAssembly 核心模块或组件（最多 8 个），由宿主的 `wasm` 服务在沙盒中运行，有截止时间和内存上限。核心模块的函数只拿到自己的输入，接触不到文件、网络、时钟或其他应用。组件还可以读取时钟和随机数；可以访问有配额的应用私有文件和 HTTP，并在与脚本相同的实际授权检查下调用可用的公开宿主服务。它接触不到其他应用（[WebAssembly 组件](#webassembly-组件)）。Agent 工具可以用 `host_method: "wasm.<function>"` 运行它。函数的编写、构建和调用方法见 App Flow 的[运行自己的 Rust 代码](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/RUST.zh-CN.md)。 | Run its own sandboxed functions on this device; they reach only what the app itself may | 桌面 RC2 的标准构建在 macOS 和 Linux 上提供它，属于有限支持（[服务及其限制](https://github.com/OctoSense-org/OctoSense/blob/main/docs/wasm.zh-CN.md#服务)）；RC1 默认关闭。受支持的 Android Home 源码构建也提供它。当前源码包含 Windows 和 OpenHarmony（Pulley），但不包含 iOS；真实平台与发布版验收仍需分别完成。组件源码集成已在 OctoSense #453 合并。 |
| `sheet` | 通过 `sheet` 引擎（gridcraft）处理电子表格：工作簿、公式、重算和 xlsx，均在应用自己的文件内。 | Use the device's spreadsheet engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面和 Home 构建中提供。 |
| `photo` | 通过 `photo` 引擎（photocraft）处理图像和照片文档：检查、转换、编辑命令和预览，均在应用自己的文件内。 | Use the device's image-editing engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面和 Home 构建中提供。 |
| `word` | 通过 `word` 引擎（wordcraft）处理文档：创建、读取、检查，并在 docx、Markdown、HTML、RTF、ODT 和 PDF 之间转换，均在应用自己的文件内。 | Use the device's document engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `deck` | 通过 `deck` 引擎（deckcraft）处理幻灯片：按大纲创建、渲染幻灯片，并转换为 PPTX 或 PDF，均在应用自己的文件内。 | Use the device's presentation engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `cad` | 通过 `cad` 引擎（cadcraft）处理图纸：检查、查询、测量、渲染，并转换 DXF 和 DWG，均在应用自己的文件内。 | Use the device's CAD engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `light` | 通过 `light` 引擎（lightcraft）处理 RAW 照片：元数据、显影控件，以及单张或批量显影，均在应用自己的文件内。 | Use the device's raw-photo engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `sound` | 通过 `sound` 引擎（soundcraft）离线处理音频文件：信息、波形峰值、转换、裁剪和混音；它从不打开音频设备，均在应用自己的文件内。 | Use the device's audio-editing engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `design` | 通过 `design` 引擎（designcraft）处理页面排版：文档信息、页面渲染，以及 PDF、IDML 或 EPUB 导出，均在应用自己的文件内。 | Use the device's page-layout engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `film` | 通过 `film` 引擎（filmcraft）处理视频：容器信息、导出 PNG 帧，以及用其自带编解码器的有界导出，均在应用自己的文件内。 | Use the device's video-editing engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `effect` | 通过 `effect` 引擎（effectcraft）处理动态图形：工程信息、帧渲染，以及 Lottie 导入和导出，均在应用自己的文件内。 | Use the device's motion-graphics engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `vector` | 通过 `vector` 引擎（vectorcraft）处理矢量图：检查、转换，并渲染 SVG、PDF、EPS 和 DXF，均在应用自己的文件内。 | Use the device's vector-drawing engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |
| `pdf` | 通过 `pdf` 引擎（pdfcraft）处理 PDF：信息、文本、页面渲染、合并和拆分，均在应用自己的文件内。 | Use the device's PDF engine on its own files | 仅系统应用。桌面 RC2 自带该引擎供系统助手使用，但其准入代码早于这个能力名，商店应用不得声明它；OctoSense `main` 在桌面构建（macOS、Linux、Windows）中提供，Home 不提供。 |

声明不会授予访问权限或补上缺失的服务。尚不支持：面向商店应用的 `photos` 和 `youtube` 服务。脚本如何调用各项能力，见 App Flow 的[能力](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/CAPABILITIES.zh-CN.md)文档；哪些能力会真正响应商店应用、支持哪些平台、从哪个版本开始，见其[宿主 API 能力族](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-FAMILIES.zh-CN.md)。

源码：`crates/app-contract/src/manifest.rs` 中的 `KNOWN_CAPABILITIES`。

### 精确的服务名：`octos.*`、`matrix.*`、`palpo.*`

这 78 个名称各是一项独立的用途声明，按全名精确匹配。`octos.` 或 `matrix.` 这样的前缀属于未知能力。在当前 OctoSense 宿主中，明确提供助手的应用，经用户同意后可以使用四个公开的 `octos.*` 方法，无须逐个声明这些方法的能力名。宿主仍核对准入的应用身份、会话归属、方法白名单和用户同意。公开这些方法本身不会创建助手，也不会授予其他应用的工具。Rinx 迷你应用采用独立的宿主授权路径。

| 组 | 名称 | 描述的用途 | 目前由谁提供 |
| --- | --- | --- | --- |
| `octos.*` | 4 个：`octos.session.open`、`octos.session.history`、`octos.turn.start`、`octos.turn.interrupt` | 与应用自己的 Agent 对话，对话由 OctoSense 运行的 Agent 内核 octos 承载：打开对话、读取对话历史、开始一轮、停止应用发起的那一轮。应用从不指定提供商、模型或密钥。 | OctoSense，前提是用户允许了该应用的 Agent。在此之前，调用会得到 `Waiting for the person to allow this app's agent (OctoSense asks the first time)`。 |
| `matrix.*` | 45 个，例如 `matrix.read_messages`、`matrix.room_members`、`matrix.send_message` | 每个名称对应一项 Matrix 操作，使用用户当前的账户，限于用户允许的房间。 | 没有任何 OctoSense 宿主服务提供它们。OctoSense 随附的原生应用 Rinx 通过自己的宿主，向用户导入其中的迷你应用提供这些名称；这不属于 App Hub 的安装途径。 |
| `palpo.*` | 29 个，例如 `palpo.projects.list`、`palpo.inbox.decide` | 每个名称对应一项 Palpo 管理操作。 | 尚不支持：OctoSense 中没有任何组件提供它们。 |

源码：商店为每个名称显示的文字位于 `crates/app-policy/src/services.rs` 和 `crates/app-contract/src/palpo.rs`。

### ID 与保留名称

ID 由 1 到 64 个 `[a-z0-9.-]` 字符组成，不能以 `.` 开头，也不能包含 `..`。准入检查还会拒绝：

- **所有以 `os.` 开头的 ID。** 这些 ID 属于随设备发布的系统应用，任何商店都不会安装它们。
- **本身是保留名称、或最后一段是保留名称的 ID。** 宿主以 ID 为键管理应用的存储和用户同意记录，以命名空间为键管理应用的工具，因此 `com.example.notes` 会顶替原生 Notes 应用。保留名称共 23 个：

| 保留原因 | 名称 |
| --- | --- |
| OctoSense 以原生应用形式随附的应用 | `apphub`、`appcard`、`browser`、`calculator`、`clock`、`notes`、`octoscode`、`reference`、`reminders`、`rinx`、`sheets`、`task`、`terminal`、`weather` |
| Shell 自身使用的身份名称，或 Shell 拥有的工具命名空间 | `agents`、`card`、`dev`、`octos`、`os`、`shell`、`system`、`toolbox`、`workflow` |

```text
[refused] identity: app id "com.example.notes" ends in "notes", which is reserved: its tools would be notes.*, a native app's or the host's
```

系统应用自己的命名空间不在保留之列：`com.example.news` 是允许的。附带 `tools.json` 的应用，命名空间必须符合 `[a-z0-9_]{1,24}`：`com.example.mynotes` 可以声明工具，`com.example.my-notes` 则不能。

ID 请以你能控制的反向域名前缀开头，例如把自己的域名倒过来写（`example.com` 写作 `com.example`），或者用 `io.github.<your-account>`；准入检查不会核实你是否控制这个前缀。

源码：`crates/app-contract/src/manifest.rs` 中的 `RESERVED_NAMES`。

### 网络主机

`network.hosts` 披露预计访问的目的地。即使没有声明 `net` 或列出主机，运行时网络模块也可用。已列出的主机仍须符合披露格式：纯主机名，不带协议、路径、端口或通配符。这项格式规则不会限制运行时的网络目的地。

```text
[refused] policy: host "https://api.open-meteo.com/v1" must be a bare host name, with no scheme or path
```

主机列表是每个签名版本的一部分，所以只列出稳定的主机：隧道改名不会更新已发布的披露信息。`localhost` 指向的是用户自己的设备，而不是你的服务器。

### 存储与配额

| 字段 | 含义 |
| --- | --- |
| `storage.max_bytes` | 应用请求的存储空间，单位为字节。 |
| `storage.accounts` | 设为 `true` 时，每个账户各有自己的数据文件夹和 Agent。默认只有一个 `device` 文件夹。 |
| `storage.agent_workspace` | `"account"`（默认）：Agent 读取所属账户的文件夹。`"none"`：Agent 不读取任何文件，只通过自己的工具工作。 |
| `storage.cache_max_bytes` | 应用为 `cache/` 请求的空间，单位为字节；必须大于 0。 |
| `compute.instruction_budget` | 每次会话累计可执行的脚本指令数。 |
| `compute.memory_bytes` | 隔离环境的堆大小。 |

配额只是请求。宿主会把每项配额限制在对应的上限（即宿主允许的最大值）以内；未填写的配额直接取上限。`hub check` 输出的 `declarations:` 行显示限制之后应用实际获得的存储空间。

| 项目 | 上限 |
| --- | --- |
| 存储 | 16 MiB |
| 每次会话的指令数 | 20,000,000 |
| 堆 | 64 MiB |

### research 范围

`research` 和 `crawl` 共用一个范围，即清单顶层的 `research` 对象。准入检查按 octos 自己的规则检查它，宿主则把同一段 JSON 交给系统工具箱。

```json
{
  "capabilities": ["research", "crawl"],
  "research": {
    "langs": ["en", "zh"],
    "regions": ["US", "CN"],
    "domains_allow": [],
    "domains_deny": ["example-spam.com"],
    "max_age_days": 7,
    "categories": ["news"],
    "max_results": 20,
    "max_depth": 2,
    "max_pages": 20
  }
}
```

| 字段 | 含义 | 规则 |
| --- | --- | --- |
| `langs` | 应用可以搜索的语言，采用 BCP-47 标签。 | 语言标签，例如 `en`、`zh-CN` 或 `zh-Hant`；`zh_cn` 会转换为 `zh-CN`。 |
| `regions` | ISO 3166-1 alpha-2 地区代码。 | 两个字母；会转为大写。 |
| `domains_allow` | 只允许这些域名及其子域名。 | 纯域名：`example.com`、`.example.com` 或 `*.example.com`；不带协议、路径或端口。 |
| `domains_deny` | 始终排除这些域名。 | 同上。 |
| `max_age_days` | 允许的最旧内容，以距今天数表示。 | 整数天数。 |
| `categories` | 元搜索类别。 | `news`、`general`、`science`、`it` 或 `social`。 |
| `max_results` | 每次搜索最多返回的结果数。 | 大于 0；默认 20。 |
| `max_depth`、`max_pages` | 抓取限制：单次抓取的链接深度和页数。 | 声明 `crawl` 时两者都须大于 0；否则可以省略或设为 0 来禁用抓取。即使未声明 `crawl`，也会校验已提供的范围。 |

列表为空或省略，或者省略 `max_age_days`，都表示不设限制。`{}` 只保留默认值：每次搜索 20 条结果，不抓取。未知字段一律拒绝。

商店会用通俗的话展示这个范围。例如，`research` 的范围为 `{"langs":["en","zh"],"categories":["news"],"max_age_days":7}` 时，商店的隐私概要（[商店信息](#商店信息)）显示“Searches news in English and Chinese, from the last 7 days”。

### 窗口尺寸

`window` 请求桌面宿主以指定尺寸打开应用窗口，单位为点：

```json
{
  "schema_minor": 1,
  "window": { "width": 1536, "height": 1024 }
}
```

| 字段 | 规则 |
| --- | --- |
| `width` | 320 到 4096。 |
| `height` | 240 到 4096。 |

尺寸超出范围时，解析器会拒绝清单；清单没有写 `"schema_minor": 1` 却设置了 `window` 时，也会被拒绝：

```text
window requires "schema_minor": 1 or later, so that hosts which predate window ignore it instead of refusing the app
```

`window` 只是提示，不是授权。它不改变任何能力、配额或审核，商店也不显示它。

- 读取它的桌面宿主按这个尺寸打开应用窗口，并限制在屏幕范围以内。用户仍然可以调整窗口大小。
- 手机和其他全屏宿主忽略它。
- 不加 `--size` 时，`card-host` 按这个尺寸打开窗口。它是渲染目标，所以不会把尺寸限制在屏幕范围以内（[在本地运行应用包](DEVELOPMENT.zh-CN.md#在本地运行应用包card-host)）。
- 基于 1.12.0 之前的契约构建的宿主只认识 `schema_minor` 0。它以宽松方式读取写有 `"schema_minor": 1` 的清单：忽略 `window`，把它列入 `AppManifest::ignored_fields()`，并按自己的默认尺寸打开应用。清单如果没有写 `"schema_minor": 1`，这样的宿主会把 `window` 当作未知字段拒绝。

没有 `window` 时，宿主按自己的默认尺寸打开应用。

源码：`crates/app-contract/src/manifest.rs` 中的 `WindowHint`。

## 应用的 Agent 与工具

应用可以自带 Agent，由清单中的 `agent` 字段声明：工具写在 `tools.json` 中，指令写在 `AGENT.md` 中，只含数据的技能放在 `skills/` 下。每个文件都在应用包摘要的覆盖范围内，因此运行的 Agent 就是审核过的那一个。商店把这个 Agent 称为应用的助手（assistant）。

### 清单中的 `agent`

```json
"agent": {
  "profile": "workspace-write-never-ask",
  "tools": [],
  "max_iterations": 8,
  "token_budget": 120000,
  "model": {
    "needs": ["tool_calling", "long_context"],
    "tier": "standard",
    "per_task": { "triage": { "needs": ["tool_calling"], "tier": "fast" } }
  },
  "background": true,
  "triggers": { "schedule": ["0 7 * * *", "0 19 * * *"], "events": ["news.items.new"] },
  "instructions": "AGENT.md",
  "skills": ["news-digest"]
}
```

| 字段 | 含义 | 拒绝条件 |
| --- | --- | --- |
| `profile` | Agent 会话的权限配置：`read-only`（每次写入前都先询问）、`workspace-write`（可读写自己的工作区，其他操作都先询问）或 `workspace-write-never-ask`（同上，但其他操作不经询问直接拒绝）。没有任何权限配置能授予完全访问权限。 | 其他任何值（`manifest is not valid`）。 |
| `tools` | 通用宿主工具（`ledger.read`、`ledger.write`、`net.fetch`、`storage.read`、`storage.write` 和 `card.render`）和一个内核工具 `ask_user_question`。应用自己的工具来自 `tools.json`；Agent 只获得这两类工具。 | 除 `ask_user_question` 以外的内核工具，或宿主不提供的名称。 |
| `max_iterations`、`token_budget` | 每次请求最多可用的模型迭代次数和 token 数，分别以 8 次迭代和 200,000 个 token 为上限。 | |
| `model` | Agent 对模型的要求，而不是提供商或模型名称。`needs` 列出以下任意几项：`tool_calling`、`vision`、`long_context`、`reasoning`、`structured_output` 和 `multilingual`；`tier` 取 `fast`、`standard` 或 `strong`（默认 `standard`）；`local_only` 用于不得离开用户设备的数据（作用于整个应用，单个任务不能放宽）；`per_task` 为 `AGENT.md` 中引用的具名任务单独设置要求。 | `needs` 或 `tier` 的值未知（`manifest is not valid`）；`per_task` 超过 8 项；任务名不符合 `[a-z_]{1,32}`。 |
| `background` | 申请在应用关闭时运行。由用户按应用逐个授予。 | 设为 `true` 却没有触发器。 |
| `triggers.schedule` | 五段式 cron 表达式，按本地时间计。 | 不是由数字和 `* , / -` 组成的五个字段；超过 16 项。 |
| `triggers.events` | 应用自己的宿主服务事件，位于应用的命名空间内（`news.items.new`）。 | 事件不在应用的命名空间内；超过 16 项。 |
| `instructions` | Agent 的指令，按惯例写在 `AGENT.md` 中。 | 不是应用包内的 `.md` 路径。 |
| `skills` | 要加载的技能，以 `skills/` 下的目录名指定。 | 超过 16 个技能，或同一技能列出两次。 |

Agent 的工作区遵循应用及账户的存储范围和 `agent_workspace` 设置。工具选择、research 范围、跨应用共享授权与用户同意，独立于描述用途的 `capabilities` 和 `network.hosts`；声明服务族不会添加工具。在提供 research/crawl 工具的宿主中，应用须在 `agent.tools` 中请求确切名称，并提供有效的 `research` 范围。商店应用的默认可选工具不包含这些工具（见[代码导读](CODE-WALKTHROUGH.md#6-what-declaring-an-app-agent-enables)）。

### 带 `tools.json` 的应用都有 Agent

只要应用带有 `tools.json`，即使没有 `agent` 字段，OctoSense 也会为它提供“Ask &lt;app&gt;”。`hub check` 对这样的应用仍会输出 `agent none`，因为 `declarations:` 行只反映清单中的 `agent` 字段。商店的隐私概要如何描述这类应用，取决于构建该商店所用的 App Hub 版本：

| 商店 | 有 `tools.json`、没有 `agent` 时的隐私概要 |
| --- | --- |
| 基于 App Hub `main` 构建的商店 | “Offers the host's Ask assistant for its admitted tools, only after you consent. Your conversation and tool results may be sent to your configured AI provider.”和“No app-declared background assistant or automatic triggers.” |
| OctoSense 桌面版 0.1.0-beta.2 中的商店 | “Runs no assistant.” |

每个 OctoSense 构建显示的，都是它锁定的 App Hub 版本给出的概要。既没有 `agent` 也没有 `tools.json` 的应用，在所有商店中都显示“Runs no assistant.”；声明了 `agent` 的应用，显示的则是“Runs an assistant limited to this app's own data.”这条概要。

参考应用 GitHub Notes 正好体现了这一区别。0.1.0 版带了三个工具，却写着 `"agent": null`：`hub check` 输出 `agent none`，OctoSense 桌面版 0.1.0-beta.2 的商店显示“Runs no assistant.”。0.1.1 版声明了 `read-only` 的 `agent` 字段：`hub check` 输出 `agent read-only`，每个商店都显示“Runs an assistant limited to this app's own data.”。

带了 `tools.json`，就同时声明 `agent`，并在隐私政策中说明这个 Agent。如果应用不应有 Agent，就不要带 `tools.json`。

### `tools.json`：应用的工具

以下摘自[新闻示例](../crates/app-policy/tests/fixtures/news-agent)，有删节：

```json
{
  "schema": 1,
  "tools": [
    {
      "name": "news.list",
      "description": "List collected stories, newest first, optionally for one topic or since a time.",
      "input_schema": {
        "type": "object",
        "properties": {
          "topic": { "type": "string" },
          "since": { "type": "string", "format": "date-time" },
          "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
        }
      },
      "output_schema": {
        "type": "object",
        "properties": { "items": { "type": "array", "items": { "type": "object" } } },
        "required": ["items"]
      },
      "risk": "read",
      "background": true,
      "shareable": true,
      "private_data": false,
      "implemented_by": "host-service"
    },
    { "name": "news.read",         "risk": "read", "…": "…" },
    { "name": "news.topics.get",   "risk": "read", "…": "…" },
    { "name": "news.topics.set",   "risk": "act",  "…": "…" },
    { "name": "news.digest.write", "risk": "act",  "…": "…" }
  ]
}
```

准入检查接受这些工具。调用仍须有对应解析方法的已注册实现，并通过服务的实际授权。命名空间本身不会创建服务；共享服务应使用经审核的 `host_method` 映射。较旧发布宿主的行为单独记录在下文（见 [OctoSense 目前的支持情况](#octosense-目前的支持情况)）。

| 字段 | 含义 |
| --- | --- |
| `name` | `<namespace>.<tool>`。命名空间是应用 ID 的最后一段：`os.news` 和 `dev.example.news` 的命名空间都是 `news`。每一段都由 `[a-z0-9_]` 组成。OctoSense 的工具代理（tool broker）负责注册和分派应用工具，它把命名空间之后的部分写成代理名称，用下划线代替点（`news.topics.get` 变为 `topics_get`）。准入检查拒绝超过 32 个字符的代理名称。 |
| `description` | 写给模型的说明：工具做什么、何时使用。长度为 1 到 1024 个字符。 |
| `input_schema`、`output_schema` | JSON Schema，只能使用以下关键字：`type`、`title`、`description`、`properties`、`required`、`items`、`enum`、`const`、`default`、`minimum`、`maximum`、`minLength`、`maxLength`、`minItems`、`maxItems`、`additionalProperties`、`format` 和 `pattern`。不支持 `$ref`，不支持 `anyOf`、`oneOf` 或 `allOf`，也不支持条件关键字。最大 8 KiB，最多嵌套 8 层。输入为对象。 |
| `risk` | `read`（只读取）、`act`（修改应用自身的状态）或 `destructive`（发送、发帖、分享、购买、删除，即任何超出应用本身的操作）。必填。也接受工具代理的写法 `Read`、`Act` 和 `Destructive`。 |
| `background` | 工具可以在并非由用户发起的一轮对话中运行。默认 `false`。 |
| `shareable` | 可以授权给应用自身 Agent 以外的调用方，例如 OctoSense 的系统 Agent（覆盖整台设备，用户直接与它对话）和其他应用的 Agent。默认 `false`。 |
| `private_data` | 结果中含有用户的私人数据。`local_only` 应用的可共享工具必须设为 `false`；带 `host_method` 的工具必须设为 `true`，但运行 `wasm.<function>` 的工具除外，因为函数只能看到自己的输入。 |
| `implemented_by` | `host-service`：由宿主服务运行。`app`：由应用自己的脚本在已打开的应用中运行（[脚本工具执行](#脚本工具执行script-tools-v1)）。必填。 |
| `host_method` | 工具映射到的已审核共享服务方法（[把工具映射到共享服务](#把工具映射到共享服务host_method)）。可选。 |
| `outward` | 如果 `act` 工具的调用会触及设备之外（发送、发帖、分享），就设置此项。这样每次调用都会像破坏性（`destructive`）调用一样，等待用户批准。默认 `false`；准入检查拒绝在 `read` 工具上设置它。 |
| `auto_approvable` | 常设规则（例如“一小时内允许”）可以批准调用。默认 `true`。对于删除、付款、账户或安全设置的变更，以及向设备之外分享，请设为 `false`，让用户逐次当场批准。 |
| `confirm` | 执行破坏性或对外的调用之前，由谁询问用户：`host`（默认，即宿主的批准流程）或 `app`（应用自己的确认界面）。准入检查只允许应用自己实现的工具使用 `app`；如果脚本工具写了 `app`，OctoSense 会拒绝它的每一次破坏性或对外调用（[由谁确认调用](#由谁确认调用)）。 |

### 把工具映射到共享服务：`host_method`

`host_method` 把工具路由到共享宿主服务中一个已审核的方法，写作 `family.method`，其中 family（服务族）就是服务本身，例如 `github.read` 中的 `github`。工具在应用的命名空间中保留自己的名称，调用以应用的身份运行。参考应用 [GitHub Notes](SUBMITTING.zh-CN.md#三个参考应用) 把它的 `githubnotes.read` 工具映射到 `github.read`：

```json
{
  "name": "githubnotes.read",
  "risk": "read",
  "private_data": true,
  "implemented_by": "host-service",
  "host_method": "github.read",
  "…": "…"
}
```

`host_method` 必须满足以下每一条规则，否则准入检查会拒绝：

| 规则 | 拒绝消息 |
| --- | --- |
| 工具声明了 `implemented_by: "host-service"`。 | `host_method is only valid for implemented_by "host-service"` |
| 取值为 `family.method` 形式，各段由 `[a-z0-9_]` 组成，不超过 96 字节。 | `host_method must be family.method with nonempty [a-z0-9_] segments, at most 96 bytes` |
| 任何一段都不是 `sheet`。 | `host_method cannot target a host sheet or approve an action` |
| 该方法列在下表中，或者是 `wasm.<function>`：应用自带的函数之一，`wasm.` 之后只有一段。 | `host_method "<m>" is not in the reviewed shared-service tool contract` |
| 工具的 `risk` 不低于该方法要求的最低风险（`wasm.<function>` 除外）。 | `host_method "<m>" requires at least <risk> risk` |
| 工具声明了 `"private_data": true`（`wasm.<function>` 除外，它只看得到自己的输入）。 | `shared-service tools must declare private_data: true` |
| 服务族声明用于披露用途。 | 遗漏声明不会拒绝已经审核的方法。 |

| 服务族 | 最低风险为 `read` 的方法 | 最低风险为 `act` 的方法 |
| --- | --- | --- |
| `github` | `github.repositories`、`github.files`、`github.read` | |
| `gcalendar` | `gcalendar.calendars`、`gcalendar.sync`、`gcalendar.refresh`、`gcalendar.cached`、`gcalendar.get`、`gcalendar.prepare` | |
| `device_calendar` | `device_calendar.permission.status`、`device_calendar.calendars.list`、`device_calendar.events.list`、`device_calendar.events.get` | |
| `mail` | `mail.compose_status` | `mail.compose`（仅操作本地草稿） |
| `gmail` | `gmail.labels`、`gmail.messages`、`gmail.message`、`gmail.draft.get`、`gmail.event.status` | `gmail.draft.open`、`gmail.draft.edit`、`gmail.event.decide` |
| `glance` | `glance.list` | `glance.publish`、`glance.withdraw` |
| `auth` | `auth.backend.me`、`auth.backend.request`（仅声明的 `GET` 操作） | |
| `runtime` | `runtime.list`、`runtime.describe` | |
| `model` | `model.capabilities`、`model.video.status` | `model.image`、`model.audio`、`model.embeddings`、`model.video`、`model.video.cancel` |
| `camera` | `camera.permission.status` | |
| `microphone` | `microphone.permission.status` | |
| `location` | `location.permission.status`、`location.get` | |

App Hub 已允许声明 `device_calendar` 工具别名和 `mail.compose` / `mail.compose_status`；桌面 RC2 在 macOS 和兼容的 Android 构建上实现了它们，RC1 没有，Windows 和 Linux 可以撰写草稿和读取状态，但不能批准发送。这些别名仍要求 `private_data: true` 及所列风险级别；当前策略不会仅因遗漏服务族披露而拒绝路由。日历权限申请、日历选择和事件修改仅限前台，`mail.review_send` 和 `mail.send` 也一样。准备草稿不会发送邮件；兼容宿主仍须重新核验授权，并在用户批准对外写操作前显示宿主自己的不可变审阅内容。

`wasm.<function>` 运行应用自带的函数之一（`fns/*.wasm`，用 `wasm` 披露用途）。它只在提供 `wasm` 的宿主上可用（见[能力](#能力)）：桌面 RC2 的 macOS 和 Linux 构建提供，RC1 默认关闭。RC2 在[平台限制](HOST-API.zh-CN.md#限制)内实现上表中的 `auth`、`runtime`、`camera`、`microphone`、`location`、`device_calendar` 和 `mail` 方法。上面的规则同样适用于这些方法，`runtime.list` 和 `runtime.describe` 也不例外。通过准入不等于已经配置账户、取得权限或补上缺少的 API：请用 `runtime.describe` 查询宿主实现了什么。映射到 `auth.backend.request` 的工具只能执行后端声明的 `GET` 操作；写操作仍须应用在前台，并由用户在宿主的原生审阅界面上批准。权限的 `request` 和 `revoke`、账户管理，以及 `app_tools.dispatch@1` 等运行时 ABI，都没有 `host_method`。

上述七个媒体别名需要 [OctoSense #368](https://github.com/OctoSense-org/OctoSense/pull/368)，桌面 RC1 和 RC2 已包含这些方法。当前策略保留 `private_data: true` 和方法风险要求，不再以 `model` 声明作为执行门槛。生成与向量请求可能计费，因此它们和视频取消至少需要 `act` 风险，并使用有资源上限的模型服务。发现 API 不代表供应商账户已有权益；远端视频任务已运行时，取消可能失败。额度和进程内任务生命周期见双语[媒体契约](https://github.com/OctoSense-org/OctoSense/blob/main/apps/ai-providers/host-service/MEDIA.zh-CN.md)。这些策略别名不构成真实供应商或设备验证。

账户/业务写入、登录、确认和批准都没有 `host_method`：这些操作由用户在应用自己的界面上发起。上述有上限的模型任务是模型供应商提交的明确例外。

对于映射到 `glance.publish` 的工具，让 `input_schema` 只接受 `template` 加 `initial`，或 L0 `source` 加 `data`。绝不要接受 `script`。OctoSense 桌面版 0.1.0-beta.2 按应用自身的策略发布 Agent 提交的脚本卡片，因此模型写出的 `script` 会作为你的应用运行。自 OctoSense 桌面 RC1 起，宿主会拒绝这类卡片，报错 `Agents cannot publish executable Splash; choose an admitted template with initial data, or L0 source`；它也拒绝 L1 的 `source`。

源码：`crates/app-policy/src/agent.rs` 中的 `SHARED_HOST_METHODS`。

### 脚本工具执行（`script-tools-v1`）

脚本工具是 `"implemented_by": "app"` 的工具：由应用自己的 Splash 代码在已打开的应用中运行。它需要提供 `app_tools.dispatch@1` 的宿主，例如[桌面 RC4](../README.zh-CN.md#下载兼容宿主)。OctoSense 桌面版 0.1.0-beta.2 拒绝这类工具，返回 `app_tool_unavailable`。

添加脚本工具：

1. 在清单中加入 `"requires": ["script-tools-v1"]`。
2. 在 `tools.json` 中声明工具的名称、schema 和 `"implemented_by": "app"`。
3. 在 `main.splash` 中实现 `app_tool` 钩子。下例的应用 ID 是 `dev.example.notebook`，因此工具命名空间是 `notebook`。应用不能使用 `notes`，它是保留名称（[ID 与保留名称](#id-与保留名称)）。

```splash
fn app_tool(name, call_id) {
    let request = mod.app_tools.request(call_id)
    if name == "notebook.read" {
        mod.app_tools.complete(call_id, {text: fs.read("note.txt")})
    } else {
        mod.app_tools.fail(call_id, "Unknown tool")
    }
}
```

`mod.app_tools.request(call_id)` 返回 `args` 和 `context`：`args` 是宿主按 `input_schema` 检查过的参数，`context` 包含 `app`、`account`、`caller` 和 `call_id`。`context` 由宿主填写，脚本无法选择其中的值。用 `mod.app_tools.complete` 或 `mod.app_tools.fail` 结束调用。结果不符合 `output_schema` 时，调用方收到 `invalid_result`。钩子调用 `fail`、出现脚本错误或超出 VM 的指令限制时，调用方收到 `app_error`。宿主不强制检查 `pattern` 和 `format`。

钩子在 UI 线程上运行，与应用界面共用同一个 Splash VM 和存储文件夹，并受 VM 的指令和内存限制约束。钩子也可以稍后再完成调用，例如在 `host.request` 回调中完成：`mod.app_tools.active(call_id)` 会告诉它这次调用是否仍未结束。其他应用或宿主面板无法凭 `call_id` 读取参数或完成调用。

| 限制项 | 取值 |
| --- | --- |
| 参数、结果 | 各 1 MiB |
| 期限 | 最长 60 秒 |
| 未结束的调用 | 每个应用 16 个，整个宿主进程 128 个 |

应用关闭（`app_not_running`）、应用的账户发生变化（`account_scope`）、超过期限（`timeout`）或调用方取消时，调用会提前结束。调用结束既不会中止正在运行的钩子，也不会撤销它已经执行的操作；调用结束后再调用 `complete` 或 `fail`，宿主会忽略。

脚本工具不能：

- 在应用关闭时运行。宿主既不会启动应用，也不会启动它的后台副本，即使工具设置了 `background: true` 也一样，因此调用会返回 `app_not_running`。
- 在速览栏中的应用副本或第二个已打开的副本中运行。只有最先打开的完整应用持有这些工具；第二个副本拿不到工具，并显示 `App tools unavailable: app_busy: …`。
- 弹出宿主面板，例如权限申请面板。只要应用还有未结束的调用，应用自己的界面也无法弹出宿主面板。
- 在应用自己的界面上确认调用。如果脚本工具写了 `confirm: "app"`，宿主会拒绝它的破坏性或对外调用，所以请保留默认的 `confirm: "host"`。
- 绕过当前应用准入、账户作用域、设备同意、原生审核、资源上限或宿主可用性检查。能力族声明本身既不授予也不拒绝 API。

未验证：在手机上以及搭配真实模型运行脚本工具。

### 由谁确认调用

`risk` 和 `outward` 决定调用是否需要用户参与；`confirm` 决定由谁的界面询问用户。`read` 调用，以及没有设置 `outward` 的 `act` 调用，无需用户参与即可运行。破坏性或对外的调用只有在用户批准后才会运行，用户可以当场批准，也可以通过常设规则批准：

| `risk: "destructive"`（或 `outward: true`）时的 `confirm` | 用户在场 | 用户不在场 |
| --- | --- | --- |
| `confirm: "host"`（默认） | 由宿主的批准流程询问用户。 | 应用的对话中出现一条批准请求。 |
| `confirm: "app"` | 应用自己的确认界面是唯一的确认环节（例如 Rinx 的 `send_message`），宿主不会再次询问。 | 应用的对话中出现一条批准请求。 |

同一次调用绝不会询问用户两次。破坏性工具仍然可以设置 `background: true`：准入检查会记录一条警告，而该工具只有在获得批准后才会运行。如果工具既不是破坏性的，也不是对外的，`confirm: "app"` 就不会确认任何东西，准入检查也会对此发出警告。

目前只有原生应用（例如 Rinx）有自己的确认界面。在商店应用中，准入检查会拒绝 `host-service` 工具上的 `confirm: "app"`；对于写了它的脚本工具，OctoSense 会拒绝其破坏性或对外调用（见[脚本工具执行](#脚本工具执行script-tools-v1)）。请保留默认值。

### `AGENT.md` 与技能

`AGENT.md` 写的是 Agent 的角色和指令：每个触发器触发后做什么、应用数据中哪些内容重要、评判输出的标准，以及它的记忆规则。

技能是一个 octos 技能目录，包含 `SKILL.md` 和 `manifest.json`。对商店应用来说，技能只含数据：技能的 `manifest.json` 包含 `name`（即目录名）、`version`、`description`、`uses`（它调用的工具，每一项都必须是应用自己的工具或列在 `agent.tools` 中），以及可选的 `prompts.include`。

```json
{
  "name": "news-digest",
  "version": "1.0.0",
  "description": "Write a cited morning or evening digest from collected stories.",
  "uses": ["news.list", "news.read", "news.digest.write"]
}
```

### Agent 文件的规则

出现以下情况时，准入检查会在 `tools`、`agent` 或 `skills` 检查项下拒绝：

| 文件 | 拒绝条件 |
| --- | --- |
| `tools.json` | 文件超过 64 KiB，或没有声明任何工具，或声明的工具超过 64 个；某个工具不在应用的命名空间内；命名空间不符合 `[a-z0-9_]{1,24}`；名称重复；两个名称对应同一个代理名称；代理名称超过 32 个字符；工具缺少 `name`、`description`、`input_schema`、`output_schema`、`risk` 或 `implemented_by`；含有未知字段；schema 使用了上文 `input_schema` 一行没有列出的关键字；由宿主服务实现的工具设置了 `confirm: "app"`；`read` 工具设置了 `outward`；`local_only` 应用的可共享工具缺少 `"private_data": false`；`host_method` 违反其规则。 |
| `AGENT.md` | `agent.instructions` 没有指向它；文本为空、超过 32 KiB、不是 UTF-8、含有控制字符、以 `#!` 开头，或包含 `<script`、`<iframe`、`<object`、`<embed`、`javascript:`、`vbscript:` 或 `data:text/html`。 |
| `skills/<name>/` | `agent.skills` 没有列出它；技能的清单声明了可执行字段（`tools`、`binaries`、`sha256`、`mcp_servers`、`hooks`、`hardware_lifecycle`、`tool_discovery`、`actions` 或 `make_type`）；目录中有 `.md`、`.json`、`.txt` 以外的文件，或有符号链接；清单中的 `name` 与目录名不同；`uses` 中有一项既不是应用自己的工具，也不在 `agent.tools` 中。 |
| `AGENT.md` 或 `skills/` | 清单中没有 `agent` 字段。 |

### 商店显示的内容

商店根据清单和 `tools.json` 生成类似下面的说明，在安装前与其他权限并列显示：

- “Run an assistant for this app, only after you allow it”（`agent`）
- “Its assistant can use these app tools: news.list, news.read”（声明了 `agent` 的应用在 `tools.json` 中的工具）
- “Its assistant requests these additional tools: net.fetch”（`agent.tools` 中除 `ask_user_question` 以外的工具）
- “Its assistant may work while the app is closed, on a schedule and when new data arrives; only if you allow it, and you can turn it off.”（`background`）
- “Can ask to mail.send: nothing of this runs until you approve it.”（由宿主确认的破坏性或对外工具）
- “You approve every call of pay.transfer yourself: no standing rule can.”（`auto_approvable: false`）
- “Offers news.list to other assistants you allow.”（`shareable`）

OctoSense 桌面版 0.1.0-beta.2 的商店用一行说明代替前三行：“Run an assistant for this app (&lt;tools&gt;), inside this app's own data only”，其中 &lt;tools&gt; 只列出 `agent.tools`。因此，工具全部写在 `tools.json` 中的应用会显示“Run an assistant for this app (no tools), inside this app's own data only”。

### OctoSense 目前的支持情况

在 OctoSense 桌面版 0.1.0-beta.2 上，各部分的情况如下：

| 组成部分 | 现状 |
| --- | --- |
| 与 Agent 对话 | 用户允许应用的 Agent 之后，在 Shell 的“Ask &lt;app&gt;”对话栏中进行。OctoSense 会在首次使用时询问。`card-host` 不运行 Agent。 |
| `implemented_by: "host-service"` 的工具 | 以应用的身份，在 `host_method` 指定的服务上运行，清单必须请求该服务。没有 `host_method` 的工具会调用以其命名空间命名的服务。系统应用（例如 `os.news`）可以这样调用，但商店应用的命名空间不是能力，调用会失败，返回 `not_granted`。工具调用从不弹出面板。 |
| `implemented_by: "app"` 的工具 | 尚不支持：调用会失败，返回 `app_tool_unavailable`。 |
| `AGENT.md` 和技能 | 作为指引，在每一轮对话中加载。它们不授予任何工具。 |
| `agent.tools` | `ask_user_question` 可用。尚不支持：`ledger.read`、`ledger.write`、`net.fetch`、`storage.read`、`storage.write` 和 `card.render` 的执行器。 |
| `background` 和 `triggers.events` | 只支持 `<namespace>.new_message` 事件，且仅限获得 `gmail` 和 `auth` 授权、设置了 `background: true` 的应用，并须在用户允许其 Agent 之后。 |
| `triggers.schedule` 和其他事件 | 尚不支持。 |
| `agent.model` | 尚不支持：OctoSense 会忽略它。 |
| Agent 发布的速览卡片 | 应用可以发布的任何卡片，包括 `script` 卡片。 |

OctoSense 桌面 RC1 改变了四点，RC2 沿用这些改动：

- `implemented_by: "app"` 的工具在已打开的应用中运行（[脚本工具执行](#脚本工具执行script-tools-v1)）。
- 发布速览卡片的工具调用（直接调用 `glance.publish`，或经由 `host_method`）只接受带 `initial` 对象的模板，或 L0 `source`。它拒绝 `script` 和 L1 源码，返回的错误类型为 `unsafe_card_source`。
- 批准 GitHub 或 Google 日历的保存须亲手点按（见[已连接账户](#已连接账户)）。
- 宿主只保留从 30 天前到 366 天后的 Google 日历日程。

## 商店信息

`listing.json` 是用户安装前在商店中看到的内容。它与应用包一同接受审核，并随签名目录分发，因此审核人员读到的内容就是商店显示的内容。商店信息旁的用途披露来自清单，而不是商店信息。两者都应准确描述应用行为；遗漏声明不代表应用无法存储数据或联网。

| 字段 | 规则 |
| --- | --- |
| `schema` | `1`。 |
| `subtitle` | 最多 80 个字符。 |
| `description` | 不能为空；最多 4000 个字符。 |
| `category` | 取以下值之一：`productivity`、`utilities`、`photo-video`、`news`、`weather`、`travel`、`finance`、`health`、`education`、`entertainment`、`games`、`social`、`shopping`、`lifestyle` 或 `developer`。 |
| `keywords` | 最多 10 个。 |
| `screenshots` | 1 到 8 个路径，指向应用包内的 PNG 或 SVG 文件。请使用应用运行时的真实截图（[截图](SUBMITTING.zh-CN.md#4-截图)）。 |
| `icon` | 应用包内一个正方形 PNG 或 SVG 文件的路径（[应用图标与随包素材](ICONS.zh-CN.md)）。PNG 图标不超过 1 MiB，单边不超过 1024 像素。启动器中请使用同一个图标。 |
| `platforms` | 至少一项，取自 `android`、`ios`、`macos`、`windows`、`linux`、`openharmony` 和 `web`。只列出你测试过的平台。 |
| `publisher.name` | 不能为空。 |
| `publisher.support` | URL 或电子邮件地址。 |
| `publisher.privacy_policy_url` | 以 `https://` 开头的 URL。 |
| `release_notes` | 本版本的变更内容。 |
| `age_rating` | `all`、`12+`、`16+` 或 `18+`。 |
| `license` | 源码开放时，填写 SPDX 标识符。 |

`subtitle`、`keywords`、`release_notes` 和 `license` 是可选字段，其他字段都必须填写。未知字段一律拒绝。商店信息是签名版本的一部分：要修改其中的文字，请发布新版本。

商店还会显示**隐私概要**。隐私概要根据清单生成；在基于 App Hub `main` 构建的商店中，还会参考审核过的工具。它说明应用存储什么、联系哪些主机、使用哪些设备功能，以及是否运行 Agent（[带 `tools.json` 的应用都有 Agent](#带-toolsjson-的应用都有-agent)）。不要在描述中复述隐私概要，而是把清单写对。不过，你仍可以像 GitHub Notes 0.1.1 那样，在描述中说明应用 Agent 会发送哪些数据。

## 宿主服务与面板

脚本应用从不持有凭据。需要凭据的工作，由它调用宿主服务来完成：

```splash
host.request("mail.list", {…}, fn(r){ … })
```

公开调用交给宿主为该服务族注册的服务，检查实际同意、账户 scope 和可用性；不会仅因遗漏服务族声明而拒绝。服务完成工作后返回数据，绝不返回凭据；它返回的连接句柄不透明，并与该应用绑定。如果某个服务族没有任何服务响应，对它的调用会立即失败，返回 `no service answers "<family>" on this device`；`card-host` 不注册任何服务，在那里只有 App Hub 用于发现宿主 API 的 `runtime` 会响应。哪个 Shell 提供哪个服务族，见 App Flow 的[宿主服务](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.zh-CN.md)。

`card-host` 也没有实现 `host-api-v1`、`backend-api-v1` 和 `script-tools-v1` 所需的任何 API，因此会拒绝清单要求其中任何一项的应用。这类应用请在[桌面 RC1](../README.zh-CN.md#下载兼容宿主) 中测试，它在自身的[平台限制](HOST-API.zh-CN.md#限制)内实现了这些 API。

### 面板：应用从不收集机密信息

只应由用户本人提供的输入，例如密码或账户授权，都放在**面板**上：面板是宿主绘制在应用上层的界面，运行在独立的隔离环境中，不受任何应用的策略约束。只有服务能打开面板。接收机密信息的服务方法都位于 `<family>.sheet.` 之下，并且只接受来自该面板的调用。应用自己的密码字段在运行时不接收任何输入，准入检查也会拒绝声明了密码字段的应用包。

面板显示期间，文本、按键、输入法输入、剪贴板和指针释放事件只发给面板；计时器和服务回复仍会送达应用。基于 App Hub `main` 构建的宿主会自己保存对面板的引用，因此应用中名为 `sheet` 的控件无法隐藏或替换面板。

参见 App Flow 的 [Mail 完整示例](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.zh-CN.md#完整示例mail)。

### 已连接账户

使用 GitHub 或 Google 的应用应披露 `auth` 及实际使用的提供商服务族：`github`、`gcalendar` 或 `gmail`。这些声明不会授权访问账户。用户在宿主面板上登录，应用拿到的是连接句柄，绝不是令牌。写操作要经过宿主确认。请设置 `storage.accounts: true`，让每个账户各自保存数据。哪些宿主支持这类应用，见[开始之前](SUBMITTING.zh-CN.md#开始之前)；具体如何调用，见 App Flow 的[使用已连接账户](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/CAPABILITIES.zh-CN.md#使用已连接账户)。

宿主强制执行哪些规则，取决于构建版本和平台。Windows/Linux 的受保护写操作仍不支持，会拒绝执行，见[平台限制](../README.zh-CN.md#下载兼容宿主)：

| | OctoSense 桌面版 0.1.0-beta.2 | OctoSense 桌面 RC1 和 RC2 |
| --- | --- | --- |
| 批准 Gmail 发送 | 在原生的 Approve & Send 控件上亲手点按 | 相同 |
| 批准 GitHub 或 Google 日历的保存 | 在宿主的确认面板上批准，不检查是否为亲手点按 | 在原生的 Approve & Save 控件上亲手点按；脚本和 Agent 的请求无法批准 |
| 宿主保留的 Google 日历日程 | 日历的全部历史 | 从今天前 30 天到今天后 366 天（按 UTC 计），重复日程会展开 |

如果只想识别用户身份、不读取其数据，就只声明 `auth`，并且仅请求身份类 scope：GitHub 用 `read:user`，Google 用 `openid`、`email` 和 `profile`。这样 `auth.connect` 返回的连接带有提供商的 `subject` 和用于显示的 `label`，不授予对仓库、邮件或日历的任何访问权限。

登录需要宿主中有该提供商的 OAuth 注册信息：

| 宿主 | 注册信息来源 | 没有注册信息时 `auth.connect` 的报错 |
| --- | --- | --- |
| OctoSense 桌面版 0.1.0-beta.2 | 宿主的 `oauth/clients.json`，该发布版不附带这个文件 | `OAuth is not configured. Add provider registrations in the host's oauth/clients.json` |
| OctoSense 桌面 RC1 和 RC2 | 发行方构建设置，或宿主 `oauth/clients.json` 覆盖配置；公开的 RC1 和 RC2 包都不附带注册信息 | `GitHub sign-in is unavailable in this build. Check for an OctoSense update or contact its distributor.`，或 Google 的同类消息 |

### 登录自己的后端

自 OctoSense 桌面 RC1 起，宿主可以在[平台限制](../README.zh-CN.md#下载兼容宿主)内让应用登录其开发者的后端，并调用应用声明的后端操作。在清单中声明这个后端。声明是公开的，不含任何凭据：

```json
{
  "capabilities": ["auth"],
  "storage": { "accounts": true },
  "requires": ["backend-api-v1"],
  "backend": {
    "id": "notes",
    "client_id": "<public-client-id>",
    "authorization_url": "https://login.example.com/authorize",
    "token_url": "https://login.example.com/token",
    "me_url": "https://login.example.com/me",
    "logout_url": "https://login.example.com/logout",
    "scopes": ["app.session"],
    "operations": {
      "notes.list": { "method": "GET", "path": "/api/notes", "query_keys": ["tag"] },
      "notes.create": { "method": "POST", "path": "/api/notes" }
    }
  }
}
```

| 字段 | 规则 |
| --- | --- |
| `id` | 1 到 64 个 `[A-Za-z0-9._-]` 字符。 |
| `client_id` | 后端的公开客户端 ID：1 到 256 个字符，不含空白或控制字符。 |
| `authorization_url`、`token_url`、`me_url`、`logout_url` | 四个互不相同的 `https://` URL，同源且使用 443 端口；不含用户名或密码、查询参数、片段、`%` 转义或 `\`，也不含 `.` 或 `..` 路径段。 |
| `scopes` | 必须正好是 `["app.session"]`。 |
| `operations` | 最多 64 个，以名称（`[A-Za-z0-9._-]`）为键。每个操作包含 `method`（`GET`、`POST`、`PUT`、`PATCH` 或 `DELETE`）、同一来源上的确切 `path`（不能是上述四个 URL 的路径），以及最多 32 个互不重复的 `query_keys`。 |

清单仍须设置 `storage.accounts: true`，并在 `requires` 中要求 `backend-api-v1`。应披露 `auth` 用途供审核；1.11 不再要求这个能力族才能准入后端。准入检查仍验证后端注册和账户布局。较旧工具可能报告 `backend requires auth and storage.accounts`。

运行时，调用 `auth` 服务：

1. 用 `{"provider":"backend","scopes":["app.session"]}` 调用 `auth.connect`。用户在后端自己的网页上注册或登录。在 macOS 和 Android 9 及以上版本中，宿主会在自己的网页视图中显示这个页面，该视图不会离开你的登录来源。在桌面端，如果要改用系统浏览器，就在参数中加入 `"presentation":"browser"`；如果该页面会把用户转到 GitHub 或其他提供商，就必须这样做。
2. 用返回的连接句柄调用 `auth.backend.me`，得到后端验证过的身份（`sub` 和 `label`）。
3. 用连接句柄、操作名称及其声明过的查询参数调用 `auth.backend.request`，例如 `{"connection":"<handle>","operation":"notes.list","query":{"tag":"work"}}`。写操作还要带上 JSON 格式的 `body`。宿主附上会话的令牌，按声明的方法和路径发出请求，再把后端返回的 JSON 交给应用。

`GET` 操作会立即执行，即使调用来自主屏幕磁贴或 Agent 工具。`POST`、`PUT`、`PATCH` 和 `DELETE` 操作要等用户在宿主的原生审阅界面上亲手点按、批准这个确切的请求之后才会执行，因此应用必须在前台。来自磁贴或 Agent 工具的写操作会失败，返回 `Open the app to review this backend change`。请求体和应答都是不超过 64 KiB 的 JSON，宿主拒绝重定向。修改或删除 `backend` 块、更新应用或撤回应用，都会结束应用的后端会话，用户需要重新登录。

适用范围：

- 只有实现了 `auth.backend.request@1` 的宿主才会安装这个应用：桌面 RC1 和 RC2 在 macOS、Windows 和 Linux 上声明该方法；兼容 Android 源码构建也实现它。Windows/Linux 使用外部浏览器登录（RC2 补上了所需的原生链接打开方式）及清单声明的读取，嵌入式登录和受保护的写操作不支持。缺少该方法的宿主会报 `app <id> needs a host implementing auth.backend.request@1`。iOS 不支持后端登录。
- 没有 `backend` 块的应用，只能在运维人员已把其后端登记到宿主 `oauth/backends.json` 的设备上登录。Windows 和 Linux 上的这类登录使用浏览器。
- 已在 Windows 上用 RC2 源码验证：测试程序对一个模拟后端完成了浏览器登录。未验证：对真实后端的登录、Linux 上的登录，以及在设备上实际批准一次写操作。

应用不能做的事：

- 用其他方式访问后端。应用不能选择 URL、方法、请求头或令牌，也不能发送未声明的查询参数，而且永远看不到后端的令牌。
- 自己批准写操作。脚本和 Agent 的请求都无法在宿主的审阅界面上完成批准。
- 自行收集密码，或复用宿主的 GitHub 或 Google 令牌。

协议的后端部分见 OctoSense 的[开发者后端接口约定](https://github.com/OctoSense-org/OctoSense/blob/main/crates/oauth-service/README.zh-CN.md#开发者后端接口约定)。

### 宿主服务调用的限制

每次调用都恰好得到一个应答：服务返回的数据，或者应用可以据此处理的错误。

| 限制项 | 取值 | 不满足时应用收到 |
| --- | --- | --- |
| 应答时间 | 60 秒，除非服务要求更长时间；服务的面板显示期间暂停计时 | `the host service timed out` |
| 每个应用等待中的调用数 | 32 | `too many host requests are waiting; try again when some have answered` |
| 参数 | 1 MiB 以内的有效 JSON | `the request's arguments exceed 1 MiB`，或 `the request's arguments are not valid JSON: …` |
| 应答 | 4 MiB | `the service's answer exceeds 4 MiB` |

应用关闭时，它等待中的调用随之结束，迟到的应答也不会送达应用的下一次会话。

### 面板可以出现在哪里

服务只能在前台应用之上弹出面板。对于以主屏幕磁贴形式显示的应用，以及来自 Agent 的工具调用，宿主会把 `may_prompt` 设为 `false`，并拒绝弹出任何面板，返回 `this surface cannot raise a prompt; open the app to continue`。`prompt` 能力也改变不了这一点。

## 命令

`hub` 运行的就是准入检查本身的代码，Hub 依据的也正是它的报告。按 App Flow 的[快速上手](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md)构建它。运行 `hub`、`hub help`，或在任何命令后加上 `--help` 或 `-h`，都会输出用法；这些命令都不会读取应用包，也不会写入文件。未知命令会失败，并输出 ``hub: unknown command "<name>"; run `hub help` for usage``。

| 命令 | 作用 |
| --- | --- |
| `hub publisher-prepare/attach/verify/pack/unpack/entry` | 准备 GitHub 规范化主题、附加证明、验证、交付或生成审核候选；详见 [GitHub 发布者来源证明](#github-发布者来源证明)及 `hub --help` 的完整参数。均不会发布目录。 |
| `hub stamp <bundle>` | 用准入检查的解析器解析 `manifest.json`，然后把应用包摘要写入 `integrity.bundle_blake3` 并输出。准入检查无法读取的清单，或已带 GitHub 证明的 Release，它都会拒绝处理。 |
| `hub check <bundle> [--allow-unsigned] [--publisher-key <id>=<hex>] [--catalog <file> [--anchor <hex>]] [--json] [--system-app] [--advisory-db <dir>]` | 准入检查本身。输出 `PASSED` 或 `REFUSED`、每个检查结果，以及用途声明和解析后的资源上限。有拒绝时退出码为 1。`--json` 以 JSON 输出报告（`schema`、`stage`、`passed`、`app_id`、`version`、`digest`、`findings` 和 `resources`）。`--publisher-key` 用于检查旧格式的密钥签名应用包，发布时从不需要它。 `--advisory-db` 把其组件列出的 crate 与 RustSec 检出对照检查（见[组件列出的 crate](#组件列出的-crate)）。 |
| `hub scan <bundle> [--publisher-key <id>=<hex>] [--catalog <file> [--anchor <hex>]] [--packet <out.json>] [--reviewer <cmd>] [--system-app]` | 先运行准入检查，再写出审核包，还可以把审核包交给一条审核命令。准入检查拒绝的应用包不会进入扫描。 |
| `hub component-info <file.wasm>` | 以 JSON 输出函数文件的类型（`component` 或 `module`）、它的导入，以及它导出的函数及其参数和结果（[WebAssembly 组件](#webassembly-组件)）。它只读取这一个文件。既不是核心模块也不是有效组件的文件会失败，输出 `hub: <file>: not a WebAssembly core module or component` 或验证器的错误。 |
| `hub component-prepare <file.wasm> --draft <component.json> --out <file>` | 把共享组件文件描述成一个 Release：它的摘要、大小、导入和导出，加上草稿中的字段。提供 `--repository`、`--repository-id`、`--owner-id`、`--workflow`、`--tag` 和 `--commit` 时，它写出要证明的规范化对象，文件名必须是 `octosense-component.json`；不提供时，写出未签名的开发 Release。从不覆盖已有文件（[发布共享组件](#发布共享组件)）。 |
| `hub component-pack <release.json> --wasm <file.wasm> [--attestation <bundle.json>] [--allow-unsigned] --out <new directory>` | 附加证明、运行组件的准入检查，并写出 `<id>-<version>.component.json` 和 `<id>-<version>.wasm`。准入检查拒绝时什么也不写。 |
| `hub component-check <release.json> --wasm <file.wasm> [--catalog <file> [--anchor <hex>]] [--allow-unsigned] [--json]` | 针对组件 Release 及其文件的准入检查。输出 `PASSED` 或 `REFUSED` 和每条检查结果；被拒绝时以状态 1 退出。 |
| `hub component-entry <release.json> --wasm <file.wasm> --catalog <authenticated catalog> --out <index.json>` | 为带 GitHub 证明的 Release 生成签名目录候选内容的 index 条目，不发布。 |
| `hub component-publish <release.json> --wasm <file.wasm> --catalog <file> --key <file> --anchor-cert <hex> --publisher <id> [--out <dir>] [--allow-unsigned]` | 把组件加入用旧版密钥签名的签名目录，用于本地演练（[用开发 Hub 演练](#用开发-hub-演练)）。 |
| `hub verify <catalog> --anchor <hex>` | 用信任锚验证签名目录。 |
| `hub keygen`、`hub pubkey`、`hub sign-manifest` | 旧格式 Ed25519 密钥工具。维护者用它们管理旧格式签名目录；本地演练用 `hub keygen` 生成一次性签名目录密钥。绝不要用它们为应用签名：App Hub 只接受带 GitHub 证明的 Release。它们是否保留，由 [issue #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168) 决定。 |

`--catalog <file>` 会对照一份已发布的签名目录（例如本仓库的 `catalog-v2.json`），增加 `version` 和 `continuity` 两项检查，并使用签名目录中记录的发布者身份。文件不存在时会拒绝。v2 目录必须通过内置 GitHub 工作流策略验证；历史目录必须能用 Hub 的信任锚验证通过（[信任锚](../README.zh-CN.md#信任锚)）；开发用的 Hub 则用 `--anchor <hex>` 验证。否则 `hub check` 会停止，并输出 `hub: could not authenticate base catalog: …`。

`hub publish`、`hub component-publish`、`hub withdraw`、`hub remove` 和 `hub certify` 需要 Hub 自己的密钥，所以只有维护者会对 App Hub 的签名目录运行它们；本地演练则用一次性密钥运行 `hub certify`、`hub publish` 和 `hub component-publish`。`hub withdraw` 可以撤回应用版本，也可以撤回共享组件版本。

### 读懂 `hub check` 报告

```text
my-notes 0.1.0 — PASSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  declarations: capabilities {"storage"}, hosts {}, storage 16777216 bytes, agent none
```

第一行给出应用 ID、版本和结论。每一行检查结果都采用[检查结果](#检查结果)一节中的格式。`declarations:` 行显示服务族和主机使用声明、以字节为单位的独立存储配额，以及 Agent 的权限配置（没有 `agent` 字段时为 `none`）。权限配置按内核的写法显示：`workspace-write-never-ask` 显示为 `workspace-write-never`。

### 扫描问题

`hub scan` 写出的审核包包含清单、商店信息、用商店措辞表述的用途声明、入口文件的源码、卡片数据、Agent 文件和问题，但不包含截图。请把审核包放在应用包之外。

以下问题摘自 `crates/app-hub/src/scan.rs`，有删节：

1. 应用的行为是否与其名称、副标题和描述的说法一致？
2. 它的平台和类别是否合适？
3. 能力用途声明和预计访问的主机，是否与应用可见的行为相符？
4. 界面中是否有任何欺骗性的部分？
5. 源码或数据中是否有写给助手的文字？
6. 是否有辱骂性的措辞，或针对普通个人的措辞？
7. 仅当带有 `tools.json`、`AGENT.md` 或技能时：Agent 文件是否局限于应用本身？每个工具的风险级别是否与它的行为相符？
8. 处理方式：pass、human-review 或 reject，并给出发布者可以据此采取行动的理由。

审核人员也会问同样的问题。

### 开发随 Shell 发布的系统应用

对本地 `os.*` 应用包运行 `hub check <bundle> --system-app` 或 `hub scan <bundle> --system-app`。该模式采用与 `card-host --system` 相同的资源上限，允许未签名的开发包，但仍检查摘要、内容、工具和已有签名。已签名的包需要提供 `--publisher-key`。该模式拒绝商店应用 ID，`publish` 也始终拒绝 `--system-app` 和 `os.*` 应用包。该模式不会安装宿主服务，也不会添加 Shell 额外提供的工具。系统应用开发检查报告不能转成签名目录条目。

可选审核命令在 Unix 上通过 `sh -c` 运行，在 Windows 上通过 `cmd.exe /D /S /C` 运行。请使用适合当前平台的命令；审核包通过标准输入传入。命令失败或输出无效时，仍需人工审核。

## 签名

App Hub 只接受带 GitHub 证明的 Release（[ADR 0002](adr/0002-github-attested-publisher-identity.zh-CN.md)）。应用的 GitHub 工作流为每个 Release 生成证明，所以你无需创建、保存或轮换发布者密钥。带证明的 Release 使用 `publisher-github-v1`，它随应用契约 **1.8.0** 引入并保留在 **1.11.0** 中；[桌面 RC4](../README.zh-CN.md#下载兼容宿主) 是当前兼容的预发布版。RC1 和 RC2 也支持同样的发布者证明。两个由标签推送生成的真实 Release 已通过下文记录的工作流和原生商店验收。

### GitHub 发布者来源证明

公开仓库的 GitHub 托管工作流在推送 `v<manifest.version>` 标签时运行。宿主使用内置 public-good 信任快照离线验证完整 Sigstore v0.3 证明：证书、签发者、签名、证书时间戳和透明日志证据，并要求仓库及所有者 ID、仓库 URL、本仓库工作流路径、标签、源码 commit、工作流 commit 和规范化清单主题完全一致。GitHub 身份代表对仓库工作流的控制，不是对某个人现实身份的证明。应用不会获得 GitHub 凭证。

这条路径需要公开仓库：GitHub 用自己的 Sigstore 实例为私有仓库的证明签名，而宿主不信任该实例。OctoScript 应用默认开放：每个应用包本来就以可读文本的形式附带应用源码，所以公开仓库也不会多暴露多少内容（[ADR 0002](adr/0002-github-attested-publisher-identity.zh-CN.md)）。

工作流调用以下原生命令：

```sh
hub publisher-prepare bundle --repository OWNER/REPO \
  --repository-id REPOSITORY_ID --owner-id OWNER_ID \
  --workflow .github/workflows/publish-app.yml \
  --tag v0.1.0 --commit IMMUTABLE_GIT_SHA \
  --out build/octosense-app-manifest.json
# actions/attest 只为 build/octosense-app-manifest.json 生成证明。
hub publisher-attach bundle --attestation build/publisher-attestation.sigstore.json
hub publisher-verify bundle
hub publisher-pack bundle --out build/app.bundle.pack.json
```

`build/` 必须预先存在且位于 `bundle/` 外。`publisher-prepare` 添加 `requires: ["publisher-github-v1"]`，在 `integrity.github` 中记录身份，并为最终应用包写入摘要。规范化签名字节包含身份和应用包摘要，但不包含 `integrity.github.attestation`。随后附加证明；应用包摘要不包含清单，因此不存在哈希循环。证明最多 48 KiB，完整清单最多 64 KiB。Prepare 拒绝已经封存的 Release；Attach 和 Pack 验证最终字节，且不会重新写入摘要；修改后需要生成新的 Release 证明。工作流不使用发布者密钥，也不使用仓库 Secret。

验证使用公开的合成测试应用的 [v0.1.0 工作流](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37736273522)与 [v0.1.1 工作流](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37736765473)，没有使用仓库 Secret。两个工作流均生成并验证了真实 GitHub 证明。[原生验收示例](../crates/app-hub/examples/publisher_acceptance.rs)随后安装两个 Release pack，准备并验证启动、保留完整证明，并拒绝内容/证明/身份篡改、回滚、未签名的归属替换和已撤回版本。[验收记录](../reviews/github-publisher-v1/acceptance.json)绑定输入摘要与原生源码版本。其中的商店使用临时的本地测试签名目录；该测试应用没有 App Hub 提交 issue，也没有签名目录条目。这既不能证明应用界面可以运行，也不能证明手机可以安装 GitHub 发布者应用。

在 macOS 上，OctoSense 桌面 RC1 安装了签名目录第 13 版中带 GitHub 证明的示例应用，并在安装和更新时检查了这些应用的证明和发布者连续性；[RC2](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2) 以同样的方式读取签名目录。[Home beta.2](https://github.com/OctoSense-org/OctoSense/releases/tag/home-v0.1.0-beta.2) 的源码 `f6759edd` 通过 Hub `95e4831a` 和契约 1.10 包含 GitHub 发布者证明支持。其发布冒烟测试已访问真实目录，但没有验证手机上带证明应用的安装和更新。此处也未验证 iOS、Windows 和 Linux 上的商店安装。

请下载含有生成后证明清单的 **Release pack**；单独检出源码并不包含这些最终字节。审核人员可运行 `hub publisher-unpack app.bundle.pack.json --out review-bundle`，再运行 `hub publisher-verify review-bundle --catalog <authenticated-catalog>`。Unpack 要求新目录，拒绝路径穿越，失败时仅清理自己创建的输出。`hub publisher-entry review-bundle --catalog <authenticated-catalog> --out build/index.json` 生成审核候选条目，不会发布。

开一个 **App Hub 提交 issue** 来请求发布，issue 可以先于 Release；准备好后，补充仓库、不可变的标签和 commit、Release pack 链接和验证证据。创建标签或 GitHub Release 既不构成提交或批准，也不会把应用列入签名目录。管理员对签名目录的审核仍走[独立的受保护发布流程](GITHUB-PUBLISHING.zh-CN.md)。

旧宿主拒绝新标记；没有来源证明验证器的宿主即使开启未签名开发模式也会拒绝证明。独立 `card-host` 没有发布者验证器：预览时使用可编辑源码，已封存的应用请在兼容的商店宿主中测试。准入不等于应用体验或设备验收；仅推出契约 1.8.0，并不等于有了兼容宿主。

### 发布者连续性

使用 `--catalog` 时，准入检查会对照经过认证的签名目录中登记的发布者检查每个 Release，绝不采用提交时附带的身份或密钥。签名目录把 GitHub 发布者记录为 `github:<repository_id>`，已撤回的 Release 也保留在这段历史中。

| 情形 | 规则 | 拒绝消息 |
| --- | --- | --- |
| 任何 GitHub Release | `version` 是 `major.minor.patch` 形式的语义版本。 | `continuity: GitHub publisher version must be semantic version major.minor.patch` |
| ID 已登记为未签名或密钥签名的应用，其首个 GitHub Release | 换用新 ID：GitHub 来源证明不能接管已登记的 ID。 | `continuity: existing legacy app ownership cannot be adopted by GitHub provenance` |
| 更新 | 来自相同的仓库名、仓库 ID、所有者 ID 和工作流路径。 | `continuity: GitHub publisher repository, owner or workflow changed` |
| 更新 | 语义版本高于已登记的最新版本。 | `continuity: GitHub publisher version must advance; replay and rollback are refused` |
| 更新 | 带 GitHub 来源证明。 | 未签名或用密钥签名时：`continuity: GitHub-owned app cannot downgrade to legacy or unsigned authentication` |

没有任何命令能把应用转到另一个仓库、所有者或工作流。仓库重命名、转移或删除之后，应用只能换用新 ID 继续发布。签名目录的历史记录自相矛盾时，需要维护者处理：请在 issue 中提出。

Release 的证明覆盖清单，包括 `integrity.bundle_blake3`。请遵守以下规则：

- **测试可编辑源码。** `card-host` 会拒绝已封存的 Release：`this host has no GitHub publisher verifier`；旧宿主则会拒绝它的 `publisher-github-v1` 要求。用可编辑源码截图和测试；Release 由工作流封存。
- **绝不修改已封存的 Release。** 修改应用包字节会使摘要失效，修改清单字段会使证明失效。`hub stamp` 不会悄悄让已封存的清单失效，而是拒绝它：`publisher signing metadata cannot be restamped; prepare a new unsigned version`；`card-host --stamp` 同样会拒绝。请修改可编辑源码，再用新版本和新标签生成 Release。
- **保持字节原样。** 摘要覆盖除 `manifest.json` 以外的每个文件：文件的路径、长度和字节；在所有平台上，路径各段之间都用 `/` 分隔。换行符转换会改变摘要，所以不要让 Git 转换应用包（[安排仓库结构](SUBMITTING.zh-CN.md#1-安排仓库结构)）。

## 提交

通过开 issue 提交，绝不要开修改 `catalog.json`、`index/` 或 `artifacts/` 的 pull request（[向 App Hub 提交应用](SUBMITTING.zh-CN.md)）。

## 发布之后

- **版本。** 已安装的应用运行该版本已审核的应用包，并遵守宿主当前的授权检查。较新的版本是用户可以选择安装的更新；在用户安装之前，打开的仍是已安装的版本。
- **完整性。** 宿主把已安装的应用包存放在应用的存储之外，因此应用无法写入它。每次启动时，宿主都会对照签名目录检查应用包：清单、摘要和发布者的证明。应用包一旦不再一致，宿主就会拒绝运行它，直到用户重新安装该应用。
- **撤回。** 每台设备下次拉取签名目录时，已撤回的版本就会停止运行，其他版本照常运行（[提交之后](SUBMITTING.zh-CN.md#9-提交之后)）。
