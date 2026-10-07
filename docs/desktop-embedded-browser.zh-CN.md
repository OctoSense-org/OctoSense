# 桌面嵌入式浏览器

[English](desktop-embedded-browser.md) | 简体中文

源码构建为 Linux X11/XWayland（WebKitGTK）和 Windows（Microsoft Edge WebView2）
增加了原生 `WebReader` 适配器。网页作为子视图嵌入 Makepad 窗口，不会启动外部
浏览器，也不使用 CEF。发布二进制必须包含新的运行时补丁才能提供此功能；
仅安装应用包不能升级旧宿主。

## 运行条件

| 平台 | 条件与行为 |
| --- | --- |
| Linux X11 / XWayland | GTK 3 与 WebKitGTK 4.1；兼容的 4.0 为后备。运行时动态加载库，普通宿主编译不需要 WebKit 开发头文件。引擎沙箱保持开启。 |
| 原生 Wayland | 尚未实现嵌入子视图，控件明确报错。桌面入口在现有 X11/XWayland 显示可用时优先选择它；显式后端选项和 Vulkan 构建的要求优先。不会自行启动 X 服务。 |
| Windows | 需要已安装且提供 `ICoreWebView2_27` 的 Microsoft Edge WebView2 Runtime。引擎缺失或版本过旧会明确失败，宿主不会下载引擎。适配器使用 `webview2-com` 0.39.1。 |
| macOS / Android | 本次桌面改动不改变已有原生阅读器适配器。 |

由于 `dpkg-shlibdeps` 看不到动态加载的库，`.deb` 发布配置显式声明 GTK 3 和
WebKitGTK 4.1 或 4.0 运行库依赖。AppImage 用户需要另行安装这些库。Windows
部署需要提供受支持的 WebView2 Runtime。本次源码改动不打包任何引擎。Windows 适配器要求用于阻止原生“另存为”和屏幕捕获的接口，
不会在旧引擎上悄悄跳过这些限制。

## 应用与宿主边界

应用继续使用已有 `WebReader` 控件及权限检查。受限阅读器只能停留在已准入的
文档（可改变片段位置）。拥有现有 `web` 授权的阅读器可以访问公开 HTTPS 页面。
每次导航都重新检查，包括重定向和子框架。拒绝本地文件、外部应用协议、含账号
凭据的 URL 与畸形地址。宿主明确准入的初始 HTTP 文档仍可使用，因此隔离的
回环测试页面不会顺带授予任意本地地址访问权。

每个新视图使用新的私有浏览器会话。关闭会销毁原生控制器并停止页面执行；
再次打开使用新会话。隐藏或裁剪只移开可见覆盖层，保留当前页面。原生子视图
在裁剪父窗口中保持完整文档尺寸，避免宿主滚动时网页按可见小块重新排版。

网页没有 OctoSense 工具桥。拒绝网页发起的原生权限请求、下载及弹窗，不能借
浏览器权限框取得应用的摄像头、麦克风或文件权限。URL 校验不是 DNS 或网络
沙箱：获准的公开域名仍可能解析到私有地址，因此不能把阅读器当作 SSRF 隔离边界。

后端登录是独立的宿主流程。**Linux 和 Windows 仍使用已有外部浏览器认证路径**；
普通阅读器支持不会启用嵌入式 OAuth，也不会把登录 Cookie 交给应用。Linux
WebKitGTK 不能在请求认证回调前可靠证明导航属于主框架，因此拒绝嵌入式认证。
Windows 适配器包含回调拦截，但完整宿主认证流程通过验收前，生产入口仍不启用
该模式。Google、GitHub 保留现有提供方流程。见[已连接账户指南](../crates/oauth-service/README.zh-CN.md)。

## 代码导读

- `desktop/src/main.rs` 在 Makepad 选择窗口后端之前启用 Linux 桌面的 X11/XWayland
  优先策略；显式后端选择始终优先。
- `tools/runtime-patches/makepad-desktop-webview.patch` 保存运行时扩展。
  `runtime-patches.lock.json` 固定补丁摘要与最终 Makepad 树，`tools/setup.py`
  将它叠加在现有补丁上。
- Makepad 的 `system_browser::BrowserPolicy` 使用 `url` 解析和检查地址。
  `linux_webkit.rs` 将 GTK plug 嵌入 X11 子 socket，在已有 UI 线程中有界处理 GTK
  事件。`windows_webview.rs` 把 WebView2 控制器嵌入裁剪用子 HWND，并在 UI STA
  接收异步回调。
- `WebReader` 保留已有覆盖层生命周期，把原生加载、导航和失败事件转换为控件
  状态。引擎缺失会进入错误状态，不会留下声称成功的不可见视图。

## 验收

`crates/browser-smoke` 是不发布的原生测试宿主，与生产 Shell 分开。
`tools/browser-smoke.py` 在回环地址提供合成 HTML，通过私有控制目录驱动测试宿主。
检查真实页面 JavaScript、DOM 编辑、禁止导航是否在 HTTP 请求前被拦截、视图隐藏与
恢复、关闭后停止执行、重开后全新 Cookie，以及原生网络错误。不使用个人账户，
也不下载引擎。Windows 工作流还要求浏览器引擎自行产生 PNG 快照和元数据。

共享 URL 策略测试使用 `cargo test --locked -p octosense-browser-smoke`，覆盖受限
文档、公开 HTTPS、畸形/私有地址及精确主框架回调；它们本身不证明原生导航拦截。
原生驱动使用自动 DOM 输入，不代表物理键盘输入、无障碍、视觉质量、完整 Shell
UX 或 OAuth 已验收。

Linux 已使用 WebKitGTK 2.52.6 和 Xvfb 21.1.22 通过八项原生检查，引擎沙箱
保持开启。XQueryTree 探针证明 GTK plug 位于 Makepad 窗口中的子 socket 内，
且几何尺寸有效，没有捕获整个显示器。Windows 原生验收在 CI 回执通过前仍为
**未验证**；详细回执记录在 PR 和 CI 工件中。`.github/workflows/embedded-browser.yml` 的 Windows 任务在
引擎缺失或不兼容时失败，不把它算作跳过后通过。只上传合成回执与引擎自身的捕获。

修改浏览器文件时，本地 CI 合并工具同样要求 Windows 原生工作流在精确的 PR
提交上通过。macOS/Linux 本地通过不能代替这项证据。
