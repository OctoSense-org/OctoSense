# 设备宿主 API

[English](README.md) | 简体中文

在 OctoSense `main`（尚未进入任何发布版本）上，已安装的脚本应用可以向宿主申请摄像头、麦克风和位置权限。应用能否使用设备，由三项相互独立的检查决定：

- **能力。** 应用清单声明 `camera`、`microphone` 或 `location`。这是应用能得到的上限。
- **应用授权。** 用户在宿主的原生面板上允许这个应用。授权覆盖该应用的所有账户，不覆盖其他应用。
- **系统权限。** 操作系统授予 OctoSense 本身的权限。单凭它，任何已安装应用都无权使用设备。

本模块的设备服务（`DeviceService`）在 Android 和 macOS 上实现权限的查询、申请和撤销，在 Android 上还能读取最近已知位置。在其他平台上，API 发现接口（`runtime.list` 和 `runtime.describe`）不列出这些方法：`status` 返回 `os_permission: "unsupported"`，`request` 和 `location.get` 以 `unsupported_platform` 失败。获得权限并不会多出日历、文件选择器、摄像头拍摄或后台定位服务。要显示摄像头画面，请使用已有的 `CameraPreview` 控件。

## 声明应用要用的能力

在清单中要求 `host-api-v1`，并声明应用用到的每项能力。下面这段清单片段申请摄像头和位置：

```json
{
  "requires": ["host-api-v1"],
  "capabilities": ["camera", "location"]
}
```

宿主在运行应用的任何源码之前就启用设备授权检查。如果不想让应用装到缺少某个方法的宿主上，把该方法及其确切版本写进 `host_api.required`；App Hub 会在安装和启动时检查，宿主缺少该方法就拒绝安装或启动。对于应用可以不依赖的方法，改在运行时用 `runtime.describe` 查询；这需要 `runtime` 能力。

## 方法

| 方法 | 参数 | 作用 |
| --- | --- | --- |
| `camera.permission.status` | `{}` | 读取应用授权和系统摄像头权限，从不弹窗。 |
| `camera.permission.request` | `{}` | 仅限前台应用：显示宿主的授权面板，需要时再显示系统权限对话框。 |
| `camera.permission.revoke` | `{}` | 撤销本应用的授权，并停止它正在运行的 `CameraPreview`。系统授予 OctoSense 的权限不变。 |
| `microphone.permission.*` | `{}` | 麦克风的同样三个方法。撤销时还会停止正在录制声音的摄像头录像。 |
| `location.permission.*` | `{}` | 位置的同样三个方法。在 Android 上，申请获准后宿主还会启动已有的前台位置更新。 |
| `location.get` | `{}` | 仅限 Android：重新检查系统权限，然后返回最近已知位置。 |

除了 `host-api-v1`，`status` 和 `revoke` 只需要相应的能力。`request` 是应用获得授权的途径，`location.get` 则同时需要应用授权和系统权限。

## 申请权限

在用户选择需要该设备的功能时再申请，不要在应用启动时申请：

```text
host.request("camera.permission.request", {}, fn(r) {
    if r.is_ok && r.data.os_permission == "granted" {
        ui.preview.start()
    }
})
```

这是 API 示例，不是已发布的示例应用。返回结果分别报告三项检查：`app_policy_granted`（能力）、`app_consent` 和 `os_permission`。`os_permission` 的取值为 `granted`、`not_determined`、`denied` 或 `settings_required`（用户已永久拒绝，只能到系统设置中更改）。应用已有授权时，宿主不再显示面板；OctoSense 已有系统权限时，也不会再弹出系统对话框。

授权面板提供 **Not now** 和 **Continue** 两个按钮，其中 **Continue** 只接受亲手点按：Makepad 自动化或 ADB 发出的合成输入都不算数，应用也不能挂载面板控件的副本来批准自己。**Not now** 接受任何输入，它会关闭面板，并以 `cancelled` 结束申请。面板打开 5 分钟后过期，申请以 `timeout` 失败。

## Agent 与后台代码

Agent、后台卡片（例如应用在速览栏上的卡片），以及它们启动的回调和定时器，都不能申请权限。App Hub 会先拒绝这类调用，报告 `<method> is unavailable to agents/background surfaces`，因此授权面板和系统对话框都不会出现。它们仍可以调用 `status` 和 `revoke`；应用获得授权和系统权限后，也可以调用 `location.get`。

## 读取位置

