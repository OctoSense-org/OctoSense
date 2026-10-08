# 模型媒体 API

[English](MEDIA.md) | 简体中文

获授 `model` 的应用可以请求宿主生成图片、合成语音、计算文本向量，或提交视频任务。
宿主使用 **AI providers** 中的凭证并选择媒体模型。应用不能提供或取得密钥、供应商
端点、供应商模型名称或供应商任务 ID。

本实现需要包含媒体服务的新宿主；发布 manifest 不会让旧宿主自动实现 API。
独立 `card-host` 不提供这些服务。协议和生命周期测试使用合成供应商响应；真实供应商
调用和手机渲染均**未经验证**。

## 发现与声明

在 `capabilities` 中声明 `"model"`。如果调用 `runtime.list` 或
`runtime.describe`，也声明 `"runtime"`。必需 API 会阻止在缺少相应版本的宿主上
安装；可选 API 允许应用提供降级界面：

```json
{
  "capabilities": ["model", "runtime"],
  "requires": ["host-api-v1"],
  "host_api": {
    "required": {"model.embeddings": 1},
    "optional": {"model.image": 1, "model.audio": 1, "model.video": 1}
  }
}
```

`runtime.describe({method:"model.image"})` 描述实际可执行 API。
`model.capabilities({})` 返回**已配置路由的可用性**和资源上限。两者都不能证明供应商
账户具备所需服务权益或余额。每次操作都检查当前应用准入和供应商访问。
仅配置 DeepSeek 的宿主不会宣称拥有可用的向量、图片、语音或视频路由。

使用通常的 `host.request("model.embeddings", args, callback)` 传输。
成功数据位于 `r.data`；失败使用传输层的错误封装。参数对象拒绝未知字段。
`class` 为 `fast` 或 `strong`，默认 `fast`；也接受
`output:{class:"fast"}`，但不能同时在两处指定。提示词和文本输入上限按 UTF-8
字节计算。

## 方法，版本 1

| 方法 | 参数 | 成功数据 |
| --- | --- | --- |
| `model.image` | `prompt`（1–4096 字节），可选 `size`、`n:1`、`seed`、`class` | `b64_json`、`format`、`mime`、`width`、`height`、`meta` |
| `model.audio` | `text`（1–4096 字节），可选 `voice:"default"` 或 `"warm-female"`、`format:"mp3"`、`class` | `b64_json`、`format:"mp3"`、`mime`、`bytes`，可选 `duration_s`，以及 `meta` |
| `model.embeddings` | `input`：一个文本或 1–16 个文本，每个 1–4096 字节；可选 `class` | 单字符串返回 `embedding`，数组返回按输入排序的 `embeddings`；另有 `dimensions`、`meta` |
| `model.video` | `prompt`（1–4096 字节），可选 `duration_s`（4–12，默认 4）、`resolution:"768P"`、`ratio`（`16:9`、`9:16`、`1:1`）、`class` | 不透明 `job`、`status`、`poll_after_ms`、`expires_at` |
| `model.video.status` | `job` | 相同任务封装；成功后含 `result:{url,format:"mp4",duration_s,resolution}` |
| `model.video.cancel` | `job` | 远端排队任务确认取消，或返回错误 |
| `model.capabilities` | `{}` | `configured` 标志、上限和生命周期说明 |
| `model.budget` | `{}` | 现有调用/令牌预算，加上 `media` 资源使用量和上限 |

图片尺寸为 `512x512`、`1024x1024`（默认）、`1536x1024` 和 `1024x1536`。
OpenAI 路由会在提交前拒绝 `512x512` 和 `seed`；MiniMax 路由支持两者。
每次请求只生成一张图片。PNG/JPEG 的实际尺寸必须匹配请求。语音输出为 MP3；应用
应说明声音由 AI 生成。供应商不提供时长时不返回该字段，宿主不会编造时长。
向量必须由有限数值组成、维数一致且不超过 4096；供应商批量索引经过校验和排序。

H3 提供分辨率级别和宽高比，不提供精确像素尺寸、帧率或种子选项。
视频的 `size`、`fps`、`seed` 等不支持参数会被拒绝。宿主不会从分辨率标签猜测
实际像素尺寸。视频结果 URL 由供应商托管，可能过期。

## 供应商选择

宿主按现有配置顺序选择第一个兼容路由。不自动重试计费提交，也不在 HTTP 结果
不确定时自动切换供应商：第一个请求可能已被接受。再次提交前应修正配置或请求。

| 已配置供应商 | 宿主选择的媒体模型 |
| --- | --- |
| OpenAI | 图片：`gpt-image-1-mini` / `gpt-image-1`；语音：`gpt-4o-mini-tts`；向量：`text-embedding-3-small` / `text-embedding-3-large` |
| MiniMax / MiniMax CN | 图片：`image-01`；语音：`speech-2.8-turbo` / `speech-2.8-hd`；视频：`MiniMax-H3` |
| OpenAI-compatible / llama-server / LM Studio | 仅支持向量，且运营者必须在带凭证的 HTTPS 路由上显式配置 `text-embedding-*` 模型 |

