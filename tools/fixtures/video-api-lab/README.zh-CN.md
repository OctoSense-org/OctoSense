# 视频 API 验收

[English](README.md) | 简体中文

本测试使用五秒、无声的 H.264 视频，内容为三条色带和移动的白色标记，不包含私人媒体或外部素材。`tools/generate-video-fixture.swift` 使用已安装的 macOS SDK 生成该视频，生成器拒绝覆盖已有文件。

尚未发布的运行时把现有原生 `Video` 播放器的控制方法开放给 Splash：

| 方法 | 返回值或参数 |
| --- | --- |
| `prepare_playback()` / `begin_playback()` | 请求准备或播放 |
| `pause_playback()` / `resume_playback()` | 控制同一个播放器 |
| `stop_and_cleanup_resources()` | 请求清理原生资源 |
| `mute_playback()` / `unmute_playback()` | 修改播放器静音状态 |
| `seek_to(milliseconds)` | 非负毫秒位置，不超过已知时长 |
| `set_volume(value)` | 有限数值，限制在 0–1 |
| `set_playback_rate(value)` | 0.25–4 的有限数值，后端可能进一步限制速率 |
| `current_position_ms()` / `total_duration_ms()` | 位置估计值或时长 |
| `state()` / `error()` / `is_muted()` | 状态字符串、最近错误或布尔值 |

命令确认的是分派，准备和解码异步完成。数值控制遇到无效参数或尚未准备好的播放器时返回 `false`。通过 `state()` 和 `error()` 观察结果。跳转不保证逐帧精确：后端可能选取附近的帧，`current_position_ms()` 在后续原生帧更新前可能暂时返回请求的目标位置。摄像头必须使用 `CameraPreview`：隔离应用直接构造 `VideoDataSource.Camera` 会被拒绝，因为它没有对应的权限批准记录。

文件来源仍限制在应用存储隔离目录。关闭隔离环境或销毁 Video 控件后，UI 事件循环会清理原生播放器。系统暂停/恢复不会取消用户主动设置的暂停。仅隐藏视图不等于销毁视图；保留隐藏视图时应显式停止播放。

本测试只覆盖本地 MP4，不验证流媒体协议、DRM、全部编解码器或网络重定向/播放列表策略。目前网络来源只检查初始 URL；嵌套播放列表及重定向的限制仍需单独完成，不能据此声称完整遵守隔离应用的网络白名单。

macOS 本地 MP4 原生验收已通过：运行时 `fc938badf` 上的 16 项 instrument 检查全部通过，
涵盖解码与渲染、暂停、原生前后跳转、停止与重启、关闭后的资源释放。已审视 Metal 截图。
见[脱敏回执](evidence/macos-local-mp4.json)。这是独立受限测试，不代表 App Hub 签名准入。

**OnePlus 6 / Android 15 本地 MP4 验收已通过**：运行时 `7c859055` 上的 20 项检查全部通过。
有界的应用私有命令通道调用受限 Splash 按钮原有的回调，再用原生解码器事件验证结果。
跳转到 2,000 ms 后从 2,050 ms 恢复，回退后从 66 ms 恢复；停止／重启及应用关闭均释放
原生播放器，最终没有晚到帧。驱动已卸载自己的独立测试包，见[脱敏回执](evidence/oneplus6-local-mp4.json)。
这不证明 Android 像素渲染或物理触摸行为。

Android API 26+ 现使用 `MediaPlayer.SEEK_CLOSEST`；旧接口选择前一个同步帧，未能通过
真机前进定位检查。更早 Android 版本保留旧接口回退路径，仍未验证。这次仅针对 Android
的 Java 改动不会扩大 macOS 回执的范围，也不会改变其记录的运行时标识。
接口语义见 Android 的 [MediaPlayer 定位契约](https://developer.android.com/reference/android/media/MediaPlayer#seekTo(long,%20int))。
