# 已连接账户与 App Hub 示例

[English](README.md) | 简体中文

OctoSense 为已安装应用保存服务商凭据。用户登录 GitHub 或 Google，批准该应用的权限后，
应用得到绑定自身身份的连接句柄。无需 OctoSense 账户或中心登录后台。
本服务实现 [ADR 0010](../../docs/adr/0010-shared-oauth-and-connected-apps.zh-CN.md) 的共享服务部分。

## 当前交付边界

Rust 授权协议、连接器、原生审批、账户生命周期和示例界面已实现。macOS 原生宿主与
提供方浏览器流程已通过真实的 GitHub 和 Google 身份登录。GitHub 仅请求 `read:user`；
Google 使用专用测试账户，仅请求 `openid email profile`。合成后端使用真实平台凭据库，
已通过浏览器注册/登录、受保护身份、刷新故障恢复、原生进程重启、退出及两个已安装
应用之间的隔离。原生后端 WebView 在 macOS 上另通过八项验收，包括真实表单输入、
取消、重试、应用/会话隔离及进程重启；同一最终二进制的桌面浏览器回归通过七项检查。
另一个独立 OnePlus 6 后端测试应用完成真实表单登录、Glance 交接/取消、受保护身份、
原生凭据库冷启动恢复及退出。[Android 记录](../../tools/connected-e2e/evidence/backend-android-20261007/README.zh-CN.md)
明确列出视觉证据限制和未执行项目，不代表完整手机 UX 验收。
仓库写入和 Gmail 发信仍**未验证**；仅身份登录不会
授予或证明这些业务操作。后续 Mac 会话中，将专用测试账户加入 OAuth 项目的测试用户
名单后，已安装的签名 Calendar 应用完成了真实 Google 授权。用户确认日历列表已显示，
通过应用的审阅流程保存了测试日程，并在 Refresh 后再次看到它。这是用户手工验证，
没有独立 API 回读；编辑/删除及 Google 生产审核仍未验证。[脱敏记录](../../tools/connected-e2e/evidence/calendar-login-20261007.json)区分了这些观察。
真实 DeepSeek peer 已通过已安装应用的准入工具
处理合成新邮件，并通过 Chat 修改持久化回复。Calendar peer 也通过自身工具读取
选中的合成日程，回答准确标题、时间和地点。这证明模型与工具集成，不代表 Google 投递。
三个示例中，只有 GitHub Notes 在 OnePlus 6 上检查过：在一个独立的测试 APK 中检查了它的本地编辑，常用的 Home 保持不变
（[OnePlus Notes 检查记录](../../tools/connected-e2e/evidence/notes-oneplus-20261006/README.zh-CN.md)）；
Inbox Assistant 和 Google Calendar 尚未在该设备上运行。

| 平台 | 服务商授权 | 凭据保存 | Gmail 发信审批 |
| --- | --- | --- | --- |
| macOS | GitHub 设备授权；Google 浏览器/PKCE/回环回调 | 复用 Mail 的 Keychain 适配器，独立 OAuth 命名空间 | 原生鼠标来源校验；远程点击被拒绝，亲手点按未验证 |
| Windows | 同样的桌面流程，平台运行未验证 | Windows Credential Manager；未在 Windows 验证 | 不支持，明确拒绝 |
| Linux | 已在 Linux 通过协议测试和主机编译；浏览器登录和 GUI 未验证 | 需要解锁 Secret Service，不回退到明文；原生测试被构建主机未解锁/不可用的凭据库拒绝 | 不支持，明确拒绝 |
| Android | GitHub 流程存在但未验证；**Google 原生适配器完成前拒绝连接** | Mail 的 Android 凭据库，独立命名空间 | 现有的亲手点按来源校验；本示例未验证 |

