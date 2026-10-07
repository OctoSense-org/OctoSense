# 联网应用原生验收

[English](README.md) | 简体中文

这些驱动通过私有、临时签名的 App Hub 目录安装公开示例包，并运行经过校验的
已安装副本。控件、存储、账户授权、服务分发和审核面板均使用真实主机实现；
只有提供商 HTTP 网络和凭据库使用合成替身，不使用真实客户端注册、令牌、仓库或日历。

在 OctoSense 仓库执行：

```sh
cargo build --locked --release -p octosense-shell \
  --features mobile-apps,acceptance-fixtures \
  --example connected-app-host --example connected-install \
  --example connected-inbox-e2e
python3 tools/connected-e2e/notes.py \
  --bundle ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle
python3 ../OctoScript-App-Design-Flow/examples/connected-apps/google-calendar/scripts/verify-installed.py \
  --host target/release/examples/connected-app-host
```

`notes.py` 使用全新临时配置，只在内存保存签名私钥，对已有摘要的应用包副本签名，
经 `Store.install_staged` 安装，再经 `prepare_launch` 和 `validate_prepared_launch`
验证并打开。重启复用同一私有目录和配置。`connected_support` 拒绝非空的安装根目录，
不会原地签名或修改源码应用包。

手动隔离启动示例（路径需替换；该占位路径未执行）：

```sh
target/release/examples/connected-install \
  --keep-profile=/absolute/new-empty-test-root/apps \
  ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle
MAKEPAD_HIDE_WINDOWS=1 target/release/examples/connected-app-host \
  --installed-app=org.octosense.samples.githubnotes \
  --app-data=/absolute/new-empty-test-root/apps --provider-fixture=github --remote
```

`acceptance-fixtures` 默认关闭。普通二进制不能用 `--provider-fixture` 打开此能力。
原生替身注册要求隔离、明确标记的配置、空提供商注册和合成账户元数据，拒绝普通配置；
它只对该准确根目录生效，不改变服务权限、账户归属、应用身份或审核检查。
不得将用户的 GitHub／Google 提供商凭据复制到这些配置中。真实模型流程使用
另行配置的私有内核配置，绝不把它作为公开证据导出。

Notes 验证选中账户、Unicode 编辑／预览、重启、分页及空仓库、目录和第二文件选择、
未保存修改保护、准确主机审核／取消、新旧文件提交、SHA 冲突、响应丢失后不自动重试，
以及离线重启。四次明确的合成写入尝试与提供商记录逐项核对。响应丢失案例故意模拟
远程提交已发生但回复丢失，应用必须保留未确认草稿，等待用户核对。

每次运行在 `target/connected-notes-e2e/run-*` 保留源码／二进制／PNG 摘要、控件快照、
原生日志和回执，也保留失败。驱动只关闭自己创建的进程并清理临时配置。
视觉验收还必须逐张检查原始 PNG。`native.py` 驱动 Makepad instrument；输入事件是
合成事件，不是物理批准。

小型主机不启动生产代理内核、新邮件收集器或 Glance。Inbox／Calendar 主机流程使用
嵌入实际 Shell 的 `connected-inbox-e2e`。提供商替身不能证明真实 OAuth、真实投递、
Android／Windows／Linux 原生界面、公开目录发布或真人批准。

## 平台证据（2026-10-06）

