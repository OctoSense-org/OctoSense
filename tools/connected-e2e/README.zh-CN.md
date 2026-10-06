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
| OnePlus 6 | ADB 未发现连接的设备。此共享服务尚未实现 Google 原生授权；手机键盘、生命周期和真实提供商验收待完成。 |

启动包含可选原生应用的 shell 单元测试前，应将 `RINX_DATA_DIR` 指向全新、私有、
绝对路径目录。Rinx 首次访问时缓存根目录，运行中修改不能隔离已有进程。
验收不得使用开发者日常 Matrix 配置；不要把原始 shell 日志或模型配置作为公开证据导出。
