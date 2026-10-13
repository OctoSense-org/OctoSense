# 更新 OctoSense

[English](updater.md) | 简体中文

在桌面版或 Home 启动器中打开 **OctoSense Updates**。Android Home 也提供
**Settings → Updates → Open OctoSense app updater** 入口。先选择通道，再点
**Check for updates**。查看版本号与下载大小，点 **Download update**；校验完成
后，再明确选择安装操作。

| 通道 | 可选版本 |
| --- | --- |
| Stable（稳定版） | 仅稳定版。 |
| Release candidate（候选版） | 稳定版和 `rc` 预发布；排除 beta、alpha、nightly。 |
| Preview（预览版） | 稳定版及所有符合语义版本格式的预发布版。 |

已安装 RC 时默认选择候选版通道。版本按数值比较：`rc.10` 高于 `rc.9`，相应
正式版高于其 RC。切换通道不会允许降级。

桌面 [RC6](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.6)
是首个包含更新器的桌面发布版。从 RC5 或更早版本升级时，请先手动安装一次 RC6，
之后通过 **OctoSense Updates** 获取符合所选通道的后续版本。Home `0.1.0-beta.2`
不含更新器，本次桌面发布也不更新 Home。没有发布标签的开发构建无法确定当前
安装的发行版，因此不提供更新。

## 安装

| 平台 | 交接方式 | 用户还需完成的操作 |
| --- | --- | --- |
| macOS，Apple silicon | 本源码构建的已签名应用包带辅助程序时提供 **Install & Restart**；RC6 仍打开已校验的 `.dmg` | 保存工作或处理应用关闭提示。辅助程序替换应用包并以相同配置重新启动；保留 **Open installer manually** 手动入口。 |
| Windows，x64 | 直接启动已校验的 NSIS `.exe` | 按提示安装，在要求时关闭 OctoSense。 |
| Linux，x86_64，Debian 包 | 把已校验的 `.deb` 复制到“下载”文件夹并再次校验，再用桌面包处理程序打开 | 在包管理器中确认安装并重启。没有图形包处理程序时，用发行版的包管理器安装该文件。 |
| Linux，x86_64，AppImage | 把校验过的可执行副本放进“下载”文件夹，并打开该文件夹 | 退出 OctoSense，用新文件替换旧 AppImage，再打开。更新器不覆盖运行中的镜像。 |
| Android，ARM64 | 对照已安装 Home 检查 APK，通过 Android 包安装程序暂存 | 查看并确认操作系统的安装提示。 |

