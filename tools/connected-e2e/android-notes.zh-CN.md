# 使用独立 Android 测试应用验证 Notes

[English](android-notes.md) | 简体中文

OnePlus 6 使用 **OctoSenseNotesTest**，包名
`dev.makepad.octosense.connectednotes`，与已安装的 Home、Mail 和个人账户隔离。
此流程验收本地 Markdown，不代表真实 GitHub 登录或仓库写入；不复制提供商凭据。

先运行 `python3 tools/setup.py` 准备固定版本的源码，再按
[Home 构建前置条件](../../rom/docs/home-build.md)准备 Android SDK、完整 JDK 17、
Gradle 和固定版本的 Android octos 内核。下列命令已使用本次任务的私有绝对路径执行；
复现时替换变量。`NOTES_LAB` 必须是全新私有目录，不能复用日常 OctoSense 配置。

设置 `JAVA_HOME`、`ANDROID_HOME`、`ANDROID_SDK_ROOT`、
`OCTOSENSE_GRADLE_HOME`、`NOTES_KERNEL`、`NOTES_LAB`，从仓库根目录先导出 Java 契约：

```sh
(cd phone/android && ./gradlew --no-daemon :contracts:exportHomeContracts)
cargo build --release \
  --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml \
  --target-dir "$NOTES_LAB/packager"
```

不能省略契约导出：本次首个构建因此在 Java 编译失败。应从此检出目录构建固定版本的
打包器：二进制保存源码目录路径，并在打包时读取 Java 文件。运行时 Java 补丁变更后
重新打包 APK。从 `phone/` 执行：

```sh
MAKEPAD_FORCE_DEBUGGABLE=1 CARGO_BUILD_JOBS=4 \
MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=$NOTES_KERNEL" \
"$NOTES_LAB/packager/release/cargo-makepad" makepad android \
  --sdk-path="$ANDROID_HOME" --abi=aarch64 --version-code=2026100620 \
  --package-name=dev.makepad.octosense.connectednotes \
  --app-label=OctoSenseNotesTest \
  build -p octosense-home --release --locked --features dev-mode
```

回到根目录，用[验收安装器](README.zh-CN.md)创建只包含原始 Notes 应用包的临时签名目录：

```sh
target/release/examples/connected-install \
  --keep-profile="$NOTES_LAB/apps" \
  ../OctoSense-App-Flow/examples/connected-apps/github-notes/bundle
python3 tools/connected-e2e/android_notes.py \
  --apk phone/target/android/makepad-android-apk/octosense_home/apk/octo_sense_notes_test.apk \
  --profile "$NOTES_LAB/apps" \
  --build-tools "$ANDROID_HOME/build-tools/35.0.0" \
  --debug-keystore .sources/makepad/tools/cargo_makepad/debug.keystore \
  --out "$NOTES_LAB/packaged"
```

打包工具检查独立的 debuggable 包名和 Notes 签名安装回执，拒绝已有 `.host` 目录的
配置，只保留类型正确的公开回执字段，并加入 Android 支持的
[调试启动包装脚本](https://developer.android.com/ndk/guides/wrap-script)，对齐 APK、
使用已有开发密钥签名并验证。包装脚本只设置应用私有目录路径和公开信任锚。正常宿主
在启动时验证目录签名；打包工具不替代或绕过该准入流程。工具回执只说明打包结果，
不声称已安装或通过手机 UX 验收。

在指定设备上用 ADB 安装结果（`--no-incremental` 避开本次出现的增量安装失败），
通过 `run-as` 将全新的 `apps` 目录复制到测试包的 `files/apps`，再启动
`.MakepadApp`，其 `makepad.APP_CONFIG` 字符串为：

```json
{"test_actions":["launch-hub:org.octosense.samples.githubnotes"]}
```

不能覆盖日常 Home 或传输个人配置。固定版本的 Makepad remote 在 Android 是空实现，
设置 `MAKEPAD_REMOTE` 或 ADB 端口转发不会启用它。手机使用 ADB 输入和原始设备截图；
Mac 浸泡使用 Makepad instrument，分别记录证据。

用铅笔/眼睛图标切换原文与预览，预览调色板可打开次级富文本块编辑器。
检查真实软键盘及返回/文件图标进入 Repository 后的往返。更新测试目录时保留
`org.octosense.samples.githubnotes` 草稿目录。完整替换受管理的
`.bundles/org.octosense.samples.githubnotes` 目录，并将旧包单独保留以便回滚；
仅覆盖文件可能残留已删除资源，导致签名摘要校验拒绝启动。读取**测试包**内
最新的 `draft-a.json`／`draft-b.json` 版本，核对精确原文；只停止该进程后重启，
再检查恢复后的可见内容。硬件 Enter 与软键盘 Enter 要分别测试：首个版本在硬件
Enter 时丢失正在组合的末词，已有独立运行时补丁及回归测试。保留失败与重测证据，
不能用桌面检查推断手机已通过。

[Rinx 编辑器真机记录](evidence/notes-rinx-phone-20261006/README.zh-CN.md)
记录新图标布局与键盘期间悬浮导航遮挡修复。
