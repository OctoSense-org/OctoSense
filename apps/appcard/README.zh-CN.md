# AppCard

[English](README.md) | 简体中文

**AppCard 助手运行时**，即“Ask anything”磁贴：`octos-app` crate workspace 以及
编译进它的卡片提示语料。OctoSense 的 Shell 把它放在一个磁贴中：你输入一个请求，
路由大脑（AMA）选择或组合一个应用 Agent，由该 Agent 生成一张实时的交互卡片。
卡片是 Splash DSL 卡片或 webview 卡片，在渲染时绑定真实数据。

与本仓库的其他应用不同，AppCard（目前）还不是带 `bundle/` 的隔离运行脚本应用。
它是这里唯一的**原生**应用：一个由 Shell 进程内链接的 Rust 模块（`octos-app`）。

目录内容（路径相对于 `apps/appcard/`）：

```
app/              Cargo workspace 根目录。
  app/            octos-app：路由大脑（router + composer）、多 Agent 调度、
                  Splash 卡片渲染器与生成后校验器、L0 卡片生成、
                  webview 卡片的 WebView 浮层。
  crates/
    octos-app-store/      AppState reducer 与 selector（不依赖 Makepad）。
    octos-app-transport/  使用 octos UI Protocol v1 的 WebSocket + REST 传输层。
    octos-app-render/     流式 markdown 渲染封装。
a2app/            Splash 卡片的应用记忆：只含需求的规格、控件模式、
                  实时数据辅助文档和各应用的 lint 规则。
                  通过 include_str! 编译进 octos-app。
a2app-l0/         L0 卡片语料：框架、目录以及用于提示 L0 卡片生成的
                  各应用示例卡片。同样编译进应用。
personal-data/    octos 技能：对邮件/日历数据的只读搜索。
vendor/           内置的第三方 crate（见仓库的 NOTICE）。
tools/            setup-native.py（同级运行时）、octos macOS/OHOS 启动器、
                  build-android.sh、dev-goal bridge、llm-qr、splash-research。
docs/             架构、协议、构建与评审笔记。
```

## Octos

所有 octos crate（`octos-core`，以及 OpenHarmony 上的 `octos-cli` 和它引入的约
20 个 crate）都来自**同一个**来源：git `https://github.com/octos-org/octos.git`，
版本为 `app/Cargo.toml` 中唯一的 rev（目前是 `18fcd3f1`，在合入 octos main 之前
位于 `appcard/mate70-on-main` 分支）。octos 中适配 OpenHarmony 的 `nix` 也从同一
rev patch 进来。没有 octos submodule。同样依赖 octos 的 Shell 必须使用同一 rev，
使其依赖图中只有一份 octos；可用 `cargo tree -i octos-core --target all` 检查。

构建内核*二进制*的启动器（`tools/build-android.sh`、`tools/octos-ohos.py`、
`tools/octos-macos.py`）使用该 rev 的 octos 检出，默认是本仓库旁边的 `octos/`
（可用 `OCTOS_SOURCE` 指定其他位置）；`build-android.sh` 会拒绝其他 rev 的检出。

## 同级 workspace

应用本身不带 Makepad。`app/Cargo.toml` 把 Makepad、Octoscript 和
Octoscript-Makepad patch 到本仓库旁边的检出（从 `app/` 看是
`../../../../makepad`、`../../../../octoscript`、`../../../../octoscript-makepad`）。
它们必须处于 `native-runtime.lock.json` 选定的版本。目录布局如下：

```
octosense-org/
  OctoSense-System-Apps/   本仓库；AppCard 位于 apps/appcard/
  octoscript-makepad/      共享 UI 框架；其 runtime.json 固定各引擎版本
  octoscript/
  makepad/
```

在 `apps/appcard/` 中准备并校验：

```sh
python3 tools/setup-native.py          # 按锁定版本克隆/准备同级仓库
python3 tools/setup-native.py --check --cargo-manifest app/Cargo.toml
```

`--update` 会把干净的同级检出移动到新版本，有未提交改动的目录保持不动。
`OCTOSENSE_WORKSPACE` 可指定本仓库父目录以外的 workspace。详见
[docs/NATIVE-WORKSPACE.md](docs/NATIVE-WORKSPACE.md)。

## 构建与测试

```sh
cd app
cargo check
cargo test --workspace
cargo clippy -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
PYTHONPATH=tools python3 -m unittest core.test_native_runtime   # 在 apps/appcard 中运行
```

CI（[.github/workflows/appcard.yml](../../.github/workflows/appcard.yml)，只在
`apps/appcard/` 有改动时运行）会准备锁定的运行时，运行运行时锁测试和 clippy，
并检查 Cargo 依赖图中只有一份 Makepad。Android 见
[docs/BUILDING-ANDROID.md](docs/BUILDING-ANDROID.md) 和 `tools/build-android.sh`；
OpenHarmony 见 [docs/BUILDING-OPENHARMONY.md](docs/BUILDING-OPENHARMONY.md)。

## 使用方

Shell 使用不含独立入口的 `octos-app`，把它作为控件挂载：

```toml
# Shell 的做法：对其固定检出的本仓库使用 path 依赖
octos-app = { path = "<checkout>/apps/appcard/app/app", default-features = false }
# 或者使用 git 依赖（Cargo 会按包名在仓库中找到它）
octos-app = { git = "https://github.com/OctoSense-org/OctoSense-System-Apps.git", rev = "<sha>", default-features = false }
```

它还必须把 Makepad、Octoscript 和 Octoscript-Makepad patch 到锁定版本，并在
Cargo 配置中设置 `OCTOSENSE_WORKSPACE`（构建时会从该 workspace 嵌入框架资源；
从 git 检出时，默认值即本仓库的父目录并不存在）。依赖的 `[patch]` 不会作用于
使用方，所以为 OpenHarmony 构建的 Shell 也要像 `app/Cargo.toml` 那样，从同一
octos rev patch `nix`。

宿主 API 位于 `app/app/src/host.rs`：调用 `octos_app::register_script_mods(vm)`，
然后挂载 `AppShell::create(vm)`，这是一个持有应用、绘制 `OctosAppBody`（去掉独立
`Window` 的应用根视图）的控件。`AppShell::ask` 像在输入框中输入并发送一样提交
文本；宿主释放 isolate 之前调用 `AppShell::shutdown`。

- **OctoSense ROM** 的 `home/apps/appcard` 和 **OctoSense-Desktop** 的
  `apps/appcard`：一个围绕 `AppShell`、实现 Shell 的 `AppModule` trait 的
  `AppCardModule`。两者都在
  [OctoSense-ROM#18](https://github.com/OctoSense-org/OctoSense-ROM/pull/18) 和
  [OctoSense-Desktop#36](https://github.com/OctoSense-org/OctoSense-Desktop/pull/36)
  （均未合并）中改为使用本仓库（`4d99cb58`，octos `18fcd3f1`）。它们的 `main`
  分支仍固定在 OctoSense-org/Octoscript-AppCard（现为 OctoScript-App-Design-Flow）
  的 `9e8e4898`，因此在这两个 PR 合并之前，这里的改动不会到达它们。
- **Rinx** 嵌入了 AppCard 磁贴；它的版本迁移是单独的后续工作。

## 来源

从 OctoSense-org/OctoSense-AppCard
的 `d0a836b8` 迁移而来，该仓库又是从
[OctoSense-org/OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
（`app/`，提交 `cbbda4da`）拆分出来的。这些文件的完整历史保留在那里。
