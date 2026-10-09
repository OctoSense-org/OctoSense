# 更新 OctoSense

[English](updater.md) | 简体中文

在桌面版或 Home 启动器中打开 **OctoSense Updates**。Android Home 也提供
**Settings → Updates → Open OctoSense app updater** 入口。先选择通道，再点
**Check for updates**。查看版本号与下载大小，点 **Download update**；校验完成
后，还需单独操作才能打开安装程序。

| 通道 | 可选版本 |
| --- | --- |
| Stable（稳定版） | 仅稳定版。 |
| Release candidate（候选版） | 稳定版和 `rc` 预发布；排除 beta、alpha、nightly。 |
| Preview（预览版） | 稳定版及所有符合语义版本格式的预发布版。 |

已安装 RC 时默认选择候选版通道。版本按数值比较：`rc.10` 高于 `rc.9`，相应
正式版高于其 RC。切换通道不会允许降级。

更新器随本次源代码变更引入。此前发布的桌面版 `0.1.0-rc.2` 和 Home
`0.1.0-beta.2` 不含此功能；需要先手动安装一次包含更新器的新版本。没有发布
标签的开发构建无法确定当前安装的发行版，因此不提供更新。

## 安装

| 平台 | 交接方式 | 用户还需完成的操作 |
| --- | --- | --- |
| macOS，Apple silicon | 打开已校验的 `.dmg` | 退出 OctoSense，将新应用拖入 Applications，确认替换，再打开。 |
| Windows，x64 | 直接启动已校验的 NSIS `.exe` | 按提示安装，在要求时关闭 OctoSense。 |
| Linux，x86_64，Debian 包 | 用桌面包处理程序打开 `.deb` | 在包管理器中确认安装并重启。没有图形包处理程序时使用发行版的包管理器。 |
| Linux，x86_64，AppImage | 打开下载目录 | 退出 OctoSense，替换旧 AppImage，在文件属性中允许执行，再打开。更新器不覆盖运行中的镜像。 |
| Android，ARM64 | 对照已安装 Home 检查 APK，通过 Android 包安装程序暂存 | 查看并确认操作系统的安装提示。 |

打开安装程序**不代表**安装完成。首个版本不会定期检查、静默退出、自行提权、
覆盖运行中的应用或删除用户配置。操作系统原有的安装与签名检查仍然生效。

需要 Android 13（API 33）或更新版本。Android 仅更新**独立版 Home**，System Bridge 与 ROM 继续使用各自的更新途径。
已安装包与候选包必须使用公开独立版 Home 的签名身份；系统内置 Home、ROM
签名、隔离测试包或其他签名均会被拒绝。候选包的 Android 版本代码必须更高，
最低 Android 版本必须兼容。若尚未允许 OctoSense 安装应用，用户可以打开系统
权限设置，返回后再次明确点击安装。更改权限后不会自动恢复安装，也不会先卸载
Home 来绕过签名不匹配。

## 校验与隐私

宿主从 GitHub 的 `OctoSense-org/OctoSense` 读取公开发布。桌面版选择
`desktop-v…` 标签，Home 选择 `home-v…`。语义版本与确切的平台文件名决定候选
更新；发布时间先后不决定版本高低。

发布必须同时提供 GitHub 的 SHA-256 文件摘要与 `SHA256SUMS`。更新器检查
校验和文件本身的摘要，确认其中的安装包哈希与 GitHub 元数据一致，再校验整个
下载文件及声明的大小。文件下载到私有暂存目录，交给操作系统前再次校验。无效
元数据、非预期 URL、符号链接、不完整或已修改文件均会被拒绝。此机制通过
HTTPS 信任项目的 GitHub 发布账户；校验和不是独立的发布者签名。

桌面版下载位于 shell 的 `updates/` 目录（设置了 `OCTOSENSE_HOME` 时以其为
根目录，否则是 `~/.octosense/updates`）。Android 使用应用私有缓存中的
`octosense-updates/`。检查发布不需要 OctoSense、GitHub 或 Google 登录。
不会发送邮件、应用内容、模型凭证或代理对话。更新器是宿主原生界面，不是
App Hub 应用或代理工具。

