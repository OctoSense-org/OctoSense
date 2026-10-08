# 复现真实邮件 → 卡片 → 日历演示

[English](README.md) | 简体中文

使用你自己的邮箱和模型账户。新邮件触发 Mail Agent，由模型判断是否生成卡片。
你可以在卡片内聊天、修改已保存的回复、审阅，并选择是否发送。明确在 Chat 中要求
预约后，Mail 会调用 Calendar 的工具，保存本地事件并发布 Calendar 自己的卡片。
卡片内的 **Open Calendar** 打开真实 Calendar 应用中的同一条事件。

这是系统应用的真实流程。OctoSense App Flow（原 Design Flow）中的黑客松示例使用虚构的本地发件箱和日历，不会重现 Gmail 的真实投递。

## 1. 获取完整源码

运行时检查点为 OctoSense 提交
`4081c30e432ad0c3d0260c90be804d5e8aa5a9a1`（PR #342），其中已包含此前
Mail、卡片和 Calendar 的变更。使用本指南所在分支，或包含本指南的 main 版本，
才能同时取得输入生成器。不要只摘取最后一个 UI 提交。

| 依赖 | 此检查点锁定的版本 |
| --- | --- |
| App Hub | `7bb63ff925d19f9aecbc72b8946679320e53cfa1`，已由 PR #93 合并 |
| Octoscript-Makepad | `27e9c1bfdbf6021bcad87214ae4ebbe6d683b406` |
| Makepad 基础版本 | `c155f61d0e1600d2ec474209374444a38a09a470`，另加 `runtime-patches.lock.json` 中已提交的补丁 |
| Octoscript | `2e37d9e657a246f16718d9a475e167ccd2d5b5fa` |
| octos 内核 | `056173e85b150e387805fc307fe231064ac1ed35` |

