# 正常 Home 的共享 Glance — 2026-10-05

[English](shared-glance-home-2026-10-05.md) | 简体中文

最初的 [Mail 后台测试](mail-background-2026-10-05.zh-CN.md)使用独立测试包，
没有填充用户日常桌面。本次后续验证从 Android 实际 Home 入口开始，检查多个
应用是否共用信息流。

## 部署与状态保留

用户要求接入正常 Home。在指定 OnePlus 6（API 35）上，`dev.makepad.octosense`
保留 Home 角色，使用原签名身份从 2026100216 升级至 2026100516。没有刷写 ROM
或 Bridge。迁移前备份了两个包的数据和旧 Home APK；MailTest 已停止，避免重复收取。

目标包原本没有 Mail 账号。保留了它的模型提供商配置与其他应用。迁移保留已有
Mail 账号元数据、邮件缓存、事件策略、账号工作区、发布记录、通知发件箱以及
此前已授予的 Mail 同意记录。复制后 271 个选定普通文件的哈希一致；打开卡片
及 Chat 标签后，11 个已保存草稿文件仍未变化。没有批准或发送回复。用户输入凭据前，已恢复原来的保持唤醒设置，
并将 ADB 恢复为非 root 模式。

**凭据没有成功迁移。**复制的密码文件带有 `OSK1` 头，依赖源包的 Android
Keystore 密钥，Home 的 UID 无法解密。文件哈希相同不能证明登录可用。用户在
正常 Home 中重新登录之前，Gmail 收取仍被阻断。Reconnect account 操作打开
既有的宿主登录面板；使用相同邮箱地址、用户名与收件服务器设置，可以更新原账号，
不删除草稿。没有导出密钥，也没有切换到明文凭据存储。

## 设备观察

| 检查 | 结果 |
| --- | --- |
| Android Home 意图 → 右滑 | 正常 Home 显示四张已保存 Mail 卡片 |
| 已保存的回复卡片 | 打开全屏 Email/Chat，原生编辑器与 Review reply 可见，聊天记录保留 |
| 系统代理 → News 代理 | DeepSeek 以 News 的 `own_agent` 身份调用 `news.list` 与 `news.notify` |
| 混合信息流 | News 与 Mail 同时出现在正常 Home 的同一个 Glance 中 |
| 展开 News | 从摘要打开通用全屏 Card/Chat 工作区 |
| 2026100517 重新连接 | 收件箱操作可见可点，打开宿主登录面板；凭据由用户在手机上输入 |
| 共享信息流排序测试 | 已有 `published_cards_lead_the_feed_by_priority_then_recency` 回归通过 |

Codex 通过 ADB 操作手机并审视平台截图。DeepSeek `deepseek-v4-flash` 根据实际
缓存新闻提供通知文案，`news.notify` 使用宿主固定模板，并非本次模型新编写的
L0 源码。模型使用同一个测试卡片 id 发布两次，第二次替换原通知。没有改写已有
Mail 卡片。本轮没有新增 MiniMax、Calendar、Photos、Maps、Camera 或 YouTube 验收。

## 构建记录与待验证项

Home 2026100516 使用源码 `e42ac4ee`、`mobile-apps` 与固定内核。
APK SHA-256：
`2607d21f0c2bab06b1a593b3f34d9b60dd56d67c5f22f4dc00a3fd23e7a4ce35`。
随后已构建并安装带有 Reconnect 操作的 Home 2026100517。其 APK SHA-256：
`379bc439a4aa836ad0d769a1ee603b27b19666ad5961b28e8b1c3e22aa55e685`；Mail bundle SHA-256：
`f2429306414abc929e64e9f2ea6e1b6116d433f73fbe8a0b4a42c8ad51afd3de`。
重新认证成功和恢复 Gmail 收取仍**待完成**。

本次证明了共享信息流与两类工作区入口，不代表完整 UX 或性能评分。自然周期、
Doze 与重启投递仍未验证。本轮手机采用浅色外观，没有单独验证深色外观。Mail 当前四张存活卡片上限限制后续发布，旧卡片没有
被清除。通用 News 通知保存在内存中，本次不证明所有应用都能在重启后恢复卡片。
原始邮件、草稿、截图、账号与模型提供商配置均保持私有。
