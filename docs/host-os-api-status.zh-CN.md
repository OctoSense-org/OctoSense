# 宿主 OS API 状态

[English](host-os-api-status.md) | 简体中文

本文记录 **2026-10-08 检查的首批 OS 能力接线，尚未发布**。本批先把已有原生设施接入受限应用，再扩展 Rust 引擎 API；不会改变用户已经下载的桌面版或 Home。源码实现、编译、自动化测试和真机行为分别记录，不能互相替代。

## 本批新增内容

| 入口 | 契约与边界 | 平台范围 |
| --- | --- | --- |
| 文档导入、导出 | `files.status@1`、`files.import@1`、`files.export@1`。前台导入把用户选中的一个文档复制到应用内的新文件；导出保存应用文件的快照。需要 `files` 和 `storage`。脚本只拿到应用相对路径和字节数，不获得 OS 路径或文档提供方 URI。 | macOS、Windows、Android 有字节适配器。桌面 Linux 还需要已安装且受支持的对话框辅助程序。此服务暂不支持 iOS、OpenHarmony、web 和直接使用 framebuffer 的 Linux。 |
| 应用二进制存储 | `fs.write_bytes` 接收 U8 类型数组，与 `fs.read_bytes` 配合；沿用存储 jail、配额和单文件限制，发现标识为 `storage.binary_write@1`。 | 这是应用自己的存储，不是任意 OS 路径写入权限。 |
| 新鲜位置采样 | `location.sample@1` 在调用者指定的位置年龄、精度和超时范围内等待一次定位；`location.sample.cancel@1` 取消本应用等待中的采样。必须已有应用授权和 OS 权限，采样本身不弹权限对话框。 | Android、macOS。开始采样仅限前台，不新增持续订阅或后台定位 API。 |
| 外部链接 | `LinkLabel::handle_event` 调用原生 `open_url` 前检查已有的前台弹窗权限和页面授权。受限应用可打开已声明主机的 HTTP(S) 链接；`web` 能力额外允许公共 HTTPS 页面。受限应用不能打开其他协议。 | 补齐 Windows `ShellExecuteW`、桌面 Linux `xdg-open`、Android 普通 JNI 到 `ACTION_VIEW`，以及 iOS `UIApplication` 路径。macOS 和 web 原有打开器继续使用。Linux 需要已安装的系统分发程序。 |

文件传输沿用单文件 1 MiB、整个 jail 的字节配额及 256 条目录项限制。导入拒绝覆盖，并检查归属、路径越界和符号链接。对话框及回调使用经认证的应用、isolate 身份，完成时再次检查授权与生命周期。关闭应用使等待中的请求失效，但无法回滚已经开始写入原生目标的导出。Android 文档选择不保留持久 URI 授权。文档提供方若能提供图片，可用此路径导入；专用 OS 照片库选择界面不在本批范围内。详见[文件传输契约](../crates/shell/src/files_service/README.zh-CN.md)，以及 [files_service/mod.rs](../crates/shell/src/files_service/mod.rs) 中的 `FilesService::api_methods`、`Work::storage`、`handle_event`。

位置采样默认超时 10 秒，允许的位置最大年龄为 5 秒。成功结果包含真实 Unix 秒时间戳、`age_ms`、测得的 `accuracy_m`、`source: "platform"`、`freshness: "fresh"`。Android 的大致位置权限可用；如果调用者要求更高精度，可能超时，不会自动提升权限。应用关闭、撤销授权、超时或宿主进入后台都会释放采样。旧 Android `location.get@1` 保持原样：`source: "last_known"`、`timestamp: null`、`freshness: "unknown"`，不能把它显示为刚取得的位置。详见[设备契约](../crates/shell/src/platform_services/README.zh-CN.md)、`DeviceService::api_methods`，以及 `crates/shell/src/platform_services/location.rs` 中的 `location::methods`、`Options::value`。

