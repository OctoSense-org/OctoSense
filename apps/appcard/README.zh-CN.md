# AppCard

[English](README.md) | 简体中文

**AppCard 助手运行时**，即“Ask anything”磁贴：`octos-app` crate workspace 以及
编译进它的卡片提示语料。OctoSense 的 Shell 把它放在一个磁贴中：你输入一个请求，
路由大脑（AMA）选择或组合一个应用 Agent，由该 Agent 生成一张实时的交互卡片。
卡片是 Splash DSL 卡片或 webview 卡片，在渲染时绑定真实数据。

与 `apps/` 中的其他应用不同，AppCard（目前）还不是带 `bundle/` 的隔离运行脚本应用。
它是这里唯一的**原生**应用：一个由 Shell 进程内链接的 Rust 模块（`octos-app`）。

目录内容（路径相对于 `apps/appcard/`）：

```
app/              AppCard 的 crate（仓库根 workspace 的成员）。
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
module/           octosense-appcard：把 octos-app 挂载到磁贴中的 Shell 模块。
tools/            setup-native.py（运行时检查）、octos macOS/OHOS 启动器、
                  build-android.sh、dev-goal bridge、llm-qr、splash-research。
docs/             架构、协议、构建与评审笔记。
```

## Octos

所有 octos crate（`octos-core`，以及 OpenHarmony 上的 `octos-cli` 和它引入的约
20 个 crate）都来自**同一个**来源：git `https://github.com/octos-org/octos.git`，
版本为仓库根目录 `Cargo.toml` 的 `[workspace.dependencies]` 中唯一的 rev（目前是
octos main 上的 `665209e5`），与 `crates/kernel` 和两个 Shell 共用。octos 中适配
OpenHarmony 的 `nix` 在根目录的 `[patch.crates-io]` 中 patch（来自 octos `18fcd3f1`）。
没有 octos submodule；可用
`cargo tree --locked -p octos-app -i octos-core --target all --depth 0` 检查依赖图中只有一份 octos。

构建内核*二进制*的启动器（`tools/build-android.sh`、`tools/octos-ohos.py`、
`tools/octos-macos.py`）使用 octos 检出，默认是框架 workspace 中的 `octos/`
（可用 `OCTOS_SOURCE` 指定其他位置）；`build-android.sh` 会拒绝其他 rev 的检出。
它们从 `app/Cargo.toml` 读取该 rev，而迁入根 workspace 时这个文件已被移除：
**未验证**，这些启动器很可能需要设置 `OCTOS_SOURCE` 并修复后才能再次使用。Shell 的
Android APK 则通过 `rom/scripts/build-home.sh` 打包内核。

## 框架源码

应用本身不带 Makepad。它的 crate 以 `workspace = true` 引用 Makepad、Octoscript 和
Octoscript-Makepad，根目录 `Cargo.toml` 把它们 patch 到仓库根目录 `.sources/` 中的
检出，版本由 `native-runtime.lock.json` 选定（本目录的锁文件副本指向同一版本）。
在仓库根目录准备并校验：

```sh
python3 tools/setup.py                 # 准备 .sources/（makepad、octoscript、octoscript-makepad）
python3 tools/setup.py --check --cargo # 校验，包括依赖图中只有一份 Makepad
```

`app/app/build.rs` 从 `OCTOSENSE_WORKSPACE` 嵌入框架资源，根目录的
`.cargo/config.toml` 把它设为 `.sources`。本目录自己的 Python 工具
（`tools/setup-native.py`、`build-android.sh`、octos 启动器）也读取这个变量，但在
cargo 之外默认仍是仓库根目录的父目录；运行它们时请设置
`OCTOSENSE_WORKSPACE=<repo>/.sources`。原先的同级目录布局见
[docs/NATIVE-WORKSPACE.md](docs/NATIVE-WORKSPACE.md)。

## 构建与测试

在仓库根目录：

```sh
cargo clippy --locked -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
cargo test --locked -p octos-app-transport -p octos-app-store
cargo run -p octos-app                                          # 独立窗口
(cd apps/appcard && PYTHONPATH=tools python3 -m unittest core.test_native_runtime)
```

CI 是 [.github/workflows/apps.yml](../../.github/workflows/apps.yml) 的 `apps` 任务
（`apps/`、`crates/` 和 workspace 文件有改动时运行）：准备 `.sources/`，运行运行时锁
测试、四个 crate 的 clippy 以及 transport 与 store 测试，并检查 Cargo 依赖图中只有
一份 Makepad、octos、App Hub 和 Rinx。Android 见
[docs/BUILDING-ANDROID.md](docs/BUILDING-ANDROID.md) 和 `tools/build-android.sh`；
OpenHarmony 见 [docs/BUILDING-OPENHARMONY.md](docs/BUILDING-OPENHARMONY.md)。

## 使用方

Shell 使用不含独立入口的 `octos-app`，把它作为控件挂载：

```toml
# 本仓库内：workspace 依赖（apps/appcard/module 的做法）
octos-app = { workspace = true, default-features = false }
# 本仓库外：git 依赖（Cargo 会按包名在仓库中找到它）
octos-app = { git = "https://github.com/OctoSense-org/OctoSense.git", rev = "<sha>", default-features = false }
```

仓库外的使用方还必须把 Makepad、Octoscript 和 Octoscript-Makepad patch 到锁定版本，并在
Cargo 配置中设置 `OCTOSENSE_WORKSPACE`（构建时会从该 workspace 嵌入框架资源；
从 git 检出时默认值并不存在）。依赖的 `[patch]` 不会作用于使用方，所以为
OpenHarmony 构建的使用方也要像根目录 `Cargo.toml` 那样 patch `nix`。

宿主 API 位于 `app/app/src/host.rs`：调用 `octos_app::register_script_mods(vm)`，
然后挂载 `AppShell::create(vm)`，这是一个持有应用、绘制 `OctosAppBody`（去掉独立
`Window` 的应用根视图）的控件。`AppShell::ask` 像在输入框中输入并发送一样提交
文本；宿主释放 isolate 之前调用 `AppShell::shutdown`。

- **两个 OctoSense Shell**（桌面端和 Home）都通过 [`module/`](module)
  （`octosense-appcard`）挂载它：一个围绕 `AppShell`、实现 Shell 的 `AppModule`
  trait 的 `AppCardModule`，需用 `--features app-appcard` 显式启用。它们从同一个
  提交构建，所以这里的改动在同一个 PR 中就会到达它们。
- **Rinx** 嵌入了 AppCard 磁贴；它的版本迁移是单独的后续工作。

## 来源

2026-09-27 之前属于 OctoSense-System-Apps 仓库，之后连同历史导入 OctoSense。它从 OctoSense-org/OctoSense-AppCard
的 `d0a836b8` 迁移而来，该仓库又是从
[OctoSense-org/OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
（`app/`，提交 `cbbda4da`）拆分出来的。这些文件的完整历史保留在那里。
