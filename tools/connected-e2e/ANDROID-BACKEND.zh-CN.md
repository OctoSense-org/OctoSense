# Android 后端登录验收

[English](ANDROID-BACKEND.md) | 简体中文

这是独立调试包的开发者后端登录测试，不是 Google 登录。真实 HTML 表单负责注册／登录；宿主执行一次性授权码、PKCE、回调、令牌交换、身份查询和 Android 凭据库持久化。不得注入已登录账号或令牌，虚构密码只在后端页面输入。

使用指定测试手机以及已有 JDK 17、Android SDK／build-tools 35 和仓库现有 Makepad Android 工具链。不要替换 Home 或其他测试应用。所有产物放在新的私有目录。英文页给出了同一套已执行命令；其中路径变量由测试者提供，不需要提交个人路径。

1. 编译 `.sources/makepad/tools/cargo_makepad`，通过 `phone/android/gradlew --offline --no-daemon :contracts:exportHomeContracts` 导出合约。从 `phone/` 构建，设置 `MAKEPAD_FORCE_DEBUGGABLE=1`，使用包名 `dev.makepad.octosense.backendlogin`、标签 `OctoSenseBackendLoginTest`、`--abi=aarch64` 及 `build -p octosense-home --release --locked --offline --no-default-features --features dev-mode,app-hub,octosense-shell/acceptance-fixtures`，指定已有 `--sdk-path`。这个最小登录构建不包含模型或内核。
2. 在单独终端运行 `python3 tools/backend-login-fixture.py --directory "$LAB/server"` 并保持运行。metadata 只有本地公开端点；不要整体发布运行目录。
3. 按英文页的三个命令运行 `android_backend_lab.py prepare`、`android_backend.py` 和 `android_backend_lab.py deploy`。准备过程通过普通签名目录／安装校验，包含两个固定测试应用 ID。设置已有 JDK 的 `JAVA_HOME` 供 apksigner 使用；多设备时只选择指定的 `--serial`，不得公开设备编号。

部署拒绝已存在的包，只传输新建的已签名测试目录，建立一个 ADB loopback reverse，并通过一次性 `makepad.APP_CONFIG` Intent extra 启动。Android 会清除只有环境变量的 app config，因此不能省略 Intent。需要新后缀时，APK 构建、封装和部署必须使用同一包名。准备脚本会嵌入已跟踪的 `backend-login/glance.splash`，发布仍经过普通应用 Glance 权限校验。测试插图和界面不是生产应用素材。

实际手机操作，不重放结果未知的输入：

- 仅当权限框明确属于本测试包时拒绝无关定位／媒体权限；不要操作其他应用提示。
- 点击 **Connect in WebView**，阅读宿主授权提示，再在真实后端表单使用虚构 `example.invalid` 地址及测试密码注册／登录。确认回调完成和 **Protected identity**。不使用个人账号。
- 再次登录，检查错误密码拒绝、键盘可达性、原生 Back／Cancel、同源信息页和返回应用。取消不能创建连接。新尝试不应带上上次会话 Cookie；只检查测试服务日志中的 Cookie 是否存在，不读取或公开值。
- 只强停本测试包，用相同一次性 fixture 配置重启。**Load active account**／**Protected identity** 应读取原生凭据库持久连接，**Disconnect** 应撤销。第二个已签名测试应用需验证账号隔离，不能使用第一个应用的句柄。
- **Show login in Glance** 发布测试卡。进入正常 Glance 页面打开它，点击 **Sign in from Glance**，分别验证取消和完成登录回调。这才覆盖 Android 暂停／恢复时宿主登录页的交接，普通应用内登录不能替代。

原生登录 Activity 启用 `FLAG_SECURE`，故无法截取登录画面。ADB UI 层次可验证控件标签、边界和表单操作，但不等于原始像素视觉验收。仅在返回测试应用后截图，避免其他应用、通知和键盘建议。本次 OnePlus Android 15 在带键盘登录成功返回后，ADB 仍可能产生全黑图片。已有 dev-mode `capture:<私有文件>` 仅读取 Makepad 应用纹理；将其有效画面与 ADB 黑帧配对，可区分应用绘制和系统截图。这不截取受保护的原生 WebView，也不能证明人在物理屏幕上实际看到了什么。公开记录只包含事件类型／状态以及源码／APK 哈希，不能包含账号、密码、令牌、state、回调、设备编号、私人路径或临时端口。

结束后停止自己启动的服务和测试应用，只删除自己的 ADB reverse。私有失败记录单独保留。构建／部署成功不是手机验收通过；脱敏结果必须逐项列明实际完成的原生流程。

[2026-10-07 OnePlus 结果](evidence/backend-android-20261007/README.zh-CN.md) 绑定实际 APK／源码哈希，分别列出最终验证、早期失败和未运行项目。
