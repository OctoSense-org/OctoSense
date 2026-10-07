# OnePlus 6 本地 Notes 检查 — 2026-10-06

[English](README.md) | 简体中文

独立 **OctoSenseNotesTest** APK 通过本地 Markdown 编辑、真实软键盘输入、硬件
Enter、预览、精确冷启动恢复，以及边缘返回测试 Home；未替换日常 Home。
最新 APK 和未修改的签名 Notes 包摘要见 [receipt.json](receipt.json)。
未复制 GitHub 连接、提供商令牌或个人配置。

首个 APK 在输入已经稳定后，硬件 Enter 仍会丢失最后一个组合中的词：
`# OnePlus Notes test` 变为 `# OnePlus Notes ` 加换行。
[原始失败截图](01-before-hardware-enter.png)已保留。运行时补丁先结束组合再插入
换行，不改变普通输入法提交替换语义。重编 APK 通过相同硬件序列和真实软键盘路径。
[修复后的键盘](02-fixed-hardware-enter.png)、[新进程恢复](03-fixed-cold-reopen.png)
及[预览](04-fixed-preview.png)原图均逐张检查；Java／Rust 回归与补丁栈检查
[单独记录](../android-enter-validation.json)。

这是 ADB 真机检查，不是 Mac 的 Makepad instrument 浸泡。Android 固定版本的
remote 模块为空实现。[复现说明](../../android-notes.zh-CN.md)涵盖签名私有目录、
独立包名、契约导出和打包。最初的 Java 缺少契约错误及增量安装失败，分别通过导出
契约和流式安装解决；原始构建日志及 APK 只保存在本地私有目录。

截图也记录尚待优化的问题：键盘打开时，shell 浮动菜单可能遮住编辑器右侧；
Repository 的一行说明在手机宽度被截断。因此不提供数值 UX 评分，也不声称完整
手机浸泡通过。真实 GitHub 登录／读写、真人批准和 Android 后台生命周期尚未验证。
测试应用留在前台供用户试用，只有虚构的本地笔记。
