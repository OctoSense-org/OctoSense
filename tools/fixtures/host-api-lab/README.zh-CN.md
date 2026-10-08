# 原生宿主 API 验收

[English](README.md) | 简体中文

[Android 复现步骤与 OnePlus 6 结果](ANDROID.zh-CN.md)：同一原生宿主完成了全部 14 项手机检查，没有启动模型、登录账户或批准权限。

此开发测试示例展示：应用自己的 Splash 工具调用已编译进 OctoSense 的
Rust 服务，读取真实 macOS 相机权限状态，更新同一应用的界面，并把结构化
结果返回给原生调用方。它不会采集媒体或批准设备访问，不是 App Hub 投稿，
也不能加载任意 Rust 动态库。

调用路径：临时签名目录 → Store 安装并验证启动 → 应用隔离环境 →
`apilab.inspect` → `app_tool(name, call_id)` →
`host.request("camera.permission.status", ...)` → Rust `DeviceService` →
Makepad 原生权限查询 → macOS → 回调 → 更新界面并调用
`mod.app_tools.complete(call_id, result)`。

原生测试直接进入已授权的工具队列。生产环境中的 agent 还必须经过 shell
路由、应用 agent 同意、账户和工具授权。本测试不验证这些上游模型及 peer
步骤；它验证真实签名包检查、Splash 隔离环境和设备处理器。

## 在 macOS 运行

需要 macOS 图形会话。窗口隐藏，不会抢占焦点。使用 `tools/setup.py`
准备的兼容源码依赖，在 OctoSense 目录运行：

```sh
python3 tools/setup.py
cargo build --locked --release -p octosense-shell --example host-api-lab --features acceptance-fixtures
# 构建宿主已固定的 App Hub CLI 依赖：
cargo build --locked -p octosense-app-hub --bin hub
python3 tools/test-host-api-native.py --hub target/debug/hub
```

脚本创建并打印私有证据目录，复制示例、抓取原生预览用作 listing 截图，
计算副本摘要，用仅存在于内存的密钥签名，安装到全新测试配置，然后运行
原生检查。源码和普通用户配置不会更改。此签名只使用隔离测试信任根，
不会创建发布者身份或修改公共目录。

即使准备失败，`result.json` 也会记录结果和未验证范围。
`native-result.json` 保存原生状态与工具回答，PNG 和控件快照展示真实
预览与执行后的界面。日志和私有配置留在证据目录，分享前请先本地审阅。
脚本仅关闭自己启动的测试进程。

## 验证内容

- 已签名应用执行自己声明的 `implemented_by: "app"` 工具。
- 工具读取真实原生权限状态，并更新正在运行的应用界面。
- `runtime.describe` 能发现已有 API，并报告不存在的自定义函数。
- 未声明麦克风能力的应用无法读取麦克风权限状态。
- 工具的异步宿主回调不能打开权限批准面板。
- 错误账户、未声明工具、非法输入均被拒绝。
- 卸载工具所有者后调用返回 `app_not_running`。

工具会刻意尝试后台权限请求以验证拒绝行为，但无法批准它。能力声明、
应用同意、系统授权是三个独立检查。系统之前已授权也可作为有效测试状态，
但全新应用配置的 `app_consent` 必须为 `false`。

**未覆盖：**真实模型推理、物理批准权限、相机采集、Android 执行、
Linux/Windows 设备服务，以及兼容宿主安装包发布。独立的真实 VM 运行时回归测试
覆盖了分离定时器、暂停任务、HTTP/WebSocket 回调及原生设备辅助函数的权限
检查；这里测试连续的宿主回调。

## 复用方式

开发者可以复用声明和 `app_tool` 模式，并只申请实际需要的能力。该钩子
在已打开应用的 VM 中运行，关闭应用后不可执行。`host_method` 工具则
直接映射到允许使用的 Rust 服务方法。两者都不会从应用包编译新的原生
函数。用 `host_api.required` 拒绝不兼容宿主，用可选 API 发现提供手动
降级路径。宿主边界见 [ADR 0012](../../../docs/adr/0012-app-host-api-discovery.zh-CN.md)。
