# octos-app workspace

[English](README.md) | 简体中文

AppCard 助手的 Cargo workspace：面向 [octos](https://github.com/octos-org/octos)
Agent 内核的原生 Makepad 与 Splash 客户端。OctoSense 的 Shell 把它作为控件挂载；
它也可以构建为独立应用。它在本仓库中的位置以及所需的同级运行时，见
[../README.zh-CN.md](../README.zh-CN.md)。

## Crate

| 路径 | Crate | 作用 |
| --- | --- | --- |
| `app/` | `octos-app` | 应用本体：路由大脑（router + composer）、多 Agent 调度、Splash 卡片渲染器与生成后校验器、L0 卡片生成、WebView 浮层；`src/host.rs` 是宿主 API（`register_script_mods`、`AppShell`、`OctosAppBody`） |
| `crates/octos-app-store/` | `octos-app-store` | `AppState` reducer 与 selector，不依赖 Makepad |
| `crates/octos-app-transport/` | `octos-app-transport` | octos UI Protocol v1 的 WebSocket 与 REST 传输层 |
| `crates/octos-app-render/` | `octos-app-render` | 流式 markdown 渲染封装 |

其他目录：`splash-native/`（研究：把 Splash 卡片渲染为 Android 原生视图）、
`scripts/smoke-live.sh`。

`octos-app` 只有一个 feature：`standalone`（默认），包含 `fn main`、Android 入口
和开发监视器。挂载 `AppShell` 的宿主以 `default-features = false` 构建。

## 构建、测试、运行

先准备同级运行时（在 `apps/appcard` 中：`python3 tools/setup-native.py`）。
然后在本目录中：

```sh
cargo check
cargo test --workspace
cargo clippy -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
cargo run -p octos-app
```

`Makefile` 封装了相同的命令（`make check`、`test`、`run`、`clippy`、`fmt`），
另有 `make smoke-live`：针对 `OCTOS_LIVE_URL`（默认 `http://127.0.0.1:56831`）
运行默认忽略的实时传输测试。存在本地 `.env` 时会读取它。

独立运行的应用按以下顺序连接 octos：

1. Shell 的 octos 内核（`octosense-octos-core`，仓库中的 `crates/octos-core`），
   只要能运行：Android 上是 APK 内置的 `liboctos.so`，OpenHarmony 上总是可用
   （链接进应用），桌面上需由 Shell 或 `OCTOS_APP_CORE_BIN` 指定内核二进制（数据
   目录为 `OCTOS_APP_CORE_DIR`，否则 `~/octos-home/.octos`；
   `../tools/octos-macos.py` 会设置两者，见 `../tools/OCTOS-MACOS.md`）。内核与
   Shell 的其他使用方共享，AI 服务商变化时重启；传输层随后重连并重新打开会话；
2. `~/.config/octos-app/server.json`（服务器 URL 和 profile），bearer 来自
   `OCTOS_APP_TOKEN` 或系统钥匙串；
3. 否则使用 `OCTOS_BASE_URL`、`OCTOS_BEARER` 和 `OCTOS_PROFILE_ID`
   （默认 `https://localhost:8080`）。

绝不提交 token 或 `.env` 文件。

## CI

CI 是仓库根目录的
[.github/workflows/appcard.yml](../../../.github/workflows/appcard.yml)。本
workspace 内的 `.github/workflows/` 是代码从早先仓库带过来的，GitHub 在这里不会运行它。
