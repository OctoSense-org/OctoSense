# Wasm 手机验收

[English](README.md) | 简体中文

这个合成应用验证签名商店准入 → 应用工具 → `host.request` → Wasm 服务的真实路径，不使用个人账户或模型。它检查成功调用、客体错误、陷阱和超时之后的新实例状态，以及未经重组的原生响应能否通过有大小限制的 JSON 转换。它也检查账户范围、未声明工具、无效参数、已关闭端点和审批面板未出现。

使用已提交且干净的检出目录和获准测试的 Android 设备。编译时把源码提交及运行时树标识嵌入测试应用，驱动拒绝过期或未提交的构建。只安装全新的 `dev.makepad.octosense.hostapilab.*` 包，并在结束后停止该包，不读取或替换 Home。这是可调试的测试 APK，不是发布包或性能基准。

使用已有 Android SDK、JDK、ADB 和固定版本的 Makepad 打包器。把 `CARGO_MAKEPAD`、`MAKEPAD_ANDROID_SDK`、`ADB`、`AAPT2`、`HUB`、`WASM_TARGET`、`WASM_EVIDENCE` 设置为本机路径；证据目录必须尚不存在。在仓库根目录运行：

```sh
python3 tools/setup.py --check --cargo
MAKEPAD_FORCE_DEBUGGABLE=1 CARGO_TARGET_DIR="$WASM_TARGET" "$CARGO_MAKEPAD" makepad android   --sdk-path="$MAKEPAD_ANDROID_SDK" --abi=aarch64 --version-code=2026100703   --package-name=dev.makepad.octosense.hostapilab.wasm1 --app-label=OctoSenseWasmIsolationTest   build -p octosense-wasm-phone-smoke --release --locked --offline
python3 tools/test-wasm-phone.py --adb "$ADB" --aapt2 "$AAPT2" --hub "$HUB"   --apk "$WASM_TARGET/makepad-android-apk/octosense_wasm_phone_smoke/apk/octo_sense_wasm_isolation_test.apk"   --out "$WASM_EVIDENCE"
```

驱动只给临时副本重新计算摘要。原生宿主在内存中生成临时签名密钥，使用正常的签名、摘要、能力和工具准入检查，不发布密钥或服务商凭据。回执记录 APK、源码及运行时标识和各项结果，不包含设备序列号或 SDK 路径。保留旧回执；重新安装时选择新的包后缀，或只删除自己上一次的测试包。

这个独立测试会在 Wasm 工作线程重新检查准入前，明确选择其临时旧格式目录。正常 OctoSense 仍默认使用 GitHub 目录；本测试不能证明生产目录发布或免开发者密钥的 App Hub 界面验收。Apple arm64 主机的 Android 构建使用工作区按目标设置的 AWS-LC CC 配置，见 [Android 构建说明](../host-api-lab/ANDROID.zh-CN.md)。

`state.wat` 是 `bundle/fns/state.wasm` 的源码。重建已提交的模块：

```sh
cargo run --locked --offline -p octosense-wasm-host --example encode_phone_fixture --   tools/fixtures/wasm-phone-lab/state.wat tools/fixtures/wasm-phone-lab/bundle/fns/state.wasm
```

客体在实例复用时会主动保留输入；通过测试要求宿主在每次调用（包括成功调用）之后创建新实例。先前的 19 项结果使用了显式组装的字段，不能证明原始响应转发。修复后的原始路径增加一项转发检查，可复现的公开测试还增加两项编译标识检查。[OnePlus 6 回执](acceptance-oneplus6.json)记录 Android 15 上 **22/22 项通过**，构建源码为 `e67ce63ebca6054961899691e1644a54c2b4f081`，运行时树为 `0fc8e29e2411fd7eb94d845ba16a261d2ebc9dfc`。上面的构建和驱动命令已使用本机工具路径执行。真实模型/对等代理转发、性能和发布 APK 行为仍未验证。

应用列表图片是单独在 macOS 隐藏 Makepad 窗口中捕获的真实界面，源码为 `d3535a95`，拍摄时此测试应用的签名应用工具已经完成。它不是手机截图，也不把 OnePlus 的 22 项回执扩展到后续仅替换图片的提交。该图片及更新后的测试包摘要替换了先前无关的 Host API Lab 图片。

## 真实 GitHub 共享组件验收

