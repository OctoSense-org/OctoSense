# Mail 收取与 Android 后台验证 — 2026-10-05

[English](mail-background-2026-10-05.md) | 简体中文

范围：独立收取与投递、模型按重要性作出决定，以及 [ADR 0008](../adr/0008-quiet-android-mail-jobs.zh-CN.md)。使用指定的 OnePlus 6（Android API 35）、独立测试包 `dev.makepad.octosense.studio`（OctoSenseMailTest）及其中已有的 DeepSeek `deepseek-v4-flash` 配置。没有替换已安装的 Home 或 ROM。升级前后 Gmail 凭据与提供商配置文件的哈希一致。测试驱动没有发送回复邮件。

## 本地检查

| 检查 | 结果 |
| --- | --- |
| 在 `phone/` 运行 `cargo test --locked --features mobile-apps -p octosense-shell -p octosense-mail-service` | 930 个 Shell 测试、54 个 Mail 测试通过；2 个可选 Mail 测试忽略 |
| 在 `phone/` 运行 `cargo check --locked -p octosense-home --features mobile-apps` | 通过 |
| 根目录运行 `cargo check --locked -p octosense`，以及带 `--features mobile-apps` 的版本 | 通过 |
| 根目录 `bash tools/check-shell-graph.sh -p octosense`；手机版本使用 `-p octosense-home` | 通过，仅一个固定 octos 修订版 |
| `python3 tools/setup.py --check --cargo`；`python3 tools/native_apps.py --check` | 通过 |
| `python3 -m unittest discover -s rom/tests -p test_no_local_paths.py` | 通过 |
| 独立、可调试 Android APK，保留账户数据与内核哈希 | 构建并安装；最新测试版本 2026100512 |

最终检查使用隔离的 `OCTOSENSE_HOME` 与 `RINX_DATA_DIR`。两个忽略项分别需要真实 Gmail 凭据失败测试或 macOS 钥匙串。模拟上下文和传输测试验证调度、回执、取消与持久化边界，不代表真实模型表现；真实 DeepSeek 与设备证据单独列在下方。

## 设备观察

| 场景 | 观察结果 |
| --- | --- |
| 已有待处理事件时收取新邮件 | 队列非空时收件箱仍增长；新收取线程不再等待队首失败事件 |
| 重要性偏好 | 系统代理保存了用户的医疗、物流、日程、学校、工作和家庭选择性策略。多个普通新邮件事件成功执行 `mail.peek` → `mail.skip_event`，没有产生原生通知 |
| 网络诊断 | 应用 UID 的 HTTPS 起初失败，shell UID 成功；Android 实际阻止原因是 `APP_BACKGROUND`。全局受限联网也已开启，但此应用对该规则有豁免，它不是实际阻止项 |
| 用户授权的设置变更 | 按用户要求关闭全局受限联网；测试期间临时修改的 USB/AC 保持唤醒设置已恢复原值 |
| 任务注册 | Android 报告了持久化、周期 15 分钟、弹性窗口 5 分钟、要求联网的任务 |
| 后台执行 | 通过 JobScheduler 测试命令强制运行时，Activity 关闭、屏幕休眠情况下仍收取并运行 DeepSeek；任务期间有效网络阻止状态为 `NONE` |
| 冷进程 | 升级或终止测试进程后，系统任务无需打开 Activity 即可启动宿主，使用原配置继续收取和决策 |
| 时限与中断 | 四分钟租约结束后，活动事件保留在队列中，状态记录取消；任务结束后 Android 恢复普通后台联网限制。显式 JobScheduler 超时也能停止任务 |
| 原生通知 | 真实新邮件通过 `own_agent/events-mail:…` 调用模型编写的 `mail.publish_card`，进入私有发件箱并产生 Android 通知。被拒绝的卡片源码由模型自行调用工具修复 |
| 进程终止后的通知 | `am kill` 后确认测试应用 PID 不存在；点击真实 Android 通知后产生新 PID，并打开正确的全屏卡片，包含已保存草稿、Email/Chat 标签与 Review reply |
| 边缘返回 | 边缘手势从卡片返回 Home，测试进程继续存在；清单声明的扩展已加载 |
| 重启去重 | APK 升级后没有再次发布已点击、已投递的通知；私有发件箱仍保留已投递状态 |

在 2026100509 和 2026100512 上观察到原生通知发布。首次完整的“通知 → 进程重启 → 卡片/返回”使用 2026100509；最终版还加强了通知权限处理、静默重新发布持久化与发布版本优先级。

## 待检查项与限制

- 物流模拟邮件已进入收件箱，仍排在已有队列后面；最终卡片结果正在观察。
- AgentMail 已接受单独标注的无操作对照邮件，但尚未证明它到达被监控的收件箱，不能算作代理已跳过。
- 当前运行的任务通过 Android 测试命令强制触发；自然周期、Doze 与重启周期目前均**未验证**。注册成功不代表延迟已经验证。
- 本次未在设备上逐项覆盖拒绝通知权限、禁用频道、运行中移除账户及其他 Android 版本。已有同意/账户测试与新增发件箱范围/到期测试覆盖宿主边界。
- 大量积压、慢模型或 Mail 的四张存活卡片上限会延迟后续决定。队列仍限于 128 个事件。模型的重要性判断不保证完全符合每个人的偏好。

截图、模型与审计对应关系、已脱敏的数量/时间观察和构建哈希保留在本地。公开记录不包含原始邮件、草稿、账户标识、提供商配置或私有产物路径。
