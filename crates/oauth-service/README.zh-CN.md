# 联网账户与 App Hub 示例

[English](README.md) | 简体中文

OctoSense 为已安装应用保存服务商凭据。用户登录 GitHub 或 Google，批准该应用的权限后，
应用得到绑定自身身份的连接句柄。无需 OctoSense 账户或中心登录后台。
本服务实现 [ADR 0010](../../docs/adr/0010-shared-oauth-and-connected-apps.zh-CN.md) 的共享服务部分。

## 当前交付边界

Rust 授权协议、连接器、原生审批、账户生命周期和示例界面已实现。确定性的网络替身测试
及 macOS 隐藏窗口测试不能证明真实服务商操作。真实 GitHub/Google 登录、仓库写入、
Gmail 发信和 Calendar 写入均**未验证**。真实 DeepSeek peer 已通过已安装应用的准入工具
处理合成新邮件，并通过 Chat 修改持久化回复。Calendar peer 也通过自身工具读取
选中的合成日程，回答准确标题、时间和地点。这证明模型与工具集成，不代表 Google 投递。
三个普通示例尚未在 OnePlus 6 上测试。

| 平台 | 服务商授权 | 凭据保存 | Gmail 发信审批 |
| --- | --- | --- | --- |
| macOS | GitHub 设备授权；Google 浏览器/PKCE/回环回调 | 复用 Mail 的 Keychain 适配器，独立 OAuth 命名空间 | 原生鼠标来源校验；远程点击被拒绝，真人点击未验证 |
| Windows | 同样的桌面流程，平台运行未验证 | Windows Credential Manager；未在 Windows 验证 | 不支持，明确拒绝 |
| Linux | 已在 Linux 通过协议测试和主机编译；浏览器登录和 GUI 未验证 | 需要解锁 Secret Service，不回退到明文；原生测试被构建主机未解锁／不可用的凭据库拒绝 | 不支持，明确拒绝 |
| Android | GitHub 流程存在但未验证；**Google 原生适配器完成前拒绝连接** | Mail 的 Android 凭据库，独立命名空间 | 现有物理触摸来源校验；本示例未验证 |

本变更不会删除或迁移内置 Mail、Calendar 应用。

## 用户登录

由发行方配置好的版本会自带 OctoSense 的提供方注册信息。在应用中选择
**Connect GitHub** 或 **Connect Google**，审阅访问权限，再到浏览器完成登录。
用户不需要开发者账户、Google Cloud 项目或 JSON 配置文件。个人令牌仍保存在
宿主的平台凭据库中，并绑定到发起请求的应用。

如果当前构建没有相应注册信息，登录面板会说明该版本暂不支持登录，并建议联系
发行方或更新版本。新增解析器不会自动向提供方注册 OctoSense；维护者提供注册
信息并完成验证后，发行版本才具备登录条件。现有 beta.2 下载包不含注册默认值。

## 身份、提供方数据与应用自己的后端

三者是独立选择，均不要求用户拥有 OctoSense 账户。

| 用途 | 当前接口约定 |
| --- | --- |
| 在应用内识别 GitHub 用户 | 授予 `auth` 并请求 `read:user`。宿主验证 GitHub 数字用户 ID 和登录名，返回绑定该应用的句柄，以及 `app_id`、`provider`、`subject`、`label`、`scopes` 和可选 `expires_at`。不需要仓库访问权限；也不提供已验证的邮箱地址。 |
| 在应用内识别 Google 用户 | `auth` 也允许仅用于身份的 `openid`、`email`、`profile` 权限，无需 Gmail 或 Calendar 能力。宿主验证提供方的 subject，并仅在 Google 确认邮箱已验证时将邮箱作为标签。同样受平台授权支持范围限制。 |
| 访问提供方数据 | GitHub 仓库另外需要 `github` 能力及仓库权限。Google Gmail、Calendar 分别需要 `gmail` / `gcalendar` 能力和相应权限，与应用选择哪种登录身份无关。 |
| 注册或登录应用自己的后端 | 可复用的宿主管理后端登录／会话服务仍是**提议，尚未实现**。本地连接句柄或返回的资料不是后端可验证的 SSO 凭证。 |

拟议的后端流程由开发者的 HTTPS 登录页面提供 GitHub 登录，或自己的注册和登录。
后端负责验证身份并签发自身会话，宿主再为该应用保存独立的后端会话。共享连接器的
GitHub 或 Google 令牌不会导出给应用后端。开发者后端可以通过自身 OAuth 流程，
取得用户另行授权的 GitHub 令牌。现有网络访问能力不会让本地 GitHub 资料变成
远程后端可信的身份证明。应用自身不得收集密码或提供方秘密凭据。

如果身份提供方允许嵌入，后端自己的登录页面可以使用专用的宿主认证 WebView。
该适配器**尚未实现**：现有 Makepad 阅读器 WebView 缺少这里需要的回调拦截和
会话隔离接口。Google 授权使用受支持的浏览器流程，后端页面上的 Google 登录
按钮也须遵守这一要求。后端登录同样可以通过浏览器完成，不要求嵌入 WebView。