同一个原生宿主还支持包含真实 GitHub 证明的 `catalog-v2.json`、两个已审阅的消费应用包及固定版本共享组件的镜像，数据来自[合成发布者](https://github.com/ymote/octosense-component-demo)。
这个模式使用默认 GitHub 信任通道和实际商店准入，不创建旧格式签名，也不授予额外能力。
第一个应用不声明能力；第二个声明 `wasm` 和 `storage`。两个应用调用同一份组件字节，但各自保留实例和私有存储。
四次交替应用工具调用检查计数器、Markdown、文件隔离、别名发现，以及组件到宿主 `runtime.describe` 的真实往返，不启动模型或使用个人账户。

通过已审阅的 [App Hub 候选目录工具](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/SHARED-COMPONENT-REHEARSAL.zh-CN.md)和受保护的管理员工作流 `dry_run: true` 所生成的证明封装准备镜像，不改动正式目录。
以下构建和驱动命令已使用本机工具路径执行，先验证隐藏桌面窗口，最后验证获准使用的 OnePlus 6。
复现时使用新的证据目录、明确获准测试的 OnePlus 6 序列号和本机工具路径：

```sh
cargo build --locked -p octosense-wasm-phone-smoke
python3 tools/test-shared-components.py --host target/debug/octosense-wasm-phone-smoke \
  --mirror "$SHARED_MIRROR" --out "$SHARED_MAC_EVIDENCE"
MAKEPAD_FORCE_DEBUGGABLE=1 CARGO_TARGET_DIR="$WASM_TARGET" "$CARGO_MAKEPAD" makepad android \
  --sdk-path="$MAKEPAD_ANDROID_SDK" --abi=aarch64 --version-code=2026100901 \
  --package-name=dev.makepad.octosense.hostapilab.shared1 --app-label=OctoSenseSharedComponentTest \
  build -p octosense-wasm-phone-smoke --release --locked --offline
python3 tools/test-shared-components.py --adb "$ADB" --aapt2 "$AAPT2" --serial "$ONEPLUS_SERIAL" \
  --apk "$WASM_TARGET/makepad-android-apk/octosense_wasm_phone_smoke/apk/octo_sense_shared_component_test.apk" \
  --mirror "$SHARED_MIRROR" --out "$SHARED_PHONE_EVIDENCE"
```

驱动拒绝已有的测试包，把回执绑定到干净的编译源码及运行时，只卸载本次新安装的测试包，不记录设备序列号。
原生回执记录真实目录载荷摘要、应用包和组件标识，以及四次完整的结果对象。
两种模式验证不同约定：核心函数使用新实例；共享组件共享不可变字节，但保留应用私有实例。
两者都不是性能基准或正式 Home 升级测试。

较早一次完成的验收绑定到干净源码 `c5f0c5c1948d9b730c707b409e9713af49e3e614`，
Makepad 运行时树为 `a6fae94aa4d233503b14bc6d0a36bcbc2b264b44`。
两个组件和两个应用由[发布者工作流 38024551136](https://github.com/ymote/octosense-component-demo/actions/runs/38024551136)
构建并取得证明，见 [v0.1.0 发布](https://github.com/ymote/octosense-component-demo/releases/tag/v0.1.0)。
[受保护的管理员工作流 38026083032](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/38026083032)
以 `dry_run: true` 生成真实的第 16 版目录证明封装，载荷 SHA-256 为
`87bde245807a5ff6a1b3297c409d4ef6684414e47b038519a196feab29f42a7e`。
两次原生执行之前均验证了证明和全部产物摘要，公开目录保持不变。

| 平台 | 原生组件断言 | 驱动检查 | 证据 |
| --- | --- | --- | --- |
| macOS，隐藏 Makepad 窗口 | 28/28 | 12/12，包括最终应用和计数器可见 | [驱动](evidence/shared-components/macos.json)、[原始原生结果](evidence/shared-components/macos-native.json)、[原生截图](evidence/shared-components/completed.png) |
| OnePlus 6 | 28/28 | 13/13，包括指定设备、全新独立包和编译标识 | [驱动](evidence/shared-components/oneplus6.json)、[原始原生结果](evidence/shared-components/oneplus6-native.json) |

回执未经改动直接复制，证明实际商店安装、Splash 应用工具调用、别名发现、组件到宿主调用、
只读组件字节去重，以及各应用独立保留的计数器和文件。OnePlus 测试包已卸载，未使用个人账户或模型，
也未改动已安装的 Home。这些是开发宿主的回执，不是发行二进制验收。
本次不包含真实模型转发、性能、OpenHarmony 设备执行或正式 Home 升级。

## 最终源码验收

源码 `8b09e05d1cb41b677340fa3f9415ee37ac88eace` 由
[#457](https://github.com/OctoSense-org/OctoSense/pull/457) 合并为 `40ca21da`，
两者的树均为 `703c30ecfdc6f2fa2db6323dafd526c9e0c9610c`。最终验收使用上文具有真实
证明的同一私有目录，公开目录保持不变。较早的 `c5f0c5c1` 回执保留为历史证据，不更改其归属。

| 测试入口 | 结果 | 证据 |
| --- | --- | --- |
| 完整 macOS 桌面，全新配置 | 11/11 项驱动检查：App Hub 搜索/安装、应用启动、组件执行和冷重启 | [开发二进制回执](evidence/final-8b09/macos-desktop-rehearsal.json) |
| OnePlus 6，独立组件宿主 | 28/28 项原生断言和 13/13 项驱动检查 | [驱动](evidence/final-8b09/oneplus6.json)、[原生结果](evidence/final-8b09/oneplus6-native.json) |

Mac 回执明确记录 `release_archive_tested: false`，不是 RC4 归档包验收。手机回执绑定嵌入的
干净源码/运行时及 APK 标识，并记录新安装测试包的卸载。两项测试均未使用个人账户或真实模型，
未改动公开目录，也未升级 Home。Home beta.2 较旧。真实模型转发、性能和 OpenHarmony
设备执行仍**未验证**。最终 OS API 结果单独记录于
[Host API Lab](../host-api-lab/README.zh-CN.md#最终源码验收)。

## Windows 缓存验证

`a8e170d4` 的 [Windows CI](https://github.com/OctoSense-org/OctoSense/actions/runs/38066866571)
通过全部 42 项运行时测试（7 项单元、14 项组件、7 项客体、14 项核心运行时；无跳过），
随后通过含 Wasm 服务的 Shell 编译检查。同一提交的本地 CI 为 52/52 步通过，无失败或跳过。
[#467](https://github.com/OctoSense-org/OctoSense/pull/467) 以相同树合并为 `af205d9c`。

此次更改移除了测试中“缓存必须在 50 毫秒内命中”的假设。严格内部测试仍要求真实的
模块/组件缓存反序列化与执行、损坏修复，以及完整的并发发布。
较早的 [Windows 失败](https://github.com/OctoSense-org/OctoSense/actions/runs/38065611111)
只能证明发生了成功的源码编译回退，不能确定其具体原因。

[派生验证摘要](evidence/windows-cache-tests/validation.json)记录源码比较结果：排除新增的
`cfg(test)` 字段、分支和测试模块后，生产缓存源码与 RC4 源码 `9266b008` 相同；其余更改
均为集成测试。RC4 tag 和归档包源码仍是 `9266b008`，**不包含 #467**。此结果不证明
重新构建的二进制等价、发行归档包验收或已安装 Windows 桌面界面验收。
原始本地 CI 回执保留在私有目录；摘要只包含公开源码标识及汇总结果。

## 未发布的 RC3 候选包

`desktop-v0.1.0-rc.3` 的[发布工作流](https://github.com/OctoSense-org/OctoSense/actions/runs/38031827821)
使用源码 `40ca21da`。Mac 和 Windows 打包通过；Linux 已完成编译，但安装包任务在隐私扫描中
误判公开字符串而失败。签名和草稿 Release 任务均被跳过。扫描修复由
[#466](https://github.com/OctoSense-org/OctoSense/pull/466) 合并为 `9266b008`。
新建的不可变 tag `desktop-v0.1.0-rc.4` 指向该提交，树为
`eade406807c20587801a5bfb2169f454dc4e43f7`；其
[工作流](https://github.com/OctoSense-org/OctoSense/actions/runs/38065627444)随后成功完成。
相对于运行时基线 `40ca21da`，只改动了扫描器及其测试。

该次工作流未发布的 Mac app ZIP 在全新配置中通过 11/11 项 App Hub/组件归档包验收。
ZIP 和挂载后的 DMG 内容也通过隐私扫描。二进制只有链接器生成的 ad-hoc 签名，
**没有 Developer ID 签名，也未公证**。首次验收在初始截图阶段失败，尚未执行任何检查。
成功的第二次验收保留了最初空白帧，再通过额外只读观察看到真实同意面板，然后拒绝应用 Agent。
驱动未经修改，完成全部 11 项检查，没有预置同意状态。

- [原始驱动回执](evidence/rc3-unpublished-macos/receipt.json)。
- [产物、源码、签名及尝试记录](evidence/rc3-unpublished-macos/source-provenance.json)。

以上文件均原样复制。这里的 `release_archive_tested: true` 只表示驱动测试了 app ZIP；
配套记录明确注明工作流失败、Release 未发布。这**不是 RC4 验收**。
RC4 候选包验收及后续发布文件的核对证据分别记录如下。

## RC4 Mac 候选包（未发布）

[工作流 38065627444](https://github.com/OctoSense-org/OctoSense/actions/runs/38065627444)
在源码 `9266b008` 构建的未签发开发者签名 Mac Actions 候选包，在全新配置中
**首次尝试即通过全部 11/11 项归档包检查**。拒绝 Agent 前，真实同意界面已经清晰可读；
两个应用经 App Hub 安装，其组件调用更新了各自隔离的状态并渲染笔记。测试没有重试、
额外截图、外部缓存预热或期限修改。app ZIP 与挂载后的 DMG 通过完整隐私扫描：
578 个文件、8 类模式。二进制只有链接器生成的 ad-hoc 签名，
**没有 Developer ID 签名，也未公证**。

- [原始驱动回执](evidence/rc4-macos/receipt.json)。
- [候选包来源及验收详情](evidence/rc4-macos/candidate-provenance.json)。

两份记录均原样复制。这是**未发布的 Actions 候选包**：`release_archive_tested: true`
记录的是 app ZIP 这一交付形式，不代表当时已发布 Release。后续[发布证据](#rc4-发布证据)
将这些未修改的记录与最终文件绑定。带真实 GitHub 证明的演练目录没有修改公开目录；测试未使用个人账户或
真实模型，也没有升级 Home。RC3 较早的截图失败和重试证据继续单独保留。

## RC4 发布证据

[桌面 RC4](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4)
使用源码 `9266b0083544d86bd7636543b5ff60c61b26460f`，树为
`eade406807c20587801a5bfb2169f454dc4e43f7`，构建来自
[工作流 38065627444](https://github.com/OctoSense-org/OctoSense/actions/runs/38065627444)。
该预发布版于 2026 年 10 月 10 日 18:25:40 UTC 发布，共九个文件。
原样保留的[产物校验](evidence/rc4-macos/verified-assets.json)与
[下载来源](evidence/rc4-macos/download-provenance.json)记录发布前的完整校验和验证，
其中的草稿 URL 属于历史记录。后续[匿名公开访问检查](evidence/rc4-macos/public-download-verification.json)
为 9/9 通过：完整下载了 `SHA256SUMS`，其余每个文件只下载首字节，
没有再次匿名完整下载所有安装包。
Mac app ZIP 在候选包首次验收中通过 **11/11** 项检查，SHA-256 为
`a58fe9efdece0fa6a58c51a5767b368127a9713a1c7048509366427eead2837e`。
新增的[最终文件绑定记录](evidence/rc4-macos/final-byte-binding.json)证明最终 ZIP、DMG
和打包回执与已测候选包逐字节相同，因此没有重复运行 UI 测试。候选包回执保持原样，
包括测试当时尚未发布的状态。
[签名与扫描证据](evidence/rc4-macos/signing-and-scans.json)记录三个平台的打包扫描
均通过，最终扫描覆盖 9 个产物、1,205 个文件和 6 种模式。Mac ZIP 和挂载后的 DMG
另通过 578 个文件、8 种模式的扫描。macOS 只有链接器临时签名，没有 Developer ID
签名或公证；Windows 安装包没有 Authenticode 签名。此次 Mac 归档包验收没有覆盖
Windows/Linux 安装后的界面或系统升级路径。

上文源码与设备结果来自开发宿主和独立 Android 测试包，没有升级已安装的 Home。
`a8e170d4`/`af205d9c` 的 Windows 缓存验证单独记录：RC4 不包含后续仅改测试的修复，
生产源码相同也不等于重建后的二进制相同。这些检查仍未验证真实模型推理、真实
账户写入或 OpenHarmony 设备执行。共享组件目录发布与宿主兼容是两件事；带真实证明的
试运行条目没有加入公开目录。
