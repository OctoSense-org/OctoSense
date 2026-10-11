# Windows 本地相机静态拍摄

[English](windows-camera-capture.md) | 简体中文

Windows 运行时补丁为已打开的 Media Foundation 相机增加
`CameraCaptureRequest::Photo`。受限应用使用现有 `CameraPreview.capture()`，
只有 JPEG 保存成功才收到 `on_capture`。应用仍需要宿主相机授权、OS 相机权限和
自己的存储沙箱。后端不会增加授权、选择其他应用的存储，也不会因后台工具调用而打开相机。

拍摄意图由参数明确表达：`capture()` 和 `capture({library: false})` 都只保存到本地。
清单声明不会自动触发图库导出。Windows 在拍摄前明确拒绝
`capture({library: true})`；现有图库导出适配器仅支持 OpenHarmony。
支持录像的平台通过 `record_start({audio: true})` 请求声音，仍需麦克风同意。
不传参数时，录像默认为静音且只存本地。Windows 的录像开始、暂停、继续和停止也明确报告不支持。

每个相机按需启动长期工作线程，同时只接受一个静态拍摄请求。Media Foundation
回调只把请求后的下一帧复制到有界队列。JPEG 解码、编码和文件写入都在工作线程执行。
支持经过验证的 MJPEG，以及偶数尺寸的 NV12、YUY2；YUV 转换沿用现有读取器的
BT.709 解释。其他格式明确失败。

限制为每边最多 4096 像素、最多 8 百万像素、输入和输出各最多 32 MiB，截止时间十秒。
关闭或切换相机会取消尚未提交的请求。输出使用 `create_new`，不会覆盖已有文件；
写入失败只清理本次创建的拍摄文件。写入和最后的取消检查完成后才发出完成事件。
提交完成后再关闭相机，不会删除已经拍好的照片。

读取状态区分空闲、等待和处理帧。同一相机重新打开时，UI 和回调不能重复安排下一次读取。
仍有帧在途时拒绝切换格式，应等相机空闲后重试。硬件获取、隐私指示灯、断开和重连行为
仍需真正的 Windows 相机测试。

现有共享控件在请求拍摄前检查应用是否还有存储空间，但未把剩余额度传给原生后端。
上述固定上限不是对应用剩余额度的预留。这是共享控件边界，不能宣称完整执行了拍摄存储额度。

## 验证

使用真实源码的便携测试已 **9/9 通过**：解码 NV12/YUY2 JPEG 像素、严格 MJPEG 验证、
不覆盖文件、忙碌拒绝、取消后复用、关闭、超时、畸形和过大输入。测试替换平台 ID 与
动作投递，因此不是 Media Foundation 测试。运行 `tools/windows-camera-still/check.py`，
用 `--output` 指定全新目录；驱动通过优化的 `rustc` 编译现有仓库内置编码库，记录
日志与哈希，并保留失败结果。见其 `validation.json`。

在准备好的运行时中，
`cargo check --locked --offline --target x86_64-pc-windows-msvc -p makepad-platform -j1`
已通过。它检查 Windows 源码类型，包括 Media Foundation 集成，但不会链接或执行
Windows 程序。现有工作区重复构建目标、内置 Windows 库以及未使用窗口 DPI 辅助方法的警告仍存在。

真实拍摄仍**未验证**。本轮未在 Mac 上操作 Windows 硬件或相机授权。
只有编译通过，不能算 Windows 相机演示通过。
