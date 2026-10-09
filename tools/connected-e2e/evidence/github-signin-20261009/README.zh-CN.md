# GitHub 登录面板与 Notes 账户卡片 — 2026-10-09

[English](README.md) | 简体中文

宿主的提供商登录面板与 GitHub Notes 的账户卡片一并重新设计。
[notes_signin.py](../../notes_signin.py) 在 Linux 构建主机的无头 Weston 下驱动二者：
使用本分支 `connected-app-host` 与 `connected-install` 的 debug 构建，以及未登录的合成
GitHub（`--provider-fixture=github-sign-in`）。登录服务、面板、连接存储和已安装应用都是
真实实现，只有 github.com 换成替身。应用包为 GitHub Notes 0.2.2 候选版的 `main.splash`，
使用示例 id `org.octosense.samples.githubnotes`。[receipt.json](receipt.json) 记录二进制、
源码与截图摘要及五项检查。该驱动连续三次运行均通过。

改版前，面板只用 id 称呼应用；代码是小链接上方的纯文本，等待时 Continue 仍然可见；
拒绝授权时显示提供商的原始错误（[授权](00-before-consent.png)、[代码](00-before-code.png)、
[拒绝](00-before-declined.png)）。

改版后：

- [卡片](01-connect-card.png)只有一个 Connect GitHub 操作，并用通俗的话说明可选的访问范围；
- [面板](02-consent.png)在 id 上方显示应用名称并列出访问权限；
- [代码步骤](03-code.png)提供 Copy 和 Open GitHub（同时复制代码）、粘贴位置、
  动态等待提示和剩余时间；
- 批准后[卡片](04-connected.png)显示账户名称与访问范围，并加载仓库；
- [断开](05-disconnected.png)前先确认，且保留笔记；
- 拒绝授权时[面板](06-sheet-declined.png)和[卡片](07-app-declined.png)都用通俗的话说明，
  卡片提供 Try again；
- [取消](08-cancelled.png)后显示中性提示；
- 在 1200 px 宽的窗口中，[面板](09-wide-code.png)是 560 px 的一栏。该帧由临时驱动截取；
  `notes_signin.py` 没有宽窗口模式。

以上截图已逐张检查。真实 GitHub 登录、此面板中的 Google 登录、macOS 与 Android 渲染以及
亲手批准均**未验证**。
