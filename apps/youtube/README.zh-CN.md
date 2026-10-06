# YouTube 音乐卡片

[English](README.md) | 简体中文

YouTube 搜索、应用智能体与 Glance 音乐卡片共用由宿主管理的真实搜索结果缓存。
`youtube.search`、`youtube.read`、`youtube.recommend` 与 `youtube.publish`
支持跨应用共享，但调用方仍须获得明确授权。`youtube.preferences` 仅供 YouTube 自身使用。

在应用中选择 Morning（早晨）、Noon break（午间休息）、Relax（午后放松）、
Dinner（晚餐）或 Sleep（睡前），即可搜索音乐并把首条结果发布为该时段的静默
Glance 卡片。展开后可见标题、频道与时长；卡片内的“在 YouTube 播放”打开
应用 WebReader 中该视频的 YouTube 移动版观看页面。宿主原生 Chat 标签可与 YouTube 智能体交谈、
调整建议，仍遵循智能体同意机制。定时任务不会播放音频；YouTube 可能要求再次点击或登录。

“Daily music”（每日音乐）默认关闭。启用并在设置中允许 YouTube 智能体后，设备本地时间 07:00、12:00、
15:00、18:00、21:00 对应五个时段。可请智能体修改五个互不重复的小时及查询词。
缺少或撤销智能体同意时，开关显示 paused（已暂停），不会悄悄运行。
宿主运行时才会检查，Android 暂停进程会延迟或停止检查；这不是新增 Android
后台任务。每次仅考虑当前时段，不补发错过的时段；成功发布的日期与时段会持久保存。

宿主在 `.host/youtube/recommendations.json` 保存查询词、搜索元数据、设置和
定时记录，在 `.host/youtube/cards.json` 保存卡片的准确视频记录。卡片可静默恢复，
不会延长有效期；移除和撤销移除也会保存，当天同一时段的调度不会重新发布已移除卡片。
不保存 YouTube 登录凭据。查询会发送到 YouTube 公共搜索接口。
超过一天的结果须重新搜索后才能发布；编造的视频 ID 会被拒绝。搜索失败不覆盖
旧卡片。播放历史仍位于应用自身的设备账户目录中。

实现位于 `crates/shell/src/youtube.rs` 与应用包内的 `tools.json`、`AGENT.md`、
`recommendation.card`。L0 模板是宿主作者编写的界面，不能归功于 DeepSeek 或
MiniMax。真机验证发现，把 iframe 播放器直接作为顶层页面打开会出现 Error 153。
现已改用固定版本运行时 `sys.video` 的移动版观看地址，也覆盖旧播放历史；
修复后的 OnePlus 6 复测打开了准确的已保存视频，显示播放画面和 YouTube 的
“Tap to unmute”提示。实际听感、YouTube 模型对话及自然时段调度尚未验证。
已在 macOS 隐藏原生窗口中采用 Android 手机尺寸验证真实搜索、卡片发布、
重启恢复、聚焦展开、原生 Card/Chat、未发送文本保留、缺少同意时的暂停提示，
以及打开准确视频的导航。Makepad 帧缓冲截图中原生 WebReader 图层为黑色，
该检查不能证明音频播放成功。

本次已执行验证：十项原生 Shell 确定性测试通过；另行执行的 YouTube 公共实时
搜索测试也通过，使用 Makepad 平台网络后端。实时测试证明视频标识与元数据来自
实际搜索，不代表音频播放成功。执行命令：

```sh
cargo test --locked -p octosense-shell --features mobile-apps youtube::tests -- --test-threads=1
cargo test --locked -p octosense-shell --features mobile-apps youtube::tests::live_public_search_returns_actual_video_results -- --ignored --test-threads=1
```
