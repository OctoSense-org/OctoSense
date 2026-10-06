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

## 宿主配置

宿主读取 `<apps root>/.host/oauth/clients.json`，该文件必须位于应用包和源码管理之外。
以下仅为占位符：

```json
{
  "github": { "client_id": "REGISTERED_GITHUB_CLIENT_ID" },
  "google": {
    "client_id": "REGISTERED_GOOGLE_DESKTOP_CLIENT_ID",
    "client_secret": "GOOGLE_DESKTOP_REGISTRATION_VALUE_IF_REQUIRED"
  }
}
```

注册启用设备授权的 GitHub OAuth 应用。Google 桌面需注册 Desktop 应用，启用示例所需的
Gmail/Calendar API，并配置授权同意页和测试用户。注册及真实登录**未在替身测试中执行**。
参考 [GitHub 官方说明](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
和 [Google 原生应用说明](https://developers.google.com/identity/protocols/oauth2/native-app)。
安装式应用的 client secret 不能替代 PKCE 和应用身份隔离。

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

阅读顺序：`providers.rs` → `oauth.rs`/`authorize.rs` → `store.rs` → `host.rs`。
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
