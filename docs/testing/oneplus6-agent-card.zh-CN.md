# OnePlus 6：系统 Agent → Mail 卡片

[English](oneplus6-agent-card.md) | 简体中文

**真机测试通过：**系统 Agent 将任务交给 Mail Agent，后者发布通知卡片；
通知显示在系统聊天上方，手指点击后打开完整卡片，点击 × 后回到 Glance，
同一张卡片仍可见。验收卡片 ID 为 `os.mail/agent-card-e2e-accepted`。

测试运行时来自基于 main `e6bfad93` 的干净提交
`c3eaea2ce8bc06192e805438b590f885c870db29`，与 App Studio 实现独立。
[构建记录](evidence/agent-card/build.json) 包含 APK 和原始 1080×2280 截图的
SHA-256。打包使用提交 `88550095` 的独立包名构建辅助脚本，在仓库外运行，
仅将仓库根目录指向当前源码；编译的 Rust 源码仍为 `c3eaea2c`。
测试包名为 `dev.makepad.octosense.studio`。

## 执行过程与证据

两个 Agent 都实际调用已获授权复用的 DeepSeek `deepseek-v4-flash` 配置。
测试人员通过 ADB 发送 Android 触摸输入并检查截图；模型没有执行点击或视觉验收。
Mail 使用现有 `mail_demo` 传输层，通过宿主登录界面添加测试身份
`agent-test@example.invalid`。记录中没有读取邮件或发送邮件的工具调用。

构建包含开发者选项，但未启用开发者授权。首次尝试出现真实的
“Let Mail's agent start?” 同意界面，由测试人员点击 Allow；最终重放复用了
该授权和 peer。生产 Home、Bridge、默认启动器和 ROM 均未替换。

| 阶段 | 实际观察 |
| --- | --- |
| 系统 Agent 委派 | `peer_send_input` 将验收卡片任务发给 `os-mail-f6e8245bc1167c11`。 |
| Mail Agent 发布 | 独立 Mail peer 调用 `mail_notify`，返回成功、相同卡片 ID 和 `replaced: false`。 |
| 通知出现 | Mail 通知条显示在仍然打开的系统聊天上方。 |
| 点击打开完整卡片 | Android 原生触摸 `(540,175)` 打开对应卡片，标题和完整正文可读。 |
| 关闭后显示 Glance | 点击测得的 × 位置 `(973,807)`，同一测试 Activity 内关闭弹层，Glance 显示相同标题和正文。 |
| 结果返回系统 Agent | `peer_gather` 取得已完成的 Mail 结果，两个会话均有正式提交的最终回复；均未声称自行检查屏幕。 |

[聊天上方的通知](evidence/agent-card/01-notification-over-chat.png) ·
[完整卡片](evidence/agent-card/02-full-card.png) ·
[关闭后的 Glance](evidence/agent-card/03-glance-after-close.png)

[脱敏会话记录](evidence/agent-card/agent-evidence.json) 仅保留工具参数、结果预览
和正式回复，不包含推理内容或供应商凭据。
[路由日志](evidence/agent-card/notification-route.log) 与
[触摸记录](evidence/agent-card/observation.json) 将工具调用与实际显示关联起来。
通知截图上方保留了先前轮次的回复；应通过验收卡片 ID 判断当前轮次。

## 重复测试

1. 在已授权的手机上使用独立测试包和已授权的供应商配置，启动时在
   `makepad.APP_CONFIG` 中设置 `mail_demo: true`。
2. 通过宿主登录界面添加上述测试身份，使用 demo 传输层的公开口令 `demo`。
3. 向系统聊天发送[记录的提示词](evidence/agent-card/system-prompt.txt)。本次使用
   现有 `system-chat-send:` 测试动作提交；模型根据需要取得同意、委派给真实 Mail
   peer，并收集结果。
4. 保持聊天打开，点击通知检查完整卡片，再点击 × 检查 Glance。触摸位置应以实际
   屏幕和布局为准；记录的像素坐标只适用于本次手机密度。
5. 对照 peer 工具结果与宿主日志中的卡片 ID。Agent 报告成功不能单独证明渲染或点击成功。

## 发现的问题与验证

修复前，聊天遮住通知，且通知只处理鼠标按下，忽略 Android 触摸。修复后，
通知和卡片弹层位于聊天上方，匹配的触摸释放才激活卡片；拖动会取消激活但保持
事件归属，被其他弹层中断的触摸可恢复。通知打开指定卡片，底层切到 Glance；
卡片 × 和背景支持触摸关闭。

在 `c3eaea2c` 上，**838 个 shell 测试通过**，包含 7 个新增触摸回归测试。
桌面默认和 `mobile-apps` 检查、Home 的 `mobile-apps` 检查、两套 shell 依赖图检查
以及 `tools/setup.py --check --cargo` 均通过。Android 构建和上述真机流程也通过。
单元测试不是无窗口 UI 端到端测试；交互证据来自真实手机。

## 范围与清理

- 验证的是 **OctoSense 应用内通知条**；未验证 Android 系统通知栏、后台通知或内部通知抽屉。
- `mail.notify` 将内容填入宿主现有 L0 通知模板；没有生成新的 L0 源码，也没有打开 Mail 完整应用。
- 最终验收未测试 Calendar、真实 IMAP/SMTP、卡片替换、过期或进程重启后的持久化。
- 较早重放中，Android Back 关闭卡片后也退出了改名后的测试 Activity。当前固定版本的
  Makepad 按 `<package>.MakepadAppExtension` 查找扩展，而 Home 的 Java 类保留原包名，
  因而测试包没有加载 Home 的原生 Back 接管逻辑。最终流程使用 ×；生产 Home 的 Back
  行为在本次测试中仍属**未验证**。

已移除临时供应商配置、恢复屏幕超时时间、停止测试包，并确认 ADB 为非 root。
保留合成 Mail 状态和测试证据供检查，见[清理记录](evidence/agent-card/cleanup.json)。
