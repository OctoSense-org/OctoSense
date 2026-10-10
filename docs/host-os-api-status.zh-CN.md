# 宿主 OS API 状态

[English](host-os-api-status.md) | 简体中文

本文记录 [Desktop RC2](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2) 中的 **OS 与公共 API 集成**。本批先把已有原生设施接入受限应用，再扩展 Rust 引擎 API。公共声明已发布在 [**app-contract 1.10.0**](https://crates.io/crates/octosense-app-contract/1.10.0) 中；已安装的宿主还必须在对应平台实现所请求的方法。更新 SDK 或安装应用不会更新宿主。已发布的 Android Home beta.1 不包含本批新增能力；用于 OnePlus 验收的隔离测试 APK 不是 Home 发行包。源码实现、编译、自动化测试和真机行为分别记录，不能互相替代。

## 本批新增内容

| 入口 | 契约与边界 | 平台范围 |
| --- | --- | --- |
| 文档导入、导出 | `files.status@1`、`files.import@1`、`files.export@1`。前台导入把用户选中的一个文档复制到应用内的新文件；导出保存应用文件的快照。需要 `files` 和 `storage`。脚本只拿到应用相对路径和字节数，不获得 OS 路径或文档提供方 URI。 | macOS、Windows、Android 有字节适配器。桌面 Linux 还需要已安装且受支持的对话框辅助程序。此服务暂不支持 iOS、OpenHarmony、web 和直接使用 framebuffer 的 Linux。 |
| 应用二进制存储 | `fs.write_bytes` 接收 U8 类型数组，与 `fs.read_bytes` 配合；沿用存储 jail、配额和单文件限制，发现标识为 `storage.binary_write@1`。 | 这是应用自己的存储，不是任意 OS 路径写入权限。 |
| 新鲜位置采样 | `location.sample@1` 在调用者指定的位置年龄、精度和超时范围内等待一次定位；`location.sample.cancel@1` 取消本应用等待中的采样。必须已有应用授权和 OS 权限，采样本身不弹权限对话框。 | Android、macOS。开始采样仅限前台，不新增持续订阅或后台定位 API。 |
| 外部链接 | `LinkLabel::handle_event` 调用原生 `open_url` 前检查已有的前台弹窗权限和页面授权。受限应用可打开已声明主机的 HTTP(S) 链接；`web` 能力额外允许公共 HTTPS 页面。受限应用不能打开其他协议。 | 补齐 Windows `ShellExecuteW`、桌面 Linux `xdg-open`、Android 普通 JNI 到 `ACTION_VIEW`，以及 iOS `UIApplication` 路径。macOS 和 web 原有打开器继续使用。Linux 需要已安装的系统分发程序。 |
| 设备日历 | `device_calendar.*` 增加应用／账户范围的授权、日历选择、有界事件读取及需要亲手批准的写入；它与 `calendar.*`、`gcalendar.*` 分开。 | macOS EventKit、Android Home CalendarProvider。真实 OS 日历交互仍待验收，见[设备日历契约](../crates/shell/src/device_calendar/README.zh-CN.md)。 |
| 公共邮件草稿 | `mail.compose@1`、`mail.compose_status@1` 为调用应用／账户保留版本化草稿。`mail.review_send@1` 与兼容入口 `mail.send@1` 均需要原生审阅，不给 Agent 直接 SMTP 发送权限。 | macOS/Android 实现发送审阅；Linux/Windows 可编辑草稿和读取状态，没有亲手批准发送的适配器。真实 SMTP 接受结果未验证，见[邮件契约](../apps/mail/host-service/README.zh-CN.md)。 |
| 选择照片、分享文本 | `files.pick_photo@1` 通过按图片过滤的文档选择器导入经文件签名检查的 PNG/JPEG/WebP，不授予照片库访问权。`files.share@1` 交接有大小限制的文本，不支持附件。 | 照片选择沿用文档适配器的平台范围；文本分享仅支持 Android，返回选择器交接结果，不证明送达。原生选择器验收仍待完成。 |
| 独立音频 | `microphone.record_start/record_status/record_stop/record_cancel@1`、`audio.play/status/stop@1` 提供使用应用存储、归属应用 isolate、仅限前台的短时会话。录音需要应用和 OS 麦克风授权。 | macOS/Android 源码实现；真实录音和播放仍待验收。不新增 ASR/TTS 或后台流媒体，见[音频会话](../crates/shell/src/audio_service/README.zh-CN.md)。 |
| Video 控件操作 | `video.playback_controls@1` 表示 Splash `Video` 控件 ABI，包含准备、播放、暂停／继续、定位、音量／速度和状态／错误查询；**不能通过 `host.request` 调用**。 | macOS 原生播放通过 16 项检查，OnePlus 6 / Android 15 通过 20 项回调／解码器检查。[单独测试](../tools/fixtures/video-api-lab/README.zh-CN.md) 只使用本地合成 MP4；其他编解码器及后端仍未验证。 |

文件传输沿用整个 jail 的字节配额及 256 条目录项限制。`files.import` 可导入最大 64 MiB 的文档（Android 为 16 MiB，见 `max_import_bytes`）：工作线程把它暂存在 jail 旁，UI 在脚本轮次之间把它链接进来，字节不经过脚本堆。图片选择和导出仍使用单文件 1 MiB 上限。导入拒绝覆盖，并检查归属、路径越界和符号链接。对话框及回调使用经认证的应用、isolate 身份，完成时再次检查授权与生命周期。关闭应用使等待中的请求失效，但无法回滚已经开始写入原生目标的导出。Android 文档选择不保留持久 URI 授权。`files.pick_photo` 新增按图片过滤的文档选择路径，不是专用照片库授权或大文件句柄 API。其 1 MiB 上限会明确拒绝很多手机原始照片。详见[文件传输契约](../crates/shell/src/files_service/README.zh-CN.md)，以及 [files_service/mod.rs](../crates/shell/src/files_service/mod.rs) 中的 `FilesService::api_methods`、`Work::storage`、`handle_event`。

位置采样默认超时 10 秒，允许的位置最大年龄为 5 秒。成功结果包含真实 Unix 秒时间戳、`age_ms`、测得的 `accuracy_m`、`source: "platform"`、`freshness: "fresh"`。Android 的大致位置权限可用；如果调用者要求更高精度，可能超时，不会自动提升权限。应用关闭、撤销授权、超时或宿主进入后台都会释放采样。旧 Android `location.get@1` 保持原样：`source: "last_known"`、`timestamp: null`、`freshness: "unknown"`，不能把它显示为刚取得的位置。详见[设备契约](../crates/shell/src/platform_services/README.zh-CN.md)、`DeviceService::api_methods`，以及 `crates/shell/src/platform_services/location.rs` 中的 `location::methods`、`Options::value`。

外部链接使用同一个规范化 URL 做策略判断和 OS 打开，拒绝控制字符及无效的绝对 URL；错误日志不包含 URL。可信的原生宿主调用仍可使用有效的绝对协议地址。本批没有新增名为 `web.open` 的宿主请求方法：补齐的是已有控件入口，与嵌入式浏览不同。Makepad 源码符号包括 `normalize_external_url`、`LinkLabel::handle_event`、各平台的 `CxOsApi::open_url`、`android_jni::to_java_open_url`、`MakepadActivity.openUrl`。固定源码及覆盖补丁由 [runtime-patches.lock.json](../runtime-patches.lock.json) 和 [native-runtime.lock.json](../native-runtime.lock.json) 定位。

未实现拍照或录像的后端返回 `CameraCaptureResult::Failed`，不再静默忽略请求。当前集成还新增有界的 [Windows 静态拍摄工作线程](windows-camera-capture.zh-CN.md)：九项真实源码工作线程测试和 Windows 目标类型检查已通过，但没有操作 Windows 相机硬件。拍摄参数现在明确表达意图：`capture()` 只存本地；`capture({library: true})` 请求图库导出，在 Windows 上会被拒绝。清单声明不会自动开启导出。Windows 图库导出和录像仍不支持，共享拍摄存储配额预留也未补齐。这些检查不能验证原生设备回调、权限界面或隐私指示灯。

## 已有能力与剩余 OS 工作

| 领域 | 已有实现 | 仍需区分或补齐的部分 |
| --- | --- | --- |
| 日历 | 系统 `calendar.*`、连接账户的 `gcalendar.*` 和新增面向 OS 的 `device_calendar.*` 保持各自身份和存储。 | 第三方 `calendar` 授权仍不能打开系统应用服务。自定义 OS 日历调用需迁移到 `device_calendar`；真实权限、选择和事件写入验收待完成。此版本中的重复事件及参会者只读。 |
| 相机、麦克风 | 逐应用授权和 OS 权限仍相互独立。`CameraPreview` 提供相机入口，独立麦克风会话现在由自己的服务提供。 | Android 拒绝相机录像，macOS 没有相机录像器；OpenHarmony 录像器属于独立路径。Windows 静态拍摄只有源码／类型检查证据。音频会话不会补齐这些录像缺口。 |
| 音频与生成媒体 | Makepad 已有原生音频输入、输出，语音输入链路及 `Video` 播放控件。`model.audio` 已能请求生成语音；`model.image`、视频任务、embeddings 也已实现。 | 服务商生成与 OS 录制、播放是不同环节。有这些方法不代表已通过真实服务商调用或设备播放验证。 |
| 嵌入页面、身份认证 | `WebReader::open_on_platform` 已有 macOS、iOS、Android、Linux、Windows 适配器；宿主持有的 `auth` 服务及服务商、应用后端登录流程也已存在。 | 外部链接修复不会代替嵌入式浏览或身份认证。WebView 可用性、服务商注册、跳转策略及各平台登录验收仍分别成立；应用始终拿不到宿主凭据。 |
| 通知、后台工作 | `glance.publish` 可请求卡片通知；`App::glance_notify` 显示 Shell toast 和 Home 通知栏通知。Mail、已连接 Gmail 有特定后台执行及持久事件、通知发件箱路径。 | 这些不是通用 OS 闹钟、推送 token 或任意关闭应用的脚本调度器。后台任务还需要明确的事件契约、应用权限和 OS 生命周期支持，通知本身不提供这些能力。 |
| 剪贴板、分享 | 原生文本编辑和剪贴板操作已存在；受限 WebCard 按自己的授权提供 `clipboard.write`。新增 `files.share` 把 Android 文本分享接到受限应用。 | 通用剪贴板读取、附件分享及跨平台分享面板仍是独立工作。选择器交接不等于收件人收到；原生声明不表示每个平台都已实现。 |

源码入口：[CalendarService 及归属检查](../apps/calendar/host-service/src/lib.rs)、[连接账户契约](../crates/oauth-service/README.zh-CN.md)、[AuthService](../crates/oauth-service/src/host.rs)、[设备服务](../crates/shell/src/platform_services/mod.rs)、[模型媒体契约](../apps/ai-providers/host-service/MEDIA.zh-CN.md)、[Glance](../crates/shell/src/glance.rs)、[`App::glance_notify`](../crates/shell/src/lib.rs)、[mail_background](../crates/shell/src/mail_background.rs)、[connected_events](../crates/shell/src/connected_events.rs)。相关 Makepad 符号为 `CameraPreview::record_start`、`WindowVoiceInput::ensure_audio_callback`、`CxMediaApi`、`Video`、`WebReader::open_on_platform`、`web_card::policed_tool_grant`、`Cx::share_text`。

实际提交能帮助界定问题：[CFAW News #114](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/114) 使用新闻原文外链；[Navigation #116](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/116) 需要定位，也包含应用和 UI 改动；[Muse #112](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/112) 使用原生 EventKit 配套组件。本批不会让这些提交整体自动兼容标准宿主；上面的剩余能力清单也不表示每一项都是参赛者提出的需求。

## 能力发现、验证与后续工作

应用应在 `host_api.required` 中声明所需方法版本，为可降级功能声明 optional 方法，并查询实际宿主的 `runtime.list` / `runtime.describe`。发现结果必须与已注册实现、支持的平台一致。manifest 能力、逐应用授权、OS 权限、前台调用权限仍是不同检查。Shell 在 [`register_host_services`](../crates/shell/src/apps.rs) 注册服务，在 [`App::handle_event`](../crates/shell/src/lib.rs) 分发原生事件。

**Host API 验收通过：Mac 30/30 项，OnePlus 6／Android 15 44/44 项。** Host API Lab **0.4.0** 通过全部三十项具名 OS／公共服务检查（十项 OS 检查加二十项日历／邮件／媒体发现和拒绝检查），Android 还通过原有十四项。Mac 源码 `53bab40f` 和 Android APK 生产源码 `13e3b21a` 使用运行时 `fc938badf`；`53bab40f` 的改动仅影响测试代码，不包含在该 APK 中。[Mac 回执](../tools/fixtures/host-api-lab/evidence/public-api-v0.4/macos.json)和 [OnePlus 回执](../tools/fixtures/host-api-lab/evidence/public-api-v0.4/oneplus6.json)分别记录实际源码、运行时和产物摘要。Mac 还验证了签名工具完成、实时 UI 更新和原生按钮操作；三张截图均已审视，两个测试进程正常退出。Android 测试包完成后已强制停止。

单独的[回归回执](../tools/fixtures/host-api-lab/evidence/public-api-v0.4/regression.json)记录 `53bab40f` 上 **1,051/1,051 项共享 Shell 测试通过，失败和忽略项均为零**，并通过三个打包检查及原生测试应用构建。此前 14 项和 24 项记录继续保留为历史证据。这些回执不验证之后的 Android Video Java 改动、真实日历事件读写、SMTP 投递、亲手批准权限、录音／音频播放或原生文件／照片／分享选择器。这些回执早于发行候选版，不验证候选版最终打包的二进制文件。

查询 `video.playback_controls` 的 `runtime.describe` 返回 `kind: "runtime-abi"`、`callable_via_host_request: false`；应在 `Video` 控件上调用已文档化的方法。单独的 Video 在运行时 `fc938badf` 上已通过全部 16 项 macOS 原生检查，见[回执](../tools/fixtures/video-api-lab/evidence/macos-local-mp4.json)。OnePlus 6 / Android 15 在源码 `942fe1da`、运行时 `7c859055` 上通过全部 20 项回调／解码器检查，见[回执](../tools/fixtures/video-api-lab/evidence/oneplus6-local-mp4.json)。真机验证了 Android 最近帧定位修复、原生停止／重启及播放器释放，未验证物理触摸或 Android 像素。五秒无声的本地 H.264 测试片不能验证流媒体、可听见的输出或广泛编解码器支持。网络 Video 来源目前只检查初始 URL：重定向及嵌套 HLS／播放列表的白名单执行仍是明确缺口，本批不声称这些路径完整遵守受限应用网络策略。

声明时使用 `requires: ["host-api-v1"]`，在 `host_api.required` 中填写准确的方法／ABI 主版本，而不是 `host-api-v1.10` 功能字符串。contract 1.10 是 SDK 版本；实际可用性仍由已安装的宿主及其平台适配器决定。

首批 OS 工作的历史组件验证（不是当前组合源码的验收）：

- 外部链接：macOS 控件编译、Windows/Linux/Android 平台检查、14 项现有策略测试，以及全部 16 个 Android Java 模板编译通过。原有的无关编译警告和弃用警告仍存在。
- 文件：六项原生存储测试、五项对话框测试和四项集成文件服务测试通过。存储检查还覆盖了保留旧有相机大文件读取能力，避免新的传输上限影响它。
- 位置与权限：十项集成平台服务测试全部通过，覆盖新鲜度、精度、取消及超时与撤权边界。
- `d13fe639`（运行时树 `025b17f1`）的集成验证：1,027 项共享 Shell 测试使用全新配置目录运行并全部通过，无忽略项。桌面默认及 mobile 构建、Home mobile 构建、两个 Shell 依赖图检查（宿主/Android、默认/mobile 组合）、18 项 setup 测试和私有路径检查通过。即使直接运行普通 `cargo test`，真实原生模块测试也会自动隔离用户配置。
- 首批源码 `807f2bc8`（运行时树 `9121e900`）修复了类型数组丢失整数值，以及宿主回调让出执行权后丢失 VM 续执行状态的问题。127 项定向运行时测试全部通过，回调回归测试在旧运行时上会失败。1,027 项共享 Shell 测试使用全新配置目录再次全部通过，无忽略项。此次没有配置可选的真实内核链路，这个通过数不验证该链路。
- release 模式的 Mac 签名测试应用通过全部十项新增 OS 检查，同时验证原生按钮操作和实时 UI 更新。同一源码的 Windows/Linux 平台检查及独立 Android APK 构建通过。详见[已去除私有信息的本批记录](../tools/fixtures/host-api-lab/evidence/os-api-batch1/receipt.json)。
- 早先批次记录把 24 项 OnePlus 检查标为**当时待执行**：桌面验收完成后 ADB 找不到获准测试的手机。该 APK 和历史 14 项手机回执不能验证当前的 44 项集成。Android JNI 分配检查、有界数组复制、异常清理和线程附加状态清理已编译通过；Java 编译仍有原有警告。
- 本批的真实外部浏览器启动、文档提供方对话框、真机新鲜定位及 iOS 执行均**未验证**。iOS 外链代码仅经源码审阅，组件检查时没有可用的 iOS Rust target。本文不声称整个发布流程或所有平台已通过。

补齐这些 OS 边界后，下一步是**面向应用公开、由 Rust 实现的图像和电子表格 API**。现有 [`PhotoService`](../apps/photo/host-service/src/lib.rs)、[`SheetsService`](../apps/sheets/host-service/src/lib.rs) 已封装原生 Rust 引擎，但当前系统、原生应用准入规则及共享引擎状态还不是公开的逐应用契约。开放之前，需要可执行方法的发现 schema、应用独占文件或句柄、配额、有界异步任务、取消和生命周期检查。电子表格是明确的产品优先事项；可选照片、OCR 想法不代表每个提交应用的已证实阻塞点。新增适配器应复用 Rust 引擎，而非用脚本重新实现。

类似 JNI 的**动态原生库插件暂缓**；用于连接 Android OS API 的普通 JNI 调用属于当前实现。未来的原生能力包加载器需要单独设计 ABI、信任和隔离边界，不是这些共享服务的前置条件。