| 检查 | 结果及边界 |
| --- | --- |
| 最终 macOS 单元／构建检查 | [48 项 OAuth、953 项 shell 测试、桌面／Home 构建和两套源码图检查通过](evidence/final-local-checks.json)。通常忽略的系统凭据库测试也单独明确执行并通过。 |
| macOS 系统凭据适配器 | [一次明确的 Keychain 测试通过](evidence/macos-vault.json)：保存／重开／逻辑撤销及配置文件无明文凭据。不代表真实 OAuth。 |
| Linux 协议及主机适配器 | [43 项协议测试及主机编译通过](evidence/linux-provider.json)。明确运行的原生凭据库测试因主机没有可用、已解锁的 Secret Service 而失败。没有 GUI／显示环境。 |
| Windows | [在 Mac 上交叉编译](evidence/windows-unverified.json)因缺少 Windows SDK 头文件，在到达主机 crate 前停止。原生运行未验证。 |
| OnePlus 6 | [Enter 修复后通过本地 Notes 真机检查](evidence/notes-oneplus-20261006/README.zh-CN.md)：软键盘／硬件输入、预览和精确冷恢复。[复现](android-notes.zh-CN.md)使用独立签名测试 APK。[Rinx 编辑器更新](evidence/notes-rinx-phone-20261006/README.zh-CN.md)已验证图标控件及输入时隐藏悬浮导航；真实提供商验收仍待完成；共享 Google 原生授权尚未实现。 |
| 浸泡后的修复 | 宿主审核独占绘制修复通过 [954 项 shell 测试及所有桌面／Home 构建和源码图检查](evidence/glance-modal-validation.json)。之后仅改 Java 的 Enter 修复通过 [96 项 ROM、18 项 setup 测试和补丁栈检查](evidence/android-enter-validation.json)，并完成上述独立手机重测。 |

启动包含可选原生应用的 shell 单元测试前，应将 `RINX_DATA_DIR` 指向全新、私有、
绝对路径目录。Rinx 首次访问时缓存根目录，运行中修改不能隔离已有进程。
验收不得使用开发者日常 Matrix 配置；不要把原始 shell 日志或模型配置作为公开证据导出。

## Notes 重复 UX 浸泡测试

```sh
python3 tools/connected-e2e/notes_soak.py \
  --bundle ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle \
  --cycles 36 --duration-seconds 600
```

该测试使用隔离的签名安装，交替输入长短 Unicode 文档；测试按图标组件 ID 切换原文、预览与样式面板中的次级富文本块编辑器，滚动、重新聚焦并往返 Repository，然后核对持久化 Markdown 的精确内容。
每六轮通过原生富文本控件编辑并撤销，打开精确宿主审核后取消。间隔空闲时间检查异步
回调没有改动草稿；最后重启进程核对原文和目标。供应商日志必须没有任何写入尝试。

回执保留每轮内容摘要、持久化版本、活动和空闲时间、进程 RSS，以及操作耗时的
p50/p95/最大值。这里计量的是包含原生帧等待和状态轮询的 Makepad instrument 往返，
不是 FPS 或真实显示延迟。有限会话的内存趋势包含编辑历史、渲染缓存和截图分配，
不能单独证明有无泄漏。原始截图及首次失败保留在 `target/connected-notes-soak/run-*`，
视觉检查与功能断言分开记录。

[2026-10-06 Notes 浸泡记录](evidence/notes-soak-20261006/README.zh-CN.md)通过十分钟
36 轮及独立 120 轮密集测试，保留精确草稿、原生截图检查、耗时边界与内存增长记录。

Calendar 和 Inbox 的独立驱动及证据位于 App Design Flow：
[Calendar](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/feat/connected-sample-apps/examples/connected-apps/google-calendar/ACCEPTANCE.md)
及 [Inbox](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/feat/connected-sample-apps/examples/connected-apps/inbox/README.zh-CN.md)。
两者主机不同：Calendar 使用提供商主机，Inbox 使用完整 Shell 并实际调用 DeepSeek，
不能将耗时与内存数值合成同一基准。最新的[三包签名安装检查](evidence/signed-install-after-soak.json)
包含修正监控状态后的 Inbox 包，三者均通过重开和篡改拒绝，未修改公开目录。

## Rinx 编辑器与无法加载的草稿

[原生参考记录](evidence/rinx-writer-20261006/README.zh-CN.md)在相同宽窄窗口下对比
Rinx 真实文章编辑器与复用组件。新增编辑器移除 Notes 外层按钮行；返回/文件图标
进入仓库设置，纸飞机进入确切内容的宿主审核，提供商权限边界保持不变。

```sh
python3 tools/connected-e2e/notes_recovery.py \
  --bundle ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle
```

该原生测试在签名安装内放入超出 Rinx 解析上限的虚构草稿，验证重新打开时不会
出现可覆盖原文的空编辑器；明确选择有效文件后，原始恢复副本仍完整保留。
测试不执行提供商写入；正常准入、提供商流程和浸泡测试继续使用上面的命令。