打开安装程序**不代表**安装完成。检查仍由用户发起，不自行提权或删除用户配置。
macOS 只有在点击 **Install & Restart**、处理应用关闭提示、旧进程退出，并通过对照已安装应用的代码签名检查后才替换应用。操作系统的安装程序仍会执行自身检查，但
macOS 和 Windows 不会像对浏览器下载的文件那样显示警告（Gatekeeper、
SmartScreen）：更新器不把下载标记为来自互联网，而是依靠发布上的 GitHub 签名
（见[校验与隐私](#校验与隐私)）。

### macOS 安装并重启（源码构建，RC6 尚不包含）

第一个包含辅助程序的版本仍需通过 RC6 的手动安装流程安装一次。之后的更新
可以替换包括资源、内核在内的整个应用包，无需在 Finder 中拖动。重命名的应用
保留原路径；安装目录不可写时使用手动安装，不请求提升权限。

只有已安装应用的同一开发者签名的代码才能替换它。暂存任何内容之前，以及之后
在辅助程序中，Apple 的 Security 框架都会严格校验已安装的应用，并读取其指定
要求（designated requirement），该要求必须包含 Team ID。新应用必须通过同样
严格的校验（涵盖所有架构和全部嵌套代码，与 `codesign --verify --deep --strict`
的检查一致），并满足该要求：由 Apple 签发的证书链签名，Team ID 和签名标识
相同。辅助程序在交换前检查暂存的应用，在启动它之前立即再查一次，重新打开
旧应用前也会检查。未签名或临时（ad-hoc）签名的安装，即所有源码构建和开发
构建，会被拒绝并给出说明；仍可使用 **Open installer manually**。只有调试构建
（发布构建绝不）在设置 `OCTOSENSE_UPDATE_UNSIGNED_FIXTURES=1` 时接受未签名的
测试应用包。

准备工作在后台进行：重新校验带签名的 DMG、只读挂载、检查应用标识、版本和
代码签名、复制到应用旁的私有目录。指纹覆盖文件内容、名称、全部权限位（包括
setuid、setgid 和 sticky）以及不越出应用包的符号链接；含 setuid 或 setgid
文件的应用包会被拒绝。完成后，宿主通过已有保存/关闭协议请求应用退出。应用
拒绝时等待用户；未回答关闭请求则取消更新，不强制终止应用。在关闭前取消或
关闭更新窗口会取消等待中的辅助程序。**Install & Restart** 只响应真实点击：
依据 Makepad 的输入来源判断，脚本、远程控制或合成点击都无法产生，与 Android
的安装步骤相同。

辅助程序启动前被复制到应用包外。它必须收到私有管道的确认，并等宿主退出；
随后给内核 sidecar 等应用包内的进程最多 20 秒结束，若仍有进程使用该应用包则
拒绝替换。一次原子文件系统交换安装新包并保留旧包。新版本使用相同配置启动，
并在首次绘制后确认。**不会回滚用户数据**；未来数据迁移的向后兼容性仍是发布
时需要考虑的问题。

应用关闭之后，无论哪一步失败，用户都会重新得到一个应用。交换前失败（更新
已变化或并非同一开发者签名、应用包仍在使用）时，重新打开未改变的已安装应用；
启动失败，或新应用在启动前未通过检查时，只要旧包的指纹仍匹配就恢复旧包，然后
重新打开它。两种情况都会显示回滚通知。交换后未通过检查的代码会被移回暂存目录，
绝不会留在安装位置。只有已安装应用本身签名无效，或因保留的旧应用已变化、新应用
仍在运行而无法恢复时，才不重新打开任何应用。下次启动时会显示一次失败、取消或
回滚的结果。

公开 RC6 等旧目标版本不支持启动确认协议。仅为兼容这类测试，以连续存活
15 秒作为检查，收据会注明；这不证明界面正常。两种方式都不代表更新后每个
应用或模型提供方已经通过功能测试。

旧包保留在应用旁的 `.octosense-update-<id>/candidate.app`。
`updates/last-install.json` 记录结果；私有 `install-<id>.log` 记录辅助程序错误。
安装锁直接加在安装目录本身上，因此不会在应用旁创建其他文件。辅助程序意外
中断时，安装路径仍有完整应用包，两个版本均保留。关闭全部实例后，可以用保留
的辅助程序执行 `--restore <job.json>`，即使中断发生在交换前后也可恢复原包；
它拒绝已变化的应用包和不具备已安装应用签名身份的代码，并允许重复执行；若已
安装应用自身的签名已损坏，它会拒绝执行，此时可在 Finder 中把保留的应用移回
原处。每次启动以及每次安装成功后，只保留最新的一个可能存有旧应用的暂存目录
（即 `--restore` 需要的那个）：更早的会被删除，从未替换应用的尝试所留下的
目录满十分钟后删除。仍在准备中的目录、属于其他安装的目录，以及仍挂载着磁盘
映像的目录不会被改动。

Linux 包处理程序可能读不到下载所在的隐藏配置目录（Ubuntu 的 App Center 是
snap 应用，看不到隐藏文件夹），因此 Linux 包从“下载”文件夹交接
（`XDG_DOWNLOAD_DIR`，否则为 `~/Downloads`）。那里的同名文件不会被替换：副本
使用下一个可用名称，例如 `octosense_0.2.0_amd64 (2).deb`。“下载”中的副本由您
自行删除。

需要 Android 13（API 33）或更新版本。Android 仅更新**独立版 Home**，System Bridge 与 ROM 继续使用各自的更新途径。
已安装包与候选包必须使用公开独立版 Home 的签名身份；系统内置 Home、ROM
签名、隔离测试包或其他签名均会被拒绝。候选包的 Android 版本代码必须更高，
最低 Android 版本必须兼容。若尚未允许 OctoSense 安装应用，用户可以打开系统
权限设置，返回后再次明确点击安装。更改权限后不会自动恢复安装，也不会先卸载
Home 来绕过签名不匹配。**Review and install update** 和 **Allow updates in
Android Settings** 只响应真实的屏幕点按：这由 Makepad 的输入来源判断，脚本、
远程控制或模拟点击都无法伪造。

## 校验与隐私

宿主从 GitHub 的 `OctoSense-org/OctoSense` 读取公开发布。桌面版选择
`desktop-v…` 标签，Home 选择 `home-v…`。语义版本与确切的平台文件名决定候选
更新；发布时间先后不决定版本高低。

发布必须同时提供 GitHub 的 SHA-256 文件摘要与 `SHA256SUMS`。更新器检查
校验和文件本身的摘要，确认其中的安装包哈希与 GitHub 元数据一致，再校验整个
下载文件及声明的大小。文件下载到私有暂存目录，交给操作系统前再次校验。无效
元数据、非预期 URL、符号链接、不完整或已修改文件均会被拒绝。

摘要和校验和只能证明文件属于该发布：任何能发布 Release 的人都能让它们彼此
一致。因此，桌面版发布只有在其 `SHA256SUMS` 还带有 **GitHub 签名**时才会被
提供。该签名由 [`sign-desktop-release`](../.github/workflows/sign-desktop-release.yml)
工作流生成，它在 `main` 上、在 `desktop-release-signing` 环境中运行，每次运行都需
该环境的审核人批准。签名是一个 Sigstore 包，作为 `SHA256SUMS.sigstore.json`
附在发布中。更新器离线校验它，方式与设备校验 App Hub 目录相同：先用 Sigstore
公共信任根校验证书、时间戳和透明日志证明；再校验 GitHub 对这次运行的声明（本
仓库及其所有者、`main` 上的该工作流、手动触发、该环境、GitHub 托管的运行器）；
最后校验签名的对象是 `<tag>.SHA256SUMS`，且摘要与实际下载的文件完全一致。没有
该签名的桌面版发布会报错，而不是被跳过。Android Home 不需要它：Android 只安装
用固定的独立版发布密钥签名的 APK。

发布列表按从新到旧读取，最多 10 页、每页 100 个发布；每页最多 16 MB（2026 年
10 月，一个发布的说明和文件列表约占 28 KB）。

桌面版下载位于 shell 的 `updates/` 目录（设置了 `OCTOSENSE_HOME` 时以其为
根目录，否则是 `~/.octosense/updates`）。Android 使用应用私有缓存中的
`octosense-updates/`。已校验的下载在最后写入一天后删除，不论是否打开过安装
程序；中断的下载在一小时后删除。删除发生在 OctoSense 启动时和每次检查前，
只删除更新器自己的、只含普通文件的 `verified-…` 和 `pending-…` 文件夹，且不
递归删除。检查发布不需要 OctoSense、GitHub 或 Google 登录。
不会发送邮件、应用内容、模型凭证或代理对话。更新器是宿主原生界面，不是
App Hub 应用或代理工具。

## 代码与发布身份

- [`crates/updater`](../crates/updater/src/lib.rs) 负责通道、版本选择、受限下载、
  取消、已校验文件和清理过期下载。其中的 [`signature.rs`](../crates/updater/src/signature.rs)
  校验桌面版发布 `SHA256SUMS` 上的 GitHub 签名，签名由
  [`sign-desktop-release.yml`](../.github/workflows/sign-desktop-release.yml) 为每个
  发布生成（见 [desktop/README.zh-CN.md](../desktop/README.zh-CN.md)）。
- [`crates/updater-ui`](../crates/updater-ui/src/lib.rs) 负责原生模块与明确的用户
  操作。网络和哈希任务不占用绘制线程。
- [`macos`](../crates/updater/src/macos/mod.rs) 负责暂存、独立辅助程序、原子交换
  与恢复；shell 的 [`update_restart.rs`](../crates/shell/src/update_restart.rs)
  将安装连接到已有的关闭确认流程。
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

桌面交接测试检查程序名和原样参数，不启动安装程序。打包测试覆盖发布身份及
拒绝旧二进制；签名测试覆盖证书、透明日志和 GitHub 声明，包括篡改证明、其他
工作流、分支及签名对象。

[RC6](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.6)
由[正式发布工作流](https://github.com/OctoSense-org/OctoSense/actions/runs/38125407028)
从 `f9f6fea9` 构建，再经正常审核人批准，由[受保护的校验和工作流](https://github.com/OctoSense-org/OctoSense/actions/runs/38137543497)
签名。已下载全部 10 个最终文件并核对摘要。RC6 原版验证器接受了真实签名，
并拒绝了被修改的校验和与错误发布标签。

发布后，使用完全相同源码的更新器 Client，以**模拟的 RC5 已安装身份**，为
macOS arm64、Windows x64、Linux DEB 和 Linux AppImage 选出了 RC6，
并下载、重新校验了 macOS 安装包。这验证新版 Client 的候选更新与校验逻辑；
RC5 本身没有更新器。

| 安装包运行平台 | 已执行的验收 |
| --- | --- |
| macOS arm64 | 重新公开下载；RC、Preview 通道显示已是最新，Stable 通道显示无兼容版本。在隔离配置中通过公开 App Hub 搜索、安装、实际打开 Repo Radar，并验证重启后保留安装状态。 |
| Linux x64 | 6 项原生启动检查；在隔离 DEB 文件系统布局与解压后的正式 AppImage 中检查真实签名 RC，均显示已是最新。未执行系统包安装或 FUSE 启动。 |
| Windows x64 | 正式构建及归档、可执行文件、资源、内核收据静态检查。图形界面安装与启动仍属**未经验证**。 |

macOS Install & Restart 源码构建针对带签名的公开 RC6 DMG 通过 21 项隔离原生
验收：取消、自动替换并重启、RC6 与已是最新界面、Unicode 笔记不变、已安装的
Repo Radar 仍能打开、恢复和清理。起点是**标为 RC5 的源码构建**，不是历史
RC5 发布版。本地源码应用包另外通过 13 项启动确认、启动失败回滚、配置保留和
清理检查，并目视确认了回滚通知；这些测试不替代发布签名验证。实际运行
cargo-packager 确认打包了正确辅助程序和协议标记。Windows/Linux 安装、其他
应用业务流程和模型调用仍属**未经验证**。加入签名强制检查之前，隔离 macOS 测试曾执行
RC1 → RC2 下载、篡改拒绝、通道切换与取消；这些是历史检查，不代表 RC6 接受
未签名的旧版本。清理下载和 Linux 复制到“下载”文件夹在临时目录中测试。
Android 在实体设备上的真实点按检查仍属**未经验证**。

经独立安全审查后补充的单元测试：未签名的更新和由其他身份签名的更新（用
`codesign -s -` 临时签名的测试应用包）在交换前和启动前都被拒绝；交换前失败会
重新打开旧应用；并覆盖安装目录锁、备份清理和 setuid 文件。实际的辅助程序可执行
文件分别以调试和发布方式构建，在设置 `OCTOSENSE_UPDATE_UNSIGNED_FIXTURES=1`
的情况下处理一个被植入的未签名应用：发布版辅助程序仍在签名检查处拒绝，其二进制
文件中也不含该设置的名称。真实签名以只读方式用两个不同团队第三方应用的副本
检查：Developer ID 签名的那个（OctoSense 发布版即以此方式签名）被接受为已安装
应用，接受其自身签名，拒绝另一团队的应用和被修改的副本；Mac App Store 签名的
那个不被接受为已安装应用，因为其叶证书属于 Apple。用这套代码以一个已签名的
OctoSense 发布版替换另一个已签名发布版，仍属**未经验证**，因为尚无包含它的
发布版完成签名。

在自有 API 35 Android 模拟器中，使用实际 Java 更新适配器和操作系统
PackageInstaller 的测试应用通过了含清理步骤在内的 25 项验收：更改安装权限后不自动重放安装；
拒绝错误签名、包名、哈希、版本代码和最低 SDK；可取消并重试可见的系统确认；
版本代码 1000 → 1001 更新保留了 UID 与模拟应用数据，替换后可恢复状态。
此验收覆盖原生适配器，**不是完整的 Rust Home 二进制**。没有改动实体手机；
从已发布 Home 构建发起的完整安装流程仍属**未经验证**。
