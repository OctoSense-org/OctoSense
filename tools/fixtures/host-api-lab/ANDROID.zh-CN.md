# Android 上的 Host API Lab

[English](ANDROID.md) | 简体中文

**当前 OS 批次：**测试示例现在要求 24 项检查。源码 `807f2bc8` 的 APK 已构建成功，但执行最后的手机验收时，ADB 找不到获准测试的 OnePlus 6。真机执行仍为**待完成**，状态见[本批记录](evidence/os-api-batch1/receipt.json)。下文 14 项通过结果是历史记录，不能验证新增文件、定位检查或新的运行时修复。

[验收记录](evidence/android/receipt.json)确认：**OnePlus 6、Android 15，14 项检查全部通过**。[原生结果](evidence/android/native-result.json)来自实际手机进程。普通签名应用调用自己的 Splash 工具，通过生产环境宿主服务发现 API 并读取 Android 摄像头权限状态。测试同时验证跨账户调用、未声明的能力和工具、无效参数、未编译进宿主的 Rust 函数、后台权限弹窗以及工具关闭后的调用都会被拒绝。

这验证了原生应用、工具和宿主服务之间的执行链路。测试没有启动模型或 peer agent，没有拍照、批准权限、登录提供商，也不能证明物理输入审批。测试完成后只停止了自己的测试包，没有修改正常 Home 或个人资料。

使用获准测试的手机和已有的 Makepad Android SDK/JDK。在仓库根目录准备固定版本运行时及打包工具：

```sh
python3 tools/setup.py
cargo build --release --offline --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
```

将 `JAVA_HOME`、`ANDROID_HOME` 指向已有工具，将 `PACKAGER` 指向生成的 `cargo-makepad`，将 `MAKEPAD_ANDROID_SDK` 指向已有 Makepad Android SDK。构建独立、可调试的测试包。`octosense-host-api-smoke` 与桌面 `host-api-lab` 示例共用 Rust 源码：

工作区 Cargo 配置为 Android arm64 目标和 Apple arm64 构建主机选择 AWS-LC 支持的 CC 构建器。否则，Android 打包器也会强制宿主侧的测试包验证依赖使用 CMake。此设置避免额外安装 CMake，不禁用密码学或汇编功能。在写入配置前，等效的宿主目标环境变量覆盖已用于实际 Android 构建。

```sh
MAKEPAD_FORCE_DEBUGGABLE=1 "$PACKAGER" makepad android \
  --sdk-path="$MAKEPAD_ANDROID_SDK" --abi=aarch64 \
  --package-name=dev.makepad.octosense.hostapilab \
  --app-label='Host API Lab' \
  build -p octosense-host-api-smoke --release --locked --offline
```

将 `APK` 指向打包器输出的已对齐 APK，`HUB` 指向已构建的兼容 App Hub CLI，并选择全新的 `EVIDENCE` 目录，然后运行已在手机上验证的驱动：

```sh
python3 tools/test-host-api-android.py \
  --adb "$ANDROID_HOME/platform-tools/adb" \
  --aapt2 "$ANDROID_HOME/build-tools/35.0.0/aapt2" \
  --apk "$APK" --hub "$HUB" --out "$EVIDENCE"
```

驱动检查 APK 包名与可调试标记，拒绝覆盖已安装的包，只复制公开合成测试应用，为 Android listing 重新计算摘要并收集原生结果。手机宿主在内存中创建临时签名密钥，通过正常签名 Store 流程接纳应用，没有注入权限或凭据。多台设备连接时，用 `--serial` 指定获准设备；验收记录不包含设备序列号。再次测试时，构建和驱动的 `--package` 参数应统一使用新的 `.hostapilab.<suffix>` 包名。

无论成功或失败，驱动最后只停止自己的测试包，保留独立测试包及其数据供检查。公开记录不含设备序列号、个人账户、凭据或本地源码路径。上述命令已使用已有工具和缓存实际运行，不会安装构建工具。
