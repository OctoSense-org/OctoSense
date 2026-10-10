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

本次完成的验收绑定到干净源码 `c5f0c5c1948d9b730c707b409e9713af49e3e614`，
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
