# Rinx 原生写作界面对照

[English](README.md) | 简体中文

`rinx-desktop.png`、`rinx-phone.png` 是 Rinx 实际 `ArticlePanel` 写作页的原生 GPU 截图。
诊断入口仅加载虚构 Markdown 并进入已有的 `Page::Write`，没有修改界面定义，
没有使用账号、Matrix 会话、模型、仓库或发布服务。

`reference-receipt.json` 记录 Rinx 修订、原始 UI、诊断补丁、二进制和截图的哈希。
`notes_writer_reference.rs` 是独立测试宿主，`diagnostic-entry.patch` 是唯一库改动。
这些文件只供对照复现，不属于产品代码，也不是认证绕过方案。
测试使用独立工作树和全新的 `RINX_DATA_DIR`，所有测试窗口均已关闭。

复现时在记录的修订上创建临时工作树，应用补丁，将宿主复制到 `examples/`，
执行 `cargo build --locked --release --example notes_writer_reference`。
以全新数据目录和 `MAKEPAD_HIDE_WINDOWS=1` 启动；`--remote` 打开原生测试接口。
默认 1200×820，`--phone` 为 430×850，通过 `/g` 截图、`/quit` 退出。
该等价流程已在临时路径运行；不要使用已有个人资料目录。

`notes-*.png` 是相同窗口尺寸下的复用组件。`editor-receipt.json` 绑定源码、资源和二进制，
记录 Unicode、选区格式、撤销重做、视图切换、富文本输入和表格选择器的原生验证。
`reference.splash` 是无真实服务的测试内容；验收命令在记录中。
App Hub 签名安装、OAuth、GitHub 宿主审核和 Android 键盘属于独立验收，不能由这些截图证明。

Notes 文件名占据原标题位置且只读；原发布图标调用宿主审核回调，Markdown 字节不会被标题或主题重写。
主题仅为本地显示状态，样式面板保留具名主题、块编辑器和历史入口。
依赖仍固定 Rinx v1.1.0，而真实视觉对照使用单独记录的新 Rinx 修订。
本记录不声称逐像素完全一致，也不据此给整个产品打分。

本轮测试后补齐了上游图标的混合许可来源记录；Rust、测试场景和 SVG 内容均未变更。
原回执保留当时的说明文档摘要，[attribution-correction.json](attribution-correction.json)
另行记录此次仅文档修正。