`desktop-v0.1.0-beta.2` 是第一个包含已连接账户服务（`auth`、`github`、`gmail`、`gcalendar`）的发布版本；
Home（手机）还没有能安装连接账户应用的发布版本。beta.2 的 `auth` 没有后端登录，服务商注册也只来自 `clients.json`
（见[高级运维覆盖配置](#高级运维覆盖配置)）。beta.2 也早于 [#356](https://github.com/OctoSense-org/OctoSense/pull/356)：它已合入 `main`，但还没有进入任何发布版本。
所以在 beta.2 上，只有 Gmail 发信检查是否亲手点按，GitHub 和 Calendar 保存使用的宿主面板不做这项检查；Agent 的 `glance.publish` 仍接受 `script` 卡片；
Calendar 用 `gcalendar.sync` 和同步 token 同步全部日程历史，而不是下文的有限日期窗口。更早的 `desktop-v0.1.0-beta.1` 和 `home-v0.1.0-beta.1`
使用应用契约 1.1.0，这一版没有 `auth` 能力：它们的商店会列出声明了 `auth` 的应用，但拒绝安装。
这些服务不取代也不迁移内置 Mail、Calendar 应用。

## 用户登录

由发行方配置好的版本会自带 OctoSense 的提供方注册信息。在应用中选择
**Connect GitHub** 或 **Connect Google**，审阅访问权限，再到浏览器完成登录。
用户不需要开发者账户、Google Cloud 项目或 JSON 配置文件。个人令牌仍保存在
宿主的平台凭据库中，并绑定到发起请求的应用。

如果当前构建没有相应注册信息，登录面板会说明该版本暂不支持登录，并建议联系
发行方或更新版本。新增解析器不会自动向提供方注册 OctoSense；维护者提供注册
信息并完成验证后，发行版本才具备登录条件。现有 beta.2 下载包不含注册默认值。

如果 Google 显示 **403: access_denied**，并说明仅允许开发者批准的测试用户访问，
则说明已找到提供方注册信息，但当前账户未列入该 OAuth 项目的测试用户名单。
维护者在 **Google Auth Platform → Audience → Test users** 中加入专用测试账户，
再重新发起登录。这只能解除测试限制，不代表应用已通过公开分发审核。
隔离验收不使用个人账户。

## 身份、提供方数据与应用自己的后端

三者是独立选择，均不要求用户拥有 OctoSense 账户。

| 用途 | 当前接口约定 |
| --- | --- |
| 在应用内识别 GitHub 用户 | 授予 `auth` 并请求 `read:user`。宿主验证 GitHub 数字用户 ID 和登录名，返回绑定该应用的句柄，以及 `app_id`、`provider`、`subject`、`label`、`scopes` 和可选 `expires_at`。不需要仓库访问权限；也不提供已验证的邮箱地址。 |
| 在应用内识别 Google 用户 | `auth` 也允许仅用于身份的 `openid`、`email`、`profile` 权限，无需 Gmail 或 Calendar 能力。宿主验证提供方的 subject，并仅在 Google 确认邮箱已验证时将邮箱作为标签。同样受平台授权支持范围限制。 |
| 访问提供方数据 | GitHub 仓库另外需要 `github` 能力及仓库权限。Google Gmail、Calendar 分别需要 `gmail` / `gcalendar` 能力和相应权限，与应用选择哪种登录身份无关。 |
| 注册或登录应用自己的后端 | macOS/Android 使用宿主持有的登录 WebView、绑定应用的注册信息、PKCE 代码交换及受保护身份端点。桌面仍可选择外部浏览器；应用包自行注册尚未实现。 |

后端流程由开发者的 HTTPS 登录页面提供自己的注册和登录。若桌面后端页面也提供
GitHub 登录，应使用外部浏览器模式，以便访问提供方来源。
后端负责验证身份并签发自身会话，宿主为该应用保存独立的后端会话。共享连接器的
GitHub 或 Google 令牌不会导出给应用后端。开发者后端可以通过自身 OAuth 流程，
取得用户另行授权的 GitHub 令牌。现有网络访问能力不会让本地 GitHub 资料变成
远程后端可信的身份证明。应用自身不得收集密码或提供方秘密凭据。

后端登录复用原生 WebView 引擎的专用认证模式。macOS 每次登录使用独立、非持久化
WKWebView 存储；Android 9+ 使用不可导出的独立进程 Activity 及唯一 WebView 数据
目录，进程退出后删除该目录，不影响阅读器现有 Cookie。导航限制在已注册登录来源，
加载前拦截精确回调，页面没有调用应用工具的 JavaScript 桥。返回、取消、加载与
重试控件由宿主管理；受限应用不能直接打开认证模式或检查其页面。

GitHub 和 Google 保留现有提供方授权流程。需要访问其他提供方来源的后端登录应
选择桌面浏览器模式；嵌入模式不会悄悄打开外站。Windows/Linux 保留桌面浏览器登录，
iOS 后端登录仍不可用。Linux/Windows 的普通 `WebReader.open` 也会明确拒绝，
因为当前固定版本没有嵌入式 SystemBrowser 适配器。另一个可选 CEF Browser 控件
不能直接替代认证适配器：它目前使用全局持久 profile，并关闭 Chromium 沙箱，
不具备这里要求的每应用会话和导航边界。嵌入支持仍需隔离适配器及原生系统验收，
外部浏览器不能计作嵌入支持。

## 开发者后端接口约定

声明 `auth` 和 `storage.accounts: true`。调用 `auth.connect` 时传入
`{"provider":"backend","scopes":["app.session"]}`，并复用普通的
`auth.accounts`、`auth.active`、`auth.select`、`auth.disconnect` 生命周期。
macOS/Android 默认使用嵌入页面。传入 `"presentation":"webview"` 可要求该模式，
`"presentation":"browser"` 选择桌面外部浏览器；不支持的组合会明确拒绝。
`auth.backend.me` 接收本应用当前选中的 `connection` 句柄，返回后端验证的
`{"connection":"…","backend_id":"…","identity":{"sub":"…","label":"…"}}`，
其中包含后端验证的身份。应用还可声明受限业务操作，通过 `auth.backend.request`
调用。宿主附加 bearer 凭据，应用只收到 JSON 结果；调用者不能选择 URL、请求方法、
请求头或声明范围以外的路径，因此这不是开放的 HTTP 代理。

shell 通过 `host::set_backend_resolver` 提供经过摘要验证的 manifest `backend`
声明，每次使用凭据时重新检查；解析错误直接拒绝，不回退到运维配置。其 JSON 格式
与下面注册对象相同，但不含 `app_id`（身份只能来自已接纳应用包）。manifest 需要
`backend-api-v1`、`auth` 和 `storage.accounts: true`。没有应用包声明时仍支持原有
运维注册。声明变更或移除时，安装/更新方须调用
`host::invalidate_backend_registration`，持久撤销已有后端句柄，防止回退旧版本恢复
会话。请求发送前及接收结果后还会检查注册绑定与授权代次。

对于运维管理的集成，运维者在应用包和源码管理之外配置 `<apps root>/.host/oauth/backends.json`。
以下仅为配置示例，示例域名不提供实际服务：

```json
{
  "schema": 1,
  "apps": {
    "com.example.notes": {
      "id": "notes-backend",
      "app_id": "com.example.notes",
      "client_id": "registered-public-native-client",
      "authorization_url": "https://login.example.test/authorize",
      "token_url": "https://login.example.test/token",
      "me_url": "https://login.example.test/me",
      "logout_url": "https://login.example.test/logout",
      "scopes": ["app.session"],
      "operations": {
        "notes.list": {"method":"GET", "path":"/api/notes", "query_keys":["tag"]},
        "notes.create": {"method":"POST", "path":"/api/notes"}
      }
    }
  }
}
```

将以下对象作为 `auth.backend.request` 的参数，使用当前已选连接调用声明的操作：

```json
{"connection":"opaque-host-handle","operation":"notes.list","query":{"tag":"work"}}
```

```json
{"connection":"opaque-host-handle","operation":"notes.create","body":{"text":"A fictional note"}}
```

GET 在工作线程执行；POST/PUT/PATCH/DELETE 复用 GitHub/Calendar 的原生不可变
审核界面，须通过真实输入确认。脚本和 agent 不能通过 `auth.backend.sheet.save`
批准发送。后台写操作要求先打开应用。批准前取消或超时不会发送；已批准的 HTTP
请求一旦开始，取消不能承诺撤销服务端结果，错误也不会自动重试。

操作限定为登录来源下的精确 ASCII 路径，最多声明 32 个查询键，不支持路径模板或
认证端点别名。请求与响应 JSON 上限为 64 KiB，网络超时为 30 秒，拒绝重定向。
查询键必须预先声明，查询值由宿主编码。连接始终绑定所属应用和当前账号；切换、
退出、撤回应用或撤销授权会使等待中的工作失效。每应用串行执行防止账号变更穿过
远程写入，同时网络期间不持有全局凭据元数据锁。后端仍负责自己的用户授权和幂等。

后端实现公开客户端授权码流程：S256 PKCE、原样返回 state 及单次代码。嵌入登录
只允许精确回调 `https://octosense.invalid/auth/callback`，由宿主拦截，不访问网络。
桌面外部浏览器模式使用宿主的临时回环回调。令牌端点支持授权码交换及刷新，返回 OAuth bearer 令牌。
`GET /me` 返回 `{sub,label}`；`POST /logout` 撤销会话并确认
`{"logged_out":true}`。注册与密码输入均由宿主展示的后端网页处理，不进入受限应用。

各端点须为相同来源、443 端口下互不相同的精确 HTTPS URL；拒绝查询参数、片段、
URL 凭据及 HTTP 重定向。宿主把保存的连接绑定到规范化注册，修改注册后必须
重新连接，不能把旧令牌发送到新端点。退出先撤销本地句柄，再尝试远程退出，
并单独报告远程结果。嵌入回调仍检查调用者、state、有效期及单次使用。
Android 9 以下与 iOS 拒绝嵌入后端登录；Windows/Linux 的运行尚未验证。

合成后端使用真实浏览器表单、HTTP 代码交换及受保护请求。HTTP 回环仅在非默认
验收构建中通过显式隔离注册开放，不是发行版本的配置开关。参阅
[浏览器验收驱动](../../tools/connected-e2e/backend-login/README.zh-CN.md)和[原生 WebView 验收](../../tools/connected-e2e/backend-webview.zh-CN.md)。

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
和源码管理之外。以下为占位示例，需替换为发行方已注册的原生客户端信息：

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
| `auth` | `connect`、`accounts`、`active`、`select`、`disconnect`、`backend.me`、`backend.request` |
| `github` | `repositories`、`files`、`read`、`review_save` |
| `gcalendar` | `calendars`、`cached`、`refresh`、`get`、`prepare`、`review_save` |
| `gmail` | `labels`、`messages`、`message`、`draft.open/get/edit/review`、`events.status`、`event.status/decide` |

`auth.connect` 接收 provider 和 scopes。GitHub scopes 为 `read:user`、`public_repo` 或 `repo`；
Google 为 `openid`、`email`、`profile`、`calendar.list`、`calendar.events`、`mail.read`、`mail.send`；
后端登录使用 `app.session`。
这些服务商权限与 App Hub capability 各自校验。句柄不是 token；选中一个 Google 账户也不会
自动让其他应用读取它。调用示例见英文版；专用测试账户的 Calendar 授权已验证，
面向公众的 Google 生产审批仍未验证。

GitHub 保存冻结仓库、分支、路径、内容及原 blob SHA。Calendar 保存冻结日历、事件和 ETag；
过期 ETag 会报冲突，不会静默覆盖。Gmail 原生审阅界面冻结持久化草稿版本、收件人及正文。
这三种写入都必须由用户亲手点按宿主原生审阅界面上的控件；按下与释放时分别检查原生输入来源，
然后才把一次性能力交给工作线程。脚本、Agent、远程测试及 JSON 标记不能批准保存或发送。
关闭审阅界面会取消尚未提交的请求；写入前再次检查当前账户。结果不明的 Gmail 提交保持
不明状态，不会盲目重试。

`gcalendar.refresh` 原子替换有限日期范围内的日程：从今天之前 30 天的 UTC 零点，
到今天之后 366 天的 UTC 零点。Google 将重复系列展开为窗口内的真实实例，保留实例 ID、
ETag 和例外，排除取消的实例。同一次刷新所有分页使用相同范围；以后刷新会移动窗口。
失败或未完成的刷新保留上一次完整缓存及其 `window`。旧 schema-1 缓存仍可读取，
直到一次成功的有限窗口刷新替换它。

这个日程使用完整的**窗口快照**，不使用增量历史同步。Google 禁止将 `timeMin`/`timeMax`
与 `syncToken` 一起使用，所以该路径不保存或复用 `nextSyncToken`。不再开放原始
`gcalendar.sync`，请使用 `refresh` 和 `cached`。见 [Google events.list 契约](https://developers.google.com/workspace/calendar/api/v3/reference/events/list)。

提供方 HTTP 与本地草稿、缓存修改按宿主配置目录和应用分别串行执行；一个提供方响应
缓慢不会阻塞其他应用。选中账户、断开连接、卸载和最终接纳连接共享同一应用锁；
进程级元数据锁只覆盖短暂的读取、提交步骤。刷新令牌提交前重新读取最新元数据，
不会覆盖其他应用的账户修改，也不会恢复已经撤销的连接。

## Agent 如何调用共享服务

普通应用声明自己的工具名，例如 `inbox.message`，并在 `tools.json` 显式映射
`host_method: "gmail.message"`。App Hub 只准入经过审查的方法，并校验最低风险等级、
私有数据标记和服务能力。凭据管理、审批和远程写入不开放为工具别名。

Shell 从摘要校验后的包读取声明，通过 `HostServiceExecutor` 路由；检查目标服务，注入
工具所属应用当前连接，拒绝过期 peer 或模型选择的其他账户。服务再次校验应用、服务商
及 scope。跨应用访问仍须工具所有者声明 shareable、调用方获得授权；三个示例默认不共享
私有读取工具。

## 新邮件与 Glance

桌面脚本卡片先显示标题和摘要，打开模板卡片后提供有界应用视口，让编辑器及滚动
区域获得实际高度。模板工作区自己提供 Email/Reply/Chat 导航，宿主不重复添加
Chat 标签。原有未选择视口模式的脚本卡片继续按内容测量并由外层滚动。
前台发布的卡片可以在未请求代理同意前恢复；撤销 Glance 权限、明确拒绝代理、
退出账户和切换账户仍阻止恢复。

`connected_events.rs` 发现声明 Gmail/auth、已获 Agent 同意、允许后台且声明
`<应用短名>.new_message`（应用短名即应用 id 的最后一段）的已安装应用。采集器先建立只面向未来的 Gmail history 基线，
允许运行时通常每五分钟轮询。登录并允许应用 Agent 后刷新，等 `gmail.events.status`
显示 `baseline_ready: true` **再发测试邮件**。历史收件箱不会一次性变成通知。
history 失效时使用有边界的恢复扫描。

新事件进入该账户的 peer，携带已准入的 AGENT.md/技能及“不可信邮件数据”边界。模型读取邮件，
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
开发者后端的阅读顺序是 `backend.rs`（注册校验、PKCE 和有界 HTTP 请求）→
`host_backend.rs`（同意面板、回调、刷新和退出）→ `store.rs`（应用归属与注册绑定）。
Google 令牌响应仅对其文档规定的两种身份权限 URI 别名做规范化；缺少权限仍会拒绝授权。
`api.rs` 处理服务商请求；`calendar_cache.rs` 原子提交分页快照；`inbox.rs` 持有草稿/审阅/发送状态；
`inbox_events.rs` 持有游标、租约和决定。Shell 管理获准 peer、原生审阅界面和 Glance。
peer 是应用账户身份，不等于一个工作线程或 Tokio task。

以下命令已从 OctoSense 根目录运行：

```sh
cargo test --locked -p octosense-oauth-service
cargo check --locked -p octosense-oauth-service --features host
cargo test --offline --locked -p octosense-oauth-service --features host,acceptance-fixtures --lib
```

在 [#353](https://github.com/OctoSense-org/OctoSense/pull/353) 时，最后一条命令通过 75 项测试，跳过一项需要显式运行的平台凭据库测试。
#356 新增了测试，`main` 上的测试数量尚未记录。独立原生后端
验收实际使用了平台凭据库，并覆盖进程冷重启。真实提供方验收覆盖身份登录、重启后
连接元数据恢复及本地断开，不含提供方令牌刷新或远程撤销。注册信息、账户详情及
原始证据均保留在仓库之外。
这些 macOS 结果不代表 Windows、Linux 或手机登录已通过验证。
[脱敏提供方验收记录](../../tools/connected-e2e/evidence/provider-login-20261007.json)记录了确切权限、原生二进制及验证限制。

另用独立临时配置和虚构凭据实际测试了 macOS 系统凭据适配器：写入、重新打开读取、
逻辑撤销均通过；配置目录中没有出现明文访问或刷新凭据。下面的显式测试使用真实
系统凭据库，可能需要已解锁的桌面会话，普通测试运行会跳过它：

```sh
cargo test --locked -p octosense-oauth-service --features host host_vault_acceptance::platform_vault_persists_across_reopen_without_plaintext_credentials -- --ignored --exact
```

这不验证服务商授权或亲手点按的发送审批，也不能证明旧 Mail 凭据适配器中无返回值的删除
操作实际删掉了系统条目。测试不请求服务商，也不读取已有账户。

测试使用确定性传输及虚构账户，覆盖隔离、撤销、回调重放、刷新、冲突、有限窗口分页、窗口移动、重复实例、ETag、
DST、跨应用可用性、刷新提交竞态、草稿版本、注入审批拒绝、发送不明、事件重试和持久化决定。示例原生测试证据与编写说明见
[Design Flow connected-apps](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/tree/main/examples/connected-apps)。
普通 `card-host` 不提供 OAuth、Gmail、Calendar 或 octos 宿主。`connected-app-host` 是独立的
私有配置测试宿主；不启动 Agent 内核，也不能代替生产安装验证。

### 后端业务请求验证

本次在 macOS 执行：

```sh
cargo test --locked -p octosense-oauth-service --features host
cargo test --locked -p octosense-oauth-service --features acceptance-fixtures backend
```

合成服务通过真实本地 HTTP 验证注册/登录、创建/读取笔记、账号隔离、退出、拒绝
重定向、负载上限及意外凭据回显。宿主测试覆盖当前句柄/权限失效、持久撤销注册、
后台写入拒绝和原生审核取消。测试采用隔离的内存凭据库，是协议与宿主的合成证据，
不代表真实物理批准、已安装应用的渲染流程、线上 provider 或 Android/Windows/Linux
验收。系统凭据库测试仍需显式运行。
