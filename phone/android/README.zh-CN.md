# Android 契约、System Bridge 与 Home 布局服务

[English](README.md) | 简体中文

这些模块实现了 ADR 0001 的第一个原型。`contracts` 为普通的 Makepad Home 客户端导出 AIDL 接口；`system-bridge` 构建配套 APK；Gradle 的 `quickstep` 目标构建经过身份验证的 Home 布局服务。独立的平台构建现已能够编译原生 Quickstep 控制器和最近任务（Recents）。根据用户的 [Trebuchet 移除请求（英文）](../docs/android/trebuchet-removal-record.md)，其 ROM 专用模块已在 OnePlus 6 上启用。重启后，基本的原生导航以及任务恢复/移除均已通过；更全面的转场验证仍未完成。参见 [ADR（英文）](../docs/adr/0001-hybrid-android-launcher-and-system-bridge.md)和[实现记录（英文）](../docs/android/adr-0001-implementation-record.md)。

当前启动器的控制项和权限流程见[当前启动器中的系统功能（英文）](../docs/android/current-launcher-system-integration.md)。原生 Quickstep 构建还提供可选启用的[跨应用 OctoSense 面板（英文）](../docs/android/systemui-shade-replacement.md)，它使用同一个经过身份验证的 System Bridge，同时保留核心 SystemUI。[原生 OctoSense SystemUI 构建（英文）](../docs/android/octosense-systemui-build.md)补上了其余的系统界面分支，并记录了其发布签名关卡。独立的 `systemui-preview` Gradle 模块只在普通的临时应用身份下测试设备控制页面，无法替代 SystemUI。

## 使用已安装的工具构建

固定的依赖版本：Gradle 8.11.1、JDK 17、Android API 35、Build Tools 35.0.0、AGP 8.9.2 和 libsu 6.0.0。启动脚本使用已有的 Gradle 发行版，不会下载 wrapper。SDK 自动安装已禁用。常规构建会校验依赖锁文件和 SHA-256 验证元数据。

在测试台上，经明确批准的工具安装在 `~/.local/share/octosense/android-tools` 下。在本目录中运行：

```sh
export OCTOSENSE_TOOLS=~/.local/share/octosense/android-tools
export JAVA_HOME="$OCTOSENSE_TOOLS/temurin-17.0.20.1/jdk-17.0.20.1+1/Contents/Home"
export OCTOSENSE_GRADLE_HOME="$OCTOSENSE_TOOLS/gradle-8.11.1"
export GRADLE_USER_HOME="$OCTOSENSE_TOOLS/gradle-cache"
./gradlew --offline :contracts:exportHomeContracts \
  :system-bridge:assemblePrototype :system-bridge:lintPrototype \
  :quickstep:assemblePrototype :quickstep:lintPrototype
```

被 Git 忽略的 `local.properties` 必须将 `sdk.dir` 指向已安装的 SDK。测试台使用 `$OCTOSENSE_TOOLS/sdk`。离线构建要求已批准的依赖事先解析完毕；修改锁文件或校验和时，需要审查实际的依赖变更。

产物：

- `../resources/android/libs/octosense-contracts.jar` — 生成的 Home 类。
- `system-bridge/build/outputs/apk/prototype/system-bridge-prototype.apk`。
- `system-bridge/build/reports/lint-results-prototype.html`。
- `quickstep/build/outputs/apk/prototype/quickstep-prototype.apk`。

Makepad 打包器的源码变更后，在固定版本的检出目录 `.sources/makepad`（由 `python3 scripts/setup-home.py` 准备）中构建它，然后在 `home/` 下调用其独立二进制文件。以下命令只构建、不安装：

```sh
# From .sources/makepad:
cargo build --locked --offline --release -p cargo-makepad

# From home/, with an existing Makepad SDK selected:
../.sources/makepad/target/release/cargo-makepad android \
  --sdk-path="$OCTOSENSE_MAKEPAD_SDK" --abi=aarch64 \
  build --release --locked --offline --no-default-features -p octosense
```

打包器会编译 `resources/android/java/`，并将 `resources/android/libs/` 中的 JAR 同时加入 javac 和 D8 的输入。构建 Home 之前请先导出契约。框架现有的 SDK 目录结构与这里的 Gradle SDK 相互独立；单纯的跨目标 `cargo check` 同样需要已安装的 NDK 编译器环境，这通常由打包器提供。

## 可选的手机验证

常规 Home、bridge `prototype` 和 bridge `release` 的清单均不包含 instrumentation 注册。验证 APK 使用相同的应用身份和签名关系；安装后会替换这些应用并保留其数据。在用户自有手机上测试前，请保留一份经过校验和验证的回滚 APK，并记录设备设置。

