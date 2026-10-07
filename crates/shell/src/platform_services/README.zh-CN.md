# 设备宿主 API

[English](README.md) | 简体中文

已安装的应用可以通过宿主申请摄像头、麦克风和位置权限。清单能力只是上限；用户对单个应用的授权与操作系统对 OctoSense 安装包的授权是两回事。应用授权适用于该应用的各个账号，不授予其他应用。系统允许 OctoSense 使用摄像头，不表示允许所有已安装应用使用它。

首批适配器在 **Android 和 macOS** 实现权限状态、申请与撤销，并提供 **Android 最近已知位置**读取。其他平台明确返回不支持。授予权限不会凭空提供日历、文件选择器、摄像头拍摄或后台定位服务；拍摄仍使用已有的 `CameraPreview` 控件。

应用必须声明 `"requires": ["host-api-v1"]`，并声明所需能力，如 `"capabilities": ["camera", "location"]`。宿主在**执行应用代码之前**设置设备授权检查。App Hub 还可以用 `host_api.required` 校验方法版本；可选功能应查询 API 发现接口。

| 方法 | 参数 | 行为 |
| --- | --- | --- |
| `camera.permission.status` | `{}` | 查询应用授权与系统摄像头权限，不弹窗 |
| `camera.permission.request` | `{}` | 前台应用显示宿主原生授权页；需要时继续显示系统权限弹窗 |
| `camera.permission.revoke` | `{}` | 撤销该应用的设备授权并停止其正在运行的 `CameraPreview`，不修改系统对安装包的授权 |
| `microphone.permission.*` | `{}` | 相同的麦克风授权操作；撤销会停止使用声音的摄像头录像 |
| `location.permission.*` | `{}` | 相同的位置授权操作；Android 授权后启动已有的前台位置监听 |
| `location.get` | `{}` | 仅 Android：重新确认系统权限后读取最近已知位置 |

前台交互示例：

```text
host.request("camera.permission.request", {}, fn(r) {
    if r.is_ok && r.data.os_permission == "granted" {
        ui.preview.start()
    }
})
```

这是 API 示例，不是单独发布的示例应用。响应分别报告 `app_policy_granted`、`app_consent` 和 `os_permission`。系统状态包括 `granted`、`not_determined`、`denied` 与 `settings_required`。已有应用授权时不重复显示宿主授权页；系统已经授权时不重复弹系统权限框。应用应在人选择具体功能时申请权限，而不是启动时索取所有权限。

`location.get` 返回 `latitude`、`longitude`、`accuracy_m`、`source: "last_known"`、`timestamp: null` 与 `freshness: "unknown"`。现有 GPS 缓存没有时间戳，不能把该结果描述为新鲜定位，也不能用于安全关键导航。它不授予后台定位，也不保证关闭定位服务时能得到坐标。

后台或代理申请权限会收到 `authorization_required`。代理可以查询状态、撤销本应用授权，也可以读取已授权的最近已知位置，但不能批准宿主授权页。批准需要真实物理输入；Makepad 自动化或 ADB 合成点击不能替代。受限应用也不能通过复制原生控件来获得批准能力。

服务将原生操作排入宿主 UI 事件循环，按请求 ID 匹配权限结果，并在回答前再次检查安装身份与授权版本；取消的请求不会继续执行。撤销会使较早的待批准请求失效。Android 系统权限弹窗可能暂停 Activity，因此已经发出的、有时限的系统申请保留到结果返回；其他待执行操作和未批准页面在进入后台时取消。一个应用被拒绝不会清除另一个应用的授权。

对于声明 `host-api-v1` 的应用，运行时还会保护已有 `CameraPreview`、`sys.request_location`、`sys.gps` 和地图 GPS 读取路径。身份来自宿主，不来自脚本；每次检查读取内存授权缓存。重启后缓存默认拒绝，应用调用权限方法后加载此前保存的授权。旧应用保留原来的清单策略；未声明 `host-api-v1` 的应用不能调用新设备服务。

Agent 和后台卡片不能通过这些旧接口弹出系统权限窗口。相机预览和录像先检查系统授权；只有在前台发起、返回时仍在前台的请求才可弹窗。已有系统授权仍可使用，声明新协议的相机不会在超时后假定已经获准。`sys.request_location` 可能弹窗，因此需要前台界面；后台代码可在授权后读取 `sys.gps` 或 `location.get`。

授权文件位于应用存储之外的 `.host/device-api-consent.json`，只包含应用 ID、能力名称与授权版本，不记录设备读数或第三方凭据。Unix 文件使用 0600 权限，采用原子更新。

验证证据随实现提交记录。已在 macOS 运行原生 Makepad 策略和摄像头回归测试；新的服务在 OnePlus 6 上的真实权限弹窗、拍摄与位置流程仍为**未验证**，需要安装集成宿主后由用户实际操作。此适配器尚不宣称支持 Windows、Linux 或 iOS。
