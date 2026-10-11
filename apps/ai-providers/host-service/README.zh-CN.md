# octosense-llm-service：`llm` 和 `model` 宿主服务

[English](README.md) | 简体中文

AI providers 系统应用 `os.ai-providers` 通过 `llm` 服务配置 Shell 共用的模型供应商。普通应用通过 `model` 进行一次性的结构化模型调用；持久应用 Agent 则通过 `ai-host` 和 `app-peers` 使用同一个内核。这三个接口承担不同职责。

密钥、PIN 和供应商二维码由宿主界面处理，脚本应用不会得到这些秘密。方法定义见 [src/lib.rs](src/lib.rs) 和 [src/complete/mod.rs](src/complete/mod.rs)。

## 供应商配置

模型目录来自 `octosense_llm_config::catalog`。`llm.families` 和 `llm.models` 为选择器提供模型名称、上下文窗口和价格。宿主设置向导依次选择模型家族、模型、服务地址和协议、API 密钥，然后测试并保存。测试失败时可以明确选择不测试直接保存。

官方路线不保存单独的 route 对象；自定义路线保存 octos 识别的 `route_id`、`label`、`base_url` 和 `api_key_env`。模型调用按照用户设置的主供应商和备用顺序执行。

Shell 先配置唯一的 `octosense-kernel`，再注册服务。启用 `octos-core` 时，服务写入同一个内核目录的 `profiles/_main.json`，供应商变更后重启内核，消费者随后重新连接。没有该 feature 时服务只更新配置。

Shell 通常通过 [`crates/ai-host`](../../../crates/ai-host) 完成注册。`Options` 可设置：

| 配置 | 作用 |
| --- | --- |
| `core_dir` | 内核使用的配置目录 |
| `vault` | 平台秘密存储；测试可使用隔离实现 |
| `scanner` | 手机摄像头扫描，桌面可不提供 |
| `image_picker` | 从系统文件或照片选择器导入二维码图片 |
| `image_drops` | 桌面将拖放的图片交给服务 |
| `on_changed` | 保存、排序、删除、导入后的回调；内核重启后触发，可能在工作线程运行 |

`llm` 属于宿主的供应商管理接口，仍仅服务系统应用。删除普通应用的能力声明检查，不会让商店应用读取或配置用户的供应商密钥。

## 扫码、图片与导入

`QrScanner`、`QrImagePicker` 是 Shell 提供的适配器。服务本身不链接 Makepad。Shell 在 UI 线程打开扫描器或系统选择器，完成回调必须恰好调用一次；取消保留原界面，失败显示原因。

图片以原始 PNG/JPEG 字节传给服务，在工作线程中解码和查找二维码。解码前拒绝超过 20 MB 或 4000 万像素的图片。识别尝试原图、二值化和缩放副本；成功后在宿主界面输入 PIN。桌面拖放同样进入此流程，不将文件直接交给脚本 isolate。

导出界面展示 `OCTOS1E:` 代码及 PIN，最长五分钟；关闭或到期后清除。测试可以缩短有效期，不能延长。界面使用单次定时器，避免切换 Splash 内容后旧的重复定时器继续运行。

导入只增加或更新，不删除已有供应商，也不改变已有顺序：

- 没有现有配置时，代码中的第一项成为主供应商。
- 已有相同模型和路线时，保留位置并更新密钥。
- 新路线追加为备用；不同密钥使用独立 `api_key_env` 槽，避免覆盖另一条路线。
- 重复导入相同代码不产生变化；删除供应商时，仅在没有其他路线引用其槽时才删除该槽。

界面显示已添加、已更新或已存在的结果，不将密钥展示给应用。

## 密钥存储

| 平台 | 保存位置 | profile 中的值 |
| --- | --- | --- |
| macOS | 登录钥匙串，服务 `octos`，账号为环境变量名 | `keychain:` |
| Linux | `<core_dir>/secrets/<ENV>`，权限 0600 | `keychain:` |
| Android、iOS 等 | 应用私有 profile，权限 0600 | 密钥本身 |
| 开发覆盖 `OCTOSENSE_LLM_VAULT=file` | profile | 密钥本身 |

Android 内核从 profile 的 `env_vars` 读取密钥，因此不能直接使用它无法解密的 Android Keystore 密文。平台钥匙串拒绝写入时，密钥仍保存在私有 profile。这是当前实现边界；不能把脚本应用不能读取密钥误写为所有平台都使用硬件加密存储。

## 一次性 `model` 调用

`model.complete` 接受任务、输入、JSON Schema，以及可选的 `fast`/`strong` 模型类别和 URL 输出选项。宿主选择供应商，校验返回 JSON，不合格时重试一次。应用不指定供应商或密钥，不建立工具调用、浏览、记忆或持久对话。

主要限制包括：任务 4 KiB、应用输入 32 KiB、结构化输出 16 KiB；默认每个应用每分钟 6 次、每天 100 次和 100,000 tokens。账本保存在 `<host dir>/model/ledger.json`。`model.budget` 返回调用应用的预算。工具箱的模型调用也共用这条预算路径。

`capabilities` 是使用说明，省略 `model` 不阻止调用。授权仍由宿主完成：

- Shell 必须通过 `complete::Options::grants` 验证已准入应用、bundle 身份和精确宿主目录。没有回调时拒绝；直接读取应用写入的 manifest 不能授权。
- Shell 提供当前账号范围。有账号功能的应用未登录时，不会回退到设备范围；账号切换使旧结果失效。
- 工作线程保持并发、大小、速率和预算限制，并在回复前再次检查准入、账号与取消状态。
- 错误码 `capability` 为兼容保持不变，但现在表示宿主未准入该调用方，而不是缺少能力声明。

直接调用 `model` 不会自动启用持久 Agent。应用 Agent 另有启用意图和用户同意，跨应用工具仍需要共享授权和审批。

## 验证

在仓库根目录运行 `python3 tools/setup.py`，然后：

```sh
cargo test --locked -p octosense-llm-config -p octosense-llm-service
cargo test --locked -p octosense-llm-service --features octos-core
cargo test --locked -p octosense-llm-service --test complete
```

模型测试使用假传输，覆盖省略能力声明后的调用、缺少宿主准入时拒绝、输出校验、预算和取消。媒体测试还检查账号切换、准入撤销、并发限制及迟到回复。这些测试不验证真实模型供应商或手机。

真实供应商示例 `model_complete_live` 会产生真实请求和费用，只有明确进行该项验收时才运行。macOS 钥匙串测试也默认忽略，需显式选择 `-- --ignored keychain`。仓库只使用根目录统一固定的依赖及 `.sources/`，不增加个人机器路径或本地 patch。