使用 `:system-bridge:assembleValidation` 和 `:system-bridge:lintValidation` 构建 bridge 测试运行器。在 `home/` 下构建 Home 测试运行器：

```sh
python3 android/scripts/build-home-validation.py \
  --packager ../.sources/makepad/target/release/cargo-makepad \
  --sdk "$OCTOSENSE_MAKEPAD_SDK"
```

该脚本从常规 Home 清单派生出一个被 Git 忽略的验证清单，再通过 `MAKEPAD_ANDROID_MANIFEST_TEMPLATE` 选用它。如果明确指定的清单不存在，构建会失败。常规的源清单永远不会被改写。产物为 `target/android/home-validation/home-validation.apk`。bridge 的产物为 `android/system-bridge/build/outputs/apk/validation/system-bridge-validation.apk`。

获得安装授权后，使用选定设备的序列号运行：

```sh
adb -s "$OCTOSENSE_PHONE" shell am instrument -w \
  dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.validation.CallerRejectionInstrumentation
adb -s "$OCTOSENSE_PHONE" shell am instrument -w -e mode controls \
  dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation
adb -s "$OCTOSENSE_PHONE" shell am instrument -w -e mode placements \
  dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation
adb -s "$OCTOSENSE_PHONE" shell am instrument -w -e mode widgets \
  dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation
```

必须看到 `result=pass` 和 `INSTRUMENTATION_CODE: -1`，不能只看 ADB 的退出状态。控制项模式会将媒体音量调整一级，并在清理阶段恢复；其他未授权的控制项只测试“缺少前提条件”的结果。摆放模式使用可丢弃的私有缓存文件，绝不修改用户的收藏。这些都是原生 API/Binder/存储层面的检查，不涉及 UI 输入或截屏；它们无法证明布局、像素、手势行为或功能对等。测试结束后，请重新构建并安装常规 APK，以移除测试注册。

小组件模式使用单独的验证宿主 ID，发现提供方，分配并删除真实的 Android 小组件 ID，并检查一个可丢弃的恢复日志。它不会绑定提供方、不会接受 Android 授权，也不会验证小组件的渲染结果。清理阶段会验证验证宿主的 ID 与初始集合一致。

## 公开 Home 生命周期验证

bridge 验证 APK 包含一个可丢弃的原生启动器 activity。两个验证 APK 都安装后，运行 `scripts/run-launcher-ui-validation.py`，并传入 `--adb`、`--serial`、`--output`、`--bridge-normal` 和 `--bridge-validation`。测试前请保留常规 bridge APK。该运行器依赖测试台已获授权的 ADB `su` 权限，临时只禁用 `SystemBridgeService`；它不会为 bridge 应用申请 root 授权。

运行器会长按实际渲染出的图标、使用原生摆放菜单项、重建 Home 两次、启动/关闭自有的原生测试夹具，并验证服务禁用期间公开启动仍然可用。它会恢复该组件原本的默认状态，并检查自动重连。用常规 APK 替换 bridge 会移除夹具组件；再替换为验证版则会恢复它。测试会检查已保存的摆放在这段不可用期间得以保留。所有摆放都使用可丢弃的缓存日志。常规 bridge 的清理、自有 Home 进程的退出以及组件的恢复都会记录在 `results.json` 中。只有 Python 结果和原生 instrumentation 都成功才算通过；仅凭原生 instrumentation 只能验证准备与清理，无法验证完整的 UI 流程。

夹具中受令牌保护的远程接口使用经 ADB 转发的 Android 抽象 Unix 套接字。它不需要 Internet 权限，常规 bridge APK 中也不包含它。Home 只捕获自己的 Window 或 Makepad SurfaceView。添加 `--process-death` 后，会在保存收藏/程序坞摆放之后杀死已识别的自有 Home PID。运行器确认该 PID 已退出，然后启动 `launcher_resume` instrumentation，它读取现有的隔离日志而不重新写入种子数据。恢复要求 PID 和会话均已改变、摆放完全一致、两个经身份验证的服务连接都已建立，之后才会运行剩余的启动/软件包生命周期检查。第一个 instrumentation 进程会因有意发出的 SIGKILL 报告一次预期内的崩溃；恢复阶段必须通过。运行器会记录未完成的截图传输，并且只重试只读的截图，最多尝试三次。它从不重试输入或生命周期请求。

重启、用户资料锁定/解锁、原生最近任务和性能对等仍不在此流程范围内。

## Bridge 队列恢复验证

