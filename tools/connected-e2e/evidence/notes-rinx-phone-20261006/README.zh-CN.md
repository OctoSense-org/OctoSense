# OnePlus 6 上的 Rinx 编辑器

[English](README.md) | 简体中文

独立 **OctoSenseNotesTest** 测试包已更新为 Rinx 文章编辑器布局，使用签名私有
App Hub 目录。没有替换正常 Home 及其账号/数据，也没有复制提供商凭据。

原始 ADB 截图依次展示[编辑器](01-updated-source.png)、[原生键盘](02-native-keyboard.png)、
[临时编辑](03-keyboard-edited.png)、[收起键盘](04-keyboard-dismissed.png)、
[冷启动](05-cold-restart.png)和[预览](06-preview.png)。每张均单独打开检查，
仅含虚构测试笔记。格式工具栏保持在键盘上方；输入时隐藏悬浮导航，收起键盘后恢复。

测试实际点击软键盘回车，输入临时标记，并检查保存的草稿包含确切换行和文本。
随后用原生退格删除标记，确认恢复原文。强制停止后，新进程冷启动仍恢复确切草稿。
APK/目录更新也完整保留原草稿，没有执行 GitHub 写入。

[receipt.json](receipt.json) 绑定 APK、应用包、源码、检查结果和原始截图摘要。
[shell-validation.json](shell-validation.json) 记录 956 项 shell 测试与两种打包的
依赖图检查全部通过。这里只验收 Android 本地编辑和键盘行为；真实 GitHub 授权、
读取/提交、真人输入、后台生命周期及整体 UX 分数仍未验证。

复现参见[独立 Android 测试流程](../../android-notes.zh-CN.md)，使用当前源码和
新的签名测试目录。在手机打开 Notes，点击正文并使用 Android 键盘输入，通过
系统返回收起键盘，用铅笔/眼睛图标切换编辑与预览，停止测试应用后重新打开。
替换目录时保留草稿存储，不要清除正常 Home 数据。

[最终更新](final-update.json)版本2026100620包含无法加载草稿的保护和新版商店截图。
再次检查了[原文](07-final-update-source.png)与[键盘](08-final-update-keyboard.png)，
原草稿未变。更新目录时应替换受管理的签名应用包目录；直接覆盖可能残留已删除文件，
导致摘要校验拒绝启动。首次覆盖失败与正确替换后的成功验证分别记录。
