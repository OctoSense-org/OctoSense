# Android 上的 Host API Lab

[English](ANDROID.md) | 简体中文

**公共 API 批次（测试应用 0.4）：**[OnePlus 6／Android 15 验收](evidence/public-api-v0.4/oneplus6.json)已通过 **44/44 项检查**：原有 14 项、十项 OS 检查及二十项日历／邮件／媒体发现和拒绝检查。[Mac 运行](evidence/public-api-v0.4/macos.json)通过全部 30 项桌面检查。手机 APK 使用生产源码 `13e3b21a`、运行时 `fc938badf`，SHA-256 为 `2ad38099b4115bfa3b4c556cf181b7093ca1e672db73c48ea38d041b705f4dd1`。构建结束时源码为 `53bab40f`；期间改动仅影响测试代码，不包含在 APK 中。回执记录完整源码、运行时、SDK、适配器及产物摘要。这些结果不验证之后的 Android Video Java 改动。

单独的[回归记录](evidence/public-api-v0.4/regression.json)在 `53bab40f` 上通过 1,051 项共享 Shell 测试、三个打包检查及原生测试应用构建。contract 1.10 仍未发布，用户已下载的桌面版和 Home 没有更新。早先的 [24 项批次记录](evidence/os-api-batch1/receipt.json)保留其当时待执行的真机状态，下文原始 14 项记录也继续保留。

历史[验收记录](evidence/android/receipt.json)确认：**OnePlus 6、Android 15，14 项检查全部通过**。[原生结果](evidence/android/native-result.json)来自实际手机进程。普通签名应用调用自己的 Splash 工具，通过生产环境宿主服务发现 API 并读取 Android 摄像头权限状态。测试同时验证跨账户调用、未声明的能力和工具、无效参数、未编译进宿主的 Rust 函数、后台权限弹窗以及工具关闭后的调用都会被拒绝。

这验证了原生应用、工具和宿主服务之间的执行链路。测试没有启动模型或 peer agent，没有拍照、批准权限、登录提供商，也不能证明物理输入审批。测试完成后只停止了自己的测试包，没有修改正常 Home 或个人资料。

使用获准测试的手机和已有的 Makepad Android SDK/JDK。在仓库根目录准备固定版本运行时及打包工具：

```sh
python3 tools/setup.py
cargo build --release --offline --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
```

将 `JAVA_HOME`、`ANDROID_HOME` 指向已有工具，将 `PACKAGER` 指向生成的 `cargo-makepad`，将 `MAKEPAD_ANDROID_SDK` 指向已有 Makepad Android SDK。构建独立、可调试的测试包。`octosense-host-api-smoke` 与桌面 `host-api-lab` 示例共用 Rust 源码：

工作区 Cargo 配置为 Android arm64 目标和 Apple arm64 构建主机选择 AWS-LC 支持的 CC 构建器。否则，Android 打包器也会强制宿主侧的测试包验证依赖使用 CMake。此设置避免额外安装 CMake，不禁用密码学或汇编功能。在写入配置前，等效的宿主目标环境变量覆盖已用于实际 Android 构建。

```sh
python3 tools/build-host-api-android.py \
  --packager "$PACKAGER" --sdk "$MAKEPAD_ANDROID_SDK" \
  --package dev.makepad.octosense.hostapilab.publicapis1
```

构建助手临时复制 Home 的原版 `DeviceCalendarClient.java`，通过独立测试扩展接入同一个原生适配器，构建结束后移除临时副本，不安装任何工具。测试只查询权限状态，不批准日历访问。运行驱动时使用相同的新包名：`--package dev.makepad.octosense.hostapilab.publicapis1`。


将 `APK` 指向打包器输出的已对齐 APK，`HUB` 指向已构建的兼容 App Hub CLI，并选择全新的 `EVIDENCE` 目录，然后运行已在手机上验证的驱动：

```sh
python3 tools/test-host-api-android.py \
  --adb "$ANDROID_HOME/platform-tools/adb" \
  --aapt2 "$ANDROID_HOME/build-tools/35.0.0/aapt2" \
  --apk "$APK" --hub "$HUB" --out "$EVIDENCE" \
  --package dev.makepad.octosense.hostapilab.publicapis1
```

驱动检查 APK 包名与可调试标记，拒绝覆盖已安装的包，只复制公开合成测试应用，为 Android listing 重新计算摘要并收集原生结果。手机宿主在内存中创建临时签名密钥，通过正常签名 Store 流程接纳应用，没有注入权限或凭据。多台设备连接时，用 `--serial` 指定获准设备；验收记录不包含设备序列号。再次测试时，构建和驱动的 `--package` 参数应统一使用新的 `.hostapilab.<suffix>` 包名。

无论成功或失败，驱动最后只停止自己的测试包，保留独立测试包及其数据供检查。公开记录不含设备序列号、个人账户、凭据或本地源码路径。构建助手和 44 项检查驱动已使用已有工具及缓存，针对新回执中的源码／产物摘要实际执行；构建助手没有安装工具。此 Host API 测试仍不验证日历权限批准及事件读写、SMTP 投递、麦克风录音／音频播放、原生照片／分享选择器或 Video 播放；Video 使用独立验收测试。