bridge 在其有界工作队列之外保存最新的通知监听器连接。队列满时会保留一次合并后的刷新重试；断开连接会清除通知/操作状态，来自早先连接的排队事件无法修改新连接的通知。即使重连复用同一个 Java 服务实例，也能加以区分。对账会在查询 Android 之前记录一个事件序列边界：已被该快照覆盖的排队事件会被跳过，而查询期间收到的事件仍会生效。原生通知查询和对账都在 bridge 工作线程上运行。

订阅意图同样保存在队列之外。工作线程按对象身份对订阅、替换和取消订阅进行对账；来自已被替换会话的命令会被立即拒绝。某个回调的 Binder 死亡只会移除该订阅，即使队列已满也是如此。

构建 `:system-bridge:assembleValidation` 和 `:system-bridge:lintValidation`，然后在已授权的设备测试范围内，从 mobile 仓库运行：

```sh
python3 android/scripts/run-bridge-queue-validation.py \
  --adb "$OCTOSENSE_ADB" --serial "$OCTOSENSE_PHONE" \
  --validation-apk android/system-bridge/build/outputs/apk/validation/system-bridge-validation.apk \
  --output target/android/adr-0001-artifacts/bridge-queue/new-run
```

输出目录必须是新的。运行器在替换之前会拉取并验证当前已安装的 bridge APK，并在清理阶段恢复这个完全相同的 APK。它会比较前后三个 OctoSense 软件包的哈希、instrumentation 注册、Home/最近任务的身份以及已记录的设置。恢复完成后，验证进程必须退出。

`BridgeQueueInstrumentation` 使用一个私有 bridge 实例进行测试，该实例带有真实的 64 项执行器、合成的监听器回调和合成的通知数据。它覆盖以下场景：饱和状态下的连接/断开、生命周期突发事件的合并、替换服务与同实例重连、过期的发布/移除事件、访问被拒绝时的快照、普通更新以及显式的命令背压。它还会用真实的夹具发布/移除事件填满队列，以验证快照恢复不会还原旧标题、不会移除重新发布的通知、不会复活已移除的通知，并且会保留查询期间到达的更新。订阅场景包括饱和状态下的注册、替换和移除，以及一个仅用于验证的回调进程真实发生 Binder 死亡。恢复完成后，两个自有测试进程都必须退出。扩展后的手机测试套件通过了 807 项断言，其中包括用于填满和排空队列的检查。它不授予通知访问权限、不发布真实通知，也不改变设备控制项。这些检查无法确认真实的监听器权限撤销、完整的通知操作投递或渲染性能。常规 APK 不包含 instrumentation 注册。

## 快捷方式固定确认

Android 会将 `CONFIRM_PIN_SHORTCUT` 解析到专用的 `ShortcutPinActivity`。如果把这个过滤器放在 Makepad 的 Home activity 上，当 Android 以单独任务启动确认界面时，可能会产生第二个渲染器。原生 activity 会显示快捷方式的图标、标签和发布方，并提供 Add/Cancel 控件。平台查询、接受操作和日志写入都在有界工作线程上运行。未确认的请求在配置重建后依然保留；已提交的请求绝不会被重放。与参考启动器一样，确认完成后会返回发起请求的应用。Home 在恢复时会刷新已固定快捷方式目录和摆放日志。

`scripts/run-shortcut-ui-validation.py` 基于现有的验证 APK。需要提供 `--adb`、`--serial`、`--output`，以及 `--home-normal`/`--home-validation` 和 `--bridge-normal`/`--bridge-validation` 两组参数。它会验证已安装的原始 APK，临时安装验证版这一对 APK，从自有夹具发送真实的 `ShortcutManager` 请求，检查 Cancel、原生提示框重建、接受以及 Home 摆放的持久化，然后恢复完全相同的原始 APK。唯一的所有者 ID 限定了快捷方式清理的范围，包括其独立的恢复模式。instrumentation 还会检查过期的摆放写入方，而不改动生产环境的收藏。原生截图只使用应用自己的窗口和 drawable。

使用 `--tap-file PATH` 时，运行器会在捕获渲染出的快捷方式后暂停最多 60 秒。检查这张应用自有的截图，然后将窗口像素坐标按 `{"x":919,"y":604}` 的格式写入指定路径；示例坐标并不是可移植的定位方式。运行器要求被启动的 activity 收到本次运行的确切快捷方式 ID。另外，`--lifecycle` 会使用只读的渲染器命中区域，并执行下文所述的完整发布方生命周期流程。两个选项都不使用时，它不会声称进行了快捷方式启动测试。必须同时要求 Python 结果和 instrumentation 成功，然后检查截图。

