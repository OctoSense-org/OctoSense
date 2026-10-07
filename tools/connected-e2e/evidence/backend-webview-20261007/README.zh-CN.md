# 原生后端 WebView 验收 — 2026-10-07

[English](README.md) | 简体中文

**通过：macOS WebView 八项检查、外部浏览器回归七项检查。** 两者均使用二进制 `8d7b1b0032486a8020ceafb2117f51cacfe6b285fbb6270706a700956750256e`，对应验收时原生叠加源码树 `50371b7e54adf63fa8aa8c0b1acc53226dcff9ba`。[WebView 记录](receipt.json)与[浏览器回归记录](browser-regression.json)保留了相关源码哈希和签名安装证明。构建输入和两次验收的源码快照在本轮更新中均保持不变，记录还绑定了最终 Glance 生命周期源码哈希。此测试使用已安装应用宿主，不是完整 shell 的 Glance UI；后者另有测试。锁定源码树包含最终 Android 边距更新，但 Mac 运行不验证 Android 行为。

WebView 流程通过真实 WK 表单注册虚构账号，显示错误密码提示，经固定回调拦截和真实 PKCE 交换登录，再读取受保护身份。还覆盖了取消、提交表单前正常退出进程、Back、连接错误/Retry、全新的 HttpOnly Cookie 存储、跨应用句柄拒绝、冷启动恢复及退出登录。没有注入回调、授权码、Cookie、令牌或账号。会话由正常宿主和平台凭据库处理，只有一次性 HTTP 后端及其可用性故障是模拟的。

浏览器回归还覆盖受保护数据临时失败后的刷新凭据轮换及再次登录。三个浏览器诊断条目与 favicon 请求相关，未记录 CSP/form-action 诊断。这是脚本驱动验收，不是物理操作或真实服务商测试。两个完整运行分别耗时 15.063 秒和 11.128 秒，这些数字不是 UI 延迟或 FPS 指标。

以下八张原始截图已逐张检查，复制时没有修改：

| 截图 | 来源与观察 |
| --- | --- |
| [注册](04-registration-webview.png) | 原生 WK 快照：滚动后，虚构用户名、掩码密码和两个表单操作均可见。 |
| [错误密码](05-invalid-password-webview.png) | 原生 WK 快照：后端凭据错误提示清晰可读。 |
| [重试](04-retried-webview.png) | 原生 WK 快照：全新视图再次加载登录表单。 |
| [等待登录时退出后重开](02-pending-process-reopened.png) | 原生 Makepad 图像：没有账号和受保护数据。 |
| [受保护身份](06-connected.png) | 原生 Makepad 图像：应用显示真实宿主响应。 |
| [跨应用拒绝](08-cross-app-denied.png) | 原生 Makepad 图像：第二个应用不能使用第一个连接。 |
| [冷启动恢复](09-cold-restored.png) | 原生 Makepad 图像：重启进程后仍可读取受保护身份。 |
| [退出登录](10-logout-denied.png) | 原生 Makepad 图像：账号和数据清空，受保护读取被拒绝。 |

WK 图像捕获操作系统 WebView；Makepad 图像捕获自身渲染目标，不包含操作系统子视图。均未拼接，也不是整屏照片。带临时回环端点的授权宿主框架截图已私下检查，但不发布。原生日志、控件快照、配置目录、控制文件和授权材料也不发布。可见的 `example.invalid` 身份均为虚构。

两个运行均停止了原生进程、浏览器和服务器，并持久撤销全部本地测试连接句柄。通过正常宿主断开操作请求了平台凭据删除，但未独立读取凭据库验证。[此前三次运行](prior-attempts.json)均保留为历史记录。上一轮 8+7 验收的原始记录和八张 PNG 完整保存在 [Glance 更新前历史目录](historical/before-glance-refresh/README.zh-CN.md)，不会被重新标记为本轮构建的证据。本记录不覆盖 Android、同时打开多个应用窗口或物理键盘输入；同进程弹层替换另有生命周期测试。复现参见[原生驱动指南](../../backend-webview.zh-CN.md)。
