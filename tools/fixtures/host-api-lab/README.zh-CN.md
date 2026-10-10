# Host API Lab：原生宿主 API 验收

[English](README.md) | 简体中文

[Android 复现步骤与 OnePlus 6 结果](ANDROID.zh-CN.md)：测试应用 0.4 在 **OnePlus 6／Android 15 上通过全部 44 项检查**，没有启动模型、登录账户或批准权限。原始 14 项记录继续保留为历史证据。

这个开发测试示例演示应用自己的 Splash 工具如何调用已经编译进 OctoSense 的 Rust 代码。工具读取 macOS 上真实的摄像头权限状态，更新应用界面，并把结构化结果返回给原生调用方。它还发现文件/定位 API，在自身存储隔离目录中写入并读回四个合成字节，验证后台调用被拒绝。它从不采集媒体、启动定位采样、打开文件选择器或批准设备访问。它不是提交给 App Hub 的应用，也不能用来加载任意 Rust 库。

一次调用经过以下路径：

```text
临时签名目录 → Store 安装 / 校验后启动
    → 应用包身份通过验证的隔离应用
    → apilab.inspect → app_tool(name, call_id)
    → host.request("camera.permission.status", ...)
    → Rust DeviceService → Makepad 原生权限查询 → macOS
    → 回调 → 更新运行中的应用界面 + mod.app_tools.complete(call_id, result)
```

测试宿主把调用直接放进已授权的工具调用队列。真实 Agent 的调用要先经过 Shell 的中转，由它检查用户对应用 Agent 的同意，以及账户和工具权限；本测试**不**覆盖这段模型与 peer 路径。它覆盖的是真实的签名应用包检查、Splash 隔离环境和[设备服务](../../../crates/shell/src/platform_services/README.zh-CN.md)。

## 公共日历与邮件检查（测试应用 0.4）

