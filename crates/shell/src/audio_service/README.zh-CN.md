# 原生音频会话

[English](README.md) | 简体中文

已安装的应用可通过 OctoSense 的原生设备录制短语音片段和播放音频文件。
目前支持 macOS 和 Android。这些 API 不提供语音转文字、语音合成、系统声音
采集、媒体下载或后台播放。硬件验收仍为 **未验证**；合成测试不会开启麦克风或扬声器。

| 方法 | 参数 | 返回 |
| --- | --- | --- |
| `microphone.record_start` | `path`；可选整数 `max_duration_ms`（100–30000，默认 30000） | 权限检查和设备设置后的会话 ID 与 `starting` |
| `microphone.record_status` | `session` | 当前状态及最终保存路径或错误 |
| `microphone.record_stop` | `session` | `stopping`；轮询直到 `saved` 或 `failed` |
| `microphone.record_cancel` | `session` | 停止并丢弃，不保存文件 |
| `audio.play` | `path` | 有界解码和设备设置后的会话 ID 与 `starting` |
| `audio.status` | `session` | 当前状态，包括 `playing`、`completed`、`stopped` 或 `failed` |
| `audio.stop` | `session` | `stopping`；轮询最终状态 |

每次成功响应都含 `session`、`status`、`path`、`error`、`frames` 和 `format`。
只有录音保存成功后 `path` 才非空；`error` 在出错或取消前为 null。录音的
`format` 为 `wav`，播放为 null。`frames` 是设备帧数，不是转录结果或用户
实际听到声音的证明。设备回调产生帧后，`starting` 才变为 `recording` 或
`playing`；五秒内没有收到帧会明确失败。

录音需声明 `microphone` 与 `storage`，播放需声明 `audio` 与 `storage`。
同时声明 `requires: ["host-api-v1"]`，并在 `host_api.required` 中列出所用
方法的主版本 1。`runtime.describe` 返回支持的方法；不支持的平台不会列出。
全部七个方法均仅供前台使用，包括经应用代理转发的调用；不提供代理工具别名。

录音前先调用 `microphone.permission.status`，需要时调用
`microphone.permission.request`。现有宿主权限面板要求用户物理确认，随后
进入操作系统权限流程。`record_start` 只检查现有 OS 授权，不弹权限框，也
不能借用其他应用的许可。原生输入打开期间，OS 麦克风指示器保持显示。
应用应使用这些 API 提供清晰的录音状态、停止和取消入口。

会话绑定宿主确认的应用身份和存活的 Splash 堆。其他应用或重新打开的实例
不能操作它。Shell 提供当前聚焦的已安装应用或展开的 Glance 所有者，运行时
检查其前台标记。切换应用、回到 Home、关闭堆、改变存储或账户、宿主进入后台、
撤销麦克风许可或设备断开都会停止会话，且不会自动恢复。启动前检查应用准入，
工作线程随后最多每秒复查一次；清单变化或商店撤回会终止访问。App Hub 内嵌
预览没有独立前台身份，需要先在独立应用窗口中打开已安装的应用。

全局最多同时运行一个录音和一个播放器。录音与 Shell 听写共享麦克风所有权，
包括由 OS 自行采集的语音识别器。争用返回 `busy`；听写预热不能替换录音回调。
命令和采集队列都有上限；音频回调只用原子变量和固定队列，重采样、编解码
及准入检查放在工作线程。UI 通过有界通道非阻塞轮询结果。队列溢出时丢弃
录音并报错。最多保留十六个会话记录，新会话准入时清理结束超过一分钟的记录。

录音为单声道 16 kHz PCM16 WAV，最多三十秒（960,044 字节），保存到应用
现有 Splash 沙箱中的**新文件**。停止或达到时长上限会保存；取消或失去前台
则丢弃。不会覆盖已有文件，仍受单文件、整个沙箱和条目数量限额约束，包括
录音期间应用产生的其他文件。不向脚本暴露宿主路径或文档提供方 URI。

播放从同一沙箱读取最多 1 MiB，支持 PCM16/float32 WAV、MP3、FLAC、Ogg Vorbis。
解码限制为单声道或双声道、六十秒及 2,880,000 帧。文件导入导出另用 `files`
API，网络下载仍需应用正常的 HTTP 授权。本接口面向短音频，不提供音乐流媒体
或后台媒体服务。

MorningBrief 的自定义 `llm.speech`、`llm.speak`、`llm.listen_*` 补丁仍需要
迁移至单独准入的语音服务。原生录音和播放并不实现那些 ASR/TTS 接口。
`model.audio` 返回提供方合成的音频字节，仍属于单独的模型配置和计费选择。

验证覆盖合成 PCM、损坏编码、队列满载、应用和堆隔离、无需绘制的前台切换、
许可拒绝、存储变化和麦克风争用。完整 Shell、平台检查与真机验收必须另行
记录，编码单元测试不代表硬件验证通过。

现有 OpenHarmony 相机录像器也持有同一个麦克风租约，直到原生录像器停止并释放。
其他录音请求会被拒绝；无声录像无需租约。在当前运行时版本中，Android 相机录像
明确返回不支持，macOS 没有相机录像器。本服务不会为这些路径增加录像能力。

切换到其他系统窗口也会立即取消会话；重新获得焦点不会自动恢复录音或播放。
已排队的取消请求先于工作线程的就绪结果处理，防止取消操作到达后仍保存录音。
