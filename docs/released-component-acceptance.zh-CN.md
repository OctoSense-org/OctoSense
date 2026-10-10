# 在桌面发行包中验收共享组件

[English](released-component-acceptance.md)

`tools/test-released-components.py` 使用 **macOS 桌面发行包**，通过 App Hub 的
Search → Get → Install 界面安装应用，再通过正常的已安装应用入口启动。
它从带有真实 GitHub 证明的演练目录安装 `org.ymote.componentdemo.first` 和
`.second`，分别重启 Shell，检查两次组件调用、计数器状态和各自的合成 HTML
笔记。第一个应用没有声明能力，第二个声明 `wasm` 和 `storage`。
两者都不使用账号或模型，也不会修改公开商店目录。

它补充 `tools/test-shared-components.py` 的原生测试，后者覆盖应用工具派发、
并发实例隔离及不可变组件去重。原生测试通过不等于发行版 Shell 已通过验证。
本发行包测试也不证明真实模型行为或操作系统升级成功。

## 发行与验收顺序

1. 所需检查通过后合并兼容主机代码，在审核过的提交上创建
   `desktop-v<version>` 标签。现有 `.github/workflows/release-desktop.yml`
   构建 macOS、Windows 和 Linux 包，扫描产物并上传到**草稿发行版**。
   手动运行必须指定该标签和 `dry_run=false`；仅构建分支的 dry-run 不是发行。
   签名和公证依赖发行环境的配置，需要查看相关任务，不能默认已完成。
2. 下载草稿的 macOS `.app.zip`、平台构建记录和 `SHA256SUMS`。核对工作流
   源码提交及全部附件哈希，对下载包运行 `tools/release-scan.py`。
   当前本地打包脚本使用标准 `target/release` 目录，应采用工作流的干净环境，
   不覆盖 Cargo 目标目录或编译目标。
3. 使用 Hub 受保护目录 dry-run 生成的真实签名镜像，其中应包含
   `catalog-v2.json` 和审核过的原始产物。不要覆盖信任锚点、伪造证明，
   或替换为旧版目录。
4. 使用**新的**证据目录运行隐藏窗口测试：

   ```sh
   python3 tools/test-released-components.py \
     --app-zip /path/to/OctoSense_<version>_macos_aarch64.app.zip \
     --sha256 <SHA256SUMS中对应压缩包的哈希> \
     --tag desktop-v<version> \
     --mirror /path/to/verified-candidate-mirror \
     --out /path/to/new-evidence-directory
   ```

   驱动先校验压缩包哈希和包内版本，将 `.app` 解压到证据目录，不修改个人
   已安装的 OctoSense。它使用新的 `OCTOSENSE_HOME`、独立内核目录、文件凭据库、
   真实 GitHub 目录通道和隐藏的 Makepad instrument 窗口。它记录哈希、断言、
   日志和画面，最后关闭自己启动的进程。请同时检查截图和 JSON 记录。
5. 审核各平台产物及各自的验证范围。发布审核过的 RC 草稿时，明确设置为
   **prerelease**；工作流本身没有设置此标记。本 Mac 测试不代表 Windows/Linux
   安装或实体手机测试已经通过。

仅在开发演练时，可将 `--app-zip`、`--sha256` 和 `--tag` 替换为
`--binary /path/to/octosense`，记录会标明 `release_archive_tested: false`。
驱动不会构建、创建标签、下载或发布发行版。尚未执行的步骤不能算验收通过。
