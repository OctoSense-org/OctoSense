# 宿主 API 兼容性

[English](HOST-API.md) | 简体中文

契约 1.6 让应用可以声明自己需要的宿主 API，并查询宿主实现了哪些 API。它增加的是声明和发现机制，而不是访问权限：应用只能调用宿主中已编译的 Rust 服务，每次调用仍要经过应用自己的授权。同一版契约还让应用包可以声明自己的后端（[登录自己的后端](PUBLISHING.zh-CN.md#登录自己的后端)），并运行自己的 Agent 工具（[脚本工具执行](PUBLISHING.zh-CN.md#脚本工具执行script-tools-v1)）。

这些发现机制的声明已随契约 1.8.0 发布。历史 [RC2 发行版](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2)（源码 `4ccf8e06`）和 RC1 一样按下文的平台限制实现这些 API。当前契约 1.11 策略请使用[桌面 RC4](../README.zh-CN.md#下载兼容宿主)。旧宿主（例如 OctoSense 桌面版 0.1.0-beta.2）不提供这些 API，并会拒绝要求这些 API 的应用。

已发布的契约 **1.10.0** 还支持 `files`、`device_calendar` 和 `audio`
（[版本与发布凭据](../crates/app-contract/README.md#versions-on-cratesio)，英文）。
文件能力包含未单独发布的 1.9.0 工作。桌面 RC2 锁定这版契约，并在下文各节注明的平台上实现这些原生适配器；RC1 一个都没有，仅发布契约包也不会给已安装的宿主增加任何实现。

## 当前源码策略：声明与授权

`capabilities` 和 `network.hosts` 用于向用户和审核人员说明应用预计使用的
API 与网络目的地。遗漏某个服务族或主机，不会阻止调用宿主已支持的公开 API。
这项策略已包含在[桌面 RC4](../README.zh-CN.md#下载兼容宿主)中。RC2 的历史行为另行记录，不代表当前的授权规则。

每个准入应用都有独立的存储沙箱、已解析的配额和网络模块。设备访问仍需逐应用
同意和系统权限；连接账户仍按应用、账户和提供商 scope 隔离。外部写入保留
原生审阅，Agent 保留用户启用和工具审核，访问其他应用的数据或工具仍需共享
授权。宿主私有用户资料和不带调用方身份的 `agent.notify` 不是公开 API。

`requires` 和 `host_api.required` 仍用于检查兼容性，不是能力授权。组件的
ABI 与导入验证、确切摘要、发布者证明、目录撤回、配额和平台支持检查继续生效。
遗漏使用声明可以产生审核警告。`AppPolicy::allows` 与 `allows_host` 是历史
声明查询接口；宿主不能把它们当作执行授权。请用 `runtime.list` 和
`runtime.describe` 查询实际方法及其平台、同意要求。


兼容运行时必须包含显式拍摄意图：`capture({library:true})` 与
`record_start({audio:true,library:true})`；省略的选项为 false。开放运行时
能力不能隐式开始录音或导出照片。这份运行时补丁与更新的 Hub 适配器须一起发布。
只有宿主同时提供按应用区分的同意代理，并注册了 `camera.capture_intent@1`
支持时，适配器才开放麦克风和照片库运行时标志。清单声明不能代替这两个条件。

## 声明应用需要什么

下面的清单片段要求 `runtime.list`，并在宿主提供 `location.get` 时使用它：

```json
{
  "requires": ["host-api-v1"],
  "capabilities": ["runtime", "location"],
  "host_api": {
    "required": {"runtime.list": 1},
    "optional": {"location.get": 1}
  }
}
```

| 字段或标记 | 含义 |
| --- | --- |
| `host_api.required` | 应用缺了就无法运行的方法，每个都写明确切的 ABI 主版本。版本 2 不能满足对版本 1 的要求；这个版本也不是宿主的发布版本号。 |
| `host_api.optional` | 宿主提供时应用才会使用的方法。请为每个方法准备替代路径。 |
| `host-api-v1` | 使用 `host_api` 时必须声明。它还会让相机、麦克风和位置改由每个应用单独授权，因此宿主必须实现 `app_policy.device_consent@1` 运行时 ABI。只会解析这些字段的运行器并不满足这项要求。 |
| `backend-api-v1` | 使用 `backend` 时必须声明。宿主必须实现 `auth.backend.request@1`。 |
| `script-tools-v1` | 使用 `implemented_by: "app"` 的工具时必须声明。宿主必须实现 `app_tools.dispatch@1` 运行时 ABI。 |

商店在安装应用时检查这些要求，之后每次打开应用时再检查一次。宿主无法满足的应用，商店会拒绝，报 `app <id> needs a host implementing <method>@1` 或 `this host does not implement required APIs: <method>@<version>`。签名目录中的新版本即使需要更新的 API，也不会撤下宿主仍能运行的已安装版本。

执行应用源码前，共享运行器的 `apply_device_consent` 辅助函数将已验证应用包的身份绑定到
实际的 Splash 堆。调用方必须先完成应用包准入；该函数不能代替准入。原生存储访问同时
要求身份精确匹配并存在应用自己的隔离目录。绑定身份不会授予设备授权或操作系统权限。
绑定失败会清除旧身份，卡片运行器在拒绝运行或关闭时也会清除身份。未经补丁的 Makepad
仍拒绝要求其不支持的设备授权代理的应用。

## 查询宿主实现了什么

### WASI 套接字（尚未发布）

导入 `wasi:sockets` 的组件要求在 `host_api.required` 中声明 `wasm.sockets@1`，并在 `requires` 中使用已有的 `host-api-v1` 特性。这是运行时 ABI 标记，不是 `host.request` 方法；TCP、UDP 和 DNS 操作使用标准 WASI 接口。RC5 能读取此要求但不能满足，因此只拒绝安装这个应用，不会使整个目录失效。应用固定共享套接字组件时也有相同要求。准入和声明规则见 [WebAssembly 组件](PUBLISHING.zh-CN.md#webassembly-组件)。请在实际宿主中确认运行时及平台支持；本次源码变更不会更新已发布的二进制。

### 原生日历（契约 1.10）

新增 `device_calendar` 能力与 `calendar`（系统日历应用自己的存储）和 `gcalendar`（Google 连接器）互相独立，不会隐含授予它们或操作系统权限。需要原生日程的应用应声明 `device_calendar`、`host-api-v1`，并设置 `host_api.required: {"device_calendar.events.list": 1}`，以及完整流程需要的其他方法。缺少这些方法的宿主会拒绝安装。

已发布的契约允许声明该能力，App Hub 的策略允许声明只读工具别名。桌面 RC2 自带 macOS 适配器（EventKit）和 Android Home 适配器；RC1、Home beta.1、Windows 和 Linux 都没有。适配器实施应用及账户隔离、操作系统权限、日历选择和修改操作的可信确认（亲手点按）。与系统日历的实际交互仍待设备验收，重复日程和参与者为只读；支持平台与 Agent 访问范围以服务的方法描述为准。

### 邮件草稿与发送审阅（桌面 RC2）

兼容宿主提供 `mail.compose`，用于修改与应用及账户绑定的本地草稿，并通过 `mail.compose_status` 查询状态。两者的 Agent 别名最低风险分别是 `act` 和 `read`，都须声明 `private_data: true`，且不会发送邮件。前台应用调用 `mail.review_send`（或同样需要审阅的兼容入口 `mail.send`），宿主显示确切草稿，只有用户亲手批准后才发送。请声明所需方法版本；如果商店信息列出 Windows 或 Linux，请把 `mail.review_send` 放在 `host_api.optional` 下。桌面 RC2 向任何获得 `mail` 授权的应用提供这套流程：撰写草稿和查询状态在所有桌面平台可用，发送审阅只在 macOS 和 Android 上可用（在 Windows 和 Linux 上会以 `Physical Mail send approval is unavailable on this platform` 失败）。RC1 完全不提供，原有的系统邮件草稿方法仍仅限系统应用，真实的 SMTP 投递也尚未验证。

### 运行时清单

应用可以用 `{}` 调用 `runtime.list` 列出所有已描述的方法，或用 `{"method":"location.get"}` 调用 `runtime.describe` 查询单个方法：

- 方法描述包含输入和输出 schema、ABI 版本、用途服务族、支持的平台，以及 Agent 能否调用它。
- `runtime_features` 列出运行时 ABI，例如 `app_tools.dispatch@1`。`host.request` 不能调用运行时 ABI。
- 只有注册了描述的方法才会出现。一些较早的服务没有描述，所以这份列表并不完整。
- 结果不含账户数据或凭据。

API 可用不等于已经配置，也不等于已经授权。`configured: null` 表示宿主并不知道配置情况，`authorization: "checked-on-call"` 表示每次调用时，宿主都会检查应用的授权。要了解某项服务是否已配置、已授权，请调用它的状态或账户方法。声明依赖既不会开启操作系统权限，也不会添加提供商注册信息。

## 用户选择的文件传输（随契约 1.10 发布）

`files` 声明描述宿主的文件选择窗口，不代表任意宿主路径访问权限。当前宿主为每个
准入应用提供带配额的私有隔离目录；导入和导出不要求声明 `files` 或 `storage`
服务族。用户仍须在前台的宿主文件窗口中选择文件。通过 `host_api.required`
要求 `files.import@1` 或 `files.export@1`，也可以将其声明为可选并检查
`runtime.list`。桌面 RC2 实现了这些 API，RC1 没有，仅支持新契约从来不代表宿主已实现；
`files.status` 返回当前平台适配器是否可用。

配套的 OctoSense 实现将用户选中的单个文件导入应用内的新路径，或将现有
应用文件的快照导出。应用只收到应用内路径和字节数，不会收到系统路径或
Android 文档提供方 URI。传输仅限前台，agent 和后台任务不能发起选择窗口。
原生 `fs.write_bytes` 对应 `storage.binary_write@1` 运行时 ABI，不能通过
`host.request` 调用。

导入保留现有的单文件 1 MiB 上限和应用存储配额，并拒绝覆盖已有文件。
取消选择返回 `{"cancelled":true}`。当前适配器覆盖 macOS、Windows、Android
及安装了原生文件对话框辅助程序的 Linux；暂不支持 iOS、OpenHarmony、web
和直接使用 framebuffer 的 Linux。编译与真机验证状态以宿主状态和发布说明为准。

### 选择照片与分享文本（桌面 RC2）

配套宿主新增 `files.pick_photo({"path":"photos/chosen.jpg"})`，将照片导入
应用的私有隔离目录。它打开图片选择窗口，验证 PNG、JPEG 或 WebP 文件签名后，
导入应用内的新路径。返回 `path`、`bytes`、`mime`，取消则返回
`{"cancelled":true}`。单文件 1 MiB 上限仍适用：过大的原图会明确失败，不会
悄悄缩小图片。

`files.share({"text":"一条简短记录"})` 打开 Android 分享选择器，本批次仅支持 Android。
文本须为 1–8192 个 UTF-8 字节。前台调用打开原生分享选择器；只有成功交给
选择器后，才返回 `{"handoff":"chooser_opened","delivery":"unknown"}`。
这不证明对方已收到内容，也不支持附件。两种方法都没有 Agent 别名。请声明
所需方法版本并检查宿主状态；桌面 RC2 包含这些 API，RC1 不含。新适配器的设备
验收仍待完成。

## 前台音频会话（契约 1.10）

桌面 RC2 的 macOS 构建和兼容的 Android 宿主提供以下方法，RC1、Windows 和 Linux 不提供；请声明方法版本，不要仅凭能力名称推断支持情况，商店信息列出没有这些方法的平台时请把它们放在 `host_api.optional` 下：

| 方法 | 用途声明 | 行为 |
| --- | --- | --- |
| `microphone.record_start({path,max_duration_ms})` | `microphone`、`storage` | 获得应用和系统授权后，录制最多 30 秒的单声道 16 kHz PCM WAV，写入应用内的新文件。 |
| `microphone.record_status/record_stop/record_cancel({session})` | `microphone` | 查询、停止并保存，或丢弃本活跃应用的录音。 |
| `audio.play({path})` | `audio`、`storage` | 播放有大小限制的本地 WAV、MP3、FLAC 或 Ogg 文件。 |
| `audio.status/audio.stop({session})` | `audio` | 查询或停止本活跃应用的播放。 |

表中列的是用途声明，不是执行授权。当前宿主不会仅因遗漏服务族声明而拒绝
这些方法；录音仍须获得下文所述的授权。

每次返回包含不透明的 `session`、`status`、`path`、`error`、`frames` 和 `format`。
启动返回 `starting` 不代表已成功录音或播放；轮询状态，直到第一个原生回调
报告 `recording` 或 `playing`，并处理最终失败。停止录音先返回 `stopping`，
只有状态变为 `saved` 才能使用其路径。设备帧数不证明实际可听见声音。输入及
输出文件仍受现有 1 MiB 上限约束，解码后播放另有时长及内存限制。

这些 API 仅限前台，没有 Agent 别名。播放不会授予麦克风权限；录音仍需要
`microphone.permission.request` 和系统授权。会话绑定活跃应用隔离环境，应用
失去前台界面或权限时停止，且不自动恢复。宿主必须协调与听写的麦克风占用。
没有独立活跃应用身份的商店内嵌预览拒绝录音，请打开已安装应用。源码及合成
测试不能证明真实麦克风与扬声器验收通过。

## 限制

- **Agent 工具。** `host-service` 工具可以用 `host_method` 映射到 `auth.backend.me`、`auth.backend.request`、`runtime.list`、`runtime.describe`、三个 `*.permission.status` 方法或 `location.get`，最低风险为 `read`，并且需要 `private_data: true`（[把工具映射到共享服务](PUBLISHING.zh-CN.md#把工具映射到共享服务host_method)）。Agent 的调用从不弹出提示，所以 `auth.backend.request` 只执行已声明的 `GET` 操作，宿主会在发出任何 HTTP 请求之前拒绝写操作；写操作仍须在前台应用中发起，并在宿主的原生审阅界面上确认。权限申请和撤销、账户管理和面板控制都没有 `host_method`。准入不能代替宿主对账户、授权和平台的检查。
- **后端。** 宿主只从已准入的签名应用包读取 `backend` 块。它返回不透明的连接句柄，拒绝重定向和任何含有令牌的后端应答，并在原生审阅界面上让用户批准每一次写操作。后台调用和 Agent 调用都无法批准写操作。修改端点、更新应用或撤回应用，都会结束应用的后端会话。
- **脚本工具。** 签名的 `app_tool(name, call_id)` 处理函数运行在已打开的完整应用中，使用该应用自己的 VM 和存储文件夹。它不加载任何原生库或 Wasm；应用关闭时返回 `app_not_running`。
- **平台。** 设备权限方法支持 Android 和 macOS；`location.get` 只支持 Android，返回上次已知的位置，时效未知，而 RC2 的 `location.sample` 在 macOS 和 Android 上返回新鲜位置。两个 RC 都在 Windows（WebView2）和 Linux X11/XWayland（WebKitGTK）中嵌入普通 `WebReader` 网页；原生 Wayland 不支持。Linux/Windows 声明 `auth.backend.request@1`，用于清单声明的读取，并实现外部浏览器后端登录；RC2 补上了登录所需的原生链接打开方式，用 RC2 源码构建的 Windows 测试程序已对一个模拟后端完成浏览器登录，Linux 上的登录未验证。嵌入式后端登录和受保护的写操作仍不支持，会拒绝执行。
- **权限申请。** 只有在前台的应用才能申请权限。来自 Agent 或后台卡片的申请会失败，返回 `<method> is unavailable to agents/background surfaces`。宿主转入后台时仍在等待的申请会失败，返回 `authorization_required`；在用户授予应用位置权限之前，`location.get` 和 `location.sample` 也返回 `authorization_required`。
- **设备控件。** 在声明了 `host-api-v1` 的应用中，`CameraPreview`、`sys.request_location`、`sys.gps` 和地图的 GPS 读取同样需要该应用的设备授权。宿主每次启动后，这些功能都保持关闭，直到应用调用对应能力的权限方法，载入为该能力保存的授权：启动 `CameraPreview` 之前调用 `camera.permission.status`，读取 GPS 之前调用 `location.permission.status`。请在应用打开时调用这些方法。
- **`card-host`。** 三个标记所需的 API，它一个也没有实现，因此会拒绝要求这些标记的应用（[在本地运行应用包](DEVELOPMENT.zh-CN.md#在本地运行应用包card-host)）。
- **未验证：** 亲手点按批准权限、相机拍摄、真实提供商、真实模型、设备日历的读写、SMTP 投递、音频录制和播放、原生文件和照片选择器，以及宿主 API 在 Linux、Windows 和手机上的验收。

具体调用方法见 OctoSense App Flow（原 Design Flow）的[发现并使用宿主 API](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-V1.zh-CN.md)。
