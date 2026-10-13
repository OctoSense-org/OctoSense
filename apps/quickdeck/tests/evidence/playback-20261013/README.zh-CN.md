# 播放的验证记录（2026 年 10 月 13 日）

[English](README.md) | 简体中文

Quick Deck 用运行时的 `Video` 控件播放演示文稿中的视频和音频（SERVICE.md 的 "Playback"；deck 服务的 `deck.media`，#515）。这里是在本分支（变基到 main ffd5e488）上验证它的各次运行的截图和核对结果。[receipt.json](receipt.json) 记录了命令、二进制文件的摘要、核对结果和每一轮运行。

## card-host：开发夹具，浅色与深色

App Hub 的 `card-host`（使用其固定版本 38635915，基于本检出的运行时构建）在 macOS 26.6.2（Apple 芯片）上隐藏运行，通过远程桥驱动，在 `make_fixture.py --media` 的 Launch Film 上执行 `tests/ui.py` 的 `playback` 运行。剪辑是仓库中合成的五秒无声 H.264 剪辑（`dev-fixture/media/clip.mp4`）：红、绿、蓝三等分，一条白色竖条从中穿过。同一次会话中，套件的 `missing`、`fixture` 和 `states` 运行也都通过。截图缩小为一半；文件名前的 `d` 表示深色。

| 截图 | 内容 |
| --- | --- |
| [p1-clip](card-host/p1-clip.png) | 选中的视频：下方是 Play；检查器的 Video 部分说明它在放映中自动开始、播放一次 |
| [p2-playing-pair](card-host/p2-playing-pair.png) | 播放时相隔一秒的两张截图中剪辑的框，原尺寸：13,850 个像素发生变化，控件上的时间从 0:01 变为 0:03。完整截图见 [p2-playing-a](card-host/p2-playing-a.png) 和 [p2-playing-b](card-host/p2-playing-b.png) |
| [p3-paused](card-host/p3-paused.png) | 暂停：画面和时间保持 1.2 秒不变 |
| [p4-sound](card-host/p4-sound.png) | 音频：小的扬声器控件 |
| [p5-unsupported](card-host/p5-unsupported.png) | `deck.media` 的 `unsupported:`（夹具中的音频链接到演示文稿之外的文件），用通俗的话说明 |
| [p6-failed](card-host/p6-failed.png) | 已有剪辑播放过之后，一个文件不存在的剪辑："This clip couldn't be played." |
| [p7-storage-full](card-host/p7-storage-full.png) | `deck.media` 的 `storage_full:`，使用合同中的原话，以及存储提示条 |
| [p8-present-pair](card-host/p8-present-pair.png) | 放映：自动开始的剪辑，相隔一秒的两张截图：33,418 个像素发生变化 |
| [p9-present-paused](card-host/p9-present-paused.png) | 点按一次暂停（"Paused · 0:04 / 0:05"）；再点按一次继续播放 |
| [p10-present-next](card-host/p10-present-next.png) | 下一张幻灯片：没有剪辑在播放 |
| [p11-unavailable](card-host/p11-unavailable.png) | 一次尚未播放过任何剪辑的新会话，同一个剪辑："Playback isn't available on this system"，下方附有播放器自己的说明。在 Mac 上这是 AVFoundation 拒绝了不存在的文件；运行时随后尝试它的软件解码器，而后者没有媒体插件：这与没有播放器接收文件时控件给出的错误相同 |

## macOS：真实引擎

由本分支构建的桌面 Shell 在 macOS 26.6.2 上隐藏运行，使用真实的 deck 服务：在 `tests/media` 的演示文稿上执行 `playback` 运行（同样是示例剪辑），先浅色后深色。每一步之后，Shell 的日志都不能出现 `wm: removing client`，也不能有被拦下的模块故障。音频从不播放：这些运行不打开任何音频设备。

| 截图 | 内容 |
| --- | --- |
| [p0-home](macos/p0-home.png) | 演示文稿库中的原生演示文稿及其 `.pptx` 副本 |
| [p1-editor-playing-pair](macos/p1-editor-playing-pair.png) | 原生演示文稿的视频在编辑器中播放：一秒内 6,825 个像素发生变化，时间从 0:01 变为 0:03（[a](macos/p1-editor-playing-a.png)、[b](macos/p1-editor-playing-b.png)）。随后依次执行了 Pause、Play、点按视频和 Stop |
| [p2-present-playing-pair](macos/p2-present-playing-pair.png) | 放映：视频自动开始并在播放（12,814 个像素发生变化）；四次点按在同一张幻灯片上暂停并继续播放 |
| [p4-pptx-present-playing-pair](macos/p4-pptx-present-playing-pair.png) | `.pptx` 副本的放映：没有剪辑自动开始；点按后剪辑开始播放（12,814 个像素发生变化） |

[playback.json](macos/playback.json) 记录了 `deck.media` 写入的文件 `.cache/media/66ed412a….mp4`，26,739 字节，与剪辑自身的 SHA-256 相同。

## Linux：真实引擎

同样的运行在由本分支构建、隐藏运行在无头 weston 上的桌面 Shell 中执行。这台主机只装了 GStreamer 的基础插件，没有 MP4 解复用器，也没有解码器，所以控件对每个剪辑都以 GStreamer 的 "Your GStreamer installation is missing a plug-in." 失败。同一次会话中 `shell` 和 `restart` 运行照旧通过。

| 截图 | 内容 |
| --- | --- |
| [p0-home](linux/p0-home.png) | 演示文稿库中的原生演示文稿及其 `.pptx` 副本 |
| [p1-editor-unavailable](linux/p1-editor-unavailable.png) | 原生演示文稿的视频，点 Play：海报图保留，显示 "Playback isn't available on this system"，检查器中附有播放器的说明 |
| [p2-present-unavailable](linux/p2-present-unavailable.png) | 放映：尝试了设为自动播放的视频，舞台上说明无法播放 |
| [p3-sound-unavailable](linux/p3-sound-unavailable.png) | 音频，点 Play：同样的说明 |
| [p4-pptx-present-unavailable](linux/p4-pptx-present-unavailable.png) | `.pptx` 副本的放映：没有剪辑自动开始（deckcraft 不从 `.pptx` 读取自动播放设置）；点按后才尝试播放剪辑 |

[playback.json](linux/playback.json) 记录了 `deck.media` 写入的内容：示例剪辑和音调（`.wav`）；关闭演示文稿后两者都离开了缓存。

两个 Shell 运行的都是 main ffd5e488，其中已有 #516 而尚无 #519：Quick Deck 的工具栏不再叫 `bar`，所以 Shell 按名字查找自己的 `bar` 时不会再找到 Quick Deck 的控件，也就不会崩溃。

## 这里未验证的内容

在任何系统上带声音输出的音频播放（这些运行不打开任何音频设备）；Windows；循环播放的剪辑；剪辑播放时切换浅色/深色；编组中的剪辑。