Home 通过 `LauncherApps` 回调刷新已固定的快捷方式，包括发布方的标签/图标变更。不可用的图标会变暗；已禁用的快捷方式会保留发布方提供的消息（上限为 512 个码点），并在点击时说明失败原因。分发时会在 Home 的工作线程上重新查询确切的软件包/用户/固定项，这样过期的目录就不会把已移除或已禁用的快捷方式变成不受限制的启动尝试。最终的启动/访问检查仍由 Android 负责。参见 [ShortcutManager](https://developer.android.com/reference/android/content/pm/ShortcutManager) 和 [LauncherApps](https://developer.android.com/reference/android/content/pm/LauncherApps)。

`--lifecycle` 夹具只会在 Home 处于恢复状态时，更新它自己那个名称唯一、已被接受的快捷方式。它会修改标签、图标和启动 intent；带消息禁用该固定项；验证点击会被拒绝；重新启用后启动更新后的 intent。随后 instrumentation 使用公开的启动器 API 取消固定本次运行的夹具 ID，保留所有无关的固定项。Home 必须观察到移除，将其存储的摆放显示为不可用，允许“从 Home 移除”（Remove from Home），并在 activity 重建后保留该移除。清理所有者不匹配时会被拒绝。这种实时取消固定是一项测试操作，与 Home 摆放菜单无关。用户资料锁定、冷进程确认和重启需要单独测试。

## Android Home 摆放

抽屉、搜索、Home 收藏和程序坞中的 Android 应用/快捷方式图标共用原生带角标的图标缓存。长按 Android 应用或托管应用的图标会打开 Home 摆放菜单：添加/移除收藏、选择程序坞位置，或将其从程序坞移除。摆放使用组件/快捷方式身份和 Android 用户序列号，存储在 Home 应用私有的 `files/launcher-placements.json` 中。

所有存储操作都在启动器工作线程上运行。收藏会保留顺序；不可用的软件包/用户资料会保留其存储的身份，并显示为不可用条目。托管应用根据可启动目录出现在 Home 上；移除某个托管应用会将其 ID 保存到 `hidden_hosted`，而“添加到 Home”（Add to Home）会清除该排除项。程序坞摆放同时支持托管身份和 Android 身份。移动或移除某项会让原位置留空；同一个图标不能占用两个程序坞位置。实时卡片保留各自的点击行为，不会打开图标菜单。损坏的文件和更新版本的文件会产生明确的存储错误，不会被覆盖，也不会妨碍公开的应用发现。版本 1 的日志加载时不写盘，并保留原生收藏和程序坞 ID。实际编辑后会保存为版本 2，其中包括托管应用排除项和空的程序坞位置。较旧的 Home 构建会拒绝版本 2 的日志；因此生产环境降级时，除了 APK 还需要保留的版本 1 日志。验证使用可丢弃的缓存日志，不会迁移生产环境的摆放。

`android/scripts/run-hosted-ui-validation.py` 会安装提供的验证版 Home/bridge APK，运行原生存储/迁移检查，测试图标菜单和 activity 重建，然后恢复完全相同的原始 APK。需要提供 `--adb`、`--serial`、`--output`、`--home-normal`、`--home-validation`、`--bridge-normal`、`--bridge-validation`、`--app`（一个在 Home 和抽屉首页都可见的应用）以及 `--points PATH`。在每个截图检查点，检查应用自有的图像，并将其窗口像素坐标写入该 JSON 映射，例如 `{"hosted-home":{"x":160,"y":1475}}`。坐标取决于实际截图。后续的键依次为 `hosted-dock-first`、`hosted-dock-fourth` 和 `hosted-drawer`。Python 结果和 instrumentation 都必须通过，包括恢复步骤。

## 原生小组件工作区

在 Home 的空白处长按并选择 **Widgets**（小组件）即可打开原生工作区**。Add**（添加）会列出已解锁用户资料中的提供方。绑定授权界面和提供方的配置 activity 由 Android 负责；工作区通过可选的 activity 扩展接收其结果**。Configure**（配置）、**Resize**（调整大小）和 **Remove**（移除）作用于所选的小组件**。Done**（完成）、返回键或新的 Home intent 会关闭工作区。同一个 Home 菜单还可以打开 Android 的壁纸选择器。

`NativeWidgets` 托管真实的 `AppWidgetHostView` 实例。小组件更新、提供方的点击操作和输入都保持原生处理。发现、ID 管理、选项和磁盘存储在 Home 的工作线程上运行；Android 视图的创建/监听生命周期以及外部 activity 的启动在 Android 主线程上运行。不使用任何小组件像素复制，也不进行逐帧的 root 操作。

稳定的生产宿主 ID 为 `0x4f4354`；验证宿主为 `0x4f4355`。`files/launcher-widgets.json` 记录提供方组件、用户资料序列号、小组件 ID、尺寸、有序摆放以及待完成的绑定/配置阶段。待完成的添加操作会在启动外部界面之前写入，并可在重建后继续或取消。移除时先清除持久化记录再删除 ID；由此产生的孤立 ID 可在成功读取日志后回收。无法读取或来自未来版本的日志会阻止清理，并被原样保留。activity 销毁时会停止监听，并保留已摆放的 ID。

调整大小会遵循提供方的可调整方向和边界，并通过小组件选项报告内容尺寸，在 API 31+ 上还包括响应式尺寸列表。用户资料事件会先使可见内容失效，再重新查询访问权限；过期的查询结果无法重新发布内容。Home 禁用了备份，因此不支持跨安装的小组件 ID 重映射。

每个已摆放的小组件还会在应用页面之后占用一个 Home 页面。应用页面数量变化时，翻页器会保留小组件的身份。带修订号的快照提供提供方可用性，归一化的视口矩形用于定位原生视图，无需逐帧查询提供方或复制像素。原生视口限定在小组件保存的尺寸内；其下方的空白区域仍可用于 Home 手势。工作区编辑的是同一个日志。

目前，抽屉、通知面板、概览或键盘打开时，小组件会隐藏。它们是独立的 Android 视图，不会出现在 Makepad 的模糊纹理中。小组件内容中的手势仲裁以及两层之间的帧同步仍有待验证。拖动摆放、小组件固定请求、自动切换、动态主题色以及手机上的配置/调整大小/用户资料撤销验证仍未完成。打开壁纸选择器并不等于实现了壁纸偏移或转场集成。请遵循 Android 的[小组件宿主职责](https://developer.android.com/develop/ui/views/appwidgets/host)。

### 自有小组件 UI 验证

只有验证清单包含 `octosense.validation=true`。使用 `--remote` 布尔 intent extra 启动该 APK 时，会启用一个受令牌保护的回环端点。`/surface` 复制其 Makepad SurfaceView；`/g` 复制裁剪到应用覆盖层范围的应用 Window；`/gq` 复制后关闭自有 activity。这些是独立的图层，并非显示器/合成器截图。常规 APK 无法通过 intent extra 启用该端点。

在已批准的测试范围内安装验证版 APK 后：

```sh
python3 android/scripts/run-widget-ui-validation.py \
  --adb "$OCTOSENSE_ADB" --serial "$OCTOSENSE_PHONE" \
  --output target/android/adr-0001-artifacts/widget-pages/new-run
```

该命令使用 instrumentation 模式 `widget_ui`、测试宿主 `0x4f4356`、一个隔离的缓存日志，以及已安装且未配置的 `com.android.deskclock` 提供方。它只临时获取 `BIND_APPWIDGET`，并在启动 Home 之前放弃该权限。这并不验证用户授权或配置流程。夹具会释放其 ID，验证宿主清理，并在中断后恢复其已知日志。遇到未知的分配或无法读取的存储时会以失败告终（fail closed）。不会编辑任何生产环境的小组件摆放。参见 Android 的[临时 instrumentation 权限 API](https://developer.android.com/reference/android/app/UiAutomation#adoptShellPermissionIdentity(java.lang.String...))。

只有在该设备的应用自有工作区截图中确认了 Done 按钮的中心位置后，才能使用 `--done-tap X Y`；这会额外测试原生按钮输入。测试结束后，请重新安装常规 Home APK，以移除验证元数据和 instrumentation 注册。

## Home 布局服务原型

`dev.makepad.octosense.quickstep/.HomeIntegrationService` 通过 `IHomeIntegration` 接收 Home 的几何信息。绑定需要签名权限 `dev.makepad.octosense.permission.BIND_HOME_INTEGRATION`；每个事务还会检查实际调用方的 UID、签名者、软件包和 Android 用户。只允许 Home 软件包调用。安装此 APK 不会配置 ROM 的最近任务，也不会添加 Quickstep 控制器。控制器和转场两个能力标志均为 false。

版本 1 的布局使用显示像素的 LTRB 矩形、显示器 ID、旋转方向、边衬距离、客户端 epoch 以及一个正数修订号。它最多包含 128 个可见的应用目标，以组件和用户序列号标识。Home 只发布实际以原生纹理绘制出的图标的边界。快捷方式、已锁定或已暂停的应用以及不在当前页的图标不提供转场目标。协议 1.1 为该布局新增了可选的非负 `transition_id`；省略时表示 `0`，用于普通发布或旧版发布。未来的控制器在使用某个目标之前，必须要求其当前的正数转场 ID。工作线程侧的 `boundsFor(component, userSerial)` 查找要求精确匹配，并返回防御性副本。平台适配器的 `HomeTransitions` 协调器使用新的正数 ID，并异步报告有界的生命周期/进度事件。SDK 原型并未启用这个原生端点。无论是结构校验还是协调器测试，都不能证明原生动画的行为。服务在缓存几何信息之前会先复制并校验。修订号匹配的 `setHomeReady` 会让缓存变为可用；格式错误的几何信息、epoch 变化或不匹配的就绪信号都会使其失效。epoch 变化和队列溢出需要重新订阅。最终解绑和回调死亡会清空缓存。屏幕/用户资料/配置以及软件包变化会让缓存布局的代次作废。为 false 的就绪信号同样会丢弃缓存布局；之后为 true 的消息也不能再复用它。发生外部变化后，服务会报告 `layout_invalidated`，Home 随后异步请求并发布新的渲染器几何信息。

Home 在其现有工作线程上合并异步布局更新。视口、焦点、目录/用户资料以及原生工作区的变化都会使就绪状态失效，直到匹配的新渲染器几何信息到达。渲染器会回传当前的转场 ID；已作废的连接/订阅回调无法修改它。原生控制器的源码已准备就绪，但尚未证实任何从应用到 Home 的转场。

安装验证版 Home 后，使用上文的运行器运行 instrumentation 模式 `home_layout` 和 `home_transport`。前者校验结构；后者测试真实 Home UID 下的 Binder 顺序、失效和重新订阅。传输运行器还会切换其未导出、仅用于验证的 activity 别名，并恢复其原本的启用状态，以检查真实的软件包变更广播，以及对旧布局延迟到达的就绪信号的拒绝。bridge 验证运行器还接受 `-e target quickstep`，用于在 Android 允许绑定之后，检查对同一签名者的其他软件包的拒绝。在 `run-widget-ui-validation.py` 中添加 `--geometry --require-quickstep`，可在小组件工作区覆盖 Home 时，要求原生时钟图标的几何信息得到确认，并要求就绪状态失效。其收藏是一个隔离的缓存夹具。在现有的限定范围 ADB root 测试授权下，`--invalidate-layout` 还会只向 OctoSense Quickstep 发送一条受保护的软件包变更广播，并检查 Home 以新的已确认修订号重新发布未改变的图标边界。它不修改任何软件包，也不会通知 Home 独立的目录监听器。这些测试结束后，请恢复常规的 Home 和 bridge APK。

Quickstep 的 `validation` 变体新增了一个隔离的协调器运行器：`adb shell am instrument -w -r dev.makepad.octosense.quickstep/dev.makepad.octosense.quickstep.TransitionInstrumentation`。它借助真实的 Android 用户资料/软件包查询和一个测试回调接收端，检查目标身份、取消、取代、过期和队列溢出。它不会注册手势控制器，也不会更改最近任务。测试结束后，请恢复常规的布局 APK，以移除其 instrumentation 注册。

## 原生 Quickstep 平台构建

平台源码和工具链使用经单独批准的 Docker 环境，详见[构建记录（英文）](../docs/android/quickstep-build-environment.md)。`platform-build/stage-quickstep.py --tree /build --check` 会对照固定的 Trebuchet 修订版本验证源码准备情况。写入独立的 `OctoSenseQuickstep` 模块时，还需要提供来自一次成功的上游 Quickstep 构建的 `--baseline-result`；它会保留上游 Java 代码，并使用自己的、特定于软件包的 BuildConfig 和概览观察者。基线记录必须标明已安装 ROM 的 Trebuchet 修订版本、经过验证的平台源码清单，以及所保留的上游 APK 的 SHA-256。仅完成源码准备并不允许进行暂存。暂存之后，构建主管在持有构建锁的情况下调用 `stage-quickstep.py --tree /build --verify --baseline-result PATH`。验证会根据当前的适配器输入重新计算预期的源码哈希，并检查已暂存的文件、Blueprint 片段和原始的上游 Java 代码。它不做任何写入，且必须在编译候选版本之前运行。

部署之前，对已签名的候选版本运行 `platform-build/inspect-quickstep-apk.py`，并提供 SDK 中 `aapt`/`apksigner` 的路径以及现有 OctoSense 证书的 SHA-256。它会检查实际 APK 的软件包、服务、provider authority 以及 Home 边界，还要求原生控制器端点的清单元数据处于启用状态。这是一道打包关卡；ROM 授权、SystemUI 绑定、转场、回滚和性能都需要手机上的证据。原生控制器的部署与布局原型的批准相互独立。

`platform-build/package-quickstep-module.py` 将经过检查的已签名 APK，与一个仅含资源的最近任务覆盖层和一个同分区的权限白名单打包在一起。它需要经过审查的确切手机框架 APK、固定的权限清单、现有的 SDK 工具，以及明确提供的密钥库。密码从指定名称的环境变量中读取。它会把一个 ZIP 和可供审查的载荷写入一个空的输出目录；它从不安装它们。安装程序固定针对经过审查的 OnePlus 6 ROM，并检查载荷哈希。确切的候选版本、变更、检查和回滚见[部署审查（英文）](../docs/android/quickstep-deployment-review.md)。

## 通知的应用身份

Home 使用 [Android PackageManager](https://developer.android.com/reference/android/content/pm/PackageManager) 解析发布通知的软件包的标签和带角标的应用图标。这一步在 Home 的工作线程上、在获得经身份验证的当前用户 bridge 快照之后运行。通知自带的应用标签和文件路径会被替换。缺失的软件包会保留其包名并使用备用图标；其内容仍然可见。PNG 每边上限为 192 像素，并且只为当前通知快照中的软件包保留。软件包/目录变化会使元数据缓存失效；未改变的图像文件会被复用。

Rust 在其有界工作线程池上解码启动器图标和通知图标。通知面板缓存包含标签、图标身份和图标可用性，因此异步图像解码完成后，会使已记录的过期卡片失效。具有相同通知句柄的更新会在更新文本的同时保留卡片身份。最终的 JSON 快照以 UTF-8 字节数对照 JNI 限制进行衡量。必要时会省略最旧的条目，并设置 `notifications_truncated=true`，同时保留设备状态和最近的通知。

`android/scripts/run-notification-ui-validation.py` 接受 `--adb`、`--serial`、`--output`、`--home-normal`、`--home-validation`、`--bridge-normal` 和 `--bridge-validation`。它先运行原生的身份/缓存/预算检查，然后使用合成的卡片内容和真实已安装软件包的元数据。仅用于验证的只读渲染器探针会报告实际的通知目标边界、通知面板的打开状态以及已解码图标的可用性。运行器通过应用窗口输入点击目标，并要求通知面板在捕获每个卡片状态之前完全展开。请检查生成的每一张通知图像；仅通过模型检查并不能证明像素正确。运行器会恢复完全相同的原始 APK，并检查自有进程的清理情况。它不授予通知访问权限、不发布真实通知，也不分发通知操作。

## 通知回复

标记为自由格式回复的操作会在 Home 中打开一个原生编辑器。文本编辑、选择和输入法输入由 Android 负责。**Send**（发送）通过现有的经身份验证的 bridge，使用新的命令 ID 提交草稿；普通的通知操作保持原有行为。草稿只保存在内存中，上限为 2,000 个 UTF-16 码元，并在 Home 离开前台或编辑器关闭时清除。空回复和重复点击 Send 会被拒绝。

编辑器在分发之前会再次检查实时操作句柄。通知发生变化或被移除时，未发送的草稿会失效。提交后若 bridge 断开连接或 15 秒内未返回结果，会显示结果不确定，并且不会重试。之后到达的匹配完成结果可以解除这一状态；完成仅表示发起应用的 PendingIntent 已被分发，并不代表收件人已收到消息。代次令牌将新编辑器与旧的命令结果隔离开。原生操作对象始终留在 bridge 内部。

安装已授权的验证版 Home 后，instrumentation 模式 `notification_input` 会验证 Android RemoteInput 能投递到一个可丢弃的应用内广播，涵盖 Unicode、无效文本、不可变操作和已取消的操作。它不授予通知访问权限，也不联系任何收件人。`run-reply-ui-validation.py --adb PATH --serial SERIAL --output DIRECTORY` 使用模式 `reply_ui` 打开一个可丢弃的原生编辑器夹具。它测试原生 InputConnection 和应用内的按钮输入，只捕获 Home 的 Window，并验证取消、过期结果、队列压力和超时。该夹具测试编辑器并模拟完成回调；它不能证明 Rust/JNI/Binder 的完整往返，也不能证明通知监听器的授权/生命周期。验证后请恢复常规的 Home APK。

`run-bridge-notification-validation.py` 使用 instrumentation 模式 `notification_roundtrip` 运行真实的监听器路径。它需要明确的同意和 `--allow-temporary-notification-access` 标志，以及 `--adb`、`--serial`、`--output`、`--home-normal`、`--home-validation`、`--bridge-normal` 和 `--bridge-test`（常规的 bridge 原型）。新的输出目录只保留夹具的测试结果。传入的快照在测试将其入队之前，会被过滤为只含夹具的唯一软件包/标题。

宿主只有在夹具观察到访问权限已被撤销后才授予权限，然后在夹具通知仍处于活动状态时再撤销它。测试涵盖投递/更新、向应用内接收器分发 Unicode RemoteInput、命令去重、过期的操作句柄、单条移除以及权限撤销后的清理。不联系任何收件人，也从不使用“全部清除”。清理阶段会恢复原始 APK、通知发布权限/AppOp 以及监听器设置条目；Android 对现有仅含包名条目的规范化处理也会被明确处理。这测试的是经过身份验证的 Home UID Binder 调用，而不是完整的通知面板、编辑器、Rust 与 JNI 输入路径，也不是 Android 的授权界面。

添加 `--ui-flow --visible-seconds 20` 可测试可见路径。该模式让 Home 的常规 bridge 订阅保持主导：应用内触摸打开通知面板，滑动真实卡片以显示 Reply/Clear，并在通过 Android 的 InputConnection 输入文本后点击原生 Send 按钮。夹具发布真实通知并接收自己的 PendingIntent 回复；它从不注入模拟的 bridge 完成结果，也不会直接打开回复编辑器。它还会检查更新/撤销导致的草稿失效，以及渲染器和 NotificationManager 中的单条移除。三个主要界面会按指定时长暂停。

可选启用的验证构建会在 Home 将快照入队或渲染之前过滤掉无关通知。只读渲染器探针会暴露实际的卡片/操作命中边界，远程接口会报告生产环境编辑器的状态。截图对 Home 的 SurfaceView 或 Window 使用 PixelCopy，不包括系统显示和键盘。流程驱动器以 16 KiB 分块获取有界的内存截图，以避免过大的 ADB 转发响应。在声称视觉验收通过之前，请检查保存的 PNG。清理阶段会撤销访问权限、取消夹具通知、恢复 APK 和发布权限、移除自有的端口转发，并验证测试 PID 已退出。对于幂等的清理操作，短暂的 ADB 重连会被重试；投递情况不确定的输入不会被重放。

`--skip-captures` 是一个诊断选项，不代表视觉验收。在当前的 OnePlus 测试台上，整图和分块两种 PixelCopy 截图尝试都与 Mac 端 ADB USB 读取失败同时出现。不截图的运行通过了完整的 UI 操作流程。在传输/截图故障解决且组合流程的像素得到检查之前，请在报告中保留这一区别。

Android 的 [RemoteInput API](https://developer.android.com/reference/android/app/RemoteInput) 定义了原生回复载荷，[PendingIntent 分发](https://developer.android.com/reference/android/app/PendingIntent)则将其投递给原始应用。

## 签名与访问

除非提供受管理的密钥库，`prototype` 使用框架自带的开发密钥。该密钥是公开的：这个变体仅用于开发，无法支撑任何生产签名/安全方面的声明。它使用经过优化、不可调试的代码。没有受管理的密钥库时，`release` 保持未签名状态。

要构建受管理的 bridge，请在构建环境中设置 `OCTOSENSE_KEYSTORE`、`OCTOSENSE_KEYSTORE_PASSWORD` 和 `OCTOSENSE_KEY_ALIAS`。`OCTOSENSE_KEY_PASSWORD` 默认与密钥库密码相同。构建 `:system-bridge:assembleRelease`。使用打包器的 `--keystore` 和 `--keystore-key-alias` 选项，以相同的密钥库和别名为 Home 签名；`MAKEPAD_KEYSTORE_PASS` 提供其密钥库密码。不假定存在 ROM 平台密钥。请将密钥库和密码保存在源码控制之外。

bridge 会为每个操作检查实际的 Binder UID、签名者、软件包和 Android 用户。通知监听器、修改系统设置、勿扰模式、相机和蓝牙的权限提示均由 Android 负责。bridge 的原生设置入口会打开这些授权界面。只有当用户按下 **Connect optional root controls**（连接可选的 root 控制项）时才会开始 root 绑定；不存在自动的 root 授权。

固定的 root 适配器识别 API 35、`ro.lineage.device=enchilada` 和 LineageOS 22.2。它通过固定的命令参数提供 Wi-Fi、蓝牙和省电模式的设置功能。它自身的 Magisk 身份/授权、强制模式下的行为以及设置结果仍需在手机上验证。root 从不接收来自 UI 的 shell 文本，也不会为每一帧动画执行操作。

蓝牙 root 控制项还需要 `BLUETOOTH_CONNECT` 才能观察其当前状态。请使用 bridge 的 **Bluetooth and flashlight**（蓝牙和手电筒）授权按钮。权限结果以及从授权界面返回，都会在 bridge 工作线程上刷新能力。