## 代码与发布身份

- [`crates/updater`](../crates/updater/src/lib.rs) 负责通道、版本选择、受限下载、
  取消和已校验文件。
- [`crates/updater-ui`](../crates/updater-ui/src/lib.rs) 负责原生模块与明确的用户
  操作。网络和哈希任务不占用绘制线程。
- [`install.rs`](../crates/updater-ui/src/install.rs) 用原样参数交给桌面安装程序，
  不把发布内容插入 shell 命令。
- Android 原生扩展独立检查包名、版本、签名和文件字节，再提交需要操作系统
  用户确认的安装会话。
- [`native-apps.json`](../native-apps.json) 为桌面版和 Home 注册模块，不提供代理
  更新工具。

Cargo 通用的 `0.1.0` 包版本不是实际发行版身份，因此打包时嵌入
`OCTOSENSE_RELEASE_TAG`。[桌面打包程序](../desktop/scripts/package.py) 从显式
的 `--version-from-tag` 或 `--version` 设置标签。带标签的 `--skip-build` 必须
匹配先前构建的标签和二进制 SHA-256 收据，防止旧程序被标成新版本。普通构建会
清除继承标签。桌面构建和打包程序显式选择本机 Rust 目标，二进制与身份收据来自
`target/<host-triple>/release/`。继承的 `CARGO_TARGET_DIR`、
`CARGO_BUILD_TARGET` 或 Cargo 配置不能把构建重定向后，让打包继续读取旧的
`target/release` 程序。[Home 构建程序](../rom/scripts/build-home.py) 只对指定版本、正式
签名的独立构建设置标签；开发与 ROM 构建留空。

## 借鉴 OctosCode

原生 OctosCode 应用在 `d47fade94498` 的文档中要求手动下载 RC，并不包含 CLI
更新器。独立的 [`octoscode update` CLI](https://github.com/octos-org/octoscode/blob/8722b551a7ee653c2457f1152a636160351b9b1f/src/cmd/update.rs)
使用 `axoupdater` 更新带 cargo-dist 安装收据的程序；由 Homebrew、npm 或 Cargo
管理的安装则交还相应管理器。它的预发布选项解析确切通道标签，并测试 RC 序号的
数值顺序。OctoSense 采用这些安装归属与明确通道的原则，但把完整应用包交给原生
安装程序。与该 CLI 显式切换预发布通道不同，本界面不提供正式版降级到 RC。

[octos 内核更新器](https://github.com/octos-org/octos/blob/084baa522a508cf3af7d7ce9585868ac710eb1d9/crates/octos-services/src/updater.rs)
可备份、替换指定 CLI 二进制并回滚，但不是完整 Makepad 应用包或 Android
安装器。OctoSense 对旧发布也强制要求校验和，不采用内核更新器允许旧版缺失
校验和的兼容策略。

## 验证范围

桌面交接测试检查程序名和原样参数，不启动安装程序。打包测试检查发布身份、拒绝
旧二进制和构建计划，不生成发布包。

隐藏窗口中的原生 macOS 界面检查了真实的 RC1 → RC2 候选更新，下载并校验了
297 MB 磁盘镜像。故意修改隔离下载文件后，点击 **Install** 在任何安装程序打开
之前即拒绝该文件。还验证了切换通道和取消下载；实际桌面 shell 也在隔离配置中
打开了 Updates 模块并检查了真实 RC2 候选更新。
替换已安装的 macOS 应用仍属**未经验证**，Windows 与 Linux
真实安装也尚未验证。

在自有 API 35 Android 模拟器中，使用实际 Java 更新适配器和操作系统
PackageInstaller 的测试应用通过了含清理步骤在内的 25 项验收：更改安装权限后不自动重放安装；
拒绝错误签名、包名、哈希、版本代码和最低 SDK；可取消并重试可见的系统确认；
版本代码 1000 → 1001 更新保留了 UID 与模拟应用数据，替换后可恢复状态。
此验收覆盖原生适配器，**不是完整的 Rust Home 二进制**。没有改动实体手机；
从已发布 Home 构建发起的完整安装流程仍属**未经验证**。