外部链接使用同一个规范化 URL 做策略判断和 OS 打开，拒绝控制字符及无效的绝对 URL；错误日志不包含 URL。可信的原生宿主调用仍可使用有效的绝对协议地址。本批没有新增名为 `web.open` 的宿主请求方法：补齐的是已有控件入口，与嵌入式浏览不同。Makepad 源码符号包括 `normalize_external_url`、`LinkLabel::handle_event`、各平台的 `CxOsApi::open_url`、`android_jni::to_java_open_url`、`MakepadActivity.openUrl`。固定源码及覆盖补丁由 [runtime-patches.lock.json](../runtime-patches.lock.json) 和 [native-runtime.lock.json](../native-runtime.lock.json) 定位。

未实现拍照或录像的后端现在返回 `CameraCaptureResult::Failed`，不再静默忽略请求，使控件可以清除等待或录像状态。这不表示新增了视频编码器或拍摄后端。

## 已有能力与剩余 OS 工作

| 领域 | 已有实现 | 仍需区分或补齐的部分 |
| --- | --- | --- |
| 日历 | OctoSense 系统 Calendar 管理宿主保存的事件，`calendar.*` 以 `os.calendar` 执行。Google Calendar 的 `gcalendar` 支持读取及经审阅的保存，使用该应用已获授权的连接。 | 两者都不是通用设备日历适配器。共享 Apple EventKit / Android CalendarProvider 接口、原生日历选择及权限生命周期仍待实现。第三方 manifest 声明 `calendar` 不会直接获得系统应用服务的访问权。 |
| 相机、麦克风 | Android/macOS 已有逐应用授权及 OS 权限方法；`CameraPreview` 已有预览、拍照和录像入口，并检查麦克风权限。Android 已实现拍照，但明确拒绝录像；存在入口不等于后端已实现。 | 获批权限本身不等于独立拍照或纯音频录制服务。通用、逐应用隔离的录音与播放会话 API、资源生命周期、中断处理及完整平台验收仍待补齐。 |
| 音频与生成媒体 | Makepad 已有原生音频输入、输出，语音输入链路及 `Video` 播放控件。`model.audio` 已能请求生成语音；`model.image`、视频任务、embeddings 也已实现。 | 服务商生成与 OS 录制、播放是不同环节。有这些方法不代表已通过真实服务商调用或设备播放验证。 |
| 嵌入页面、身份认证 | `WebReader::open_on_platform` 已有 macOS、iOS、Android、Linux、Windows 适配器；宿主持有的 `auth` 服务及服务商、应用后端登录流程也已存在。 | 外部链接修复不会代替嵌入式浏览或身份认证。WebView 可用性、服务商注册、跳转策略及各平台登录验收仍分别成立；应用始终拿不到宿主凭据。 |
| 通知、后台工作 | `glance.publish` 可请求卡片通知；`App::glance_notify` 显示 Shell toast 和 Home 通知栏通知。Mail、已连接 Gmail 有特定后台执行及持久事件、通知发件箱路径。 | 这些不是通用 OS 闹钟、推送 token 或任意关闭应用的脚本调度器。后台任务还需要明确的事件契约、应用权限和 OS 生命周期支持，通知本身不提供这些能力。 |
| 剪贴板、分享 | 原生文本编辑和剪贴板操作已存在；受限 WebCard 按自己的授权提供 `clipboard.write`。Makepad 的 `Cx::share_text` 有 Android 处理器。 | 本批不新增通用受限应用剪贴板读取或跨平台分享面板服务。声明了 `Cx::share_text` / `show_notification` 不等于每个平台后端都处理这些操作。 |

源码入口：[CalendarService 及归属检查](../apps/calendar/host-service/src/lib.rs)、[连接账户契约](../crates/oauth-service/README.zh-CN.md)、[AuthService](../crates/oauth-service/src/host.rs)、[设备服务](../crates/shell/src/platform_services/mod.rs)、[模型媒体契约](../apps/ai-providers/host-service/MEDIA.zh-CN.md)、[Glance](../crates/shell/src/glance.rs)、[`App::glance_notify`](../crates/shell/src/lib.rs)、[mail_background](../crates/shell/src/mail_background.rs)、[connected_events](../crates/shell/src/connected_events.rs)。相关 Makepad 符号为 `CameraPreview::record_start`、`WindowVoiceInput::ensure_audio_callback`、`CxMediaApi`、`Video`、`WebReader::open_on_platform`、`web_card::policed_tool_grant`、`Cx::share_text`。