当前版本增加公共 `device_calendar` 和 `mail` API，需要已发布的 [app-contract 1.10.0](https://crates.io/crates/octosense-app-contract/1.10.0) 声明及兼容的宿主实现，并保留之前的 `files` 能力。仅安装 SDK 不会提供这些宿主实现。新增十一项检查涵盖四个方法描述、原生日历权限状态、未获应用同意时拒绝列出日历、拒绝后台权限申请/日历选择/事件修改，以及无账户时拒绝准备邮件和拒绝后台发送。测试邮件服务使用合成传输，无法投递真实邮件。

另外九项检查发现照片选择、文字分享、播放、录音 API 和 Video 控制运行时 ABI，并验证后台媒体请求（包括录音）被拒绝。未声明麦克风能力、未获得应用授权时，仍能读取麦克风权限状态。文字分享仅在 Android 上声明可用，不打开任何媒体设备。共二十项检查补充下文的十项 OS API 检查，不证明真实日历读写、亲手批准或 SMTP 投递。[Mac 回执](evidence/public-api-v0.4/macos.json)记录全部 30 项检查通过；[OnePlus 6 回执](evidence/public-api-v0.4/oneplus6.json)记录全部 44 项 Android 检查通过，其中包含原有 14 项。这些记录绑定各自列出的源码和运行时摘要；此前 14 项和 24 项记录继续作为历史证据保留。

## 在 macOS 上运行

需要 macOS 图形会话。窗口始终隐藏，也不会抢占焦点。在 OctoSense 检出目录中运行以下命令；构建所用的固定版本框架源码由 `tools/setup.py` 准备：

```sh
python3 tools/setup.py
cargo build --locked --release -p octosense-shell --example host-api-lab --features acceptance-fixtures
# 用本宿主固定的 App Hub 版本构建 Hub CLI：
cargo build --locked -p octosense-app-hub --bin hub
python3 tools/test-host-api-native.py --hub target/debug/hub
```

脚本会创建一个私有证据目录；如果要自己指定目录，请用 `--output` 传入一个尚不存在的路径。脚本最后输出一行 JSON 结果：成功时为 `{"result": "pass", "evidence": "<证据目录>", "error": null}`，退出码为 0；失败时 `result` 为 `"failed"`，`error` 说明失败原因，宿主的输出保存在证据目录下的 `preview.log` 和 `signed.log` 中。

脚本会复制测试示例，截取它的原生预览作为商店截图，为副本写入摘要，用只存在于内存中的密钥签名，安装到全新的测试配置目录中，再运行原生检查。它既不改动源码文件，也不改动任何普通用户配置。只有本次运行的隔离测试信任根接受这个签名；签名不会创建发布者身份，也不会改动公开的签名目录。

即使准备阶段失败，`result.json` 也会记录结果和仍未验证的范围。`native-result.json` 保存原生状态和工具的返回结果。PNG 图片和控件快照展示真实的预览和执行完毕的应用。日志和私有配置目录都留在证据目录中；分享之前请先自行检查。脚本只关闭它自己启动的测试进程。

## 检查内容

- 已签名的应用调用自己声明的 `implemented_by: "app"` 工具。
- 工具读取真实的原生权限状态，并更新正在运行的界面。
- 应用界面上的按钮也能调用同一个宿主服务。
- `runtime.describe` 能发现已编译的 API；对于宿主没有的自定义函数，它报告不可用，而且不执行任何代码。
- 没有 `microphone` 声明时，应用仍能读取原生麦克风权限状态，且 `app_consent` 保持为 false。来自应用工具的录音请求由仅限前台调用的边界拒绝。
- 工具的异步宿主回调不能打开权限面板。
- 能发现文件状态、导入、导出和定位采样 API；二进制存储被标明为运行时 ABI，而不是可通过 `host.request` 调用的方法。
- 真实的隔离 VM 用 `fs.write_bytes` / `fs.read_bytes` 往返读写字节 `0、127、128、255`，随后删除自己的临时文件。
- 文件状态返回应用的有界存储可用性和字节上限；后台导入、导出在原生界面打开前被拒绝。
- 来自工具后台回调的定位采样返回确切的 `location.sample is unavailable to agents/background surfaces` 拒绝错误。它的 API 描述也必须标明仅限前台调用。
- 账户不对、工具未声明或输入无效的调用都会遭到拒绝。
- 持有工具的应用关闭后，调用以 `app_not_running` 失败。

工具会在宿主回调中刻意调用 `camera.permission.request`；这个回调保留了工具的后台来源，以此证明 App Hub 会拒绝这次申请。测试示例中没有任何环节能批准权限。应用身份、应用授权和系统权限是独立的检查。能力声明描述预期用途，不授权设备访问。这台 Mac 可能早已授予 OctoSense 摄像头权限，但全新的测试配置目录仍必须报告 `app_consent: false`。

当前原生定位检查证明宿主保留回调的后台来源，并在启动原生工作之前拒绝采样。为兼容原有记录，回执键仍为 `location_without_consent_refused`，但断言现在要求上述确切的后台拒绝错误；早先的 `authorization_required` 结果属于下文的历史回执。独立的 `platform_services::tests::location_sampling_broker_lifecycle` 回归测试会在隔离测试存储中建立应用授权，并仍要求该后台拒绝错误，而且没有排队请求、权限检查、授权审阅或正在运行的定位采样。该回归测试后续使用模拟权限结果检查生命周期；两种检查均不证明实际批准了权限或获得了真实定位。

0.4 的 release 模式 Mac 运行在源码 `53bab40f`、运行时 `fc938badf` 上通过 **30/30 项具名 OS 和公共服务检查**，以及签名工具完成、实时 UI 更新和原生按钮操作。三张原生截图均已审视，两个测试宿主进程均正常退出。[Mac 回执](evidence/public-api-v0.4/macos.json)用源码、运行时和二进制摘要绑定结果。[OnePlus 6 运行](evidence/public-api-v0.4/oneplus6.json)在 **Android 15 上通过 44/44 项检查**，APK 使用生产源码 `13e3b21a` 和相同运行时；构建期间的 `53bab40f` 改动仅影响测试代码，不包含在该 APK 中。完成后已强制停止独立测试包。

单独的[回归回执](evidence/public-api-v0.4/regression.json)记录 `53bab40f` 上 **1,051/1,051 项共享 Shell 测试通过，失败和忽略项均为零**，同时通过三个打包检查（桌面默认／mobile、Home mobile）及原生测试应用构建。这些回执不验证之后的 Android Video Java 改动，也不验证真实账户或硬件操作。SDK 1.10.0 已发布；[宿主分发状态](../../../docs/host-os-api-status.zh-CN.md)单独记录。这些历史回执不验证最终 Desktop RC2 发行包，也不会更新已发布的 Home beta.1。

[早先批次记录](evidence/os-api-batch1/receipt.json)记录源码 `807f2bc8` 的十项 OS 检查；`evidence/android/` 保留原始 14 项手机记录。这些历史结果不能验证当前源码。当前测试将“未声明即拒绝”改为麦克风状态可读、应用尚未授权、后台录音与定位精确拒绝；这些语义需要新的验收回执。

**此前已验证**：`.github/workflows/desktop.yml` 的 `native-host-api` 任务在 GitHub `macos-14` 运行器上，为添加本测试示例的改动运行了上述命令（另加 `--output` 指定证据目录），全部通过。

**本次 macOS 运行未覆盖**：真实的模型推理、亲手批准权限、摄像头拍摄、交互式文件／照片／分享选择器、实时定位采样、原生浏览器启动、日历事件读写、SMTP 投递、录音与音频播放、Android、Linux 和 Windows 设备服务，以及发布兼容的宿主二进制文件。Android 的结果见[单独的 OnePlus 6 验收记录](ANDROID.zh-CN.md)。另有在真实的 Splash VM 上运行的运行时回归测试，覆盖分离的定时器、暂停的任务、HTTP 和 WebSocket 回调，以及原生设备辅助函数中的检查；本测试示例覆盖的是链式宿主回调。

## 复用这一模式

把工具声明和 `app_tool` 钩子复制到你的应用中，并描述真正用到的能力。设备访问仍需运行时应用授权和系统权限。钩子在已打开应用的 VM 中运行，所以应用关闭后无法执行这些工具。`host_method` 工具则直接映射到白名单中的某个 Rust 服务方法。两种机制都不会从应用包编译新的原生代码。把应用必需的方法写进 `host_api.required`，App Hub 就不会把它装到不兼容的宿主上；再用 `host_api.optional` 配合 `runtime.describe`，在缺少某个方法时提供降级路径。App Hub 的[脚本工具参考](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.zh-CN.md#脚本工具执行script-tools-v1)介绍了这个钩子，[ADR 0012](../../../docs/adr/0012-app-host-api-discovery.zh-CN.md) 说明了宿主边界。
