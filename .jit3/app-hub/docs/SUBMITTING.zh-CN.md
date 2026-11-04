# 向 App Hub 提交应用

[English](SUBMITTING.md) | 简体中文

审核人员是 App Hub 维护者；管理员是在本仓库拥有管理员权限的维护者。在 App Hub 上发布应用分四个阶段：

| 阶段 | 发生什么 | 步骤 |
| --- | --- | --- |
| 1. 请求 | 你开一个提交 issue，填写仓库、版本和所需能力，之后再补充标签、commit、截图和 Release 证据。开 issue 就是请求发布；Release 还没准备好时也可以先开。 | [7](#7-开提交-issue) |
| 2. 检查 | 审核人员对标签对应的 commit 和 Release pack 中的原样字节运行准入检查，并在 issue 中反馈发现的问题。 | [8](#8-审核检查什么) |
| 3. 批准 | App Hub 管理员审核这次提交并批准。 | [9](#9-提交之后) |
| 4. 发布 | Hub 发布已批准的签名目录条目。之后，用户就能在兼容的 OctoSense 构建中搜索、安装和运行这个应用。 | [9](#9-提交之后) |

推送标签或创建 GitHub Release 既不会提交应用，也不会批准应用。

App Hub 只接受带 GitHub 证明的 Release（[ADR 0002](adr/0002-github-attested-publisher-identity.zh-CN.md)）。应用的首个版本和每次更新，都由应用仓库的 GitHub 工作流准备 Release、生成证明并打包，因此你不需要发布者密钥，也不需要仓库签名 Secret。安装这类应用需要支持 `publisher-github-v1` 的宿主；请使用 [RC4 发行版及下载指南](../README.zh-CN.md#下载兼容宿主)。

公开签名目录第 15 版提供带 GitHub 证明的 `io.github.ymote.*` 参考应用（[当前应用](../README.zh-CN.md#应用)）。App Hub 已在签名目录第 14 版撤回较早那批用密钥签名的 `org.octosense.samples.*` 条目；后面的步骤仍以它们为例（见[三个参考应用](#三个参考应用)）。准入、能力、清单和签名规则见[发布参考](PUBLISHING.zh-CN.md)。

```text
提交 issue（可以先开）→ 构建 hub 和 card-host → tools/octo doctor
→ 仓库、清单、商店信息和截图
→ 测试可编辑源码：tools/octo run、shot 和 check，hub scan
→ GitHub 标签工作流 → 验证 Release pack：hub publisher-unpack、hub publisher-verify
→ 在 issue 中补齐证据 → 审核人员检查 → 管理员批准 → 发布到签名目录
```

## 开始之前

宿主负责运行应用包：开发时是 `card-host`，用户安装后是 OctoSense Shell（桌面版或手机版）。宿主服务（例如 `github`、`model`）是 Shell 中的代码，应用通过 `host.request` 调用。

### 准备工具

| 工具 | 来源 | 用途 |
| --- | --- | --- |
| `hub` | App Hub `main` | 为可编辑源码写入摘要并检查、扫描，再准备、验证和打包带 GitHub 证明的 Release。审核人员运行的是同一份代码。 |
| `card-host` | App Hub `main` | 运行未签名的应用包，驱动它并截图。 |
| `tools/octo` | [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)（原 Design Flow） | 创建、运行应用并截图。它封装了 `card-host` 和 `hub`。 |
| OctoSense 桌面版 | [RC4 预发布版，源码 `9266b008`](../README.zh-CN.md#下载兼容宿主)；各平台下载及运行条件见该指南 | 安装带 GitHub 证明的 Release 并运行兼容宿主服务，包括组件、仅作披露的能力策略，以及各自支持的平台上的文档、设备日历、邮件草稿和音频 API。连接账户示例使用 macOS；不附带 OAuth 注册信息。 |

按 [QUICKSTART §1](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md#1-前置条件) 准备好工作区，然后从最新的 `main` 构建 `hub` 和 `card-host`：

```sh
cd ~/octosense-ws/OctoSense-App-Hub
git pull
cargo build --release -p octosense-app-hub --bin hub
cargo build --release -p octosense-card-host --bin card-host
export PATH="$PWD/target/release:$PATH"
hub
```

不带参数运行 `hub` 会输出用法，第一行是 `hub — the OctoSense app hub command (ADR 0003)`。任何命令加上 `--help` 或 `-h`（例如 `hub check --help`）也会输出用法。如果 `hub check --help` 输出的是 `hub: No such file or directory (os error 2)`，说明你的 `hub` 比 `main` 旧：拉取最新代码后重新构建。

如果构建失败并报 `no variant … TextInputStateQuery`，请看 [`card-host` 构建失败](DEVELOPMENT.zh-CN.md#card-host-构建失败)。

然后在 App Flow 检出目录中运行 `tools/octo doctor`，之后再构建或检查应用：

```sh
cd ~/octosense-ws/OctoSense-App-Flow
tools/octo doctor
```

`tools/octo doctor` 会查找 `hub` 和 `card-host`，排除 GitHub 推出的同名 `hub` CLI（与本工具无关），并检查 Python 和应用模板。成功时最后一行是 `ready: tools/octo new <dir> --platform <target> && tools/octo run <dir>/bundle`。否则它会输出一行 `[fail]`、查找过的每个位置，以及修复用的命令。

旧版或打过补丁的 `hub` 可能放行一些应用包，而审核人员用的构建会拒绝它们。请使用未打补丁的 `main` 构建，并在提交之前不要改动这个检出目录。记录精确工具版本，使用经过认证的 `catalog-v2.json` 检查发布者连续性；`catalog.json` 是需要显式选择的旧格式签名目录（[第 6 步](#6-冻结并验证发布)）。

### `hub` 命令

`hub help` 会输出每个命令的确切用法；只使用它列出的命令。下表列出每个命令由谁运行、用在哪一步。参数和准入检查报告的说明见发布参考的[命令](PUBLISHING.zh-CN.md#命令)一节。

| 命令 | 由谁运行 | 步骤 | 用途 |
| --- | --- | --- | --- |
| `hub stamp` | 你 | 2、5 | 把可编辑应用包的摘要写入清单。 |
| `hub check` | 你，然后是审核人员 | 5、8 | 运行准入检查。 |
| `hub scan` | 你，然后是审核人员 | 5、8 | 运行准入检查，再写出审核包。 |
| `hub publisher-prepare`、`hub publisher-attach`、`hub publisher-pack` | 应用的 GitHub 工作流 | 6，推送标签时 | 封存 Release、附上 GitHub 的证明，并打包应用包。 |
| `hub publisher-unpack` | 你，然后是审核人员 | 6、8 | 把下载的 Release pack 解包到一个新目录。 |
| `hub publisher-verify` | 工作流、你和审核人员 | 6、8 | 验证 Release 的证明，并运行准入检查。 |
| `hub publisher-entry` | 审核人员 | 8 | 生成候选签名目录条目；不发布任何内容。 |
| `hub catalog-prepare`、`hub catalog-envelope`、`hub catalog-verify` | 管理员，通过受保护的签名目录工作流 | 9 | 准备、封装并验证签名的 `catalog-v2.json`（[GITHUB-PUBLISHING.zh-CN.md](GITHUB-PUBLISHING.zh-CN.md)）。 |
| `hub publish`、`hub withdraw`、`hub remove`、`hub certify`、`hub verify` | 维护者，用于旧格式签名目录；你只会在演练用的签名目录上运行 `hub certify`、`hub publish` 和 `hub verify` | 9 | 在 `catalog.json` 中发布、撤回或删除版本，为它的工作密钥签发证书，或验证它。 |
| `hub keygen`、`hub pubkey`、`hub sign-manifest` | 维护者，用于旧格式签名目录；你只会在演练时用 `hub keygen` 生成一次性签名目录密钥 | — | 创建 Ed25519 密钥、输出公钥，或为旧格式清单签名。绝不要用它们为应用签名或发布应用：App Hub 只接受带 GitHub 证明的 Release。它们是否保留，由 [issue #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168) 决定。 |

App Flow 的 `tools/octo` 会替你运行其中几个命令，参数见其 [`tools/octo` 命令表](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/README.zh-CN.md#toolsocto)。

| `tools/octo` 命令 | 运行的 `hub` 命令 |
| --- | --- |
| `new` | `hub stamp` |
| `check` | 先 `hub stamp`，再 `hub check --allow-unsigned`，并把 `--catalog` 和其他 `hub check` 参数原样传给它 |
| `doctor` | `hub help`，用来确认找到的正是这个 `hub` |
| `publish-github` | 本身不运行：它安装 Release 工作流，推送标签时由该工作流运行 `hub publisher-prepare`、`hub publisher-attach`、`hub publisher-verify` 和 `hub publisher-pack` |

在本地[商店演练](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/PUBLISHING.zh-CN.md#4-在本地演练商店流程)中，你还要对一次性的测试签名目录运行 `hub keygen`、`hub certify`、`hub publish` 和 `hub verify`。你在这里创建的密钥只为这个签名目录签名，从不为你的应用签名；演练也不会向 App Hub 发布任何内容。

### 应用能在哪里运行

| 宿主 | 能运行 | 不能 |
| --- | --- | --- |
| `card-host` | 单个未签名的应用包，并提供远程控制接口，用于驱动和截图 | 提供用于发现宿主 API 的 `runtime` 以外的任何宿主服务：其他每个 `host.request` 都会失败，返回 `no service answers "<family>" on this device`。它也拒绝已封存的 Release，以及要求 `host-api-v1`、`backend-api-v1` 或 `script-tools-v1` 的应用。 |
| [RC4 预发布版](../README.zh-CN.md#下载兼容宿主)，源码 `9266b008` | 契约 1.11、仅作披露的能力/网络策略、Wasm 核心模块和组件，以及经验证目录中的共享组件安装；保留已有账户、设备同意及可信审阅流程 | 不会补充发行方 OAuth 注册信息、把组件自动发布到公开目录、升级手机版，或绕过平台及授权限制。构建、归档包和设备验收范围见发布来源记录。 |
| 历史 RC2 发行版 `4ccf8e06`（RC1 `933abbcf` 的签名目录和发现机制相同，但没有 RC2 的 API） | 公开 v2 目录、`publisher-github-v1`、发现及已打开应用的 Splash 工具；macOS 和 Linux 上应用自带的 Wasm 函数；文件、设备日历、邮件草稿和音频服务，受平台限制（[宿主目前提供什么](../README.zh-CN.md#宿主目前提供什么)） | 附带 OAuth 注册信息、在 Windows 上运行 Wasm、在 Windows 或 Linux 上批准邮件发送或提供设备日历、音频会话和新鲜位置采样，或在其他系统上运行仅声明 macOS 的应用。Linux/Windows 的嵌入式后端登录及受保护写操作仍不支持；外部浏览器后端登录和读取已实现，用 RC2 源码构建的 Windows 测试程序已对一个模拟后端完成登录。 |
| 历史 desktop-v0.1.0-beta.2（macOS，Apple 芯片） | 已安装的应用，包括使用 `auth`、`github`、`gmail` 和 `gcalendar` 的应用 | 在宿主的 `oauth/clients.json` 中配好 OAuth 注册之前，登录 GitHub 或 Google（[配置方法](https://github.com/OctoSense-org/OctoSense/blob/desktop-v0.1.0-beta.2/crates/oauth-service/README.md)）；该发布版不附带任何注册信息。让应用登录它自己的后端。安装请求 `wasm` 的应用：它的商店会以 `unknown capability "wasm"` 拒绝。未在该发布版上验证：真实的提供商登录。 |
| desktop-v0.1.0-beta.1 和 home-v0.1.0-beta.1（目前唯一发布的手机版本） | 所用能力都在旧版应用契约之内的商店应用 | 安装请求 `auth`、`github`、`gmail`、`gcalendar`、`calendar`、`photos`、`youtube`、`wasm` 或 `palpo.*` 的应用。它们的商店会拒绝这类应用，例如报 `unknown capability "auth"`。 |

RC1 和 RC2 发行版与历史 desktop beta.2 有以下不同；RC2 还新增了[宿主目前提供什么](../README.zh-CN.md#宿主目前提供什么)中列出的 API。[下载发行文件并确认运行条件](../README.zh-CN.md#下载兼容宿主)。

- 发行方可以在构建时编入 GitHub 和 Google 注册信息。你自己从源码构建的版本不带注册信息，需要自行添加，例如写入 `oauth/clients.json`（见发布参考的[已连接账户](PUBLISHING.zh-CN.md#已连接账户)一节）。
- 应用可以登录清单声明的后端，并在[下文的平台限制](#确认你的平台)内调用清单列出的后端操作（见发布参考的[登录自己的后端](PUBLISHING.zh-CN.md#登录自己的后端)一节）。
- 声明了 `host-api-v1` 的应用可以在 macOS 和 Android 上使用设备权限方法；在 RC2 上，还能在具备相应功能的平台上使用新鲜位置采样、设备日历、邮件草稿和音频会话（见[宿主 API 兼容性](HOST-API.zh-CN.md)）。
- 批准 GitHub 或 Google 日历的保存时，须在宿主的确认面板上亲手点按；批准 Gmail 发送在两个版本上都有这个要求。
- 应用 Agent 调用 `glance.publish` 时，宿主拒绝可执行的 Splash（`script`）和 L1 卡片源码。
- 宿主只保留从 30 天前到 366 天后的 Google 日历日程，而不是日历的全部历史。

想在 App Hub 发布之前试用应用的某个 Release，可以在从源码构建的 Shell 中，从本地测试签名目录安装它（[演练步骤](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/PUBLISHING.zh-CN.md#4-在本地演练商店流程)）。未验证：用带 GitHub 证明的 Release 进行演练；已记录的结果来自一个用密钥签名的测试应用。

### 确认你的平台

- **macOS（Apple 芯片）**：当前公开示例使用兼容 RC4 预发布版。安装、本地草稿和更新与真实提供商效果是不同的验收范围，见[下载与账户限制](../README.zh-CN.md#下载兼容宿主)。
- **Windows x64 / Linux x86_64**：RC4 安装包不会改变应用的 `listing.platforms`。当前四个示例只声明 macOS。原生浏览器和宿主测试不是你的应用验收；每个声称支持的平台都要验证依赖和缺少服务时的状态。
- RC2 的邮件发送审阅、设备日历、音频会话和新鲜位置采样只在 macOS（和 Android）上有，Windows 和 Linux 没有。商店信息列出 Windows 或 Linux 时，请把这些方法放在 `host_api.optional` 下，否则应用无法在那里安装（见[宿主 API 兼容性](HOST-API.zh-CN.md)）。
- Windows 使用 `python tools/octo`，它能找到 `.exe`。用[第 1 步的 `.gitattributes`](#1-安排仓库结构) 保留应用包字节，再验证全新检出（[第 6 步](#6-冻结并验证发布)）。
- Linux WebReader 需要 GTK 3/WebKitGTK 和 X11/XWayland，不支持原生 Wayland。Windows 使用 WebView2；这些引擎不随包附带。Linux/Windows 已实现外部浏览器后端登录和清单声明的读取；RC2 补上了登录所需的原生链接打开方式，用 RC2 源码构建的 Windows 测试程序已对一个模拟后端完成浏览器登录，Linux 上的登录仍未验证。嵌入式后端登录和受保护的写操作仍不支持，会拒绝执行。见[浏览器要求](https://github.com/OctoSense-org/OctoSense/blob/4ccf8e068399b1da139771a9ed94cef05fa6ae60/docs/desktop-embedded-browser.zh-CN.md)。

## 三个参考应用

要下载当前版本，请使用[公开签名目录表](../README.zh-CN.md#应用)中带 GitHub 证明的版本（GitHub Notes 为 0.2.2，其余为 0.2.1）及新 ID。0.2.0 → 0.2.1 更新保持同一 GitHub 发布者身份，不需要开发者签名密钥。

### 历史 0.1.x 示例

签名目录第 10 版收录了发布者 `ymote` 的三个 macOS 开发者预览版，每个应用有两个版本：首个版本 0.1.0 和后续的 0.1.1，后者针对 [0.1.0 的教训](#010-的教训)中的三个问题做了改进。这六个条目用发布者密钥签名；接替它们的是上文带 GitHub 证明的 0.2.x 版本。每个应用都有自己的公开仓库，标签为 `v0.1.0` 和 `v0.1.1`。下表描述的是 0.1.1：

| | [GitHub Notes](https://github.com/ymote/octosense-github-notes) | [Inbox Assistant](https://github.com/ymote/octosense-inbox-assistant) | [Google Calendar](https://github.com/ymote/octosense-google-calendar) |
| --- | --- | --- | --- |
| 应用 ID | `org.octosense.samples.githubnotes` | `org.octosense.samples.inbox` | `org.octosense.samples.googlecalendar` |
| 演示内容 | 把 Markdown 草稿保存为经用户确认的 GitHub commit | 读取 Gmail、共用一份回复草稿、在后台分拣新邮件 | 日程视图，含日程草稿、经用户确认的保存和提供建议的应用 Agent |
| 能力 | `storage`、`auth`、`github` | `storage`、`auth`、`gmail`、`model`、`glance`、`octos.session.open`、`octos.turn.start` | `storage`、`auth`、`gcalendar`、`glance`、`octos.session.open`、`octos.turn.start` |
| 应用 Agent | `read-only` 权限配置，只在前台运行；3 个 `read` 工具 | `read-only` 权限配置，`background: true`，由 `inbox.new_message` 触发；1 个技能；9 个工具（4 个 `read`，5 个 `act`） | `read-only` 权限配置，只提供建议；4 个 `read` 工具 |
| 速览卡片 | 无 | 模板文件 `glance-workspace.splash`，以 `template` 和 `initial` 发布 | `main.splash` 中的 L0 卡片，以 `source` 和 `data` 发布 |
| 受保护的写操作 | 通过 `github.review_save` 创建 commit，在宿主的确认面板上批准 | 通过 `gmail.draft.review` 发送，须在宿主的审阅界面上亲手点按批准按钮 | 通过 `gcalendar.review_save` 保存，在宿主的确认面板上批准 |

应用 Agent 是 OctoSense 为单个应用运行的 AI Agent，使用该应用的工具。`read-only` 是 Agent 自身会话的权限配置：Agent 每次写入工作区（即应用的存储）之前都要先征得同意。它不约束 `tools.json` 中的工具：这些工具的调用是否要等用户确认，由各自的 `risk` 级别决定。Inbox 的 5 个 `act` 工具会直接修改回复草稿、记录分拣结果、发布速览卡片，无需询问。

App Flow 的[连接账户参考应用 README](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/examples/connected-apps/README.zh-CN.md) 介绍了每个应用的工作方式。其中 GitHub Notes 和 Inbox 的副本包含与 0.1.1 相同的修复；Google Calendar 的副本不显示日期范围。

### 0.1.0 的教训

每个应用的 0.1.0 版都通过了审核，但各有一个问题，0.1.1 版对此做了改进。你的应用应从一开始就避开这些问题：

- **卡片工具绝不接受 `script`。** Inbox 0.1.0 的后台工具 `inbox.notify` 映射到 `glance.publish`，除了 `template`，还接受最大 16 KiB 的 `script` 卡片。OctoSense 桌面版 0.1.0-beta.2 会按应用自身的策略运行 Agent 发布的 `script` 卡片，因此一轮遭到提示注入的后台对话就可能发布任意 Splash 代码。Inbox 0.1.1 只接受已准入的模板加 `initial` 数据；RC1 和 RC2 发行版都会拒绝 Agent 发布的 `script` 卡片。只接受 `template` 加 `initial`，或 L0 `source` 加 `data`。
- **带了工具，就声明 `agent`。** GitHub Notes 0.1.0 在 `tools.json` 中带了三个工具，却写着 `"agent": null`。Hub 照样把这些工具作为应用 Agent 准入，OctoSense 也照样提供这个应用 Agent，但准入检查的 `grants:` 行输出的是 `agent none`，OctoSense 桌面版 0.1.0-beta.2 的商店显示的也是“Runs no assistant.”。GitHub Notes 0.1.1 声明了 `read-only` 的 `agent` 字段并附带 `AGENT.md`，于是准入检查输出 `agent read-only`，每个商店都会显示这个应用 Agent（见发布参考的[带 `tools.json` 的应用都有 Agent](PUBLISHING.zh-CN.md#带-toolsjson-的应用都有-agent)一节）。在隐私政策中说明应用 Agent。如果应用不应有 Agent，就不要带 `tools.json`。
- **显示日期范围。** Google Calendar 0.1.0 列出日历的全部历史，最早的日程排在最前面：OctoSense 桌面版 0.1.0-beta.2 会同步所有日程，Agent 的 `cached` 工具也会把它们全部返回。Google Calendar 0.1.1 在宿主报告日期范围时显示“past 30 days / next 366 days”，没有报告时显示“date range unavailable”，也不再把范围之外的日程说成已删除。RC1 和 RC2 发行版只同步这个范围。显示一段日期范围，并只把选中的日程交给 Agent，而不是整个缓存。

0.1.1 中还留着一个问题：Inbox 的后台工具可以改写回复草稿，包括 `to` 地址；改了收件人的回复，只有宿主的审阅界面（发送须亲手点按）能拦住。让后台工具只做读取，以及需要用户确认的写操作。

## 1. 安排仓库结构

Hub 只准入 `bundle/`。准入检查不读取仓库中的其他内容，但审核人员会打开你的隐私政策和支持页面。把 Release 工作流 `.github/workflows/publish-app.yml` 与可编辑的 `bundle/` 一起 commit。应用仓库包含以下文件：

```text
my-app/
  bundle/                             提交的内容：清单、商店信息、代码、工具、素材、截图
  .github/workflows/publish-app.yml   Release 工作流
  .gitattributes                      防止 Git 转换应用包的字节
  PRIVACY.md                          privacy_policy_url 指向的页面
  SUPPORT.md                          如何报告问题
  README.md                           应用做什么，以及如何验证 Release
  review/                             准入检查输出（GATE.txt）和扫描问题的回答（ANSWERS.md）
  LICENSE, NOTICE
  .gitignore                          把 build/ 和 .local-state/ 挡在 Git 之外
```

[GitHub Notes 0.2.1](https://github.com/ymote/octosense-github-notes/tree/v0.2.1) 采用的就是这种结构。其中的 `publisher.json` 是 0.1.x 留下的，你不需要这个文件。

开发 README、密钥、审核包和 `.local-state/` 放在 `bundle/` 之外。素材要求保留的许可证和来源说明应以 `.txt` 或 `.md` 随包提供；这些文档中的链接不会授予网络权限。准入检查会按规则检查应用包中的每个文件，并计入 8 MiB 的大小上限。`tools/octo new` 生成的 `.gitignore` 已经排除了 `build/`、`.local-state/` 和 `*.key`。

添加一个 `.gitattributes` 文件，让 Git 原样保存和检出应用包的字节。参考应用的 0.1.x 标签没有这个文件，所以检出时只要设置了 `core.autocrlf=true`（Windows 上很常见），Git 就会把它们的文本文件转换成 CRLF 换行，连摘要也对不上。

```sh
cd ~/apps/my-app
printf 'bundle/** -text\n' >> .gitattributes
git check-attr text -- bundle/manifest.json
```

成功时输出 `bundle/manifest.json: text: unset`。如果应用包已经 commit 过，运行 `git add --renormalize bundle`，再重做[第 5 步](#5-生成最终字节)。

Git 从仓库根目录开始匹配这个模式，所以 `bundle/**` 只覆盖根目录下的应用包。如果应用包在更深的目录中，就写出它的实际路径，例如 `apps/my-app/bundle/** -text`；也可以写 `**/bundle/** -text`，覆盖任意层级的 `bundle/`。然后对这个应用包的 `manifest.json` 运行 `git check-attr`，对它的目录运行 `git add --renormalize`。输出 `text: unspecified` 说明这个模式没有覆盖应用包，Git 仍可能转换其中的文件。

## 2. 写对清单

编辑 `bundle/manifest.json`。下面是 GitHub Notes 0.1.0 在写入摘要之前的清单：

```json
{
  "schema": 1,
  "id": "org.octosense.samples.githubnotes",
  "version": "0.1.0",
  "name": "GitHub Notes",
  "integrity": { "bundle_blake3": "" },
  "capabilities": ["storage", "auth", "github"],
  "storage": { "accounts": true, "max_bytes": 4194304 }
}
```

- **`id`** 永久不变。只能用 `[a-z0-9.-]`，最长 64 个字符。准入检查拒绝以 `os.` 开头的 ID。ID 的最后一段会成为工具的命名空间（`githubnotes.*`），不能是保留名称；[常见拒绝原因](#常见拒绝原因及修复)中 `identity` 一行列出了全部 23 个。`tools/octo new` 也会拒绝这些名称。
- **`version`** 每次提交都要用新值。标签是 `v` 加上这个版本号。
- **`capabilities`** 只列出界面确实用到、而且有宿主提供服务的能力（见 [Hub 目前做不到的事](#hub-目前做不到的事)）。商店会在安装前把每一项展示给用户。
- **`integrity`** 存放应用包摘要，由 `hub stamp` 计算并写入（即“写入摘要”）。不要手工修改。

然后写入应用包摘要：

```sh
hub stamp bundle
```

成功时输出应用包摘要：对 `bundle/` 中除 `manifest.json` 以外的所有文件计算的 BLAKE3，共 64 个十六进制字符。如果 `hub stamp` 输出 `hub: manifest is not valid: unknown field …`，说明清单中有契约未定义的字段；消息里列出了所有合法字段。`hub stamp` 不检查能力和商店信息，这些由第 5 步的 `hub check` 负责。

## 3. 撰写商店信息

`bundle/listing.json` 是商店在安装前展示给用户的内容。Release 的 GitHub 证明通过应用包摘要覆盖了它，所以以后哪怕只改一个字，也需要新版本。不要写任何会过时的内容。

| 字段 | 规则 | 参考应用 |
| --- | --- | --- |
| `subtitle` | 最多 80 个字符 | `"Markdown drafts with reviewed GitHub saves"` |
| `description` | 最多 4000 个字符 | 写明哪些是虚构数据、哪些未经验证 |
| `category` | [发布参考](PUBLISHING.zh-CN.md#商店信息)列出的 15 个类别之一 | `productivity` |
| `keywords` | 最多 10 个 | 各 4 个 |
| `screenshots` | 应用包内的 1 到 8 个 PNG 或 SVG 文件 | 2 到 4 张真实的 PNG 截图 |
| `icon` | 应用包内的正方形 SVG 或 PNG（[图标](ICONS.zh-CN.md)） | `assets/icon.svg` |
| `platforms` | 只写实际运行过的平台 | `["macos"]` |
| `publisher` | `name`、`support`（URL 或电子邮件地址），以及以 `https://` 开头的 `privacy_policy_url` | 仓库的 Issues 页面和 `PRIVACY.md` |
| `age_rating` | `all`、`12+`、`16+` 或 `18+` | `all` |

把模板商店信息中的占位内容全部换掉（`example.com` 链接和“Replace with …”文字），并把 `platforms` 设为你实际运行过的平台。准入检查接受占位内容，`tools/octo check` 则会把它们标出来。

提交之前先写好隐私政策。说明哪些数据会离开设备、去往哪里：通过宿主服务访问的提供商 API、发给用户所选模型提供商的文本、速览卡片，以及应用是否带有应用 Agent。GitHub Notes 发布 0.1.0 之后修订过隐私政策，因为它的 `tools.json` 带来了一个政策中没有提到的应用 Agent。

推送标签之前，先检查商店信息中的链接；推送之后再发现 URL 有误，就只能用新版本来修正。先公开隐私政策页面，再逐个请求 `publisher` 字段中的 URL：

```sh
python3 -c 'import json; p = json.load(open("bundle/listing.json"))["publisher"]; print(p["privacy_policy_url"]); print(p["support"])' \
  | grep -E '^https?://' \
  | while read -r url; do echo "$(curl -sL -o /dev/null -w '%{http_code}' "$url") $url"; done
```

成功时每个 URL 都输出 `200`。GitHub Notes：

```text
200 https://github.com/ymote/octosense-github-notes/blob/main/PRIVACY.md
200 https://github.com/ymote/octosense-github-notes/issues
```

`support` 是电子邮件地址时，这条命令会跳过它。`-L` 会跟随重定向，所以放在你自己域名上的隐私政策页面也能报告最终状态。输出 `404` 表示页面尚未公开、仓库是私有的，或者地址拼错了。

## 4. 截图

商店信息里，只有截图能让审核人员对照运行中的应用核实。在 `card-host` 中运行未签名的应用包，通过远程控制接口把应用驱动到要展示的每个状态（路由见 [QUICKSTART](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md)），逐一截图。在 App Flow 的检出目录中运行：

```sh
tools/octo run ~/apps/my-app/bundle --port 8141 --detach
tools/octo shot 8141 ~/apps/my-app/bundle/screenshots/01-main.png
curl -s 127.0.0.1:8141/quit
```

成功时输出：

```text
wrote …/bundle/screenshots/01-main.png (824x1784, 32539 bytes). Look at it before you ship it.
{"ok":1}
```

使用前逐张打开 PNG 查看。不要发布错误画面、空白的首帧或效果图。如果应用一直在播放动画，`shot` 仍会保存最后一帧，并在输出中注明 `(still changing after 2s, e.g. an animation; this is the last frame)`；请检查这一帧。其他截图问题见 [QUICKSTART 的故障排查](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md#故障排查)。

未验证，仅限 Linux：如果在软件渲染（llvmpipe、WSL）下 `shot` 超时，请在启动 `tools/octo run` 或 `card-host` 之前设置 `MAKEPAD_WRITE_FRAMEBUFFER_PNG=<file>`，Makepad 的 OpenGL 后端每次绘制窗口时都会把窗口画面保存到这个 PNG 文件；在 macOS 上，`shot` 仍是经过验证的截图方式。

需要宿主服务或原生控件（例如 GitHub Notes 的 Markdown 编辑器）的界面，在 `card-host` 中渲染不出来。这样的界面，请在提供该服务或控件的 Shell 中用虚构数据截图；否则，截下它的不可用状态，就像 Google Calendar 的 `03-host-required.png` 那样。在描述中注明哪些截图用了虚构数据。

`hub scan` 生成的审核包不含截图。审核人员会在应用包和你的 issue 中查看截图。

## 5. 生成最终字节

应用的首个 Release 和每次更新，都要使用公开仓库的 GitHub 发布者来源证明；这是 App Hub 接受的唯一路径。OctoScript 应用默认开放：每个应用包本来就以可读文本的形式附带应用源码，所以公开仓库也不会多暴露多少内容（[ADR 0002](adr/0002-github-attested-publisher-identity.zh-CN.md)）。这条路径需要应用契约 1.8.0，以及支持 `publisher-github-v1` 的宿主。一个测试应用的两个 Release 均由标签推送生成，并通过了原生商店的安装、更新和启动检查（[证据与限制](PUBLISHING.zh-CN.md#github-发布者来源证明)）。如需当前的公开签名目录，请使用 [RC4 预发布版](../README.zh-CN.md#下载兼容宿主)。

1. 如果尚未开提交 issue，现在就可以开。Release 证据可以稍后补充；未完成的检查标为待完成，不要编造通过结果。
2. 测试可编辑源码并截取真实界面。运行准入检查和 `hub scan bundle --packet build/review.json`，其中 `build/` 位于应用包外。逐题回答审核包中的问题：共七题，带 `tools.json`、`AGENT.md` 或技能时为八题。注明每个回答依据的文件及未测试的行为。
3. 用 App Flow 的 `tools/octo publish-github <app-directory>` 安装并评审 `.github/workflows/publish-app.yml`（`tools/octo new` 也会提供）。工作流不需要发布者密钥，也不需要仓库签名 Secret。原生命令见 [GitHub 发布者来源证明](PUBLISHING.zh-CN.md#github-发布者来源证明)。
4. Commit 测试过的可编辑源码、截图、商店信息、隐私政策/支持页面、`.gitattributes` 和工作流。每个 Release 使用新的语义版本和精确的 `v<manifest.version>` 标签。工作流生成带证明的清单及最终 Release pack；不要用这些生成的字节覆盖可编辑源码并 commit。

常规更新沿用相同的仓库名、工作流，以及不可变的仓库 ID 和所有者 ID，并提高语义版本。绝不要给已封存的 Release 重新写入摘要：修改可编辑源码，再用新版本生成 Release。

## 6. 冻结并验证发布

标签标识测试过的可编辑源码和工作流。Release pack 包含最终带证明的清单；仅检查源码克隆并不能验证这个 pack。

1. 评审测试过的 commit 后，推送新的 `v<version>` 标签。记录 `git rev-parse "v0.1.0^{commit}"`（换成你的版本），并用 `git ls-remote origin 'refs/tags/v0.1.0*'` 确认。附注标签的 `^{}` 行标识 commit。不要移动、删除或重建已推送的标签；修正需要更高版本和新标签。
2. 等待 GitHub 工作流成功。保存工作流运行 URL、精确 commit、Release URL、`app.bundle.pack.json` 和 `release-receipt.json`。记录中的 `pack_sha256` 必须与下载的 pack 匹配，例如在 macOS/Linux 上运行 `shasum -a 256 app.bundle.pack.json`。
3. 验证下载的最终字节，不要修改它们：

   ```sh
   hub publisher-unpack app.bundle.pack.json --out review-bundle
   hub publisher-verify review-bundle --catalog /path/to/catalog-v2.json
   git -C ~/octosense-ws/OctoSense-App-Hub rev-parse HEAD
   ```

   `review-bundle` 必须是尚不存在的目录。使用当前经过认证的目录，且拟提交版本尚未准入；这会同时核对发布者连续性和版本递增。保留完整验证输出及精确工具版本。拒绝结果是需要修复的问题，不能靠删除证明或添加 `--allow-unsigned` 绕过。
4. 在已有的提交 issue 中补充这些产物及测试证据。审核人员会验证源码 commit 和下载的 pack。Release 工作流成功并不等于安装、提交或批准应用。

## 7. 开提交 issue

用 [Submit an app 表单](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/new?template=submit-app.yml)开一个 issue，标题写作 `Submit <app id> <version>`。开这个 issue，就是请求发布这个应用。**第 1–6 步尚未完成时也可以先开。** Release 证据准备好后，补充到同一个 issue 中；待完成字段不代表获得批准。

在 App Hub 首次发布你的应用之前，始终使用这一个 issue：每个新 Release 都以评论的形式发在其中，写明它的标签、完整的 commit SHA 和工作流运行链接，并把 issue 标题和正文中的 Version 字段改成新版本。首次发布之后，每个新版本都要开新 issue（见[第 9 步](#9-提交之后)）。

| 表单字段 | 何时提供什么 |
| --- | --- |
| Release status（Release 状态） | 现在：证据待补充，或 GitHub Release 已准备好 |
| App id / Version / Repository URL（ID/版本/仓库） | 现在：清单 ID、计划版本及公开 GitHub 仓库 |
| Requested capabilities and app behavior（能力与行为） | 现在：应用用途及每项能力的理由 |
| What is not verified（未验证项） | 现在及每次更新后：未完成检查、宿主/平台/提供商限制 |
| Tag / Full commit SHA / Bundle path（标签/commit/路径） | 批准前：不可变源码标签及 commit、开发应用包路径 |
| GitHub workflow run URL（工作流运行） | 批准前：与这个 Release 对应的成功标签推送运行 |
| Release and pack URL / Release pack SHA256（下载与摘要） | 批准前：最终 pack、receipt 及匹配的下载摘要 |
| Bundle BLAKE3 digest（应用包摘要） | 批准前：带证明的 Release 清单中的摘要 |
| Privacy policy URL / Support contact（隐私/支持） | 批准前：商店信息中的有效链接或支持邮箱 |
| Platforms tested / App Hub revision the gate ran on（平台/工具版本） | 批准前：精确宿主/工具版本和已测试行为 |
| Gate and publisher verification output（准入与证明验证） | 批准前：完整开发准入检查及下载 pack 的验证输出 |
| Scan answers / Screenshots（扫描回答/截图） | 批准前：附源码依据的完整回答及真实原生截图 |
| Confirmations（确认项） | 无机密、推送后标签不可变、如实列出待完成证据 |

不要开修改 `catalog.json`、`catalog-v2.json`、`index/` 或 `artifacts/` 的 PR。审核后的签名目录发布是 App Hub 管理员的独立[受保护 GitHub 工作流](GITHUB-PUBLISHING.zh-CN.md)。

## 8. 审核检查什么

由审核人员（而不是机器人）检查你的提交：开 issue 不会触发任何工作流。审核人员对标签对应的 commit 和 Release pack 中的原样字节运行准入检查，并核验 GitHub 身份、源码 commit、Release 证明、pack 摘要、隐私与界面证据。审核人员会把发现的问题以评论的形式发到你的 issue 中。受保护的签名目录工作流会再次核对同样的字节，但从不运行提交者的代码。历史参考应用 0.1.0 版的准入记录 [`reviews/connected-apps-0.1.0/admission.json`](../reviews/connected-apps-0.1.0/admission.json) 列出了对每个应用确认过的内容：

| 检查项 | 0.1.0 记录中的字段 | 自己怎么查 |
| --- | --- | --- |
| 标签解析到所声明的 commit | `tag_verified` | `git ls-remote`（第 6 步） |
| 隐私政策和支持地址返回 HTTP 200 | `public_privacy_and_support_http` | 第 3 步的链接检查 |
| Release 下载文件与哈希值一致 | `release_download_hashes_match` | 创建了 Release 时：`shasum -a 256 -c SHA256SUMS` |
| 下载的 Release 上，发布者的证明验证通过 | `downloaded_publisher_signature_verified` | 对下载的 pack 先运行 `hub publisher-unpack`，再运行 `hub publisher-verify`（第 6 步） |
| 下载的应用包通过准入检查 | `downloaded_gate_output` | 同上 |
| 应用包摘要与 issue 中的一致 | `bundle_digest` | `integrity.bundle_blake3` |
| 每项申请的能力所属的服务族会在商店信息所列的平台上响应商店应用 | 0.1.0 记录中没有；从签名目录第 14 版起检查 | App Flow 的 [HOST-API-FAMILIES](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-FAMILIES.zh-CN.md) |

历史版本 0.1.1 的记录见 [`docs/admissions/connected-apps-0.1.1`](../docs/admissions/connected-apps-0.1.1/README.zh-CN.md)。这份记录还保存了每个应用签名后的准入检查输出和源码审核结论，并记录了一项测试：用商店的代码安装每个应用，并从 0.1.0 升级。

审核人员会对照应用包逐题核对你的扫描回答：商店信息中的声明、平台与类别、最小授权、欺骗性界面、写给 AI Agent 的指令性文字、辱骂性措辞，以及每个工具的范围和风险。审核人员不承诺审核时间。

准入并不证明应用能与真实的提供商正常配合。0.1.0 的记录写明了 `"live_provider_login_and_remote_effects_verified": false`，0.1.1 的记录也不证明原生界面、提供商流量或亲手点按批准。请在商店信息中说明你尚未验证的内容。

## 9. 提交之后

检查通过后，由 App Hub 管理员审核这次提交并批准；未经批准，Hub 不会发布任何内容。随后，管理员运行受保护的[签名目录工作流](GITHUB-PUBLISHING.zh-CN.md)：工作流准入审核过的原样字节，让 GitHub Actions 用 Sigstore 为新的 `catalog-v2.json` 签名，再把它 commit 到 `main`。之后，用户就能在读取 `catalog-v2.json` 并支持 `publisher-github-v1` 的 OctoSense 构建中搜索、安装和运行这个应用。例如 [RC4 预发布版](../README.zh-CN.md#下载兼容宿主)。历史 desktop-v0.1.0-beta.2 只读取 `hub publish` 生成的旧格式 `catalog.json`。

- 审核人员会在 issue 中列出需要修复的问题；应用进入签名目录后，审核人员关闭 issue，并注明应用所在的签名目录版本号。在 issue 中回答提问，但不要改动标签所指的内容。
- **首次发布之前，每个新 Release 都发在同一个 issue 中。** 要修复问题，就提高 `version` 并重做第 4 到 6 步（界面有变化时才需要第 4 步），然后在提交 issue 中发评论，写明新的标签、完整的 commit SHA 和工作流运行链接，并更新 issue 标题和 Version 字段（见[第 7 步](#7-开提交-issue)）。
- **发布之后，每个新版本都要开新 issue。** 提高 `version`，开一个新 issue，链接前一个 issue（新 issue 可以先于 Release 开），重做第 4 到 6 步，然后在新 issue 中补充这个 Release 的证据。Hub 从不替换已发布的版本。
- 维护者 commit 经过审核的撤回候选之后，App Hub 管理员可以通过同一个受保护的工作流，发布附带理由的签名目录撤回记录。各商店下次拉取签名目录时，会停止运行该版本已安装的副本；其他版本不受影响。如需撤回，请开一个 issue，写明应用 ID、版本和要向用户展示的理由。撤回的版本号不能再用，修复请以新版本发布。

## 常见拒绝原因及修复

`hub check` 每个检查结果输出一行：`[refused|warning] <check> (<file or property>): <detail>`。准入检查无法读取的应用包不会得到报告，只有一行 `hub: …`。完整规则见[发布参考](PUBLISHING.zh-CN.md#准入检查的规则)。

| 检查项或消息 | 原因 | 修复 |
| --- | --- | --- |
| `digest: the bundle hashes to …, the manifest claims …` | 写入摘要之后字节变了：有改动、在 commit 之后才写入摘要（`tools/octo check` 会重新写入）、Git 检出产生了 CRLF 换行，或者用了比 `main` 旧的 Windows 版 `hub`（它用 `\` 拼接路径来计算摘要）。 | 从 `main` 构建 `hub`，加上[第 1 步](#1-安排仓库结构)的 `.gitattributes`，然后为可编辑源码重新写入摘要并生成带 GitHub 证明的新版本，验证下载的 pack；不要给已封存的 Release 重新写入摘要。 |
| `continuity: existing legacy app ownership cannot be adopted by GitHub provenance` | 该 ID 已登记给未签名或用密钥签名的应用，例如照搬的 `org.octosense.samples.*` ID。 | 换一个新 ID。 |
| `continuity: GitHub publisher repository, owner or workflow changed` | 更新来自另一个仓库、所有者或工作流文件，或者仓库已重命名或转移。 | 用已登记的仓库和工作流生成 Release。重命名或转移后的仓库不能再更新这个应用；请换用新 ID 发布。 |
| `version: version … is already published; publish a new version` | 该版本已在签名目录中。 | 提高 `version`，使用新标签并验证工作流生成的新 Release pack（第 5、6 步）。 |
| `policy: app … requests unknown capability "<name>"` | 名称不在契约中（`contacts`、`model.image`，或 `octos.` 这样的前缀），或者你的 `hub` 太旧，还不认识这个能力。 | 使用[发布参考](PUBLISHING.zh-CN.md#清单)中的确切名称，并从 `main` 重新构建 `hub`。 |
| `hub: manifest is not valid: unknown field …`，没有报告 | `hub stamp` 和 `hub check` 无法解析清单。 | 删除该字段，或改用正确的字段名。消息中列出了合法字段。 |
| `contents: <file> has extension "…", which a bundle may not hold` | `.DS_Store`、`LICENSE`，或其他扩展名不在允许范围内的文件。 | 删除它，或移出 `bundle/`。 |
| `contents-invalid (<file>): cannot decode the image: …` | 图片损坏、其他格式改名成了 `.png`，或者单边超过 4096 像素。 | 重新截图或导出。 |
| `resource-invalid (…/font_src): not a portable bundle path: "makepad_widgets:resources/…"` | 引用了允许列表之外的内置字体。 | 使用 Inter、LXGW WenKai Regular 或 Bold 的准确资源名，或随包提供有许可的字体子集。完整名称和旧版宿主限制见[字体](PUBLISHING.zh-CN.md#字体)。 |
| `assets: <file> contains https://…` | 卡片数据含有外部 URL。脚本或 Agent 的 HTTPS 链接不会因目的地未披露而被拒绝。 | 随包提供卡片素材。准确披露脚本目的地；脚本和 Agent 指令仍拒绝 `http://`、`file://` 和 `../`。普通 `.txt`、`.md` 文档链接不会触发此检查，也不授予网络权限。 |
| `identity: … is under os.`，或 `identity: app id "…" ends in "…", which is reserved` | ID 以 `os.` 开头，或者 ID 本身或其最后一段是以下名称之一：`agents` `apphub` `appcard` `browser` `calculator` `card` `clock` `dev` `notes` `octos` `octoscode` `os` `reference` `reminders` `rinx` `sheets` `shell` `system` `task` `terminal` `toolbox` `weather` `workflow`。 | 在首次发布之前换一个 ID。 |
| `listing: listing has more than 10 keywords`、`… more than 8 screenshots` 或 `listing platform "…" is not one of […]` | 商店信息超出上限，或名称拼错。 | 精简列表，或使用消息中给出的名称。 |
| `listing: screenshots/01-main.png is named by the listing but is not in the bundle` | 文件不存在。 | 截图（[第 4 步](#4-截图)），或修正路径。 |
| `hub: the bundle exceeds the size limit`，没有报告（`--json` 显示 `bundle-invalid`） | 除 `manifest.json` 以外的文件超过 8 MiB（8,388,608 字节）。 | 压缩图片，对字体做子集化。 |
| `entry (main.splash): the bundle has no entry: …` | 应用包根目录下既没有 `main.splash`，也没有 `page.card`。 | 把入口文件放在 `bundle/` 的根目录。 |
| `secrets: <file> declares is_password:true: …` | 声明了密码或一次性验证码字段。 | 删除它。登录由宿主服务负责。 |
| `agent: AGENT.md is in the bundle but agent.instructions does not name it` | 有 Agent 文件，却没有对应的 `agent` 声明。 | 在 `agent` 中声明 `"instructions": "AGENT.md"`，或删除该文件。 |
| `card-host: refused: this host has no GitHub publisher verifier` | `card-host` 只运行未签名的应用包，从不运行已封存的 Release。 | 用可编辑源码运行和截图；Release 由你的 Release 工作流封存（[第 5 步](#5-生成最终字节)）。 |

## Hub 目前做不到的事

| 需求 | 现状 | 替代做法 |
| --- | --- | --- |
| 登录你自己的后端 | RC1 和 RC2 在[平台限制](#确认你的平台)内提供宿主运行的后端登录和清单声明的读取；兼容 Android 源码构建另有嵌入式流程。每次写操作都需要受支持的原生审阅流程（见发布参考的[登录自己的后端](PUBLISHING.zh-CN.md#登录自己的后端)一节）。 | 只需提供商身份时，用仅验证身份的登录来识别用户：`auth` 搭配 GitHub 的 `read:user`，或 Google 的 `openid`、`email` 和 `profile`。需要提供商数据时，再加上 `github`、`gmail` 或 `gcalendar`。 |
| 在应用中保存 API 密钥或令牌 | 不支持。准入检查只拒绝密码和一次性验证码字段，因此发现不了输入到普通字段或存放在存储中的密钥。 | 不要附带任何密钥。生成文本请用 `model`，它调用的是用户自己的 AI 提供商。 |
| 生成图片、音频、视频或向量嵌入 | RC1 和 RC2 在 `model` 能力下提供相应方法；媒体方法名不是独立能力。仍受提供商配置、权益和平台限制约束。 | 运行时发现方法，并处理提供商不可用的状态；见[媒体指南](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/AI-SERVICES.zh-CN.md#媒体与嵌入向量model)。 |
| 使用 `llm`、`news`、`calendar`、`matrix.*`、`prompt`、`ledger.read`、`clipboard`、`palpo.*` 或某个引擎服务 | 准入检查接受它们，但没有宿主向商店应用提供这些服务。`llm` 和 `news` 只响应 `os.*` 应用，`calendar` 只响应 `os.calendar`，`matrix.*` 只在 Rinx 内部由 Rinx 自己的宿主提供；引擎服务（`sheet`、`photo`、`word`、`deck`、`cad`、`light`、`sound`、`design`、`film`、`effect`、`vector`、`pdf`）可以在 App Hub `main` 上声明，但只向系统助手提供，rc.2 的准入检查也不认识它们。其余的没有任何宿主处理。 | 不要请求它们。访问 Google 日历请用 `gcalendar`。哪些服务族会响应商店应用、支持哪些平台、从哪个版本开始，见 App Flow 的 [HOST-API-FAMILIES](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-FAMILIES.zh-CN.md)。 |
| 在应用中发送邮件 | RC2 上，用户先用 `mail.add_account` 登录账户，然后 `mail.review_send`（或 `mail.send`，它打开同一个审阅界面）显示宿主的原生审阅界面，由用户亲手点按批准；`mail.compose` 和 `mail.compose_status` 用于保存草稿和查看状态。审阅界面只在 macOS 和 Android 上有；在 Windows 和 Linux 上会以 `Physical Mail send approval is unavailable on this platform` 失败。RC1 没有发送路径，真实的 SMTP 投递也尚未验证（[OctoSense #409](https://github.com/OctoSense-org/OctoSense/issues/409) 记录了错误提示的问题）。 | 商店信息列出 Windows 或 Linux 时，把 `mail.review_send` 放在 `host_api.optional` 下；在这两个系统上，读取已授权的账户（`mail.accounts`、`mail.folders`、`mail.list`、`mail.message`）并发出通知，发送由用户在 Mail 应用中完成。 |
| 用 Agent 工具运行应用自身的逻辑 | RC1 和 RC2 发行版会在完整应用打开期间运行 `implemented_by: "app"` 的工具；应用关闭时返回 `app_not_running`。OctoSense 桌面版 0.1.0-beta.2 拒绝这类工具，返回 `app_tool_unavailable`。没有 `host_method` 的 `host-service` 工具指向以应用的命名空间命名的服务。命名不会安装服务：调用仍需要真实注册的实现。 | 要调用共享服务，用 `host_method` 把工具映射到 `github`、`gcalendar`、`gmail` 或 `glance` 的某个方法（见发布参考的[把工具映射到共享服务](PUBLISHING.zh-CN.md#把工具映射到共享服务host_method)一节）。要运行应用自身的逻辑，在清单中声明 `requires: ["script-tools-v1"]`，并实现 `app_tool` 钩子（见发布参考的[脚本工具执行](PUBLISHING.zh-CN.md#脚本工具执行script-tools-v1)一节）。请在[兼容 RC2 发行版](../README.zh-CN.md#下载兼容宿主)中测试。 |
| 在应用中附带原生 Rust 代码 | 商店应用包不能携带原生代码。准入检查会拒绝原生库，原生代码需要随 Shell 新版本发布（[交付路径](DEVELOPMENT.zh-CN.md#选择合适的交付路径)）。 | 要做纯计算，把 Rust 代码编译成 `fns/` 中的 WebAssembly 模块，并请求 `wasm` 能力（见发布参考的[能力](PUBLISHING.zh-CN.md#能力)一节）。桌面 RC2 的标准构建在 macOS 和 Linux 上运行它；Windows、iOS 和 OpenHarmony 的构建不包含 `wasm` 服务，RC1 在所有平台上都默认关闭。具体做法，以及设备 API、网络和文件各走哪条路，见 App Flow 的[运行自己的 Rust 代码](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/RUST.zh-CN.md)。 |
| 提交系统应用（`os.*`）或原生应用 | 这里没有提交途径。系统应用随 Shell 一起发布，原生代码需要随 Shell 新版本发布（[交付路径](DEVELOPMENT.zh-CN.md#选择合适的交付路径)）。 | 用自己的 ID 做一个商店应用。 |
| 在手机上安装 `auth` 应用 | 目前没有任何已发布的手机版本能做到。 | 这些只声明 macOS 的示例使用 Mac 上的 RC2；Android Google 授权不可用。 |
| 在卡片 kit 中引用 Makepad 内置的 CJK 字体 | 当前 Hub 和锁定运行时支持 Regular 与 Bold 的准确资源名。 | 名称、打包字体及旧版宿主限制见[字体](PUBLISHING.zh-CN.md#字体)。Mac 上已验证 `card-host` 原生显示；尚未在每种 Shell 和平台上验证。 |

每个宿主 API 服务族由哪些 Shell 向谁提供、支持哪些平台、从哪个版本开始，见 App Flow 的 [HOST-API-FAMILIES.zh-CN.md](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-FAMILIES.zh-CN.md)；各项服务的说明见 [HOST-SERVICES.zh-CN.md](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.zh-CN.md)。