`location.get` 返回 `latitude`、`longitude`、`accuracy_m`、`source: "last_known"`、`timestamp: null` 和 `freshness: "unknown"`。宿主的 GPS 缓存不记录时间，所以无法判断这个位置是何时取得的：不要把它当作当前位置展示，也不要用于安全攸关的导航。它不授予后台定位，设备关闭定位服务时也不保证能得到位置。没有最近已知位置时，它以 `location_unavailable` 失败。

## 旧的设备接口

对于要求 `host-api-v1` 的应用，同一份授权也约束这些旧接口：`CameraPreview`、`sys.request_location`、`sys.gps` 和地图的 GPS 读取。应用身份由宿主提供，授权按能力分别缓存。宿主启动后，缓存对每项能力一律拒绝，直到应用调用该能力的某个权限方法，把保存的授权加载进来。所以使用这些接口之前，请先调用对应能力的 `status`：使用 `CameraPreview` 之前调用 `camera.permission.status`，录制声音之前调用 `microphone.permission.status`，读取 `sys.gps` 或地图 GPS 之前调用 `location.permission.status`。

Agent 和后台卡片同样不能借这些接口弹出系统对话框。`CameraPreview` 在预览或录像之前先检查系统权限；只有从前台发起、并且检查结果返回时仍在前台的请求才可以弹窗。只要应用已获授权，后台代码仍可使用 OctoSense 已有的系统权限。在要求 `host-api-v1` 的应用中，摄像头绝不会把超时当作批准。`sys.request_location` 可能弹窗，因此必须在前台调用；应用获得授权后，后台代码可以读取 `sys.gps` 或调用 `location.get`。

没有要求 `host-api-v1` 的应用沿用此前只看清单的规则，它们调用这些方法会以 `host_requirement_missing` 失败。

## 错误

| 错误 | 出现时机 |
| --- | --- |
| `host_requirement_missing` | 清单没有要求 `host-api-v1`。 |
| `permission_denied` | 清单缺少该能力；或在请求等待期间，应用已卸载、失去该能力，或授权已变化。 |
| `invalid_arguments` | 参数不是 `{}`。 |
| `method_unavailable` | 方法不在上表之列，例如 `camera.get`。 |
| `authorization_required` | 在没有应用授权或没有系统权限时调用了 `location.get`；或 `request` 到达时宿主无法弹窗，例如宿主正处于后台。 |
| `unsupported_platform` | 当前平台没有实现 `request` 的适配器，或在 Android 以外调用了 `location.get`。 |
| `cancelled` | 用户选择了 **Not now**，同一能力的新申请替换了面板，或宿主离开了前台。 |
| `timeout` | 授权面板打开超过 5 分钟。 |
| `busy` | 已有 64 个设备请求或 64 个授权面板在等待。 |
| `location_unavailable` | Android 没有最近已知位置。 |
| `the host service timed out` | 超过了 App Hub 的 60 秒时限。授权面板打开期间计时暂停；例如面板关闭后系统对话框一直开着，就会出现这个错误。 |

## 宿主如何处理请求

设备服务把每个原生操作排入 Shell 的 UI 事件循环，并按请求 ID 匹配系统返回的结果。回复之前，它会再次确认应用仍已安装、仍有该能力，且授权版本没有变化。如果请求在此期间已经结束（应用已关闭，或 App Hub 已判定超时），服务会直接丢弃它。撤销授权会使所有仍在等待批准的旧请求失效；一个应用遭到拒绝，也不会清除另一个应用的授权。

在 Android 上，系统权限对话框可能让 Activity 暂停。已在等待该对话框结果的请求会保留下来；其他排队中的请求，以及所有尚未批准的授权面板，都会取消。

授权保存在 `<apps root>/.host/device-api-consent.json`，位于所有应用的存储之外。文件只包含应用 ID、能力名称、授权标志及其版本，不含设备读数或提供商凭据。在 Unix 上，文件权限为 0600，每次更新都以原子方式替换整个文件。

## 验证

**已验证：**在 macOS 上运行了原生 Makepad 策略测试和摄像头回归测试，全部通过，结果随实现改动一并记录。[Host API Lab](../../../../tools/fixtures/host-api-lab/README.zh-CN.md) 也通过本服务读取了真实的摄像头权限状态。

**未验证：**在任何平台上亲手点按批准授权面板、真实的系统权限对话框和摄像头拍摄，以及 OnePlus 6 上的定位流程。这些都需要在设备上安装集成后的宿主，并由用户亲手点按面板上的按钮。