实际提交能帮助界定问题：[CFAW News #114](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/114) 使用新闻原文外链；[Navigation #116](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/116) 需要定位，也包含应用和 UI 改动；[Muse #112](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/112) 使用原生 EventKit 配套组件。本批不会让这些提交整体自动兼容标准宿主；上面的剩余能力清单也不表示每一项都是参赛者提出的需求。

## 能力发现、验证与后续工作

应用应在 `host_api.required` 中声明所需方法版本，为可降级功能声明 optional 方法，并查询实际宿主的 `runtime.list` / `runtime.describe`。发现结果必须与已注册实现、支持的平台一致。manifest 能力、逐应用授权、OS 权限、前台调用权限仍是不同检查。Shell 在 [`register_host_services`](../crates/shell/src/apps.rs) 注册服务，在 [`App::handle_event`](../crates/shell/src/lib.rs) 分发原生事件。

本批组件工作区已记录的验证：

- 外部链接：macOS 控件编译、Windows/Linux/Android 平台检查、14 项现有策略测试，以及全部 16 个 Android Java 模板编译通过。原有的无关编译警告和弃用警告仍存在。
- 文件：六项原生存储测试、五项对话框测试和四项集成文件服务测试通过。存储检查还覆盖了保留旧有相机大文件读取能力，避免新的传输上限影响它。
- 位置与权限：十项集成平台服务测试全部通过，覆盖新鲜度、精度、取消及超时与撤权边界。
- `d13fe639`（运行时树 `025b17f1`）的集成验证：1,027 项共享 Shell 测试使用全新配置目录运行并全部通过，无忽略项。桌面默认及 mobile 构建、Home mobile 构建、两个 Shell 依赖图检查（宿主/Android、默认/mobile 组合）、18 项 setup 测试和私有路径检查通过。即使直接运行普通 `cargo test`，真实原生模块测试也会自动隔离用户配置。
- 最终源码 `807f2bc8`（运行时树 `9121e900`）修复了类型数组丢失整数值，以及宿主回调让出执行权后丢失 VM 续执行状态的问题。127 项定向运行时测试全部通过，回调回归测试在旧运行时上会失败。1,027 项共享 Shell 测试使用全新配置目录再次全部通过，无忽略项。此次没有配置可选的真实内核链路，这个通过数不验证该链路。
- release 模式的 Mac 签名测试应用通过全部十项新增 OS 检查，同时验证原生按钮操作和实时 UI 更新。同一源码的 Windows/Linux 平台检查及独立 Android APK 构建通过。详见[已去除私有信息的本批记录](../tools/fixtures/host-api-lab/evidence/os-api-batch1/receipt.json)。
- 扩展后的 24 项 OnePlus 检查**仍待执行**：桌面验收完成后，ADB 找不到获准测试的手机。APK 已构建，但尚未安装。历史上的 14 项手机记录不能验证本批变更。Android JNI 分配检查、有界数组复制、异常清理和线程附加状态清理已编译通过；Java 编译仍有原有警告。
- 本批的真实外部浏览器启动、文档提供方对话框、真机新鲜定位及 iOS 执行均**未验证**。iOS 外链代码仅经源码审阅，组件检查时没有可用的 iOS Rust target。本文不声称整个发布流程或所有平台已通过。

补齐这些 OS 边界后，下一步是**面向应用公开、由 Rust 实现的图像和电子表格 API**。现有 [`PhotoService`](../apps/photo/host-service/src/lib.rs)、[`SheetsService`](../apps/sheets/host-service/src/lib.rs) 已封装原生 Rust 引擎，但当前系统、原生应用准入规则及共享引擎状态还不是公开的逐应用契约。开放之前，需要可执行方法的发现 schema、应用独占文件或句柄、配额、有界异步任务、取消和生命周期检查。电子表格是明确的产品优先事项；可选照片、OCR 想法不代表每个提交应用的已证实阻塞点。新增适配器应复用 Rust 引擎，而非用脚本重新实现。

类似 JNI 的**动态原生库插件暂缓**；用于连接 Android OS API 的普通 JNI 调用属于当前实现。未来的原生能力包加载器需要单独设计 ABI、信任和隔离边界，不是这些共享服务的前置条件。
