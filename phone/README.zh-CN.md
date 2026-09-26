# OctoSense Home

[English](README.md) | 简体中文

OctoSense 手机 Shell：一个 Makepad 应用，也就是设备的桌面。它包括带实时磁贴和应用组合的桌面页面、手势层、通知面板（左侧通知，右侧控制）、最近任务、用于展示进行中活动的实时岛，以及在进程内绘制于磁贴中的托管应用：App Hub 及其运行的应用、系统应用、AppCard、Reference 和 Sheets。

环境准备、各目标平台的构建、版本固定和 CI 见 [根目录 README](../README.zh-CN.md)。本页深入介绍 Home 专属的内容。

## 与 OctoSense-Desktop 的关系

2026 年 9 月 15 日，Home 从桌面端 Shell [OctoSense-Desktop](https://github.com/OctoSense-org/OctoSense-Desktop)（当时名为 OctoSense）中拆分出来，拆分点是其移动端 Shell 系列提交的最新位置（PR #22 至 #28）。两者至今仍共享大量源码（`src/main.rs`、`desk.rs`、`layout.rs`、`clients.rs`、`shell/*` 以及合成器）。手机版构建是同一个 crate 的 `mobile_only` 配置：在 Android 上由 `build.rs` 开启，在其他平台上由 `--features mobile-only` 开启。仅限桌面端的工作属于桌面仓库。下一步计划是抽出共享的 `octosense-core` crate，这样修复就不必再逐个 cherry-pick。`upstream/makepad.json` 记录了哪些窗口管理器文件是从 Makepad 导入的；`scripts/upstream.py` 负责比较并合并这些文件（[docs/upstream.md（英文）](docs/upstream.md)）。

## Home 角色

该 activity 声明了 `HOME` intent 过滤器，并且是 `singleInstance`。在你能控制的设备上执行：

```sh
adb shell cmd package set-home-activity dev.makepad.octosense/.MakepadApp
```

或者在 Android 的桌面选择器中选择 OctoSense。之后按下 Home 键或使用 Home 手势时，正在运行的 Shell 会收到 `Event::HomeIntent` 并显示桌面页面。Home 角色**不会**改变的是：系统仍保留自己的底部手势区域、自己的最近任务（上滑并停顿）以及自己的状态栏通知面板**。三按钮导航**可以消除与手势区域的冲突，是推荐的模式：

```sh
adb shell cmd overlay enable-exclusive --category com.android.internal.systemui.navbar.threebutton
```

（`…navbar.gestural` 可恢复手势导航。）特权路线（接管手势区域和最近任务）的工作量评估见 [docs/android/launcher-plan.md（英文）](docs/android/launcher-plan.md)，尚未开始。

## 手势

| 位置 | 手势 | 作用 |
|---|---|---|
| 桌面页面中部 | 下拉 | 打开搜索，输入框已获得焦点，键盘随即弹出 |
| 桌面页面右侧四分之一 | 下拉 | 通知面板的控制页（Wi-Fi、亮度等） |
| 桌面页面左侧四分之一 | 下拉 | 通知面板的通知页 |
| 顶部边缘左侧 / 右侧 | 下拉 | 同样打开通知 / 控制 |
| 桌面页面 | 左右滑动 | 切换页面：概览 ⇠ 应用 ⇢ 应用库 |
| 应用库 | 拖动 | 滚动网格；超出两端时会拉伸并回弹（返回或 Home 可关闭） |
| 应用库或搜索 | 在内容区向右滑动 | 回到离开时的桌面页面并收起键盘 |
| 底部条带（位于系统条带之上） | 上滑 / 停顿 / 左右滑动 | 回到桌面 / 最近任务 / 快速切换 |
| 左右边缘 | 向内滑动 | 返回 |
| 应用图标 | 长按 | 添加到桌面或从桌面移除、添加到程序坞、应用信息、卸载 |
| 桌面页面图标 | 长按后拖动 | 重新排列页面（放到图标之间）、放入程序坞（放到程序坞上）、创建文件夹（放到另一个图标上）或加入文件夹（放到文件夹磁贴上） |
| 应用组合磁贴 | 长按 | 更换其中任一应用，或移除该组合 |
| 文件夹磁贴 | 长按 | 移除其中一个应用，或移除整个文件夹 |
| 应用磁贴 | 长按 | 移除该磁贴（桌面菜单中的“显示隐藏的磁贴”可将其恢复） |
| 桌面空白处 | 长按 | 小组件、浅色/深色外观、网格：4 列或 5 列、下拉方式（启动器通知面板或系统级面板）、系统设置、显示隐藏的磁贴 |

下拉手势在拉到 40 % 时即生效（在 1080 像素宽的手机上约为 135 px）；导航类滑动则需要滑完整段距离或快速轻扫。下拉过程中页面会变暗，搜索框跟随手指移动；手势生效时会有一次短促的触感反馈。每个隐藏手势在首次使用之前，桌面页面都会显示一行对应提示（`src/mobile_hints.rs`；Android 会记住哪些提示已经看过）。在已停稳的桌面页面上再次按下 Home，会回到主页面。

搜索只能通过在桌面上下拉打开；应用库没有搜索栏。搜索结果中，名称以输入内容开头的应用排在前面，按回车即可打开最佳匹配。在应用库中，右侧的字母栏可快速跳转网格；获得使用情况访问权限后，顶部会显示一行“建议”，列出最近使用的应用。应用在通知面板中有通知时，其图标会带一个圆点。最近任务以卡片形式列出托管应用；在 Android 设置中授予使用情况访问权限后（最近任务中的卡片可打开该设置），还会显示一行最近使用过的 Android 应用。每个可点按区域都是带有语音标签的无障碍节点，因此 TalkBack 和 UI 自动化都能读取并操作 Shell（已在安装 TalkBack 的情况下以及通过 UiAutomation 探针验证：无障碍焦点能落到节点上，其点击操作可以打开应用、通知面板或应用抽屉；注意 `adb shell input` 的点按会绕过 TalkBack 的触摸浏览，因此无法用脚本模拟真实的读屏触摸）。标签会跟随 Android 的字体大小设置。Shell 跟随 Android 的深色主题，并绘制在透明的系统栏之下；通知面板中的深色模式磁贴会覆盖外观设置，直到系统设置下一次变更。桥接层的失败原因会以通俗的句子呈现给用户（见 `src/android_integration.rs` 中的 `result_copy`），而不是原因代码。

## 系统应用

News、Photos、Maps、Camera 和 Mail 都是隔离运行的脚本应用（[ADR 0004（英文）](docs/adr/0004-system-apps-are-contained-script-apps.md)）。它们的应用包位于 OctoSense-System-Apps（`apps/<name>/bundle/`，由 `native-apps.lock.json` 固定版本）；`system-apps.json` 指定本 Home 附带哪些应用，并挂载由 Home 自有的素材（Photos 的示例图库 `apps/photos/resources/photos`）。无论是在独立的 Home 中还是在 ROM 中，App Hub 的 Card 运行器都会按照各应用清单中的策略，在各自独立的 isolate 中运行它们。每个应用都保留简短的启动器 id（`os.news` 对应 `news`），因此图标、磁贴和程序坞都不受影响。

Mail 通过 `mail` 宿主服务（OctoSense-System-Apps 中的 `apps/mail/host-service`）收发邮件：用户在宿主自己的面板上登录，密码保存在钥匙串中或由 Android Keystore 密钥保护，应用本身从不持有套接字或密码。使用演示邮箱（密码为 `demo`）：

```sh
# desktop, from home/
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release --features mobile-only
# phone
adb shell am start -n <package>/.MakepadApp --es makepad.APP_CONFIG '{"mail_demo":true}'
```

`app-news`、`app-photos` 和 `app-maps` 会链接早期的原生模块来替代对应的脚本应用，用于在脚本应用完成真机测量之前进行对比；相关说明见 [docs/photos.md（英文）](docs/photos.md) 和 [docs/maps.md（英文）](docs/maps.md)。Mail 和 Camera 已不再有原生模块。

## App Hub

App Hub（`apphub`）用于浏览已签名的 OctoSense 应用目录、搜索、查看应用详情、安装经过验证的应用包，并维护已安装应用的应用库。已安装的应用在隔离的 Card 实例（`card`）中打开，并在启动器和最近任务中单独显示。两者都来自 App Hub 的共享 Shell crate `octosense-app-hub-app`（OctoSense-App-Hub 中的 `crates/app-hub-app`），由默认的 `app-hub` feature 链接，且包含在所有移动端构建中。**预览目录**开关会在线上目录为空时显示内置应用。

参见该 crate 在固定版本下的 [README（英文）](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/4605128d46fb982828d8198e0d71d62a39c7d6d6/crates/app-hub-app/README.md) 以及 [原生设计依据（英文）](docs/design/app-hub/README.md)。应用开发者可从 [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) 开始。

## 在桌面电脑上运行

同一个 Shell 以手机尺寸的窗口运行，支持 Metal、DirectX 或 OpenGL：

```sh
cargo run --release --features mobile-only
cargo run --release --features mobile-only -- --test-action island:demo --test-action capture:/tmp/shell.png
```

`--test-action` 用于注入测试夹具（`island:demo`、`island:expand`、`page:<n>`、`ask-appcard:<text>`、`launch-<app id>`、`taps:<x>,<y>@<s>`），`capture:<path>` 每 5 s 写出一次当前呈现的帧，因此脚本化运行无需屏幕也能检查。在无法传入命令行参数的场合，可用 `MAKEPAD_APP_CONFIG='{"test_actions":[...]}'` 传入同样的列表。直接执行 `cargo run` 得到的是桌面仓库中的通用桌面 Shell；它在这里仍能构建，但不是本仓库的产品。它启动时使用 **OctoSense Light** 风格及其内置壁纸；Omarchy 等其他风格仍可在风格菜单中选择。

## 性能

在 OnePlus 6（Android 15、Adreno 630、60 Hz）上的目标是：所有 Shell 转场都达到 **≥ 55 fps，且 p95 帧间隔 ≤ 20 ms**，空闲屏幕大约每秒只呈现一次。截至 2026 年 9 月 16 日，通知面板（打开/关闭）、页面切换、分组打开/关闭、最近任务双向切换（空列表和有内容时）以及 AppCard 打开，在热启动和全新进程的测试组中均已达标；不过原生 SystemUI 仍没有早期跳帧，而我们有少数场景仍会出现。剩余早期跳帧经测量的原因是 GPU 的 DVFS 下限（手势开始后约 120 ms 内为 257 MHz），因此工作准则是：一帧转场在 710 MHz 下的 GPU 开销必须 ≤ 约 4.5 ms。未经修改的 Vulkan 后端更慢（它会让 CPU 和 GPU 串行执行，频率也始终升不上去），不是达成目标的途径。

使用以下手机工具进行测量：

- `scripts/measure_android_frames.py`：针对一次注入的手势采集 SurfaceFlinger 的呈现时间戳；如果启动应用时带上 `--es makepad.TRACE phone.frames`，还会与 Shell 的标记（logcat 中的 `[phone.frames]`、`[phone.input]`、`[phone.scene]`）关联起来。
- 测试台 `target/perf-artifacts/` 中的辅助脚本（`run_cases.py` 用于运行场景测试组，`kgsl_gpu_timeline.py` / `kgsl_frames_summary.py` 用于从 kgsl ftrace 中得出每帧的 Adreno GPU 执行时间和频率），说明见 [docs/android/perf-gap-analysis.md（英文）](docs/android/perf-gap-analysis.md)。
- 在状态栏电池图标上快速点按三次可开关设备端帧监视器；在时钟上快速点按三次可推送实时岛演示，仅限测试台运行。

记录：[docs/android/](docs/android/README.zh-CN.md)（差距分析、计划、启动器计划、验证日志、Vulkan 探测）以及较早的 [docs/perf-mobile-shell.md（英文）](docs/perf-mobile-shell.md)。

## 目录结构

- `src/mobile*.rs`：手机 Shell。状态与导航（`mobile.rs`）、手势识别器（`mobile_gestures.rs`）、绘制桌面、应用抽屉、键盘和浮层的界面（`mobile_surface.rs`），以及页面、磁贴、分组、通知面板、实时岛、思考中的章鱼和性能监视器。
- `src/apps.rs`：本构建链接哪些模块，系统应用和已安装应用如何作为启动器条目出现，以及各自的托管方式。
- `src/desk/phone.rs`：desk 的手机端合成：托管应用的截取、保留的桌面场景及其模糊金字塔，以及合成器路径。
- `resources/android/AndroidManifest.xml.template`：activity 定义（Home 角色、分享和深度链接 intent）。
- `resources/icons/apps/<style>/`：本 Shell 为 News 和 OctosMap 自带的图标，每种框架风格一个 64x64 的 SVG，由 `python3 tools/build_app_icons.py` 生成（`--sheet <path>` 还会用 `rsvg-convert` 渲染一张审阅图）。渲染器不支持裁剪路径、蒙版、滤镜或文字，因此图形在设计上就不会超出磁贴；有一项测试负责确保文件满足这一点。
- `apps/appcard`：托管 AppCard 助手（`octos-app`，一个指向 `../.sources/system-apps/apps/appcard/app/app` 的路径依赖）。
- `apps/reference`：参考模块。
- `apps/news`、`apps/photos`、`apps/maps`：用于对比的原生模块（feature 分别为 `app-news`、`app-photos`、`app-maps`）。它们的设计说明位于 `docs/plans/`。
- `android/`：System Bridge、契约、Quickstep 和 SystemUI 项目（[android/README.md](android/README.zh-CN.md)）。
- `docs/`：记录和操作指南；`docs/adr/` 存放 Home 的决策记录；`docs/android/` 存放性能和启动器相关记录。

## 依赖

- 框架：由 `native-runtime.lock.json` 选定的 Octoscript-Makepad 发布版本；其 `runtime.json` 固定了 Makepad 和 OctoScript 的版本。Cargo 的 `[patch]` 段把所有 Makepad crate 都解析到 `../.sources/makepad`，因此依赖图中只有一套 widgets/platform/script。不要换成会变动的分支。该分支与上游 Makepad 的关系以及如何更新固定版本，见 [docs/makepad-fork.md（英文）](docs/makepad-fork.md)。
- App Hub：`octosense-app-hub-app` 及其后端 crate，固定在同一个修订版本（`Cargo.toml` 中的 `[patch]` 说明了 `www.github.com` 别名的用途）。
- OctoSense-System-Apps（`native-apps.lock.json`）：系统应用包、Mail 宿主服务以及 `octos-app`，后者从 `octos-org/octos` 的某个固定修订版本引入 octos。
- AppCard 内核不是 Cargo 依赖：`liboctos.so` 在构建 APK 时通过 `MAKEPAD_ANDROID_EXTRA_LIBS` 打包进去（[docs/android-appcard-build.md（英文）](docs/android-appcard-build.md)；其中的固定版本早于当前版本）。没有它时，AppCard 磁贴会回退到 WebSocket 传输和登录界面。

## 测试与状态

`cargo test --features mobile-only mobile -- --test-threads=1` 运行 Shell 的单元测试（手势、页面、实时岛、通知面板、分组、磁贴）。完整的 CI 测试集见 [根目录 README](../README.zh-CN.md#测试与验证)。`scripts/smoke.py` 在 `MAKEPAD_REMOTE` 下启动发布版构建，并通过 HTTP 驱动它。[docs/validation.md（英文）](docs/validation.md) 和 [docs/android/validation-record.md（英文）](docs/android/validation-record.md) 记录了真机验证情况。

状态数据在桌面电脑上保存在 `~/.octosense` 下，在 Android 上保存在应用的数据目录中；可通过 `OCTOSENSE_HOME` 改变其位置。
