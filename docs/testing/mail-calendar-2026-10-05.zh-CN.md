# Mail 调用日历验证 — 2026-10-05

[English](mail-calendar-2026-10-05.md) | 简体中文

Mail 现申请日历可共享的 `calendar.events`、`calendar.add_event`、`calendar.notify`。
系统 Agent 对这三项有单独的显式授权。App Hub 按应用提供的宿主接纳范围、manifest
申请与 Shell 中转的调用者检查是不同环节。服务调用仍归属日历执行器；Mail 不会
获得日历目录的原始访问权。

Mail 冷启动注册日历服务，并延迟加载经接纳的工具目录和执行器，无需打开日历或
准备其代理。独立新进程测试覆盖此路径；这与下述手机前台验证不同。

日程接受 IANA 时区和稳定的 `request_id`。完全相同的重试复用已保存日程；相同键
但负载不同会被拒绝。未知结束时间应省略。夏令时切换中不存在或有歧义的时间会被
拒绝，卡片显示日程时区。安排日程需要用户请求或明确配置的策略，邮件本身不提供
授权。这是本地日程，不是 Google Calendar 同步、邀请邮件或定时提醒。

## 自动检查

共享 Rust 测试通过 1,203 项，忽略两项可选 Mail 测试。其中包括 939 项 Shell、
55 项 Mail、5 项 Calendar、17 项 Appstore 单元测试，以及 App Contract/Policy/Hub
测试与文档示例。覆盖冷启动接纳与服务注册、未授权调用拒绝、Mail 和系统的最小
授权、所有者路由、实际日历存储、重试幂等、变更冲突、旧记录和具名时区。

在已准备的固定依赖工作区中，从 `phone/` 执行：

```sh
cargo test --offline --locked --features mobile-apps \
  -p octosense-shell -p octosense-mail-service -p octosense-calendar-service \
  -p octosense-appstore -p octosense-app-policy -p octosense-app-hub \
  -p octosense-app-contract
```

九项必需检查全部通过：桌面默认/mobile-apps、手机、两种 Shell 依赖图、固定版本、
原生目录、私有路径与空白检查。另有 114 项构建工具单元测试通过。没有配置内核
二进制时提前返回的可选实内核测试，不算真实模型证据；手机结果单独记录。

## 手机验证

正常 OnePlus 6 Home 原位升级至 `2026100525`，保留原有 Gmail 和提供方配置。
源码：`33ef60804758d3ba09321eb42312a5e1e42acb1c`；App Hub：
`db7ef46aea22e6f1e8b5c7ed7a72548bf94feb7f`（[PR #93](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/93)）。
发布 APK 不可调试，未开启开发者模式。SHA-256：
`ac245c48893d5fa6b947b2854bb80c3499a5ba50a0ec642c4f50f834c28dedb1`。
原生目录版本已对齐，重新生成未产生代码变更。

升级前的私有备份包含全部 15 个草稿文件；当时不存在日历事件文件。私有邮件、地址、
账号标识、提供方配置、对话和截图均不入库。

现有配置选择 `deepseek-v4-flash`。系统 Agent 将请求委派给已有 Mail peer。宿主审计记录 Mail 为调用者
（`app/os.mail`）、Calendar 为所有者，读取、添加、回读和通知全部成功。仅保存
一个日程，保留所请求的太平洋时区、稳定重试键，没有结束时间；地址组成部分与
已接受的回复邮件一致。全部 15 个草稿文件逐字节未变。系统 Agent 收集到旧 peer
结果后重试，Mail 回读已有日程并再次通知，因而同一日程出现两个横幅。通知重试
去重尚未实现。系统最终回答还混入旧会话中的无关工作；本次不证明会话摘要质量。

启动测试快捷方式仅恢复历史，没有提交请求；驱动随后通过可见 Assistant 输入框
输入并提交。这是用户明确授权的日程请求，不是新的入站邮件触发，也未改变自动
安排策略。去掉测试动作后干净重启，事件文件逐字节不变，仍只有一个日程。Calendar 的
Glance 卡片未能跨进程重启保留：目前通用日历发布保存在内存，Mail 则有专用持久
恢复路径。需要重新发布已保存日程才能再次显示卡片。这是实际观察到的限制，不是
持久化通过。此处未验证后台任务中的日历发布，也未测试 MiniMax。驱动使用 Android/ADB 输入
与平台截图，并非 Makepad 无头控件测试。不会自动执行邮件发送审批。
