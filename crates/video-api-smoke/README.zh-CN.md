# 原生 Video 脚本验收

[English](README.md) | 简体中文

这个小型原生宿主通过受限 Splash 应用调用真正的 `Video::script_call`，播放应用
存储沙箱内的五秒合成无声 H.264 视频。它只依赖 Makepad widgets 和 JSON，不加载
OctoSense 外壳、账号或模型供应商。共享的 binary/library 入口也可以构建独立 Android
测试 APK；清单不申请相机或麦克风权限。

Python 驱动通过 Makepad 本机 instrument 点击隐藏原生窗口内的脚本按钮，检查准备、
原生帧和时间推进、暂停稳定性、跳转与继续、显式停止、重新播放，以及关闭应用后的
原生资源释放。宿主独立观察平台解码器事件，所有播放命令都来自 Splash 脚本，
逐一核对命令确认和无效数值参数拒绝结果。播放超过两秒后回退到零点，必须观察到
接近零点的原生帧；脚本乐观更新的时间不能满足该检查。隔离检查读取实际堆的存储
沙箱和网络白名单拒绝状态。
不通过 Rust `VideoRef` 直接控制。视频由 `tools/generate-video-fixture.swift` 生成，
驱动在私有日志和帧截图旁记录视频及测试二进制的 SHA-256。

**macOS 运行时 `fc938badf` 已通过 16 项 instrument 检查**，见[脱敏回执](../../tools/fixtures/video-api-lab/evidence/macos-local-mp4.json)。
如需复现，构建工作区的 `octosense-video-api-smoke`
二进制，然后用 `python3 tools/test-video-api-native.py`，指定 `--host <binary>` 和全新的
`--out <evidence-directory>`。该流程不下载依赖、不读取真实媒体库、不修改已安装的应用。
它不证明 App Hub 签名准入、网络流媒体、有声播放、录制、多应用堆并发或其他平台的
解码器；这些需要分别记录验收结果。命令被接受与原生操作完成是独立观察结果。

## Android 验收接口

使用 `tools/test-video-api-android.py`，传入 `--adb`、`--aapt2`、`--apk`、明确指定的
`--serial` 和新的 `--out` 目录。**OnePlus 6（Android 15）在运行时 `7c859055` 上通过全部 20 项检查**，
见[脱敏回执](../../tools/fixtures/video-api-lab/evidence/oneplus6-local-mp4.json)。
APK 必须使用独立包名 `dev.makepad.octosense.videoapilab.publicapis1`，执行前不得已安装。
驱动只卸载本次自己安装的测试包。

Android 没有独立的 Makepad HTTP 控制接口。此测试通过应用私有目录中的有界命令，
调用可见按钮原有的 Splash 回调，并保持输入为不可信来源。原生解码器事件独立验证
播放结果及跳转后的首批帧位置。这验证的是回调和解码器，不代表物理触摸或像素验收。
驱动不使用全局点击、系统截图、权限授权、网络监听、个人账号或真实媒体库。

真机测试发现 Android 旧定位接口收到 2,000 ms 请求后从 1,176 ms 恢复播放。
运行时现于 API 26 及更新版本使用 `SEEK_CLOSEST`；修复后首次观察到的恢复位置为
2,050 ms，回退后为 66 ms。旧 Android 版本保留有界的旧接口回退路径，本次未测试。
两个原生播放器均已释放，最终关闭后没有新帧；驱动已停止并卸载自己的独立测试包。
