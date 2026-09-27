# OctoSense

[English](README.md) | 简体中文

[OctoSense](https://github.com/OctoSense-org) 是运行在操作系统之上的 Agent 交互 Shell。本仓库集中存放它的全部内容（[ADR 0001](docs/adr/0001-one-octosense-repository.md)）：桌面端和手机端的 Shell、Shell 服务、系统应用以及 ROM 镜像。本仓库原名 OctoSense-Desktop；OctoSense-ROM 和 OctoSense-System-Apps 已连同历史一起导入。

**要开发 OctoSense 应用？** 不需要本仓库：请从 [OctoSense-org 主页](https://github.com/OctoSense-org)的阅读列表开始（先读 OctoScript-App-Design-Flow 的 `AGENTS.md`，再读 `docs/QUICKSTART.md`）。

## 目录结构

| 路径 | 内容 |
| --- | --- |
| `desktop/` | 桌面端打包，包名 `octosense`：桌面 Shell（在共享 Shell crate 落地前仍为 `src/`）、应用目录、主题、上游窗口管理器同步。[README](desktop/README.zh-CN.md) |
| `phone/` | Home 应用，包名 `octosense-home`（APK id `dev.makepad.octosense`）：Android、OpenHarmony 和 iOS 打包，手机 Shell（`src/`），设置应用，系统桥的手机端。[README](phone/README.zh-CN.md) |
| `rom/` | 仅 OnePlus 6 ROM 镜像：`vendor/`、`patches/`、镜像/刷机/OTA 脚本、`web-installer/`、产品测试。[README](rom/README.zh-CN.md) |
| `crates/kernel/` | octos 内核服务（`octosense-octos-core`）。 |
| `crates/app-peers/` | 应用与 Agent 之间的代理（`octosense-app-peers`，Rinx ADR 0007）。 |
| `apps/` | 系统应用（新闻、照片、地图、相机、邮件、AI 服务商）的脚本包、它们的宿主服务、原生模块版本（`apps/*/native`）、`apps/reference`，以及可选的 AppCard 助手（`apps/appcard`，模块在 `apps/appcard/module`）。[README](apps/README.zh-CN.md) |
| `tools/` | `setup.py`（框架源码准备）、经审查的 Makepad 运行时补丁（`runtime-patches/`）。 |
| `docs/adr/` | 仓库的架构决策记录。 |
| `Cargo.toml`、`Cargo.lock` | 单一 workspace。所有外部依赖（Makepad、OctoScript、App Hub、octos、Rinx）只在 `[workspace.dependencies]` 中固定一次。 |
| `native-runtime.lock.json`、`runtime-patches.lock.json` | OctoScript-Makepad 发布版本（并由它固定 Makepad 和 OctoScript），以及 Makepad 之上的已审查补丁。 |

## 环境准备

需要稳定版 Rust（`cargo` 位于 `~/.cargo/bin`）、Git 和 Python 3.9+。Makepad 和 OctoScript 解析到 `.sources/`（已被 git 忽略）中的检出，由准备脚本按固定版本创建：

```sh
git clone https://github.com/OctoSense-org/OctoSense.git
cd OctoSense
python3 tools/setup.py                  # 准备 .sources/（makepad、octoscript、octoscript-makepad）
python3 tools/setup.py --check --cargo  # 校验：依赖图中只有一个 Makepad、App Hub、octos 和 Rinx
```

锁文件变化后用 `--update` 移动干净的检出；`--cache DIR` 复用本地 Git 对象缓存。`.sources/` 中的本地修改会被保留。

## 构建

桌面端（在根目录或 `desktop/` 下运行）：

```sh
cargo run --release -p octosense
cargo check --locked -p octosense --features mobile-apps           # 内嵌应用集合
cargo check --locked -p octosense -p octosense-appcard --features mobile-apps,app-appcard
```

手机端（在 `phone/` 下运行，以选用手机端的系统应用）：

```sh
cd phone
cargo check --locked -p octosense-home --features mobile-apps
python3 ../rom/scripts/build-home.py --help   # Home/Bridge APK 对，内含 liboctos.so
```

CI 按路径过滤：`Desktop`（desktop、crates、apps）、`Phone`（phone、crates、apps）、`Apps and services`（apps、crates）和 `ROM`（rom、phone 的 Android 源码）。

## 许可证

Apache-2.0（见 [LICENSE](LICENSE) 和 [NOTICE](NOTICE)）；第三方声明见 `LICENSES/`。
