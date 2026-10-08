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

`state.wat` 是 `bundle/fns/state.wasm` 的源码。重建已提交的模块：

```sh
cargo run --locked --offline -p octosense-wasm-host --example encode_phone_fixture --   tools/fixtures/wasm-phone-lab/state.wat tools/fixtures/wasm-phone-lab/bundle/fns/state.wasm
```

客体在实例复用时会主动保留输入；通过测试要求宿主在每次调用（包括成功调用）之后创建新实例。先前的 19 项结果使用了显式组装的字段，不能证明原始响应转发。修复后的原始路径增加一项转发检查，可复现的公开测试还增加两项编译标识检查。[OnePlus 6 回执](acceptance-oneplus6.json)记录 Android 15 上 **22/22 项通过**，构建源码为 `e67ce63ebca6054961899691e1644a54c2b4f081`，运行时树为 `0fc8e29e2411fd7eb94d845ba16a261d2ebc9dfc`。上面的构建和驱动命令已使用本机工具路径执行。真实模型/对等代理转发、性能和发布 APK 行为仍未验证。

应用列表图片是单独在 macOS 隐藏 Makepad 窗口中捕获的真实界面，源码为 `d3535a95`，拍摄时此测试应用的签名应用工具已经完成。它不是手机截图，也不把 OnePlus 的 22 项回执扩展到后续仅替换图片的提交。该图片及更新后的测试包摘要替换了先前无关的 Host API Lab 图片。