## 配置发行版本（维护者）

由发行方以自己的身份注册一次 OctoSense：创建并启用设备授权的 GitHub OAuth
应用；为 Google 桌面创建 Desktop 应用，启用示例使用的 Gmail/Calendar API，
并配置同意页面。面向公众使用敏感或受限权限时，需要完成相应 Google 验证；
测试用户可授权处于测试阶段的注册。终端用户不需要重复这些步骤。参阅
[GitHub 官方说明](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
和 [Google 原生应用说明](https://developers.google.com/identity/protocols/oauth2/native-app)。

在 Cargo 编译宿主时提供下列环境变量，包括桌面打包工具调用 Cargo 的情况。
这些变量不是运行时覆盖项；打包工具的跳过构建选项不能把它们加入已有二进制。

| 构建变量 | 原生应用注册值 |
| --- | --- |
| `OCTOSENSE_GITHUB_CLIENT_ID` | OctoSense 的 GitHub OAuth 客户端 ID；不使用 GitHub 客户端密钥 |
| `OCTOSENSE_GOOGLE_DESKTOP_CLIENT_ID` | OctoSense 的 Google Desktop 客户端 ID |
| `OCTOSENSE_GOOGLE_DESKTOP_REGISTRATION_VALUE` | 可选 Desktop 注册值；该原生客户端需要时作为 Google 的 `client_secret` 发送 |

这些值会随宿主可执行文件分发，无法在其中保密。它们用于标识发行方的原生应用，
不是用户密码、访问或刷新令牌、签名私钥，也不是机密 Web 客户端密钥。不要把
这些私人凭据放进构建变量或应用包。真实注册值保留在源码提交之外，只使用
发行方自己拥有的注册；不要将 TV/设备或 Web 客户端用于 Google 桌面授权。

同一解析器为授权和连接器的令牌刷新提供注册信息。测试使用虚构注册，不证明
真实登录成功。分发前须用真实账户验证同意、刷新、取消和撤销流程。Google 登录
通过系统浏览器、PKCE 和回环回调完成，嵌入式 WebView 不能替代受支持的授权。
Google Android 仍需要原生适配器。

## 高级运维覆盖配置

可选的 `<apps root>/.host/oauth/clients.json` 会替换整套构建注册信息。省略的
提供方会被禁用；`{}` 禁用两者。文件格式错误、过大或无法读取时，登录失败，
不会悄悄改用另一注册。仅在文件不存在时使用构建默认值。该运维文件须放在应用包
和源码管理之外。以下为占位示例（真实注册仍**未验证**）：

```json
{
  "github": { "client_id": "REGISTERED_GITHUB_CLIENT_ID" },
  "google": {
    "client_id": "REGISTERED_GOOGLE_DESKTOP_CLIENT_ID",
    "client_secret": "NATIVE_DESKTOP_REGISTRATION_VALUE_IF_REQUIRED"
  }
}
```

改变客户端注册不会迁移已有提供方令牌；受影响的账户需要使用预期注册重新连接。
安装型应用的注册值不能替代 PKCE 或应用所有权。

宿主先展示申请应用和权限，再进入服务商授权。应用不能指定端点、回调地址或 client secret。
GitHub 设备代码仅出现在宿主面板。Google 校验 state、来源、路径、有效期和单次使用。
服务商错误经过净化；token 不会进入应用响应或连接元数据 JSON。

## 应用接口

在 manifest 中只声明所需服务。`auth` 本身不授予 Gmail/GitHub/Calendar 数据权限。
必须设置 `storage.accounts: true`，使应用 peer 和账户目录跟随选中的连接。

| 服务 | 方法 |
| --- | --- |
| `auth` | `connect`、`accounts`、`active`、`select`、`disconnect` |
| `github` | `repositories`、`files`、`read`、`review_save` |
| `gcalendar` | `calendars`、`sync`、`cached`、`refresh`、`get`、`prepare`、`review_save` |
| `gmail` | `labels`、`messages`、`message`、`draft.open/get/edit/review`、`events.status`、`event.status/decide` |

`auth.connect` 接收 provider 和 scopes。GitHub scopes 为 `read:user`、`public_repo` 或 `repo`；
Google 为 `openid`、`email`、`profile`、`calendar.list`、`calendar.events`、`mail.read`、`mail.send`。
这些服务商权限与 App Hub capability 各自校验。句柄不是 token；选中一个 Google 账户也不会
自动让其他应用读取它。调用示例见英文版，对真实授权的验证状态相同。

GitHub 保存冻结仓库、分支、路径、内容及原 blob SHA。Calendar 保存冻结日历、事件和 ETag；
过期 ETag 会报冲突，不会静默覆盖。Gmail 原生审核冻结持久化草稿版本、收件人及正文，
必须通过真人激活原生控件发信；脚本、Agent、远程测试及 JSON 标记不能批准。
结果不明的 Gmail 提交保持不明状态，不会盲目重试。

## Agent 如何调用共享服务

普通应用声明自己的工具名，例如 `inbox.message`，并在 `tools.json` 显式映射
`host_method: "gmail.message"`。App Hub 只接纳经过审查的方法，并校验最低风险等级、
私有数据标记和服务能力。凭据管理、审批和远程写入不开放为工具别名。

Shell 从摘要校验后的包读取声明，通过 `HostServiceExecutor` 路由；检查目标服务，注入
工具所属应用当前连接，拒绝过期 peer 或模型选择的其他账户。服务再次校验应用、服务商
及 scope。跨应用访问仍须工具所有者声明 shareable、调用方获得授权；三个示例默认不共享
私有读取工具。

## 新邮件与 Glance

桌面脚本卡片先显示标题和摘要，打开模板卡片后提供有界应用视口，让编辑器及滚动
区域获得实际高度。模板工作区自己提供 Email／Reply／Chat 导航，宿主不重复添加
Chat 标签。原有未选择视口模式的脚本卡片继续按内容测量并由外层滚动。
前台发布的卡片可以在未请求代理同意前恢复；撤销 Glance 权限、明确拒绝代理、
退出账户和切换账户仍阻止恢复。

`connected_events.rs` 发现声明 Gmail/auth、已获 Agent 同意、允许后台且声明
`<应用短名>.new_message` 的已安装应用。采集器先建立只面向未来的 Gmail history 基线，
允许运行时通常每五分钟轮询。登录并允许应用 Agent 后刷新，等 `gmail.events.status`
显示 `baseline_ready: true` **再发测试邮件**。历史收件箱不会一次性变成通知。
history 失效时使用有边界的恢复扫描。

新事件进入该账户的 peer，携带接纳的 AGENT.md/技能及“不可信邮件数据”边界。模型读取邮件，
决定静默或重要。重要邮件可选用包内的 `glance-workspace.splash` 模板并提供消息数据；
宿主注入当前连接并保留展开后的源码，模型无需重写 Reply/Chat 编辑器。

只有回合成功，且存在持久化静默决定或经过校验的持久化卡片，事件才确认完成。
失败保留重试并向调度器返回错误，等待 60 秒后重试，不会被误判为成功后每两秒继续
处理的队列。Chat 与手工编辑共用带版本的草稿；成功发信后撤下卡片。通知不等于发信
或创建日历事件的授权。

Android 现有 JobScheduler 适配器也会在有时限的任务中驱动该采集器。新任务会强制轮询一次，
不会被前台五分钟间隔挡住；内置 Mail 与新采集器都完成才结束。Android 可能延迟安静后台任务。
新的 Java 代码已编译，但这些示例的自然调度、冷启动通知送达仍**未验证**。

## 源码与验证

阅读顺序：`providers.rs` → `oauth.rs`/`authorize.rs` → `protocol.rs` → `store.rs` → `host.rs`。
`protocol.rs` 将 `oauth2` 5 接入宿主限制大小、固定来源的网络传输；库负责构造
授权和令牌请求、解析协议响应。调用者身份、取消、回调校验、权限准入和凭据保存
仍由宿主管理。GitHub 设备轮询每次只发一个请求，以便每次重新检查所属应用、
有效期和取消状态；库内置的轮询循环不能替代这些生命周期检查。
`api.rs` 处理服务商请求；`calendar_cache.rs` 原子提交分页快照；`inbox.rs` 持有草稿/审核/发送状态；
`inbox_events.rs` 持有游标、租约和决定。Shell 管理获准 peer、原生审核和 Glance。
peer 是应用账户身份，不等于一个工作线程或 Tokio task。

以下命令已从 OctoSense 根目录运行：

```sh
cargo test --locked -p octosense-oauth-service
cargo check --locked -p octosense-oauth-service --features host
```

另用独立临时配置和虚构凭据实际测试了 macOS 系统凭据适配器：写入、重新打开读取、
逻辑撤销均通过；配置目录中没有出现明文访问或刷新凭据。下面的显式测试使用真实
系统凭据库，可能需要已解锁的桌面会话，普通测试运行会跳过它：

```sh
cargo test --locked -p octosense-oauth-service --features host host_vault_acceptance::platform_vault_persists_across_reopen_without_plaintext_credentials -- --ignored --exact
```

这不验证服务商授权或实体发送审批，也不能证明旧 Mail 凭据适配器中无返回值的删除
操作实际删掉了系统条目。测试不请求服务商，也不读取已有账户。

测试使用确定性传输及虚构账户，覆盖隔离、撤销、回调重放、刷新、冲突、分页、410、ETag、
DST、草稿版本、注入审批拒绝、发送不明、事件重试和持久化决定。示例原生测试证据与编写说明见
[Design Flow connected-apps](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/tree/feat/connected-sample-apps/examples/connected-apps)。
普通 `card-host` 不提供 OAuth、Gmail、Calendar 或 octos 宿主。`connected-app-host` 是独立的
私有配置测试宿主；不启动 Agent 内核，也不能代替生产安装验证。
