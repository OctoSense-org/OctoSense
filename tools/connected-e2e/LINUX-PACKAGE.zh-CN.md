# Linux 桌面安装包验收

[不可变验收记录](evidence/linux-package-37723207058.json) 记录了真实 `.deb`
安装包的八项通过结果。安装包来自 [桌面打包任务
37723207058](https://github.com/OctoSense-org/OctoSense/actions/runs/37723207058)，
产物编号为 `11528131778`，构建环境为 Ubuntu 22.04。实际源码是任务指定的
PR 合并提交 `c15a0ab93d6d4991e5774e1b56028247bf9c7ac2`，不能仅用 PR
分支提交 `9f8ce11c7cc890aa77131821c42a0ae61b46fc54` 代替。

测试通过 `dpkg-deb -x` 将安装包解包到新的私有目录，没有向宿主系统安装。
真实可执行文件及其随包资源在独立 Xvfb 显示和 D-Bus 会话中运行，已有的
解包测试依赖通过只读 bwrap 挂载提供。测试使用全新 OctoSense 配置，没有
使用个人提供商账号，也没有修改系统软件包或全局设置。

八项自动检查覆盖可执行文件存在、独立显示、原生进程持续运行、控件快照、
实际渲染画面、App Hub 显示、资源加载和正常退出。对实际画面的审阅还确认了
Google Calendar、GitHub Notes 和 Inbox Assistant 三个列表项。界面上的
代理授权对话框没有被确认。所有自有测试进程均已停止，命名空间正常退出。

这项证据**不证明**从 App Hub 安装应用、登录账号、经操作系统认证后执行写入、
通过包管理器安装或最终集成发行版可用。记录绑定了安装包、可执行文件及已审阅
画面的哈希。原始配置、日志和快照保持私有。后续安装包必须生成新的验收记录，
不能修改这份记录来代表其他版本。

复现时，先核对 `.deb` 的 SHA-256，再解包到新目录。为 `usr/bin/octosense`
设置全新的 `OCTOSENSE_HOME`、`MAKEPAD_HOME` 和 XDG 目录，并使用独立 X11
显示、`MAKEPAD=linux-x11`、`MAKEPAD_REMOTE=on` 与
`MAKEPAD_WM_TEST_APP=apphub`。原生 instrument 的 `snap`、`g`、`quit` 端点
分别用于相同的结构快照、画面和正常退出检查。instrument 仅监听回环地址，
不要确认代理授权对话框；新的原始证据保存在新的私有运行目录。原测试环境专用的
启动脚本没有作为可移植驱动发布。
