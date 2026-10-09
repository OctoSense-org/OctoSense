# 桌面嵌入式浏览器

[English](desktop-embedded-browser.md) | 简体中文

从桌面版 0.1.0-rc.1 起，`WebReader` 可以在 Linux 和 Windows 桌面上使用。`WebReader` 是在脚本应用中显示网页的 Splash 控件。在这两个平台上，它把系统自带的浏览器引擎作为子视图嵌入 Makepad 窗口：

- **X11 或 XWayland 下的 Linux**（XWayland 是 Wayland 桌面为 X11 程序运行的 X 服务器）：WebKitGTK。
- **Windows**：Microsoft Edge WebView2。

它从不打开外部浏览器，也不使用 CEF（Chromium Embedded Framework）。更早的发行版本（例如 `desktop-v0.1.0-beta.2`）无法在 Linux 或 Windows 上显示嵌入网页。这项功能属于宿主的运行时，安装较新版本的应用并不能让旧宿主获得它。

## 运行条件

| 平台 | 需要什么，缺少时会怎样 |
| --- | --- |
| Linux，X11 或 XWayland | GTK 3 和 WebKitGTK 4.1；缺少 4.1 时使用 4.0。宿主在打开网页时才加载这些库，因此构建 OctoSense 不需要 WebKit 开发头文件。WebKit 自身的沙箱保持开启。缺少这些库时，阅读器会报错，并指明需要安装什么。 |
| Linux，原生 Wayland | 不支持：阅读器会报错，并建议用 `--linux-backend=x11` 重新启动。设置了 `DISPLAY` 时，OctoSense 的桌面入口已经优先选择 X11 或 XWayland，除非 `--linux-backend` 另有指定，或者构建使用了需要原生 Wayland 的 Vulkan。OctoSense 从不自行启动 X 服务器。 |
| Windows | 已安装且提供 `ICoreWebView2_27` 的 Microsoft Edge WebView2 Runtime。没有安装或版本过旧时，阅读器会报错；宿主从不下载引擎。适配器使用 `webview2-com` 0.39.1。 |
| macOS、Android | 不变：继续使用已有的原生阅读器。 |

`.deb` 安装包把 GTK 3 以及 WebKitGTK 4.1 或 4.0 列为依赖，因为 `dpkg-shlibdeps` 看不到运行时才加载的库。使用 AppImage 时，请自行安装这些库。在 Windows 上，请安装 WebView2 Runtime。OctoSense 不附带任何一种引擎。Windows 适配器要靠 WebView2 的相应接口阻止原生“另存为”对话框和屏幕捕获；运行时版本过旧时，它直接失败，而不是在缺少这些保护的情况下显示网页。

## 网页能做什么

应用照旧使用 `WebReader`，能力检查也与以前相同。网页打开之后，适配器会重新检查每一次导航，包括重定向和子框架：

- 应用没有 `web` 授权时，阅读器只能停留在它打开的文档上，在该文档的片段（`#fragment`）之间跳转。
- 应用有 `web` 授权时，阅读器还可以访问公开的 HTTPS 页面。
- 两种情况下，阅读器都会拒绝本地文件、其他应用的 URL scheme、带用户名或密码的 URL，以及格式错误的地址。

第一个页面只要已通过应用的能力检查，也可以是普通 HTTP 页面。验收测试正是借此从回环地址提供测试页面；这并不会放行其他本地地址。

每个阅读器打开时都会得到一个全新的私有浏览会话。关闭阅读器会停止网页脚本并销毁原生视图；再次打开时会开始新会话，不带上次的 Cookie。隐藏或裁剪阅读器只是把原生视图移出屏幕，网页本身保留。原生视图在负责裁剪的父窗口里保持网页的完整尺寸，所以滚动应用时，网页不会按可见部分重新排版。

网页没有通往 OctoSense 的桥：它不能调用应用的工具或宿主服务。两个适配器都会拒绝网页的权限请求、下载和弹出窗口，网页无法借此获得应用的 Splash 摄像头、麦克风或文件 API。Linux 还会取消文件选择器和打印。Windows 不拦截 HTML 文件选择器和打印，这些原生对话框在 Windows 上的行为尚未验证，因此不要把它们说成已禁用。用户为网站选择文件，与应用的宿主服务授权毫无关系。

URL 检查不是网络沙箱：公开的主机名也可能解析到私有地址。不要指望阅读器把网页挡在私有网络之外（SSRF）。

## 登录仍在外部浏览器中完成

后端登录是独立的、由宿主掌控的流程。**在 Linux 和 Windows 上，登录仍然使用外部浏览器**。阅读器支持既不会启用嵌入式登录（OAuth），也不会把登录 Cookie 交给应用。WebKitGTK 在请求登录回调之前，无法可靠地确认导航发生在主框架中，因此 Linux 适配器一律拒绝嵌入式登录。Windows 适配器能够拦截回调，但在完整的宿主登录流程通过验收之前，生产环境中的嵌入式登录保持关闭。Google 和 GitHub 继续使用各自已有的提供商流程。参阅[已连接账户](../crates/oauth-service/README.zh-CN.md)。

## 实现方式

