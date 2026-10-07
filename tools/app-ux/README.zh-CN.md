# 原生应用 UX 检查

[English](README.md) | 简体中文

这些检查使用独立应用数据，在隐藏的 Makepad 窗口中通过真实指针与键盘输入
操作宿主内的应用。原始 PNG、控件快照、进程日志和结果回执保存在
`target/app-ux/`，不重绘或裁剪验收截图。每次新测试使用新输出目录，失败记录单独保留。

## 构建与运行

在仓库根目录运行，以下命令已在 macOS 上执行：

```sh
python3 tools/setup.py
python3 tools/sync-app-interface.py --check
cargo build --locked --release -p octosense --no-default-features --features app-hub,dev-mode
python3 tools/app-ux/calendar_journey.py --output target/app-ux/calendar-final
python3 tools/app-ux/calendar_journey.py --desktop --output target/app-ux/calendar-desktop-separate-frame
python3 tools/app-ux/theme_journey.py --output target/app-ux/theme-final
python3 tools/app-ux/providers_journey.py --output target/app-ux/acceptance-providers-cancel
python3 tools/app-ux/retention_journey.py --output target/app-ux/acceptance-retention-pixels
python3 tools/app-ux/mail_journey.py --output target/app-ux/mail-journey-round2
python3 tools/app-ux/news_journey.py --output target/app-ux/news-journey-round3
python3 tools/app-ux/glance_swipe_journey.py --output target/app-ux/glance-curved-native-after
python3 apps/photos/tests/ui.py --binary target/release/octosense --output target/app-ux/photos-journey-round2
```

日历测试创建带中文标题和多行备注的事件，检查详情、发布至 Glance，并在重启后
核对实际存储。主题测试在草稿尚未保存时切换明暗外观，再保存同一份草稿。
邮件使用虚构的本地演示账户，阅读邮件并编辑回复，**不发送邮件**。
新闻读取公开订阅源，将文章收藏到本地。相册使用现有的本地模拟服务商测试
Memories，不代表真实模型调用或设备相册访问。

桌面日历测试先激活宿主窗口，每次输入只派发一次，再通过单独的帧截图等待
控件坐标更新，避免隐藏窗口拒绝立即呈现时重复已经执行的操作。这组耗时包含
PNG 截图开销，不能与仅测编辑器打开过程的基准样本直接比较。

Glance 滑动测试使用可在一屏内完整显示的总览，覆盖四个高度，以及起步时向上、
向下偏移的弧线左滑。旧构建会失败，修复版通过。本机预览注入的是鼠标事件；
对应的 Android 测试为 `android_glance_swipe.py`，须指定已分配设备的 `--serial`，
并单独安装 `dev.makepad.octosense.glanceswipe` 测试包。脚本注入 Android 触摸事件、
保存原始手机截图，其他应用进入前台时立即停止，不清除应用数据或更换 Home。

采集两种宽度与明暗外观：

```sh
python3 tools/app-ux/capture.py --output target/app-ux/final-phone-light --apps calendar photos mail ai-providers youtube maps --mode phone --settle-seconds 12
python3 tools/app-ux/capture.py --output target/app-ux/final-phone-dark --apps calendar photos mail ai-providers youtube maps --mode phone --dark --settle-seconds 12
python3 tools/app-ux/capture.py --output target/app-ux/final-desktop --apps calendar photos mail ai-providers youtube maps apphub --mode desktop --settle-seconds 12
```

采集成功只表示拿到了像素，不代表网络资源均已加载或界面已通过评审。必须查看
原始图片。额外等待用于资源加载，不作为性能样本。

`hub_journey.py` 通过 `--binary` 指定应用大厅的原生 `preview` 示例程序，
浏览明确标注的预览目录，执行搜索、空结果恢复和详情查看，不安装应用。
支持 `--size 1200x860` 和 `--dark`。示例程序须在配套 App Hub 仓库中，
按本仓库固定的版本构建。

## 性能与验收

`benchmark.py --before <binary> --after <binary> --output <directory>` 对每个程序
测量 24 次打开日历编辑器的观测耗时，记录 p50、p95、最大值、全部样本和程序哈希。
这是从派发指针操作到观察到控件的墙钟时间，包含 HTTP 开销，**不是 GPU 帧耗时或 FPS**。
复制的旧程序冷启动可能需要更久加载宿主资源；启动不计入交互样本。
解释微小差异前应控制机器负载。

分别评审任务清晰度、内容可读性、键盘可达性、状态保留、响应速度和视觉一致性。
像素诊断不能证明达到 95/100，也不能抵消失败的核心操作。记录实际作者、驱动者
和评审者；自评不代表独立验收。

手机模式是手机尺寸的桌面宿主。Android、iOS、OHOS 的实际键盘、生命周期、相机、
触摸和辅助输入验收，在指定设备上运行前均为**未验证**。预览也不能证明真实邮件送达、
模型执行、地图路线或视频播放。

修改共用 Shell 时，还须执行仓库 [AGENTS.md](../../AGENTS.md) 中的打包与功能检查。
测试只关闭自己启动的进程，不替换已安装的应用或个人配置。

## 补充验收

`providers_journey.py` 打开真正由宿主管理的新增模型表单并取消，不保存凭据或调用模型。
`retention_journey.py` 依次编辑六个独立 Glance 工作区，切换五个其他工作区后分别返回。
这类测试字段不在原生 widget 快照中，因此脚本只报告已采集；必须另行检查六张原始截图，
确认各自的 Unicode 多行草稿仍完整保留。不要把采集成功当作状态验收通过。

本次在指定 Redmi Note 12（Android 15）的独立测试包中，已验证八条 Glance 触屏
关闭路径，以及日历多行编辑、键盘上方自动显示当前输入框、键盘打开时保存及保存后复查。
这些真机结果不代表 iOS、OpenHarmony、辅助输入或外部服务端到端验证。