这些名称是**宿主实现选择**，不是应用请求字段。宿主不会把聊天模型发给媒体端点。
仅支持 Anthropic 协议的路由不会被用于媒体。宿主保留已配置代理的源和路径，拒绝
URL 内嵌凭证，并且不跟随 HTTP 重定向。不会把密钥发送到返回的媒体 URL。

MiniMax H3 需要按量计费 API 权益；仅有 M Plan/聊天账户并不足以证明支持。
路由可以已配置，但供应商仍拒绝该操作。错误不会返回原始供应商响应或账户详情。

## 预算和任务生命周期

每次生成/向量提交消耗现有的应用级模型调用/速率预算。在 HTTP 前，媒体还会预留
独立的每日资源额度：

| 资源 | 每个应用每个 UTC 日的默认上限 |
| --- | --- |
| 图片输出像素 | 8,388,608 |
| 语音输入字符 | 10,000 |
| 视频时长秒数 | 12 |
| 向量输入 UTF-8 字节 | 100,000 |

这些是资源上限，**不是货币支出保证**。宿主可以通过
`complete::Options.media_limits` 降低上限，包括设为零。预留量持久保存在应用
存储之外的宿主文件 `model/media-ledger.json` 中；供应商结果不确定、失败或取消
后都不会退还。无法读取额度文件时拒绝新提交。不会为图片、音频或视频编造文本
令牌用量。

所有 `model` 宿主方法（包括 `complete` 和 `budget`）共享全局最多四个工作线程，
每个应用/配置目录最多两个。签名包准入检查与额度文件读写在工作线程执行，
并在供应商请求和结果交付前检查取消状态及账户。供应商 HTTP 请求在
120 秒后超时；响应上限为 4 MiB，解码后图片/音频上限为 2 MiB，不存在无界工作
队列。异步处理前后检查签名应用准入、当前账户范围、供应商绑定和请求存活状态。
卸载、撤回、切换账户或更换凭证可能导致拒绝交付。关闭脚本隔离环境会丢弃其等待
响应，但不能撤销供应商已经接受的请求。

随机视频句柄绑定应用、宿主配置目录、当前账户和原始供应商凭证/路由。其他应用
无法查询或取消。运行中的宿主进程最多保留 64 个任务，期限 24 小时；重启宿主会
丢失句柄。重启或提交响应丢失后，供应商仍可能继续执行。不要自动重新提交。

轮询间隔不得短于 `poll_after_ms`（5000）。间隔内返回缓存状态，不会重新提交
视频。终态为 `succeeded`、`failed`、`cancelled` 或 `expired`。只有 MiniMax
仍认为任务正在排队时，远端取消才能成功。已经运行时取消返回错误，应用可以继续
轮询；服务不会声称已停止计费或生成。

结果是数据，不会作为 Splash 或 HTML 执行。应用可在自己的存储中保存返回的 JSON。
图片/音频 base64 接口本身不会增加播放器控件或文件下载权限；需要单独接入渲染器
或播放器。视频 URL 限于外观为公共主机名的 HTTPS 地址，不含凭证或 IP 字面量，
本服务不会下载它们。普通渲染器/网络权限和 URL 检查仍然有效。

## 代理工具别名

兼容的 App Hub 策略接受这七个审核过的 `host_method` 别名。声明
`implemented_by:"host-service"`、`model` 能力和 `private_data:true`。
生成、向量和取消至少需要 `risk:"act"`；能力查询和状态查询允许 `risk:"read"`。
应用自行选择其命名空间内的工具名称、schema、后台/共享策略和代理授权。别名不会
授予供应商访问、移除预算或启动代理。

```json
{
  "name": "example.embed",
  "description": "Embed this app's short text for local search.",
  "implemented_by": "host-service",
  "host_method": "model.embeddings",
  "risk": "act",
  "private_data": true,
  "input_schema": {"type":"object","properties":{"input":{"type":"string"}},"required":["input"]},
  "output_schema": {"type":"object"}
}
```

## 实现与验证

`complete/media/mod.rs` 负责异步调度、额度、身份范围和视频任务。
`complete/media/wire.rs` 校验应用请求、选择已配置路由并规范供应商响应。
`ai-host` 接受 shell 注入的准入/账户回调；shell 使用与应用工具执行相同的签名
安装包检查。现有 `model.complete` 现在也在每次尝试前和结果交付前检查请求存活、
准入和账户范围。

`tests/media.rs` 使用合成响应，通过真实宿主服务注册表和适配器调度，覆盖无效
输出、权益拒绝、额度持久化、账户/应用隔离、撤回、取消和并发上限。它不能替代
真实供应商权益检查或设备渲染验收。

协议来源：[OpenAI 图片](https://developers.openai.com/api/reference/resources/images/methods/generate)、
[OpenAI 语音](https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create)、
[OpenAI 向量](https://developers.openai.com/api/reference/resources/embeddings/methods/create)、
[MiniMax 图片适配器](https://github.com/MiniMax-AI/cli/blob/main/src/sdk/image/index.ts)、
[MiniMax 语音](https://platform.minimax.io/docs/api-reference/speech-t2a-http)、
[MiniMax H3 提交](https://platform.minimax.io/docs/api-reference/video-generation-v2-create)、
[状态查询](https://platform.minimax.io/docs/api-reference/video-generation-v2-query)和
[取消](https://platform.minimax.io/docs/api-reference/video-generation-v2-delete)。
