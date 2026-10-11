# Camera

[English](README.md) | 简体中文

内置的 `os.camera` 使用 Makepad 原生 `CameraPreview` 控件。预览帧不经过脚本，
`on_capture` 在拍摄完成后提供应用内的相对路径。Shell 提供应用存储与设备身份。
相机和麦克风仍需逐应用宿主授权及 OS 权限；后台回调不能因为应用后来回到前台，
就获得弹出权限对话框的权限。

manifest 要求 `host-api-v1` 和 `camera.capture_intent@1`，旧宿主会拒绝这个
bundle，而不是忽略拍摄选项。只读的 `camera.capture_intent` 方法报告默认值和支持
的选项。应用打开预览前请求宿主／OS 相机授权，有声录像前请求麦克风授权。Shell 的
统一设备授权适配器目前支持 Android 和 macOS；其他平台明确报告不支持，不绕过授权。

## 明确拍摄意图

能力声明描述预计使用的 API；API 可用并不代表应用选择录音或导出。每次调用指定选项：

```text
ui.cam.capture()                         // 本地 JPEG，不导出图库
ui.cam.capture({library: false})         // 明确选择相同结果
ui.cam.record_start()                    // 平台支持时录制本地无声视频
ui.cam.record_start({audio: true library: false})
ui.cam.record_stop()
```

两个选项均默认 `false`。只接受布尔值；字符串、数字、未知字段或多余参数会返回
`false`，并通过 `error()` / `on_error` 说明原因。`audio` 只适用于录像，不适用于
照片。明确请求声音却未获得麦克风授权时拒绝请求，不会悄悄改成无声录像。异步 OS
授权期间保存原始选项和前台／后台来源，后续调用不能修改已等待授权的拍摄请求。

`library: true` 明确要求额外导出到图库。它需要前台权限和实际支持导出的后端；
不支持的平台在**开始拍摄前**拒绝。OpenHarmony 有现有的 ArkTS 图库交接；本次
改动不会为 Android、Windows 或 Apple 增加图库导出。图库交接不代表保存成功的回执。

内置应用明确选择本地保存，视频模式明确请求声音。最近缩略图列表最多保留 30 个路径，
缩减列表不会删除拍摄文件，也不会假设图库已有副本。存储额度仍然有效；存储已满或不存在
时显示错误。

## 平台边界和验证

本次不新增拍摄后端。Android 使用已有静态拍摄，录像明确报告不支持；Windows 使用
有界静态拍摄工作线程，不支持录像；macOS 没有相机录像适配器。OpenHarmony 沿用已有
拍摄、录像和图库桥接。具体能力仍需在目标设备上验证。

运行时补丁为
[`makepad-camera-capture-intent.patch`](../../tools/runtime-patches/makepad-camera-capture-intent.patch)。
它扩展现有 CameraPreview 测试，覆盖选项类型、默认行为、缺少麦克风授权、后台导出拒绝、
等待授权期间保留意图，以及不支持导出时在创建文件前失败。在准备好的运行时中运行：

```sh
MAKEPAD_HIDE_WINDOWS=1 cargo test --locked -p makepad-widgets camera_preview::tests -- --test-threads=1
```

这些是逻辑测试，不打开相机硬件，也不验证 GPU 绘制、OS 对话框、录音或图库交付。
真实设备验收另行进行。参见 [OS API 状态](../../docs/host-os-api-status.zh-CN.md)。