- `desktop/src/main.rs` 在 Linux 上会先调用 `Cx::prefer_x11_for_embedded_browser()`，再由 Makepad 选择窗口后端。显式指定的 `--linux-backend` 选项仍然优先。
- `tools/runtime-patches/makepad-desktop-webview.patch` 保存这项运行时改动。`runtime-patches.lock.json` 固定该补丁的 SHA-256 和应用补丁后的 Makepad 树，`tools/setup.py` 把它叠加在其他运行时补丁之上。
- Makepad 的 `system_browser::BrowserPolicy` 用 `url` crate 解析每个 URL，并决定是否放行。`linux_webkit.rs` 通过 XEmbed 把 `GtkPlug` 嵌入一个 X11 子窗口（socket），并在已有的 UI 线程上处理有限量的 GTK 事件。`windows_webview.rs` 把 WebView2 控制器放进一个负责裁剪的子窗口（`HWND`），在 UI 线程的单线程单元（STA）上接收异步回调。
- `WebReader` 保留原有的覆盖层生命周期，把原生的加载、导航和失败事件转换为控件状态。拦下某次导航不算致命错误：拒绝某个 iframe 时，外层获准的页面照常显示。标题变化会传给控件。Linux 还会报告 URL 变化，并在网页调用 `window.close()` 触发的回调返回之后释放原生视图。缺少引擎时走错误路径，所以控件绝不会显示一个看不见、却自称已加载的视图。

## 验收测试

`crates/browser-smoke` 是挂载真实 `WebReader` 控件的原生测试宿主，与 Shell 分开，从不对外发行。`tools/browser-smoke.py` 在回环地址上提供合成的 HTML，并通过私有控制目录驱动这个宿主。它检查以下几点：

- 网页的 JavaScript 能运行，自动化的 DOM 编辑能生效；
- 标题变化能传到控件；
- 拒绝某个 iframe 后，外层页面仍可使用；
- 适配器在禁止的导航发出 HTTP 请求之前就将其拦下；
- 宿主接受隐藏和显示命令；
- 关闭会停止网页脚本，重新打开时 Cookie 是全新的；
- 原生网络错误能传到控件。

测试不使用个人账户，也不下载任何引擎。

在 Windows 上，工作流还要求一张由引擎自己生成的 PNG 快照，驱动脚本也会回读原生设置，证明网页消息和宿主对象均已关闭。为了证明消息计数器确实有效，测试宿主另建一个一次性的控制器，只为一个固定的合成页面开启消息，确认计数器收到了这个页面的消息，然后关闭该控制器并删除其配置目录。生产控制器从不开启消息；它的计数器只统计原生投递次数，从不读取消息内容，也从不据此执行任何操作。驱动脚本在限时等待结束后和同一视图关闭前，两次要求投递次数为零。这证明在测试观察期间没有任何消息送达。

回执只把网页消息调用的 JavaScript 返回值或异常作为诊断信息保留。[Microsoft 的文档](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.iswebmessageenabled)写明关闭消息时该调用会抛出异常，但实测的运行时在两项原生设置都关闭时仍正常返回。`chrome.webview` 存在也好，调用正常返回也好，都不能说明消息到达了宿主；只有原生计数才能说明。

GitHub 的 Windows 运行器没有 GPU，因此在该运行器上，`--software-graphics` 会设置 `MAKEPAD_D3D11_WARP=1`，让 Makepad 使用 Windows 内置的 [WARP 软件光栅器](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp)绘制，回执会记录这一模式。WebView2 仍是真实的引擎，沙箱照常开启。这次运行不能说明硬件 GPU 的性能，生产构建也照旧自行选择图形适配器。

`cargo test --locked -p octosense-browser-smoke` 运行共享的 URL 策略测试，覆盖受限文档、公开 HTTPS、格式错误或指向私有网络的 URL，以及精确匹配的主框架登录回调。仅凭这些测试，并不能证明原生引擎确实拦截了导航。

`.github/workflows/embedded-browser.yml` 中的 `windows` 任务在运行器上没有安装引擎或引擎版本过旧时会失败，绝不会把这种情况算作跳过后通过。它只上传合成的回执和引擎自己生成的截图；完整回执随 PR 及其 CI 运行保存，不进入仓库。改动涉及该工作流监视的文件时，`tools/ci-local-merge.sh` 还要求这个任务恰好在该 PR 的 head commit 上通过。macOS 或 Linux 上的本地通过不能代替它，Windows 交叉编译也不能。

### 已验证

- **Linux**：在 Xvfb 21.1.22 下使用 WebKitGTK 2.52.6，WebKit 沙箱开启，十一项原生检查全部通过。`XQueryTree` 检查确认 `GtkPlug` 位于 Makepad 窗口内的 socket 中，几何尺寸有效；该检查没有截取屏幕。同一次运行还确认，网页调用 `window.close()` 会移除原生视图，并停止网页的心跳请求。
- **Windows**：在 GitHub 的 `windows-2022` 运行器上使用 WARP，`windows` 任务中的十二项原生检查全部通过。
- **发行版本**：桌面版 0.1.0-rc.1 的发行源码也通过了 Windows 嵌入式浏览器检查（见[发行说明](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.1)）。

### 未验证

- 亲手输入、无障碍、视觉质量、完整的 Shell 体验和登录。驱动脚本使用的是自动化 DOM 输入。
- 原生视图是否真正可见：隐藏和显示检查只能确认宿主接受了命令。
- HiDPI 显示器，以及在 Windows 仍在创建控制器时关闭视图。
- Windows 上的 HTML 文件选择器和打印。