克隆 [OctoSense](https://github.com/OctoSense-org/OctoSense)，切换到本指南的
版本，在仓库根目录运行：

```sh
python3 tools/setup.py
python3 tools/setup.py --check --cargo
python3 tools/native_apps.py --check
```

已有依赖仓库时，先按[环境准备](../../../README.zh-CN.md#环境准备)配置 source hub。环境准备脚本会创建固定版本的工作树并打上仓库跟踪的补丁，不依赖任何开发者本地未 commit 的依赖改动。Cargo 会取得 App Hub 和内核版本；真实演示不需要额外检出旧 AppCard、OctoSense-mobile 或 App Flow 仓库。

## 2. 构建包含内核的 Android 安装包

先准备 [Phone 构建说明](../../../phone/README.md#build-and-run) 中的工具链。
已有 Home 时，建议安装独立测试包。把 `MAKEPAD_ANDROID_SDK` 设为你自己的
cargo-makepad SDK 目录，`ADB` 设为 Android platform-tools 可执行文件，
`DEMO_SERIAL` 设为你自己的 `adb devices` 设备编号；这些值只保存在本机。

从仓库根目录构建带补丁的打包器，再通过内核包装脚本构建：

```sh
cargo build --release --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
mkdir -p target/mail-calendar-demo
cd phone
python3 ../tools/kernel-artifact.py --sdk "$MAKEPAD_ANDROID_SDK" \
  --receipt ../target/mail-calendar-demo/kernel.json -- \
  ../.sources/makepad/target/release/cargo-makepad makepad android \
  --sdk-path="$MAKEPAD_ANDROID_SDK" \
  --package-name=dev.makepad.octosense.mailcaldemo \
  --app-label='OctoSense Mail Calendar Demo' \
  build -p octosense-home --release --features mobile-apps
cd ..
```

把 `DEMO_APK` 设为打包器输出的 APK 路径，安装前检查内核：

```sh
python3 - "$DEMO_APK" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as apk:
    assert 'lib/arm64-v8a/liboctos.so' in apk.namelist(), 'Missing agent kernel'
print('Agent kernel is bundled')
PY
"$ADB" -s "$DEMO_SERIAL" install -r "$DEMO_APK"
"$ADB" -s "$DEMO_SERIAL" shell am start \
  -n dev.makepad.octosense.mailcaldemo/.MakepadApp
```

这些可移植命令按仓库脚本整理；**尚未用这个新包名和全新账号重跑手机全流程**。
已有真实测试使用 OnePlus 6 的正常 Home，源码和 APK 哈希见
[日期记录](../../testing/mail-medical-calendar-2026-10-06.zh-CN.md)。不同签名会改变 APK 哈希。

## 3. 在同一个安装包中配置

1. 在 **AI providers** 配置自己的模型账户。医疗预约演示使用 DeepSeek
   `deepseek-v4-flash`。必须具备真实模型服务；独立 `card-host` 不能代替 Agent。
   MiniMax 验证过较早的邮件/卡片流程，但最新医疗邮件到 Calendar 的流程只验证了 DeepSeek。
2. 打开 **Mail**，在宿主登录界面输入自己的 IMAP/SMTP 凭据；Gmail 表单支持应用密码。
   不要把密码放到聊天、脚本、Git 或生成的输入中。关闭演示邮箱模式。
3. 在提示时允许 Mail Agent。打开 **Assistant**，要求用下方准确的
   `agents.provision` 参数开启自动处理。配置策略不能绕过用户授权。
4. 允许此安装包及 Mail 通知频道的 Android 通知，并保持联网。首次测试保持 OctoSense 打开。

**账号、授权、策略和 Glance 数据均按安装包隔离。** 在测试 APK 登录后，应打开
该 APK 的 Home/Glance 查看卡片；手机默认桌面不会显示另一个包的数据。要测试系统
桌面右滑，可自行在 Android 设置中将测试包选为 Home，无需刷 ROM。
正常 `dev.makepad.octosense` 包在完成自身配置后也支持此流程。

## 4. 配置重要邮件筛选并等待首次同步

选择未来日期，生成五个虚构输入文件；命令不会发送邮件或读取账号：

```sh
python3 tools/mail-calendar-demo.py --date 2026-11-12 \
  --timezone America/Los_Angeles --output target/mail-calendar-demo/inputs
```

每次使用新的输出目录。在 **Assistant** 粘贴 `policy.json`，并要求：
“用这个 JSON 原样调用 `agents.provision`，然后用 `{"app":"os.mail"}` 调用
`agents.status` 并显示工具结果，不要生成卡片。”
此策略保留已安装的 Mail 指导和分类技能（`skills: []` 不添加覆盖），只通知相关且
需要行动的邮件，保存回复草稿，并要求人明确提出请求后才能预约日历。

等待 `configured`、`enabled`、`consent`、`admitted`、
`runtime.baseline_ready` 全部为 true，`runtime.last_collection_success_at`
为近期时间，且没有收信错误。首次同步只建立基线，**不会为已有邮件生成卡片**。
之后再发送测试邮件。`account` 及原始状态、日志只保留在本机。

## 5. 发送新邮件并检验模型判断

使用自己的第二个邮箱或 AgentMail 发件账号，把 `appointment.txt` 中的主题和正文
发送到 OctoSense 内已连接的邮箱。文件不包含收件地址，请私下填写自己的地址。
AgentMail 只是可选的真实发件服务，不是运行时依赖；选择它时使用自己的账号和密钥。
另发 `quiet-newsletter.txt` 作为不应打扰的对照。保留每次唯一的演示编号，并使用
自己控制、可以回复的发件账号。

不要要求 Assistant 为这些邮件造卡片。前台轮询为 30 秒，还需要模型处理及排队时间。
Mail 应读取预约邮件、保存草稿并生成相关卡片及通知；普通资讯应被安静跳过。
这是模型判断，不保证关键词分类结果；误判意味着此项验收失败。

用 `agents.status` 区分收信成功和模型处理成功。`pending` 应逐步清空，
`last_receipt` 显示发布或跳过的结果。两封邮件接近到达时，最后一条回执可能属于
任意一封；需要结合可见卡片及本机对应工具记录判断，不能仅凭最后回执归因。

## 6. 人工接管、修改回复和真实 Calendar

1. 点击通知，或在同一包的 Glance 展开预约摘要。应进入独立交互区域，
   **Email / Chat** 共用一份已保存草稿；滚动、键盘和输入框应可用。
2. 在 **Chat** 提交 `chat-request.txt`，明确授权一次虚构的本地预约。
   Mail 应读取 `calendar.events`、通过 `mail.suggest_reply` 保存回复
   （`applied:true`），再调用 `calendar.add_event`、读回记录并调用
   `calendar.notify`。有冲突则应停下来询问你。收到邮件本身不构成预约授权。
3. 切换 **Email**，检查实际保存的正文包含正确日期、时间和时区，不能只看聊天回答。
   再要求“加一句：请确认房间号”，检查正文确实改变。手动编辑并来回切换标签，
   确认编辑保留。未应用的建议不算保存成功。
4. 展开 Calendar 卡片，点击卡片内部的 **Open Calendar**。必须打开真实
   Calendar 应用中的同一条已保存事件，并与 `expected.json` 对照。重复相同
   预约请求后仍应只有一条事件；变更时间需要新决策，不能虚报已修改。
5. 点击 **Review reply** 核对准确的收件人、主题、正文；默认演示取消并保持未发送。
   如要测试真实发信，请本人在手机上触摸批准，并在发件方邮箱检查收件结果。
   ADB、聊天文字和模型工具不能批准 SMTP；已有医疗预约检查点未测试发送。

Calendar 数据保存在 OctoSense 本地，不等于 Google Calendar 同步、邀请邮件或
Android 提醒。重启应用后检查事件和草稿仍在。Glance 卡片过期或完成后可退出列表，
不意味着删除源邮件或日历记录。

## 7. 后台和问题定位

| 现象 | 检查 |
| --- | --- |
| Mail 能看到邮件但没卡片 | 是否首次基线、已授权、已开启策略；队列、模型错误或主动跳过 |
| 测试包内有卡片但正常 Home 没有 | 是否处于不同安装包、账号或 Glance 数据库 |
| 有卡片但没有 Android 通知 | `notify`、通知权限/频道、安静重试、过期 |
| Chat 说已修改但 Email 没变 | 实际草稿版本和 `mail.suggest_reply`；`applied:false` 不算成功 |
| Calendar 权限错误 | App Hub 固定版本、Mail 跨应用授权、工具注册；不要任意扩大权限 |
| 处理很慢或没反应 | 网络限制、模型权限、收信与处理错误、排队和退避状态 |
| 强行停止后后台不工作 | 重新打开应用；Android 强行停止会禁止后台作业 |

正常关闭应用后，安静后台作业周期为 15 分钟、弹性窗口为 5 分钟，Android 可能
进一步延迟；这是轮询而非即时推送。只对模型认定重要的邮件通知。先测前台，再正常
关闭应用而不是强行停止，发送另一个唯一编号的新邮件测试后台。

## 验证范围和隐私

10 月 6 日已观察到 DeepSeek 自动分类、按人要求修改草稿、Calendar 持久化以及
卡片内跳转，邮件保持未发送。OnePlus 6 时间小格末尾的时区文字仍有裁切，因此不声称
完整视觉验收，也不保证模型每次生成相同文字。

在 `phone/` 运行本地回归：

```sh
cargo test --locked --features mobile-apps \
  -p octosense-shell -p octosense-mail-service -p octosense-calendar-service
```

2026-10-06 再次检查：**1,002 项测试通过**，两个可选 Mail 测试未运行；依赖图及
版本、原生应用目录、虚构输入生成、变更 Markdown 链接均通过，Android 内核构建
计划已检查。本次文档变更未重新构建安装 APK、登录全新账号或重跑真实模型/SMTP。

公开报告只保留脱敏统计、通过/失败结论和源码哈希。不要提交账号/模型配置、真实
邮件/草稿/聊天、原始状态、日志、截图、设备编号或签名密钥。公开输入使用本工具
生成的虚构样例，其中没有邮箱地址或凭据。
