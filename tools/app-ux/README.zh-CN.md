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
python3 tools/app-ux/calendar_storage_journey.py --appearance light --action new --output target/app-ux/calendar-storage
python3 tools/app-ux/calendar_edit_storage_journey.py --appearance dark --narrow --output target/app-ux/calendar-edit-storage
python3 tools/app-ux/calendar_history_storage_journey.py --appearance light --scenario edit --output target/app-ux/calendar-history-storage
python3 tools/app-ux/pdftools_readonly_journey.py --appearance light --output target/app-ux/pdftools-readonly
python3 tools/app-ux/pdftools_save_storage_journey.py --appearance dark --narrow --output target/app-ux/pdftools-save-storage
python3 tools/app-ux/pdftools_comment_journey.py --appearance light --output target/app-ux/pdftools-comment
python3 tools/app-ux/pdftools_edit_journey.py --appearance dark --output target/app-ux/pdftools-edit
python3 tools/app-ux/pdftools_history_journey.py --appearance light --output target/app-ux/pdftools-history
python3 tools/app-ux/pdftools_search_focus_journey.py --appearance dark --output target/app-ux/pdftools-search-focus
python3 tools/app-ux/maps_search_focus_journey.py --appearance light --output target/app-ux/maps-search-focus
python3 tools/app-ux/maps_saved_storage_journey.py --appearance dark --output target/app-ux/maps-saved-storage
python3 tools/app-ux/maps_read_storage_journey.py --appearance light --list saved --output target/app-ux/maps-read-storage
python3 tools/app-ux/maps_prefs_storage_journey.py --appearance dark --output target/app-ux/maps-prefs-storage
python3 tools/app-ux/maps_prefs_read_journey.py --appearance light --edits --output target/app-ux/maps-prefs-read
python3 tools/app-ux/maps_many_saved_journey.py --appearance light --output target/app-ux/maps-many-saved
python3 tools/app-ux/maps_saved_paging_journey.py --appearance dark --output target/app-ux/maps-saved-paging
python3 tools/app-ux/maps_phone_paging_journey.py --appearance light --output target/app-ux/maps-phone-paging
python3 tools/app-ux/maps_narrow_toolbar_journey.py --appearance light --output target/app-ux/maps-narrow-toolbar
python3 tools/app-ux/macos_spotlight_journey.py --appearance light --output target/app-ux/macos-spotlight
python3 tools/app-ux/theme_journey.py --output target/app-ux/theme-final
python3 tools/app-ux/providers_journey.py --output target/app-ux/acceptance-providers-cancel
python3 tools/app-ux/retention_journey.py --output target/app-ux/acceptance-retention-pixels
python3 tools/app-ux/mail_journey.py --output target/app-ux/mail-journey-round2
python3 tools/app-ux/mail_journey.py --desktop --output target/app-ux/mail-journey-desktop
python3 tools/app-ux/writer_journey.py --appearance light --output target/app-ux/writer-find-light
python3 tools/app-ux/writer_journey.py --appearance dark --output target/app-ux/writer-find-dark
python3 tools/app-ux/writer_draft_storage_journey.py --appearance light --scenario draft --output target/app-ux/writer-draft-storage
python3 tools/app-ux/writer_conversion_storage_journey.py --appearance dark --action save --output target/app-ux/writer-conversion-storage
python3 tools/app-ux/photos_rename_journey.py --appearance light --output target/app-ux/photos-rename-light
python3 tools/app-ux/photos_rename_journey.py --appearance dark --output target/app-ux/photos-rename-dark
python3 tools/app-ux/news_journey.py --output target/app-ux/news-journey-round3
python3 tools/app-ux/news_journey.py --desktop --output target/app-ux/news-journey-desktop
python3 tools/app-ux/glance_swipe_journey.py --output target/app-ux/glance-curved-native-after
python3 apps/photos/tests/ui.py --binary target/release/octosense --output target/app-ux/photos-journey-round2
```

日历测试创建带中文标题和多行备注的事件，检查详情、发布至 Glance，并在重启后
核对实际存储。主题测试在草稿尚未保存时切换明暗外观，再保存同一份草稿。
macOS 全局搜索测试需要 Pillow 和启用 `mobile-apps` 构建的宿主，检查选中项预览是否
跟随键盘和鼠标，并检查滚动及窄窗口。
`macos_glance_focus_journey.py` 使用虚构通知和临时配置，检查滚动后关闭并重新打开
面板时，回车和删除是否作用于可见的首条通知，以及撤销能否恢复被删除的通知。
请分别运行亮色和暗色外观。
macOS Glance 分组测试使用虚构通知和临时配置，检查分组展开、按应用清除与撤销、
键盘操作，以及分组只剩一条通知时的焦点。请分别运行亮色和暗色外观。
macOS Glance 小组件测试检查本地日期和时间、键盘焦点、拖出取消和应用打开，
包括窄窗口中的分组通知。
Calendar 存储测试只阻塞新建测试数据中的临时文件，检查 `new`（新建）或
`duplicate`（复制）失败后的提示、取消和重试；不模拟崩溃或存储替换后的失败。
编辑存储测试检查失败的修改会保留，直到重试或明确放弃；`--narrow` 会测试
700 点宽窗口。
撤销历史存储测试检查 `add`（新建）和 `edit`（编辑）在撤销或重做被阻塞后，
可以取消并重试，而不会丢失历史步骤。
PDF Tools 测试生成本地两页表单，检查只读字段导航、编辑时保存、关闭时保存并重新打开、
外观切换和 700 点宽窗口。测试使用真实的本地 PDF 服务，不使用外部文档，也不模拟服务失败。
PDF 保存存储测试仅将临时资料库目录设为只读，检查持续错误提示、重试和重新打开后的内容；
`--narrow` 还会检查 700 点宽的窗口。此测试不模拟崩溃。
PDF 批注测试使用虚构作者和本地表单，检查批注与回复的发布、外观切换、重新打开，
以及存在未发送回复时的删除操作。
PDF 编辑测试检查段落草稿在切换文档和外观后保留，并应用修改、保存和重新打开。
PDF 历史测试检查字段撤销与重做、外观切换，以及保存后重新打开的字段值。
PDF 搜索焦点测试检查外观切换后的搜索文本和光标，包括空搜索与正在编辑的表单，
然后保存并重新打开 PDF。
Maps 搜索焦点测试使用临时保存的地点，检查外观切换时的搜索光标、清空搜索、地图焦点
和选择路线起点；不测试实时搜索或路线服务。
Maps 保存地点存储测试只将临时 Saved Places 文件设为只读，检查失败提示、重复失败、
外观切换、关闭提示、重试和重新打开；不模拟部分写入或崩溃。
Maps 读取存储测试让临时 Saved 或 Recents 文件在启动时不可读，检查文件不会被
覆盖，并在权限恢复后重试。使用 `--list saved` 或 `--list recents`；`--narrow`
还会检查 480 点宽窗口，`--corrupt` 会测试截断列表在修复后的重试。
列表检查能发现外层括号缺失，但不能发现所有仍保留两端括号的损坏内容。
Maps 偏好设置测试让临时偏好文件不可写，检查失败提示、重试和重启后的地图样式；
不模拟部分写入或崩溃。
Maps 偏好读取测试让临时设置文件在启动时不可读，检查恢复和重新打开；使用
`--edits` 时还会检查恢复前的新选择与旧文件中未改动的设置能否合并；
`--corrupt` 会测试截断文件在修复后的重试。
超过 512 字节的设置文件会被视为暂不可用，以控制界面脚本的执行预算。
Maps 多地点测试使用 100 个临时保存地点，检查加载、键盘导航、滚动、右键菜单、
外观切换及重新打开后的保存状态。
Maps 地点分页测试使用 500 个临时地点，检查翻页、外观切换后的页码、键盘及
右键操作，以及重新打开后的数据。
使用 `--count 1000` 可检查更大的列表，使用 `--narrow` 还会检查 480 点宽窗口中的分页控件。
手机分页测试检查保存地点操作和切换外观后地图标记是否留在搜索面板后方；
它使用 Mac 上的手机尺寸预览，不代表实体手机测试。
Maps 窄工具栏测试检查折叠菜单、地图样式选择和窗口缩放，并用 Pillow 检查切换
外观后的菜单颜色。使用 `--offline` 时还会检查 480 和 700 点宽窗口中的离线提示；
此选项要求运行程序时阻止外部网络。
邮件使用虚构的本地演示账户，阅读邮件并编辑回复，**不发送邮件**。
新闻读取公开订阅源，将文章收藏到本地。相册使用现有的本地模拟服务商测试
Memories，不代表真实模型调用或设备相册访问。
Writer 使用临时文档检查查找替换、外观切换后的焦点和撤销。
Writer 存储测试仅将新建输出目录中的临时文件设为只读，检查提示、重试和恢复。
草稿场景包括 `draft`、`index`、`index-revert` 和 `new-index`；`--narrow` 还会检查
700 点宽的窗口。转换操作包括 `save`、`preview` 和 `export`，使用真实的本地 Word 服务。
这些测试不模拟部分写入、崩溃或导出到外部位置。
相册重命名测试使用临时图库，检查名称选中、Unicode 名称编辑、Escape 恢复和重启保留。

桌面日历测试先激活宿主窗口，每次输入只派发一次，再通过单独的帧截图等待
控件坐标更新，避免隐藏窗口拒绝立即呈现时重复已经执行的操作。这组耗时包含
PNG 截图开销，不能与仅测编辑器打开过程的基准样本直接比较。

隐藏窗口永远不是主窗口，所以其中的 `files.import` 和 `files.export` 会返回
`foreground_required`。先发送 `/w?k=focus`，告诉应用它的窗口在最前面。设置
`MAKEPAD_TEST_SAVE_DIR=<folder>` 后，隐藏窗口的 macOS 存储面板会回答
`<folder>/<建议文件名>`；不设置时面板按取消回答。打开面板也一样：设置
`MAKEPAD_TEST_OPEN_FILE=<file>` 时它选择该文件，不设置时按取消回答。

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

## Mac 外观

`macos-rubric.json` 是 Mac 桌面的 95/100 标准：八个加权维度（结构、控件、字体、颜色、
清晰度、布局、连续性、一致性）、硬性门槛和覆盖的界面。`macos_score.py <card.json>`
把评审打分换算成结果，缺项的评分表会被拒绝。它只负责计算：分数来自评审者对原始截图的判断，
作者自评不算独立验收。`--test-action style:macos` 和 `style:macos-dark` 用于选择截图的样式。

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
