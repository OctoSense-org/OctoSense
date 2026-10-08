# 原生宿主 API 验收

[English](README.md) | 简体中文

这个开发测试示例演示应用自己的 Splash 工具如何调用已经编译进 OctoSense 的 Rust 代码。工具读取 macOS 上真实的摄像头权限状态，更新应用界面，并把结构化结果返回给原生调用方。它从不采集媒体，也从不批准设备访问。它不是提交给 App Hub 的应用，也不能用来加载任意 Rust 库。

一次调用经过以下路径：

```text
temporary signed catalog → Store install / prepared launch
    → isolated app with admitted capabilities
    → apilab.inspect → app_tool(name, call_id)
    → host.request("camera.permission.status", ...)
    → Rust DeviceService → Makepad native permission query → macOS
    → callback → live app UI + mod.app_tools.complete(call_id, result)
```

测试宿主把调用直接放进已授权的工具调用队列。真实 Agent 的调用要先经过 Shell 的中转，以及用户对应用 Agent 的同意、账户授权和工具授权；本测试**不**覆盖这段模型与 peer 路径。它覆盖的是真实的签名应用包检查、Splash 隔离环境和[设备服务](../../../crates/shell/src/platform_services/README.zh-CN.md)。

## 在 macOS 上运行

需要 macOS 图形会话。窗口始终隐藏，也不会抢占焦点。在 OctoSense 检出目录中运行以下命令；构建所用的固定版本框架源码由 `tools/setup.py` 准备：

```sh
python3 tools/setup.py
cargo build --locked --release -p octosense-shell --example host-api-lab --features acceptance-fixtures
# 用本宿主固定的 App Hub 版本构建 Hub CLI：
cargo build --locked -p octosense-app-hub --bin hub
python3 tools/test-host-api-native.py --hub target/debug/hub
```

脚本会创建一个私有证据目录并打印它的位置；如果要自己指定目录，请用 `--output` 传入一个尚不存在的路径。脚本随后复制测试示例，截取它的原生预览作为商店截图，为副本写入摘要，用只存在于内存中的密钥签名，安装到全新的测试配置中，再运行原生检查。它既不改动源码文件，也不改动任何普通用户配置。这里的签名只服务于隔离的测试信任根：不会创建发布者身份，也不会改动公开的签名目录。

即使准备阶段失败，`result.json` 也会记录结果和仍未验证的范围。`native-result.json` 保存原生状态和工具的返回结果。PNG 图片和控件快照展示真实的预览和执行完毕的应用。日志和私有配置都留在证据目录中；分享之前请先自行检查。脚本只关闭它自己启动的测试进程。

## 检查内容

- 已签名的应用调用自己声明的 `implemented_by: "app"` 工具。
- 工具读取真实的原生权限状态，并更新正在运行的界面。
- 应用界面上的按钮也能调用同一个宿主服务。
- `runtime.describe` 能发现已编译的 API；对于宿主没有的自定义函数，它报告不可用，而且不执行任何代码。
- 没有 `microphone` 能力时，应用无法读取麦克风权限状态。
- 工具的异步宿主回调不能打开权限面板。
- 账户不对、工具未声明或输入无效的调用都会遭到拒绝。
- 持有工具的应用关闭后，调用返回 `app_not_running`。

工具会刻意从后台申请权限，以证明这种申请会遭到拒绝；它无法批准这次申请。能力、应用授权和系统权限始终是三项独立的检查。这台 Mac 可能早已授予 OctoSense 摄像头权限，但全新的测试配置仍必须报告 `app_consent: false`。

**已验证：**上述命令在 `.github/workflows/desktop.yml` 的 `native-host-api` 任务中，于 GitHub 的 `macos-14` 运行器上通过。

**未覆盖：**真实的模型推理、亲手点按批准权限、摄像头拍摄、Android、Linux 和 Windows 上的设备服务，以及发布兼容的宿主二进制。另有在真实 VM 上运行的运行时回归测试，覆盖分离的定时器、暂停的任务、HTTP 和 WebSocket 回调，以及原生设备辅助函数中的检查；本测试示例覆盖的是连续的宿主回调。

## 复用这一模式

把工具声明和 `app_tool` 钩子复制到你的应用中，并且只申请真正用到的能力。钩子在已打开应用的 VM 中运行，所以应用关闭后无法执行这些工具。`host_method` 工具则直接映射到白名单中的某个 Rust 服务方法。两种机制都不会从应用包编译新的原生代码。用 `host_api.required` 拒绝不兼容的宿主，用可选的 API 发现提供手动降级路径。App Hub 的[脚本工具参考](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md#script-tool-execution-script-tools-v1)介绍了这个钩子，[ADR 0012](../../../docs/adr/0012-app-host-api-discovery.zh-CN.md) 说明了宿主边界。
