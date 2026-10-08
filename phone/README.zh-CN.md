# OctoSense Home

[English](README.md) | 简体中文

**初次阅读源码**？先读[桌面、Home、ROM 与系统应用导读](../desktop/docs/code-walkthrough.zh-CN.md)，再读 [Agent 与 Tokio 导读](../docs/architecture-walkthrough.zh-CN.md)。前者追踪启动、原生托管、脚本 bundle、应用数据和 Android 平台边界。

> **在整个系统中的位置**。在手机上，OctoSense 托管的原生模块和脚本应用在 Home 进程内运行（不使用桌面式进程托管）；普通 Android 应用仍在各自的 Android 进程中运行。octos 内核在 Android 上是 APK 中作为子进程运行的 `liboctos.so`，在 OpenHarmony 上是进程内的任务，在 iOS 上没有。应用仍然只能通过 Shell 使用自己的 Agent。进程、应用 Agent 的两条通道以及一次带审批的工具调用的图示：[整体如何运作](../README.zh-CN.md#整体如何运作)；详细说明：[docs/architecture.zh-CN.md](../docs/architecture.zh-CN.md) 和 [ADR 0004（英文）](../docs/adr/0004-native-apps-hosting-and-peers.md)。

OctoSense 手机 Shell：一个 Makepad 应用，也就是设备的桌面。它包括带实时磁贴和应用组合的桌面页面、手势层、通知面板（左侧通知，右侧控制）、最近任务、用于展示进行中活动的实时岛，以及在进程内绘制于磁贴中的托管应用：App Hub 及其运行的应用、系统应用、Reference 和 Sheets、Makepad 应用 Calculator、Clock、Notes、Reminders 和 Weather（各自有自己的 Agent），另外还有作为服务的 octos Agent 内核。（AppCard 目前不随产品发布，只有使用 `--features app-appcard` 时才会链接。）

Home 是本仓库的三个产品之一（环境准备、目录结构和 CI 见[根目录 README](../README.zh-CN.md)）。把它与具有特权的系统部分一起预装的 ROM 镜像位于 [`rom/`](../rom/README.zh-CN.md)。

## 与桌面端 Shell 的关系

2026 年 9 月 15 日，Home 从桌面端 Shell 中拆分出来，拆分点是其移动端 Shell 系列提交的最新位置。此后两份副本分别位于 OctoSense-Desktop 和 OctoSense-ROM（已停用，并入本仓库；位于 `home/`），直到 2026 年 9 月 27 日两个仓库并入本仓库（[ADR 0001（英文）](../docs/adr/0001-one-octosense-repository.md)）。此后 Shell 只有一份，位于 [`crates/shell`](../crates/shell)（包名 `octosense-shell`），两个包都链接它；本包只加上入口（`src/main.rs`，一个包装 Shell `App` 的 `App`）和内置设置应用（`src/settings_*.rs`、`src/android_settings.rs`）。手机版构建是 `mobile_only` 配置：在 Android 上由 `build.rs` 开启，在其他平台上由 `--features mobile-only` 开启。`upstream/makepad.json` 记录了哪些窗口管理器文件是从 Makepad 导入的；`scripts/upstream.py` 负责比较并合并这些文件（[docs/upstream.md（英文）](docs/upstream.md)）。

## 构建与运行

先在仓库根目录准备一次锁定版本的源码（`python3 tools/setup.py`，见[根目录 README](../README.zh-CN.md#环境准备)），然后在本目录中运行 cargo：`phone/.cargo/config.toml` 会选择手机端的系统应用。

**在桌面电脑上**，以手机尺寸的窗口运行手机 Shell，包含 App Hub、六个系统应用和内置设置（CI 中只构建 macOS）：

```sh
cargo run --release -p octosense-home --features mobile-only
```

| 开关 | 作用 |
| --- | --- |
| `--features mobile-apps` | 同时链接原生模块 Reference 和 Sheets；不包括 AppCard |
| `--features app-appcard` | 同时链接 AppCard 助手，它目前默认不随产品发布 |
| `-- --module <id>` | 在进程内托管已链接的模块，而不是作为子进程 |
| `-- --test-action <name>` | 启动时触发一个 Shell 动作（见[在桌面电脑上运行](#在桌面电脑上运行)） |
| `MAKEPAD_WM_TEST_APP=<app>[:<count>]` | Shell 启动后打开某个应用（可指定次数） |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | 用演示邮箱提供 Mail（密码为 `demo`） |
| `OCTOSENSE_HOME=<dir>` | 把状态数据保存在 `~/.octosense` 以外的位置 |

**Android APK。** `rom/scripts/build-home.sh`（`build-home.py` 的包装脚本）构建 Home APK 及其 System Bridge APK，一起签名，把 octos 内核作为 `liboctos.so` 打包进去，并写出 `OctoSenseHome.apk`、`OctoSenseBridge.apk` 和一份 `build.json` 回执。它从不安装或刷机。在仓库根目录构建一对使用 Makepad 开发密钥签名的独立开发版：

```sh
cargo build --release --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
rom/scripts/build-home.sh --variant standalone --development \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk \
  --packager .sources/makepad/target/release/cargo-makepad
```

需要一个 `cargo-makepad` 的 Android SDK/NDK 目录（`cargo-makepad makepad android --sdk-path=<dir> install-toolchain`）、带 platform 35 和 build-tools 35.0.0 的 Android SDK、完整的 JDK 17+ 以及 Gradle 8.11.1；脚本不会安装其中任何一项。加上 `--dry-run` 可打印构建计划；发布版请把 `--development` 换成指向现有签名者的 `--sign-key` 和 `--sign-cert`，签名文件放在检出目录之外。请使用锁定版本的打包工具，而不是上游的：它带有本应用的 Java activity（[docs/build-tool.md（英文）](docs/build-tool.md)）。签名、回执和 ROM 变体见 [rom/docs/home-build.md（英文）](../rom/docs/home-build.md)。

**包名。** Home 的应用 ID 是 `dev.makepad.octosense`，独立版和 ROM 版相同。运行 OctoSense ROM 的手机上已经有这个 ID，并由平台密钥签名，因此开发版无法替换它。要在旁边安装测试版，请在 `phone/` 中以另一个包名直接调用打包工具：

```sh
../.sources/makepad/target/release/cargo-makepad makepad android \
  --sdk-path=/path/to/makepad-android \
  --package-name=dev.makepad.octosense.scriptapps \
  build -p octosense-home --release
```

把 `build` 换成 `run` 会同时安装并启动它；用它自己的名字访问，例如 `adb shell am start -n dev.makepad.octosense.scriptapps/.MakepadApp`。

清单中的 `dev.makepad.android.APPLICATION_EXTENSION` 元数据让应用 ID 改变后
仍使用同一个 Java 集成类。准备好运行时补丁后，应重新构建锁定版本的打包工具；
旧加载器可能忽略此设置，导致自定义包名的测试版未加载 Android 返回操作及其他
平台集成功能。安装测试版后，打开 News 或 Photos，检查从屏幕边缘向内滑动返回
是否回到 OctoSense Home。

**OpenHarmony：** `python3 rom/scripts/build-home-ohos.py --deveco-home ... --packager ... --signing-config ...` 使用现有的 DevEco 签名配置构建普通的 OpenHarmony 应用（[rom/docs/home-build.md（英文）](../rom/docs/home-build.md#openharmony-home)）。**iOS 模拟器：** 在 `phone/` 中运行 `../.sources/makepad/target/release/cargo-makepad makepad apple ios --org=dev.makepad --app=octosense run-sim -p octosense-home --features mobile-only`。两者都不在 CI 中构建。

## 应用图标

Home 与桌面包使用相同的绿色八瓣 OctoSense 标识：

| 平台 | 打包的图标资源 |
| --- | --- |
| Android | 五种传统启动器图标密度、自适应前景/背景层，以及 Android 13 主题图标使用的单色层；常规清单直接引用这些资源 |
| iOS/iPadOS | `packaging/ios/icons/Assets.xcassets`，包含尺寸正确的 iPhone、iPad 和 App Store PNG；不透明方形图片由 iOS 添加圆角遮罩 |
| OpenHarmony | `ohos/icons/`，由 `rom/scripts/build-home-ohos.py` 覆盖锁定模板中的应用、分层启动器和启动窗口图标 |
| 桌面预览 | `resources/icon_*.png`、`icon.ico`、`icon.icns`，以及工作区的 Cargo 图标环境配置 |

Android 前景位于平台的[自适应图标安全区](https://developer.android.com/develop/ui/compose/system/icon_design_adaptive)内。
iOS 资源目录直接提供 [Apple 应用图标规格](https://developer.apple.com/library/archive/documentation/Xcode/Reference/xcode_ref-Asset_Catalog_Format/AppIconType.html)要求的尺寸，
避免 Makepad 的回退逻辑在所有尺寸复用同一张图片。
常规构建使用已提交的资源；重新生成和检查方法见[应用图标](../desktop/README.zh-CN.md#应用图标)。

Android 资源和 iOS 资源目录已通过本机平台工具编译；OpenHarmony 模板替换有打包回归测试。
Home 已以 `OctoSense Icons`（`dev.makepad.octosense.icontest`）在运行 Android 17
的 Pixel 7 Pro 上全新安装：应用信息页显示品牌自适应图标，启动后 Home 页面正常渲染。
原有安装及默认 Pixel Launcher 均保留。Android 主题图标模式、iOS/OpenHarmony
安装后的外观及完整 OpenHarmony HAP 构建仍**未经验证**。

`OctoSense Icons` 仅是该独立测试安装通过命令行指定的名称。
正常 Android/iOS/桌面构建仍名为 **OctoSense**，OpenHarmony 保留原有的
**OctoSense Home** 名称。

## Home 角色

该 activity 声明了 `HOME` intent 过滤器，并且是 `singleInstance`。在你能控制的设备上执行：

```sh
adb shell cmd package set-home-activity dev.makepad.octosense/.MakepadApp
```

或者在 Android 的桌面选择器中选择 OctoSense。之后按下 Home 键或使用 Home 手势时，正在运行的 Shell 会收到 `Event::HomeIntent` 并显示桌面页面。

安装或选择 Home 后，请先通过 Android 的 Home 键或手势进入桌面，再测试侧边滑动。
使用 `am start` 显式启动 activity，可能会让已经选为默认桌面的应用仍处于普通任务中。
这时 Android 会把侧边手势排除区域限制为 200 dp，导致只有靠近底部的一小段区域
可以正常翻页。真正的 Home 任务会在整个页面内容区保留左右边缘用于翻页；进入应用、
展开卡片、显示键盘或浮层后，会把边缘交还给返回手势。
此行为已在 OnePlus 6 / Android 15 上验证。

系统仍保留底部手势区域、最近任务（上滑并停顿）及状态栏通知面板。
三按钮导航是这些系统手势的替代模式，Home 的侧边翻页不要求切换到三按钮导航：

```sh
adb shell cmd overlay enable-exclusive --category com.android.internal.systemui.navbar.threebutton
```

（`…navbar.gestural` 可恢复手势导航。）特权路线（接管手势区域和最近任务）的工作量评估见 [docs/android/launcher-plan.md（英文）](docs/android/launcher-plan.md)，尚未开始。

## 所有应用共用一个 Glance 页面

从 Home 的第一页右滑，进入共享 Glance 信息流。Mail、Calendar、News 以及其他
获得 Glance 能力的应用，都向同一个宿主卡片库发布。每张卡片保留发布应用、账户
和条目身份；点击摘要后，打开该卡片的工作区。Card/Chat 连接发布应用自己的代理，
Mail 的 Email/Chat 则操作同一份已保存的回复草稿。退出工作区后回到共享信息流。
Mail 不需要独立的 Glance 应用。

信息流按优先级和时间排序，可以滚动浏览所有保留卡片。只绘制可见摘要；屏幕外
卡片不会运行生成 UI。不再限制每应用四张卡片，也不再将手机信息流截断为六张。
负载预算与到期规则独立控制存储，不取决于滚动视口。负载紧张时旧卡片可能被
淘汰；已保存的 Mail 草稿继续保留在 Mail 服务中。

带宿主绑定的 Mail 回复，在已保存发送回执确认 SMTP 接受后自动退出 Glance。
草稿和回执继续保存；当前成功结果界面保留至用户返回。打开、编辑、取消以及发送
失败或结果不明都不算完成。重启或恢复通知不会让已接受的回复卡片重新出现。
新收到的邮件仍经过重要性筛选，可以发布各自的卡片；目前尚未按整个邮件会话合并
卡片或聊天。其他应用继续使用原有完成规则。见
[完成状态验证](../docs/testing/mail-completion-2026-10-05.zh-CN.md)。

独立 Android 测试包是 Shell 的另一份安装，各自拥有私有账户和卡片，不会向正常
Home 贡献卡片。启动器验收必须从 Android 实际指定的 Home 开始，右滑进入 Glance，
再打开不同应用发布的卡片；直接打开测试包不能证明这条路径已经接通。

## 手势

| 位置 | 手势 | 作用 |
|---|---|---|
| 桌面页面中部 | 下拉 | 打开搜索，输入框已获得焦点，键盘随即弹出 |
| 桌面页面右侧四分之一 | 下拉 | 通知面板的控制页（Wi-Fi、亮度等） |
| 桌面页面左侧四分之一 | 下拉 | 通知面板的通知页 |
| 顶部边缘左侧 / 右侧 | 下拉 | 同样打开通知 / 控制 |
| 桌面页面（Android 包括左右边缘） | 左右滑动 | 切换页面：概览 ⇠ 应用 ⇢ 应用库 |
| 应用库 | 拖动 | 滚动网格；超出两端时会拉伸并回弹（返回或 Home 可关闭） |
| 应用库或搜索 | 在内容区向右滑动 | 回到离开时的桌面页面并收起键盘 |
| 底部条带（位于系统条带之上） | 上滑 / 停顿 / 左右滑动 | 回到桌面 / 最近任务 / 快速切换 |
| 应用或已展开卡片的左右边缘 | 向内滑动 | 返回；Android 桌面的左右边缘用于翻页 |
| 应用图标 | 长按 | 添加到桌面或从桌面移除、添加到程序坞、应用信息、卸载 |
| 桌面页面图标 | 长按后拖动 | 重新排列页面（放到图标之间）、放入程序坞（放到程序坞上）、创建文件夹（放到另一个图标上）或加入文件夹（放到文件夹磁贴上） |
| 应用组合磁贴 | 长按 | 更换其中任一应用，或移除该组合 |
| 文件夹磁贴 | 长按 | 移除其中一个应用，或移除整个文件夹 |
| 应用磁贴 | 长按 | 移除该磁贴（桌面菜单中的“显示隐藏的磁贴”可将其恢复） |
| 桌面空白处 | 长按 | 小组件、浅色/深色外观、网格：4 列或 5 列、下拉方式（启动器通知面板或系统级面板）、系统设置、显示隐藏的磁贴 |

Android 托管应用在底部预留 48 个逻辑像素的导航栏，提供桌面和最近任务图标；应用的绘制与输入区域不包含这条栏。键盘显示时导航栏隐藏，把空间归还编辑器；Android 系统返回键仍可使用。桌面和 OpenHarmony 保留现有悬浮控制。启动与恢复时，旧 GPS 缓存不再主动申请定位权限，只在已经授权时更新；Maps 仅在点按 **Your location（你的位置）** 后申请定位。该操作要求应用具备定位授权，并使用 Android 标准权限流程；读取 GPS 缓存和启动均不触发申请。真实定位到达前，Maps 保留原来有名称的起点，并提示可以另选起点。原位切换主题时，已有标签（包括打开界面后才填入的邮件正文）仍保持换行，应用明确指定的单行布局不受影响。这些后续改动仍需重新进行设备验收。

在 Android 上，顶部边缘归系统所有：开启系统级 OctoSense 面板时（OctoSense ROM），它打开该面板，此时在桌面两侧下拉会打开搜索；否则打开 Android 自己的面板。OpenHarmony 上没有通知面板。

下拉手势在拉到 40 % 时即生效（在 1080 像素宽的手机上约为 135 px）；导航类滑动则需要滑完整段距离或快速轻扫。下拉过程中页面会变暗，搜索框随手指从底部升起；手势生效时会有一次短促的触感反馈。每个隐藏手势在首次使用之前，桌面页面都会显示一行对应提示（`crates/shell/src/mobile_hints.rs`；Android 会记住哪些提示已经看过）。在已停稳的桌面页面上再次按下 Home，会回到主页面。

搜索只能通过在桌面上下拉打开；应用库没有搜索栏。与 iOS 一样，搜索框位于底部、键盘上方；未输入时列表为空，每输入一个字符就会缩小结果：应用名称或其中某个词（词内的大写字母也算词首，因此 "tube" 能找到 YouTube）以输入内容开头即为匹配，不区分大小写和重音。名称以输入内容开头的应用排在前面，按回车即可打开最佳匹配。在应用库中，右侧的字母栏可快速跳转网格；获得使用情况访问权限后，顶部会显示一行“建议”，列出最近使用的应用。应用在通知面板中有通知时，其图标会带一个圆点。最近任务以卡片形式列出托管应用；在 Android 设置中授予使用情况访问权限后（最近任务中的卡片可打开该设置），还会显示一行最近使用过的 Android 应用。每个可点按区域都是带有语音标签的无障碍节点，因此 TalkBack 和 UI 自动化都能读取并操作 Shell（已在安装 TalkBack 的情况下以及通过 UiAutomation 探针验证：无障碍焦点能落到节点上，其点击操作可以打开应用、通知面板或应用抽屉；注意 `adb shell input` 的点按会绕过 TalkBack 的触摸浏览，因此无法用脚本模拟真实的读屏触摸）。标签会跟随 Android 的字体大小设置。Shell 跟随 Android 的深色主题，并绘制在透明的系统栏之下；通知面板中的深色模式磁贴会覆盖外观设置，直到系统设置下一次变更。桥接层的失败原因会以通俗的句子呈现给用户（见 `crates/shell/src/android_integration.rs` 中的 `result_copy`），而不是原因代码。

Glance 保持紧凑的发布摘要。点按后，选中的卡片在信息流上方展开为常驻的全屏工作区；摘要矩形只作为动画起点，不再决定工作区大小，信息流条目也不增高。其余卡片被覆盖且不能接收输入。**Email / Chat** 位于同一行。Email 把主要空间留给可编辑正文。**Original / Details / Review** 共用一行紧凑操作栏；Details 展开收件人和主题编辑，Review 打开准确邮件内容的批准界面。原邮件与其头部信息一起滚动。文字选择使用柔和蓝灰色高亮，不再使整个编辑框变暗。横屏键盘使可用高度过小时，正文编辑临时收起标题、页签与操作栏；关闭键盘后恢复。Chat 使用虚拟化记录和键盘上方的固定输入框；模型修改实际保存后，显示 **View updated email** 并读取同一份权威草稿。返回键依次关闭键盘、审核、工作区。回到概览后，同一进程内保留草稿、未发送聊天文字、页面和控件滚动位置；缓存三个非活动的干净工作区，未发送的人类输入不参与干净缓存淘汰。账户失效或发布撤销时退出对应工作区。草稿持久保存，但不承诺进程退出后保留未发送聊天输入。展示切换不启动新的 Activity、应用、agent 或模型生成。设备证据及已撤回的 UX 评分见 [Mail 工作流](../docs/mail-composable-cards.zh-CN.md)。

Calendar、News、Photos、YouTube 以及 Finance 原型卡片均使用同一套工作区。声明了 agent 的应用，即使发布的 L0 或 Splash 界面没有内嵌 `sys.chat`，也会获得原生 **Card / Chat** 页签。宿主将补充会话绑定到发布者、账户和卡片，把发布数据与当前 L0 状态作为有大小限制的不可信上下文传给该应用的 agent。已有的显式会话保留其线程。聊天不新增工具，也不授权外部操作；没有 agent 的应用不显示 Chat。Home 的系统应用列表现已包含 Calendar；Finance 仍不是内置系统应用。

L0 的本地修改和已交互的 Splash 实例也不参与干净缓存淘汰。仅数据变化的 L0 重新发布保留本地状态；如果工作区有本地修改或未发送聊天，新的布局会暂缓替换。此时信息流可显示更新摘要，驻留工作区保留原交互快照，直到发布撤销、到期或账户失效。这是进程内保留，并非通用的应用状态持久化服务。Mail 的保存草稿、修改工具和亲手点按的发送批准仍是 Mail 专属能力；已安装的 Gmail 应用通过 `gmail` 服务获得自己的一套（见 [OAuth 指南](../crates/oauth-service/README.zh-CN.md)）。

## 内置设置

在应用目录中打开 **OctoSense Settings**，可使用共享主题、受支持的显示与声音控制以及设备信息。它的 Octoscript–Makepad 界面会跟随实时的主题和字体大小变化，同时保留当前页面。导航、搜索、草稿、审阅和应用事件处理都在 [Octoscript 控制器](resources/settings/controller) 中执行；原生代码负责渲染、文本输入和有类型的 Android 绑定。参见 [移植设计与验证状态（英文）](../docs/adr/home/0005-settings-octoscript-controller.md)。完整替代系统设置的工作仍在进行中，部分区域仍会打开 Android 设置。参见 [当前控制项与验证（英文）](docs/android/settings.md)、[功能对齐清单（英文）](docs/android/settings-parity.md) 和 [架构决策（英文）](../docs/adr/home/0006-builtin-settings.md)。

## 系统应用

News、Photos、Maps、Camera、Mail、AI 提供商和 YouTube 都是隔离运行的脚本应用（[ADR 0004（英文）](../docs/adr/home/0004-system-apps-are-contained-script-apps.md)）。它们的应用包位于 [`apps/`](../apps/README.zh-CN.md)（`apps/<name>/bundle/`）；本目录的 `system-apps.json` 指定本 Home 附带哪些应用，并挂载由 Home 自有的素材（Photos 的示例图库 `apps/photos/resources/photos`）。无论是在独立的 Home 中还是在 ROM 中，App Hub 的 Card 运行器都会按照各应用清单中的策略，在各自独立的 isolate 中运行它们。每个应用都保留简短的启动器 id（`os.news` 对应 `news`），因此图标、磁贴和程序坞都不受影响。

Android 上，已启用的 Mail 代理还会使用静默且要求联网的后台任务，约每 15 分钟一次，Android 可能延迟执行。无需打开 Home 即可收取并评估邮件。只有模型决定发布且 `notify: true` 的卡片才发出原生通知；点击会恢复原始账户的卡片。重要性偏好由系统代理配置，普通邮件会跳过。强行停止后，必须重新打开应用才会恢复任务。见[邮件事件](../docs/mail-agent-events.zh-CN.md)及 [ADR 0008](../docs/adr/0008-quiet-android-mail-jobs.zh-CN.md)。

Mail 通过 `mail` 宿主服务（[`apps/mail/host-service`](../apps/mail/host-service)）收发邮件：用户在宿主自己的面板上登录，密码保存在钥匙串中或由 Android Keystore 密钥保护，应用本身从不持有套接字或密码。使用演示邮箱（密码为 `demo`）：

```sh
# desktop, from phone/
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense-home --features mobile-only
# phone
adb shell am start -n <package>/.MakepadApp --es makepad.APP_CONFIG '{"mail_demo":true}'
```

早期的原生 News、Photos 和 Maps 模块已删除（原生应用 ADR 0004 §1，[#113](https://github.com/OctoSense-org/OctoSense/pull/113)）；所有系统应用都不再有原生模块。

### octos 内核

octos Agent 内核是 Home 的一项服务，不依附于任何应用：`octosense-kernel`（[`crates/kernel`](../crates/kernel)），feature `octos-core`（默认开启，Android、iOS、OpenHarmony 构建总是开启）。Home 在启动时通过 Shell 的 AI 服务（[`crates/ai-host`](../crates/ai-host/README.md)）用自己的数据目录启动它，在有使用方连接之前不运行任何东西。之后每个进程只有一个内核：Android 上从 APK 原生库目录运行 `liboctos.so serve --stdio`（`rom/scripts/build-home.sh` 构建的每个 APK 都带有它），OpenHarmony 上在进程内运行，桌面上运行 `OCTOS_APP_CORE_BIN` 指定的二进制（未指定则没有），iOS 上没有。它的 core 目录在手机上是 `<数据目录>/octos-home/.octos`，在桌面上是 `OCTOS_APP_CORE_DIR`，否则 `~/octos-home/.octos`。AI 提供商应用负责配置它（见下文）；AppCard（需显式开启）以及之后的 Rinx 连接并共享它；最后一个使用方离开或 Home 关闭时内核停止。不带内核服务构建（仅桌面）：`--no-default-features` 加上需要的 feature，例如 `--features app-hub`。

**Talk to Octos**（默认关闭）：在 **AI providers → Talk to Octos** 中打开后，本机会启动一个仅监听回环地址的服务，让 Web 客户端或终端界面与本设备的助手对话。开启期间内核以 `octos serve --host-managed` 代替 `--stdio` 运行，原生应用继续通过其 WebSocket 工作；外部客户端使用单独的令牌，只能打开 UI Protocol 套接字。Web 客户端通过一次性配对码或其链接的二维码配对；本用户的终端客户端读取私有连接文件。原生应用关闭后服务仍保持运行，直到关闭该功能或 Shell 退出。见 [ADR 0003（英文）](../docs/adr/0003-shared-octos-client-access.md) 和[内核指南](../crates/kernel/README.zh-CN.md)。

### AI 提供商

AI 提供商（`os.ai-providers`）通过 `llm` 宿主服务（[`apps/ai-providers/host-service`](../apps/ai-providers/host-service)）编辑 octos 内核的 LLM 提供商，Home 在启动时通过 [`crates/ai-host`](../crates/ai-host/README.md) 注册该服务：

- 它写入的是内核的 profile，`<core 目录>/profiles/_main.json`（手机上是 `<数据目录>/octos-home/.octos`；`OCTOS_APP_CORE_DIR` 可以覆盖）；在 Android 上密钥就保存在这个应用私有的 profile 中，因为 octos 从这里读取；
- 在 Android 上，导入面板可以用相机**扫描**提供商二维码（Makepad 的 `cx.show_qr_scanner()`，即 makepad#31，自 `d0a9def5` 起包含在运行时中），也可以从**选择的图片**中读取（`QrImagePickActivity`：系统图片选择器，字节通过 `qr.image.result` 数据包中的私有缓存文件交付）；其他平台上粘贴代码；
- 每次更改后，服务会重启正在运行的内核：使用方会重新连接到读取新 profile 的内核（AppCard 保留窗口和会话；正在进行的请求会失败并提示“the octos kernel restarted”）。

## App Hub

App Hub（`apphub`）用于浏览已签名的 OctoSense 应用目录、搜索、查看应用详情、安装经过验证的应用包，并维护已安装应用的应用库。已安装的应用在隔离的 Card 实例（`card`）中打开，并在启动器和最近任务中单独显示。两者都来自 App Hub 的共享 Shell crate `octosense-app-hub-app`（OctoSense-App-Hub 中的 `crates/app-hub-app`），由默认的 `app-hub` feature 链接，且包含在所有移动端构建中。**预览目录**开关会在线上目录为空时显示内置应用。

参见该 crate 的 [README（英文）](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/crates/app-hub-app/README.md)（阅读根 `Cargo.toml` 选定的版本）以及[原生设计依据（英文）](docs/design/app-hub/README.md)。应用开发者可从 [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)（原 Design Flow）开始。

## 在桌面电脑上运行

同一个 Shell 以手机尺寸的窗口运行，支持 Metal、DirectX 或 OpenGL：

```sh
cargo run --release -p octosense-home --features mobile-only
cargo run --release -p octosense-home --features mobile-only -- --test-action island:demo --test-action capture:/tmp/shell.png
```

`--test-action` 用于注入测试夹具（`island:demo`、`island:expand`、`page:<n>`、`ask-appcard:<text>`、`launch-<app id>`、`taps:<x>,<y>@<s>`），`capture:<path>` 每 5 s 写出一次当前呈现的帧，因此脚本化运行无需屏幕也能检查。在无法传入命令行参数的场合，可用 `MAKEPAD_APP_CONFIG='{"test_actions":[...]}'` 传入同样的列表。不带 `mobile-only` 时，这个 package 启动的是通用桌面 Shell；桌面端产品是 [`desktop/`](../desktop/README.zh-CN.md)。它启动时使用 **OctoSense Light** 风格及其内置壁纸；Omarchy 等其他风格仍可在风格菜单中选择。

## 性能

在 OnePlus 6（Android 15、Adreno 630、60 Hz）上的目标是：所有 Shell 转场都达到 **≥ 55 fps，且 p95 帧间隔 ≤ 20 ms**，空闲屏幕大约每秒只呈现一次。截至 2026 年 9 月 16 日，通知面板（打开/关闭）、页面切换、分组打开/关闭、最近任务双向切换（空列表和有内容时）以及 AppCard 打开，在热启动和全新进程的测试组中均已达标；不过原生 SystemUI 仍没有早期跳帧，而我们有少数场景仍会出现。剩余早期跳帧经测量的原因是 GPU 的 DVFS 下限（手势开始后约 120 ms 内为 257 MHz），因此工作准则是：一帧转场在 710 MHz 下的 GPU 开销必须 ≤ 约 4.5 ms。未经修改的 Vulkan 后端更慢（它会让 CPU 和 GPU 串行执行，频率也始终升不上去），不是达成目标的途径。

使用以下手机工具进行测量：

- `scripts/measure_android_frames.py`：针对一次注入的手势采集 SurfaceFlinger 的呈现时间戳；如果启动应用时带上 `--es makepad.TRACE phone.frames`，还会与 Shell 的标记（logcat 中的 `[phone.frames]`、`[phone.input]`、`[phone.scene]`）关联起来。
- 测试台 `target/perf-artifacts/` 中的辅助脚本（`run_cases.py` 用于运行场景测试组，`kgsl_gpu_timeline.py` / `kgsl_frames_summary.py` 用于从 kgsl ftrace 中得出每帧的 Adreno GPU 执行时间和频率），说明见 [docs/android/perf-gap-analysis.md（英文）](docs/android/perf-gap-analysis.md)。
- 在状态栏电池图标上快速点按三次可开关设备端帧监视器；在时钟上快速点按三次可推送实时岛演示，仅限测试台运行。

记录：[docs/android/](docs/android/README.zh-CN.md)（差距分析、计划、启动器计划、验证日志、Vulkan 探测）以及较早的 [docs/perf-mobile-shell.md（英文）](docs/perf-mobile-shell.md)。

## 目录结构

- `src/main.rs`：入口：本包的 `App` 包装 Shell 的 `App`（`#[deref] shell`），并加上设置应用的运行时。
- `src/settings_*.rs`、`src/android_settings.rs`、`resources/settings/`：内置设置应用及其 Android 通道。
- `../crates/shell/src/mobile*.rs`：Shell 的手机层。状态与导航（`mobile.rs`）、手势识别器（`mobile_gestures.rs`）、绘制桌面、应用抽屉、键盘和浮层的界面（`mobile_surface.rs`），以及页面、磁贴、分组、通知面板、实时岛、思考中的章鱼和性能监视器。
- `../crates/shell/src/apps.rs`：本构建链接哪些模块，系统应用和已安装应用如何作为启动器条目出现，以及各自的托管方式。
- `../crates/shell/src/desk/phone.rs`：desk 的手机端合成：托管应用的截取、保留的桌面场景及其模糊金字塔，以及合成器路径。
- `resources/android/AndroidManifest.xml.template`：activity 定义（Home 角色、分享和深度链接 intent）。
- `../crates/shell/resources/icons/apps/<style>/`：Shell 为 News 和 OctosMap 自带的图标，每种框架风格一个 64x64 的 SVG，由 `python3 tools/build_app_icons.py` 生成（`--sheet <path>` 还会用 `rsvg-convert` 渲染一张审阅图；迁移后**未验证**：该脚本仍写入 `phone/resources/icons/apps/`）。渲染器不支持裁剪路径、蒙版、滤镜或文字，因此图形在设计上就不会超出磁贴；有一项测试负责确保文件满足这一点。
- `../apps/appcard/module`：托管 AppCard 助手（`octos-app`，位于 `../apps/appcard/app/app`）。在所有目标平台上都需显式启用：`--features app-appcard`（它隐含 `octos-core`；助手连接 Home 的内核）。默认构建、`mobile-apps` 和原生移动端构建不包含 AppCard 界面，但包含内核服务。
- `../apps/reference`：参考模块。
- `android/`：System Bridge、契约、Quickstep 和 SystemUI 项目（[android/README.md](android/README.zh-CN.md)）。
- `docs/`：记录和操作指南；`docs/android/` 存放性能和启动器相关记录。Home 的决策记录（ADR 0001–0006）位于 [`../docs/adr/home/`](../docs/adr/README.zh-CN.md)。

## 依赖

所有外部依赖都只在仓库根目录锁定一次（[根目录 README](../README.zh-CN.md#依赖)）：

- 框架：由 `native-runtime.lock.json` 选定的 Octoscript-Makepad 发布版本；其 `runtime.json` 固定了 Makepad 和 OctoScript 的版本，检出到 `.sources/`，并应用经审查的设置补丁（`runtime-patches.lock.json`、`tools/runtime-patches/`）。所有 Makepad crate 都解析到 `.sources/makepad`，因此依赖图中只有一套 widgets/platform/script。该 fork 与上游 Makepad 的关系以及如何更新固定版本，见 [docs/makepad-fork.md（英文）](docs/makepad-fork.md)。
- App Hub：`octosense-app-hub-app` 及其后端 crate，Home 与 Mail、`llm` 宿主服务使用同一个修订版本。
- 本仓库内按路径依赖：系统应用包和宿主服务（`apps/`）、Shell（`crates/shell`）及其 AI 服务（`crates/ai-host`）、octos 内核服务（`crates/kernel`）、应用与 Agent 之间的代理（`crates/app-peers`）以及 `octos-app`（`apps/appcard`）。octos 本身来自 `octos-org/octos` 的一个固定修订版本：只有 OpenHarmony（进程内内核）和 `app-appcard` 构建把它作为 Cargo 依赖。
- Android 上的内核二进制不是 Cargo 依赖：`rom/scripts/build-home.sh` 按该版本交叉编译 `liboctos.so`，并在构建 APK 时通过 `MAKEPAD_ANDROID_EXTRA_LIBS` 打包进去（`--octos-kernel` 使用预先编译好的内核，`--no-octos-kernel` 不打包；见 [docs/android-appcard-build.md（英文）](docs/android-appcard-build.md)）。没有它时手机上不运行内核；服务商设置仍会保存，已链接的 AppCard 会回退到 WebSocket 传输和登录界面。

## 测试与状态

`cargo test --locked --features mobile-apps -p octosense-shell -p octosense-home` 运行 Shell 的单元测试（手势、页面、实时岛、通知面板、分组、磁贴）以及设置应用的测试。完整的 CI 测试集是 `.github/workflows/phone.yml`（编译 Home 及其内置模块、`tools/check-shell-graph.sh` 中的 Shell 依赖图检查，以及 Shell、Home、AI 服务、App Hub 准入和运行时策略的测试）。`scripts/smoke.py` 在 `MAKEPAD_REMOTE` 下启动发布版构建，并通过 HTTP 驱动它。[docs/validation.md（英文）](docs/validation.md) 和 [docs/android/validation-record.md（英文）](docs/android/validation-record.md) 记录了真机验证情况。

状态数据在桌面电脑上保存在 `~/.octosense` 下，在 Android 上保存在应用的数据目录中；可通过 `OCTOSENSE_HOME` 改变其位置。
