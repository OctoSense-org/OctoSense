# 通用应用工具验收程序

[English](README.md) | 简体中文

这是开发专用原生程序，对明确提供的本地夹具调用真实 `script-tools-v1` ABI。
它弥补测试缺口：`card-host` 不支持此 ABI，`host-api-lab` 只检查固定应用与工具。
程序不加载模型、不授予应用代理同意、不读取日常资料。

在已准备好的 OctoSense 仓库构建：

```sh
python3 tools/setup.py
cargo build --locked --offline --release -p octosense-shell \
  --example app-tool-acceptance --features acceptance-fixtures -j2
```

配套 App Flow 的 `examples/script-tool-state/verify-native.py` 驱动创建隔离资料，
操控隐藏原生窗口并记录证据。它需要本程序、已构建的 `hub` 和全新的输出目录。
[迁移指南](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/SUBMISSION-API-MIGRATIONS.zh-CN.md)
与夹具属于配套变更，旧版 App Flow 不包含它们。

程序参数：

| 参数 | 含义 |
| --- | --- |
| `--bundle=<目录>` | 已 stamp 的可编辑夹具；不会修改原文件 |
| `--app-data=<目录>` | 新的空资料目录，或同一夹具已标记的目录用于重启测试 |
| `--receipt=<文件>` | 新 JSON 结果文件 |
| `--trigger-file=<文件>` | 驱动观察到 UI 加载完成后才创建的新文件 |
| `--tool=<名称>` | 已声明、由应用实现的工具 |
| `--args=<JSON>` | 合法输入 |
| `--invalid-args=<JSON>` | schema 必须拒绝的输入 |
| `--expected=<JSON>` | 期望的精确返回值 |
| `--preview` | 仅渲染真实商店截图，不分发工具，不声明闸门验收 |

正常路径复制应用包、用临时内存密钥签名、运行要求签名的真实闸门，检查运行时
需求，应用准入后的隔离策略，并绑定真正的 Splash `app_tool`。先检查错误账号、
未声明工具、非法参数拒绝，再提交合法调用。回调完成后比较精确结果，注销工具所有者，
验证 `app_not_running`，保留 UI 供 instrument 截图和编辑。

驱动用相同资料目录重启并调用读取工具，证明原生编辑与工具共用持久化值。
私钥不离开内存，签名不建立公开发布者身份。这验证的是签名准入，**不是**公开商店
安装或 GitHub 发布者证明。调用入口位于生产同意/peer 分发之后，不能证明用户允许代理
运行或模型推理。

设置 `MAKEPAD_HIDE_WINDOWS=1` 和独立 `MAKEPAD_REMOTE` 端口。驱动隔离
`RINX_DATA_DIR`、`OCTOSENSE_HOME`、`OCTOS_APP_CORE_DIR`，关闭系统字体回退，
保留失败日志与结果，捕获真实像素，只清理自己的进程。除 JSON 外还应检查截图，
控件文本无法证明不存在重绘或裁剪问题。

**macOS 已验证：** release 构建通过；Script Tool State 通过签名准入、真实工具/UI
修改、中英手动编辑、精确重启恢复和拒绝检查。API Migration Lab 通过原生编辑、
重启和服务缺失路径。七张原生 Metal 图均已检查。两个私有变异版本按预期拒绝：
旧回调名称和缺少运行时标记。二进制/源码身份、精确结果与范围见
[validation.json](validation.json)。不覆盖手机、提供商、OS 批准、代理同意、公开安装或外部操作。
