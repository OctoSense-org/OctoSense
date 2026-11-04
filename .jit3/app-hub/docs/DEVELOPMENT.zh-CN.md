# 应用开发指南导航

[English](DEVELOPMENT.md) | 简体中文

要开发第一个可下载的 Hub 应用，请按照 [开发你的第一个 Hub 应用](FIRST-APP.zh-CN.md) 操作。本仓库规定应用包发布前必须满足的要求：应用包格式、准入检查、签名与提交。编写工具和运行应用的 Shell 位于其他仓库：

| 仓库 | 负责内容 |
| --- | --- |
| [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)（原 Design Flow） | 应用开发工具集：快速上手、脚本 API、脚本应用模板、`tools/octo` 命令、设计流程（`flows/`）和示例应用（`examples/`）。 |
| [OctoSense `apps/appcard`](https://github.com/OctoSense-org/OctoSense/tree/main/apps/appcard) | AppCard（Shell 可选链接的“Ask anything” Agent）和 L0 卡片语言（`a2app-l0/framework/l0.md`）。 |
| [OctoSense `apps/`](https://github.com/OctoSense-org/OctoSense/tree/main/apps) | 系统应用（新闻、相册、地图、相机、邮件、日历、AI providers、YouTube），每个都是 `apps/<name>/bundle/` 下的脚本应用包；邮件、日历、新闻和 AI providers 的宿主服务位于 `apps/<name>/host-service/`。 |
| [OctoSense `crates/oauth-service`](https://github.com/OctoSense-org/OctoSense/tree/main/crates/oauth-service) | 已连接账户的宿主服务：`auth`、`github`、`gmail` 和 `gcalendar`。 |

按任务查找对应的指南：

| 任务 | 指南 |
| --- | --- |
| 搭建工作区，构建 `hub` 和 `card-host` | [快速上手](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md) |
| 准备共享的 Makepad/Octoscript 依赖 | [原生工作区](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/NATIVE-WORKSPACE.md) |
| 从可运行模板开始一个脚本应用 | [快速上手](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md) 和 [`templates/script-app/`](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/templates/script-app) |
| 用元数据和 Agent 指引搭建卡片应用仓库 | [应用起步模板](../templates/app/README.zh-CN.md) |
| 编写脚本应用：状态、处理函数、存储、请求、宿主服务 | [脚本应用流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/script-app/FLOW.md) 和 [脚本 API](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/SCRIPT-API.md) |
| 查询哪个 Shell 提供哪个宿主服务 | [宿主服务](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.zh-CN.md) |
| 把 UI 设计转成原生卡片 | [图像到卡片流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/image-to-card/FLOW.md) |
| 理解卡片的数据、状态、事件、文案、主题和视图 | [L0 语言](https://github.com/OctoSense-org/OctoSense/blob/main/apps/appcard/a2app-l0/framework/l0.md) 和 [L0 笔记](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/docs/l0) |
| 运行应用包，并通过 HTTP 驱动它 | 下文的 [`card-host`](#在本地运行应用包card-host) |
| 测试真实的原生输入、截取画面并清理测试实例 | [原生测试工具](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/core/NATIVE-INSTRUMENT.md) |
| 设置应用自有的图标和随包素材 | [图标](ICONS.zh-CN.md) |
| 打包、验证并签名应用包 | [发布](PUBLISHING.zh-CN.md) |
| 提交应用包供审核 | [提交](SUBMITTING.zh-CN.md) |
| 阅读完整示例 | [示例](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/examples) 和 [系统应用](https://github.com/OctoSense-org/OctoSense/tree/main/apps) |

## 选择合适的交付路径

| 路径 | 要构建的内容 | 交付方式 |
| --- | --- | --- |
| Hub 卡片应用 | `page.card` 及其数据和 `kit/`、本地素材、`manifest.json` 和 `listing.json`。宿主把卡片转换（lower）为原生控件：卡片自身没有逻辑，只在宿主已有的能力范围内运行。 | 商店应用包，提交到 Hub。 |
| Hub 脚本应用 | `main.splash`、本地素材、`manifest.json` 和 `listing.json`。这是一个 Splash 程序，有自己的状态、处理函数、请求和存储，在自己的隔离环境（独立的脚本运行环境）中按清单解析出的策略运行。能力和目的地声明用于披露用途。私有隔离目录与配额、设备授权、账户 scope 和服务可用性仍约束执行；遗漏服务族或主机不会阻止公开 API 调用。 | 商店应用包，提交到 Hub。 |
| 系统应用 | 使用保留 `os.` id 的脚本应用包，在构建时打包进 Shell。OctoSense 的系统应用位于 `apps/`。 | 随 Shell 版本发布。商店应用包不能使用 `os.` id。 |
| 内置原生应用 | 编译进 Shell 版本的源码，按原生工作区和所属应用的构建说明构建。共享的图标约定同样适用。 | 随 Shell 版本发布。仅声明图标，并不能让它通过 Hub 安装。 |
| Agent 生成的应用类型 | 位于 OctoSense `apps/appcard` 的规格说明和 lint 规则，教 AppCard 的 Agent 组合出一种新的应用类型。 | 不是应用包。 |

已安装的 L0 卡片在宿主应用面板内保留其测量画布。渲染器按面板的相对坐标放置子控件；当画布超过可用空间时，原生双轴滚动视图让底部和右侧控件仍可操作。验收时应测试非零面板位置，以及小于原画布的高度和宽度，并滚动到最后一个控件后点击。不带测量位置的语义角色 L0 使用与 `card-host` 相同的流式布局转换。脚本应用继续自行管理响应式布局与滚动。

商店应用包不含原生代码。新的原生 Rust 或 JNI 代码、Python 服务和浏览器控制器，都无法作为卡片应用或脚本应用安装。Rust 编译成 WebAssembly 后可以随商店应用包发布，用 `wasm` 披露这项用途。核心模块无法访问文件、网络或时钟；组件可以在应用的实际授权和资源上限内使用受支持的 WASI 与公开宿主 API。桌面 RC2 在 macOS 和 Linux 上提供核心模块支持。当前 OctoSense 源码还包含 Windows 和 Android 支持，以及使用 Pulley 的 OpenHarmony 构建；构建包含这些功能不等于完成设备或发布版验收（见 App Flow 的[运行自己的 Rust 代码](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/RUST.zh-CN.md)）。

有些 App Flow 示例包含原生服务或网站集成。以这类示例为基础开发 Hub 应用之前，先确认每一项行为都能在应用的隔离环境中运行。复制某个服务项目的源码目录，并不能让它变得可安装。

## 在本地运行应用包：`card-host`

`card-host`（`crates/card-host`）运行单个应用包，可以是卡片应用，也可以是脚本应用。它严格执行清单解析出的策略，处理顺序与设备相同：准入、解析、应用、执行。

在准备好的 [原生工作区](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/NATIVE-WORKSPACE.md) 中，从本仓库构建：

```sh
cargo build --release -p octosense-card-host --bin card-host
```

如果构建失败并报 `no variant … TextInputStateQuery`，请看 [`card-host` 构建失败](#card-host-构建失败)。

生成的可执行文件是 `target/release/card-host`。运行方式：

```sh
card-host [--bundle <dir>] [--app-data <dir>] [--allow-unsigned] [--stamp] [--system] [--static <prefix>=<dir>]... [--size <w>x<h>] [--remote [<port>]]
card-host --help
```

`--help` 打印用法，退出码为 0。遇到未知或格式错误的参数时，会打印问题和用法，退出码为 2。两者都不会打开窗口。

| 参数 | 作用 |
| --- | --- |
| `--bundle <dir>` | 要运行的应用包。默认：当前目录。 |
| `--app-data <dir>` | 应用的存储隔离目录建在 `<dir>/<app id>/`；宿主服务把状态保存在 `<dir>/.host/`。默认：`$TMPDIR/octosense-card-apps`。 |
| `--allow-unsigned` | 准入没有签名的清单。`card-host` 没有发布者验证器，因此即使加了这个参数，它也会拒绝**已封存**（带 GitHub 证明）的 Release：`this host has no GitHub publisher verifier`。请运行未签名的开发版应用包并截图。 |
| `--stamp` | 在准入之前重写清单的 `integrity.bundle_blake3`，使其与目录一致。不加这个参数时，如果应用包的字节在上次 `hub stamp` 之后有变化，`card-host` 会拒绝它。 |
| `--system` | 按系统应用的方式准入：只校验摘要，适用系统上限。空摘要会在内存中补齐。用于开发系统应用。 |
| `--static <prefix>=<dir>` | 把 `<dir>` 中的文件读入内存，在 `<prefix>/...` 路径下提供，与 Shell 提供系统应用内置素材的方式相同。相册使用 `--static photos=<dir>`。可重复使用。 |
| `--size <w>x<h>` | 窗口的内部尺寸（布局点），不带标题栏，使卡片正好获得这个视口。默认：清单设有 [`window`](PUBLISHING.zh-CN.md#窗口尺寸) 时取它的尺寸，处理方式相同，也不受屏幕大小限制；否则为 `412x892`。`card-studio` 用它为每个尺寸各启动一个 `card-host`。 |
| `--remote [<port>]` | 在 `<port>` 上启动 localhost HTTP 控制接口（见 [下文](#通过-http-驱动makepad_remote)）。不指定端口时，`card-host` 会自选一个空闲端口并记录在日志中。`MAKEPAD_REMOTE=<port>` 与 `--remote <port>` 等效。 |

从日志可以看出运行结果：

| 日志行 | 含义 |
| --- | --- |
| `card-host: <id> <version> admitted — capabilities {…}, hosts {…}, storage <n> bytes, agent <profile>` | 应用包已准入；日志列出用途声明和解析后的资源上限，不代表用户同意。 |
| `card-host: window sized <w>x<h>` | 窗口尺寸来自 `--size`，或来自清单的 `window`；后一种情况下，下一行会说明。 |
| `card-host: refused: <reason>` | `card-host` 拒绝了这个应用包。窗口显示“card-host refused this bundle”和原因，应用包中的任何代码都不会运行。 |
| `card-host: realize {json}` | L0 卡片的 lint 和 realize 报告，在绘制卡片之前记录。脚本应用没有这一行。 |
| `card-host: the card did not lower: <error>` | 卡片已准入，但无法转换为控件。 |

`realize` 报告分为四部分，可通过 `/log` 路由（见 [下文](#通过-http-驱动makepad_remote)）读取：

| 键 | 内容 |
| --- | --- |
| `lint` | `check_ui_l0`：`valid`、`level` 和 `diagnostics`。 |
| `realize` | `nodes`、`truncated`（触及上限，树不完整）和 `diagnostics`。 |
| `sources` | 每个已声明数据源的 `$state`：取数据中的 `$status` 条目；没有该条目时，有值为 `ready`，无值为 `pending`。 |
| `lowering` 或 `lower_error` | 原生 kit 包为 `design`；由角色 kit 组合的卡片为 `l0-kit`，其节点带有可检查的 id `beauty_0_1_…`。 |

`card-host` 不注册任何宿主服务，包括 `model`，以及连接 octos（OctoSense 运行的 Agent 内核）的 `octos.*` 服务。在 `card-host` 中，只有用于发现宿主 API 的 `runtime` 会响应。调用 `host.request("mail.list", …)` 的脚本应用会得到 `no service answers "mail" on this device`。依赖服务的应用，请在注册了该服务的 OctoSense Shell 中测试。[宿主服务](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.zh-CN.md) 列出了哪个 Shell 提供哪个服务。已连接账户相关的服务需要 desktop-v0.1.0-beta.2 或更高版本。

`card-host` 也会拒绝清单要求 `host-api-v1`、`backend-api-v1` 或 `script-tools-v1` 的应用，因为这些标记所需的 API 它一个也没有实现。窗口会显示“card-host refused this bundle”和 `app <id> needs a host implementing <method>@1`。这类应用请在 [OctoSense 桌面版 0.1.0-rc.1](../README.zh-CN.md#下载兼容宿主) 中测试，它在自身的[平台限制](HOST-API.zh-CN.md#限制)内实现了这些 API。

### 通过 HTTP 驱动：`MAKEPAD_REMOTE`

用 `MAKEPAD_REMOTE=<port>` 或 `--remote [<port>]` 启动，即可获得 localhost HTTP 控制接口。每个路由都是 GET，除 `/g?raw=1` 外都返回一行 JSON；坐标是窗口内的布局点。

| 路由 | 作用 |
| --- | --- |
| `/snap[?q=text]` | 列出可见控件及其矩形和文本，可直接用作点击坐标；`q` 按 id、类型或文本过滤。 |
| `/click?x=&y=` | 点击指定坐标。 |
| `/t?t=TEXT` | 向当前焦点控件输入文本。 |
| `/k?k=down\|up&c=KeyA` | 按下或释放按键（`ReturnKey`、`Backspace`、`Escape` 等）；`/k?t=TEXT` 输入文本。 |
| `/g` | 截取窗口，返回 `{"png": "<path>", …}`。`/g?raw=1` 直接返回 PNG 字节。 |
| `/log` | 返回最近的日志行，包括 `realize` 报告。 |
| `/quit` | 关闭应用。每次会话都要以它结束。 |

在输入类路由后加 `&wait=1`，会在下一帧绘制完成后才返回，这样随后的 `/g` 能看到结果。`GET /` 列出全部路由。

```sh
MAKEPAD_REMOTE=8151 card-host --bundle my-app/bundle --allow-unsigned --app-data .local-state &
sleep 7
curl -s 127.0.0.1:8151/snap
curl -s '127.0.0.1:8151/click?x=200&y=280&wait=1'
curl -s 127.0.0.1:8151/g          # {"png":"/…/grab-w0-00001.png",…}
curl -s 127.0.0.1:8151/quit
```

即使窗口隐藏，`card-host` 也需要平台的图形会话。未验证：Linux 软件渲染（llvmpipe，包括 WSL）下的截帧。在已报告的运行中，`/g` 在这些环境里返回了 `grab timeout`。

## 发布前检查卡片：`card-studio`

`card-studio`（`crates/card-studio`，[OctoSense ADR 0002](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0002-event-driven-app-agents.md) 第 7 节）在隐藏的 `card-host --remote` 中按每个目标尺寸渲染卡片，运行测量检查，并准备视觉评审请求。它通过 HTTP 驱动 `card-host`，从不链接 Makepad，因此不需要准备好的运行时也能构建。它的检查还可以直接在保存下来的截取结果上运行，无需 GPU。

1. 先构建 [`card-host`](#在本地运行应用包card-host)，再构建 `card-studio`：

   ```sh
   cargo build --release -p octosense-card-studio
   ```

   `card-studio` 按以下顺序查找 `card-host`：`--card-host`、与自身同目录的 `card-studio.json` 中的 `card_host`、`$CARD_HOST`、与自身同目录的可执行文件，最后是 `PATH`。

2. 按三个命名尺寸渲染卡片：glance `350x160`、phone `390x844`、desktop `1200x800`。

   ```sh
   export CARD_STUDIO_KIT=../octoscript-makepad/components/l0   # the L0 kit a bare card is lowered with
   target/release/card-studio render --card news.card --data digest.json \
       --size glance --size phone --size desktop --out out/
   ```

   `render` 写出 `out/report.json`。报告按尺寸列出 PNG、控件快照、控件树、日志、`card-host` 的 `realize` 报告、检查结果和指标。没有错误级结果时退出码为 0，有则为 1。修正所有错误后重新渲染。

3. 准备视觉评审请求：

   ```sh
   target/release/card-studio critique --report out/report.json --rubric skills/card-studio/rubric.md --inline > request.json
   ```

   `request.json` 包含提示词、回答的 JSON schema，以及每个尺寸的 PNG、控件快照和检查结果。请自行把它发送给视觉模型：`card-studio` 不调用任何模型。

4. 在保存下来的截取结果上重新运行测量检查。这一步不需要 `card-host`：

   ```sh
   target/release/card-studio check --snap out/glance.snap.json --tree out/glance.tree.txt --log out/glance.log.json --size glance
   ```

   它以 JSON 打印检查结果和指标，通过时退出码为 0，失败时为 1。如果卡片的行数超出 glance 尺寸的容量，输出如下（节选）：

   ```text
   {
     "findings": [
       {
         "check": "text_truncated",
         "message": "\"Artificial intelligence regulation\" gets 12x14pt, about 9.9x too small for 34 characters: it is cut or squeezed",
         …
         "severity": "error",
         …
       },
       {
         "check": "text_hidden",
         "message": "\"Grid\" was not laid out: the card ran out of room before it",
         …
         "severity": "error",
         …
       },
       …
     ],
     …
     "summary": {
       "errors": 7,
       "infos": 0,
       "pass": false,
       "warnings": 0
     },
     …
   }
   ```

该 crate 在 `src/checks.rs`、`src/report.rs` 和 `src/critique.rs` 中说明了检查项、严重级别模型、报告和评审请求。[`skills/card-studio`](../skills/card-studio/SKILL.md) 把 `card-studio` 封装成 octos 的技能。

## 维护应用仓库

- 把起步模板的 `AGENTS.md` 当作这些指南的入口。把它合并进已有仓库的指引，不要覆盖原有内容。
- 应用特有的行为、数据来源和测试，保留在应用自己的仓库中。
- 记录每次发布构建和测试时所用的 Hub 与运行时版本。
- 离线工作时，保留一份固定到已知 Hub 版本的指南副本，不要使用会悄悄过时的未跟踪副本。
- 构建或准入检查通过，并不能证明视觉正确、输入可用，或应用能在某个平台上运行。请按 [原生测试工具指南](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/core/NATIVE-INSTRUMENT.md) 测试这些方面。

## 故障排查

### `card-host` 构建失败

`cargo build -p octosense-card-host` 在编译 `octosense-appstore` 时停止：

```text
error[E0599]: no variant, associated function, or constant named `TextInputStateQuery` found for enum `makepad_widgets::Event` in the current scope
error: could not compile `octosense-appstore` (lib) due to 1 previous error
```

`crates/appstore/src/services.rs` 匹配了 `Event::TextInputStateQuery`。这是一个 IME 事件，原版 Makepad `d653bc44` 没有定义它，OctoSense 的运行时补丁（`tools/runtime-patches/makepad-settings.patch`）加入了它。商店代码只在特性（feature）`text-input-state-query` 开启时才匹配这个事件：`octosense-appstore` 默认开启它；`octosense-app-hub-app` 也默认开启，并把它传给 `octosense-appstore`。`card-host` 依赖商店代码时关闭了这个特性。

如果只构建 `card-host` 也失败，说明你的 App Hub 代码早于这项改动。更新后重新构建：

```sh
git pull
cargo build --release -p octosense-card-host -p octosense-app-hub
```

Cargo 会合并同一次构建中所有包的特性，只要其中任何一个包保留这个特性，它就会开启。不打 OctoSense 运行时补丁时：

| 包 | 不打补丁能否构建 |
| --- | --- |
| `octosense-card-host`、`octosense-app-hub`、`octosense-card-studio` | 能 |
| `octosense-appstore`、`octosense-app-hub-app` | 只有加上 `--no-default-features` 才能 |
| `octosense-appstore-app`，或整个工作区（`cargo test --workspace`） | 不能 |

要构建最后一行中的包，请按 OctoSense 的 `runtime-patches.lock.json` 列出的顺序，给 `../makepad` 打上 OctoSense 的运行时补丁。
