# ADR 0010：共享 OAuth 与独立安装的连接账户应用

[English](0010-shared-oauth-and-connected-apps.md) | 简体中文

状态：实现中；macOS 身份登录及合成后端验收已通过。用户使用专用测试账户验证了真实 Google Calendar 登录、保存日程及刷新。独立 OnePlus 后端测试应用通过了登录、Glance 交接和凭据库/退出检查，但存在视觉证据限制。其他提供方写操作和完整设备 UX 验收仍待完成；准确边界见实现指南。*（2026-10-06：App Hub 签名目录第 7 版现已收录下列三个示例，每个都从自己的公开仓库构建；App Design Flow 保留它们的开发版本。`desktop-v0.1.0-beta.2` 是第一个能安装它们的发布版本，但它的下载包不含服务商注册，运维人员添加 `clients.json` 之后才能登录。）* *（2026-10-07：签名目录第 10 版为每个示例加入 0.1.1 版本，0.1.0 条目保持不变。GitHub Notes 0.1.1 声明了只读的应用 Agent；Inbox Assistant 0.1.1 的通知工具只接受已准入的卡片模板；宿主报告日期范围时，Google Calendar 0.1.1 会显示已加载的日期范围。在 `main` 上，[#356](https://github.com/OctoSense-org/OctoSense/pull/356) 让 GitHub 或 Calendar 保存也必须在原生审阅界面上亲手点按（Gmail 发信本来就需要），并把 Calendar 缓存限定在一个日期窗口内。目前还没有发布版本包含 #356：`desktop-v0.1.0-beta.2` 只在 Gmail 发信时检查是否亲手点按。）*

[实现指南](../../crates/oauth-service/README.zh-CN.md)记录当前平台支持、配置方法和验证边界。

## 决策

OctoSense 提供可复用 OAuth 宿主服务，首先支持 GitHub 和 Google 适配器。无需创建 OctoSense 用户账户。服务商注册及凭据属于宿主；商店应用只获得绑定自身的不透明连接句柄及授权业务结果，不得到 token。

首批消费者是在 App Design Flow 维护的三个普通 App Hub 包：GitHub Notes、Inbox Assistant 和 Google Calendar。ID 不使用 `os.*`，安装不依赖打开系统 Mail/Calendar。

## 身份授权边界

- 服务商适配器固定授权、token、API 来源。应用不能指定 token 端点、client secret 或回调目的地。
- 客户端注册及服务商选项由宿主配置。
- 授权前，宿主展示申请应用、服务商及权限。
- GitHub 使用设备授权。Google 桌面使用系统浏览器、PKCE S256、随机 state 和临时回环监听器。
- Google Android 必须使用受支持的原生授权集成，不能悄悄复用桌面回环或自定义 scheme。
- 授权尝试限时、单次使用并绑定应用。取消、退出及账户替换使相关待办工作失效。
- access/refresh token 保存在宿主凭据库；连接只暴露服务商身份、权限、期限/状态和句柄。
- 每次操作检查调用者、服务商、scope 和资源；只有 `auth` 不代表获得业务数据权限。
- 外部写入保留对精确内容的宿主审核。模型输出既不是同意，也不是远端写入成功的证据。

## 使用方

### 开发者自己的后端登录

应用可使用自己的后端账户。宿主按应用配置精确、同源的 HTTPS 授权、令牌、身份与退出端点，在 macOS/Android 的宿主 WebView 或桌面外部浏览器打开后端注册/登录页面，交换 PKCE 绑定的单次代码，并将会话保存在同一套绑定应用的凭据库中。应用不获得 bearer 凭据，也不能在登录请求中提供端点。保存的注册摘要防止配置变更把旧令牌发送到新端点。

适配器提供 provider 为 `backend`、scope 为 `app.session` 的 `auth.connect`，复用账户生命周期，并用 `auth.backend.me` 读取受保护身份。后端业务 API 和应用包自行注册需要另行实现。*（2026-10-07：[ADR 0012](0012-app-host-api-discovery.zh-CN.md#后端登录与业务请求) 补上了这两项：签名应用包可以声明后端注册信息和命名操作，应用用 `auth.backend.request` 调用这些操作。已在 OctoSense `main` 上，尚未进入任何发布版本。）*macOS 嵌入会话使用非持久化 WKWebView 存储；Android 9+ 使用每次登录独立的 WebView 进程及数据目录。宿主拦截 `https://octosense.invalid/auth/callback`，限定登录来源，提供返回、取消与重试，不向受限应用开放页面桥。提供方授权保留浏览器/设备流程；Windows/Linux 保留浏览器适配器，iOS 后端登录不可用。本地撤销先于远程退出，远程失败单独报告。测试后端必须运行真实浏览器表单和代码交换，不能预置已登录账户；HTTP 回环例外仅存在于验收构建。

### 连接账户的 App Hub 示例

GitHub Notes 复用 Rinx 提取出的文章编辑组件，保留 Markdown 源码、富文本、选区/IME、预览和撤销。笔记写入用户选择的仓库路径及分支，保存 GitHub 提交回执。使用 blob SHA 检测冲突，不能静默覆盖别人修改；本地草稿保存与 GitHub 提交明确分开。

Inbox Assistant 连接邮箱、过滤新邮件、发布重要事项，Chat、手工编辑和审核共用权威草稿状态。发送沿用受保护的审批语义。邮件内容是不可信数据，不能成为 Agent 行动授权。

Google Calendar 使用真实 Calendar API：选择日历、列举/创建/编辑事件、时区与全天事件、分页增量同步、token 失效后的全量恢复和 ETag 冲突处理。应用向获准 peer 提供有边界的工具，其卡片重新打开同一个已保存的 Google 事件。虚构的本地示例事件必须明确标注。*（2026-10-07：自 [#356](https://github.com/OctoSense-org/OctoSense/pull/356) 起，`main` 上的 Calendar 不再增量同步。每次刷新都整体替换一个窗口的快照：从今天之前 30 天到之后 366 天（按 UTC 日界），重复日程展开为实际实例；宿主会拒绝 `gcalendar.sync` 调用。`desktop-v0.1.0-beta.2` 仍用同步 token 同步全部历史。）*

## 交付与验收

平台代码属于 OctoSense，capability/接纳规则属于 App Hub，示例和入门流程属于 App Design Flow。通过兼容且固定版本的依赖图复用 Rinx 组件，不复制整个聊天应用或引入第二份 Makepad。

先通过确定性传输测试协议、身份隔离和冲突。然后使用临时签名目录在干净配置下安装精确包，用 Makepad instrument 验证可见交互，再检查真实服务商的获准远程结果。Windows/Linux 需在相应平台测试；Android 使用指定的 OnePlus 6。缺少客户端注册、用户授权或未执行的设备测试必须标为待完成。

示例不能包含凭据、私人邮件、私有仓库内容、真实日历事件或伪造截图。公共目录发布与本地开发、临时目录安装测试是不同步骤。

## 参考

- [GitHub OAuth](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [GitHub 仓库内容 API](https://docs.github.com/en/rest/repos/contents)
- [Google 原生 OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)
- [Google Calendar 同步](https://developers.google.com/workspace/calendar/api/guides/sync)
