# ADR 0010：共享 OAuth 与独立安装的联网应用

[English](0010-shared-oauth-and-connected-apps.md) | 简体中文

状态：实现中；真实服务商及设备验收待完成。

[实现指南](../../crates/oauth-service/README.zh-CN.md)记录当前平台支持、配置方法和验证边界。

## 决策

OctoSense 提供可复用 OAuth 宿主服务，首先支持 GitHub 和 Google 适配器。
无需创建 OctoSense 用户账户。服务商注册及凭据属于宿主；商店应用只获得绑定自身的
不透明连接句柄及授权业务结果，不得到 token。

首批消费者是在 App Design Flow 维护的三个普通 App Hub 包：GitHub Notes、
Inbox Assistant 和 Google Calendar。ID 不使用 `os.*`，安装不依赖打开系统 Mail/Calendar。

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

## 三个消费者

GitHub Notes 复用 Rinx 提取出的文章编辑组件，保留 Markdown 源码、富文本、选区/IME、
预览和撤销。笔记写入用户选择的仓库路径及分支，保存 GitHub 提交回执。使用 blob SHA
检测冲突，不能静默覆盖别人修改；本地草稿保存与 GitHub 提交明确分开。

Inbox Assistant 连接邮箱、过滤新邮件、发布重要事项，Chat、手工编辑和审核共用权威草稿状态。
发送沿用受保护的审批语义。邮件内容是不可信数据，不能成为 Agent 行动授权。

Google Calendar 使用真实 Calendar API：选择日历、列举/创建/编辑事件、时区与全天事件、
分页增量同步、token 失效后的全量恢复和 ETag 冲突处理。应用向获准 peer 提供有边界的工具，
其卡片重新打开同一个已保存的 Google 事件。虚构的本地示例事件必须明确标注。

## 交付与验收

平台代码属于 OctoSense，capability/接纳规则属于 App Hub，示例和入门流程属于 App Design Flow。
通过兼容且固定版本的依赖图复用 Rinx 组件，不复制整个聊天应用或引入第二份 Makepad。

先通过确定性传输测试协议、身份隔离和冲突。然后使用临时签名目录在干净配置下安装精确包，
用 Makepad instrument 验证可见交互，再检查真实服务商的获准远程结果。
Windows/Linux 需在相应平台测试；Android 使用指定的 OnePlus 6。
缺少客户端注册、用户授权或未执行的设备测试必须标为待完成。

示例不能包含凭据、私人邮件、私有仓库内容、真实日历事件或伪造截图。
公共目录发布与本地开发、临时目录安装测试是不同步骤。

## 参考

- [GitHub OAuth](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [GitHub 仓库内容 API](https://docs.github.com/en/rest/repos/contents)
- [Google 原生 OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)
- [Google Calendar 同步](https://developers.google.com/workspace/calendar/api/guides/sync)
