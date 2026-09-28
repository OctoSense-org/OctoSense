# OctoSense 共享 Octos 服务

[English](README.md) | 简体中文

Shell 拥有唯一的 Octos Agent 运行时。原生应用通过受限的 app-peer 接口访问它；
OctosCode 终端和 Web 客户端连接同一个需要认证的 WebSocket 服务，不会另起内核或争用数据目录。

## 与系统 Agent 对话

1. 在 **AI providers** 配置模型，再打开 **Talk to Octos**。
2. 宿主面板显示服务器地址、WebSocket 地址，以及 `_main` profile 下的系统会话
   `_main:api:octosense#system`。
3. 输入你托管 OctosCode Web 的来源地址，例如 `http://localhost:4173`，点击
   **Save origin and restart**。修改来源会重启 Octos，并中断正在进行的任务。
   OctoSense 本次没有内置 Web 客户端资源，仍需单独托管。
4. 点击 **Copy access token**，再点 **Open web client**。在 Web 客户端填写面板中的
   服务器地址，把令牌粘贴到 **Auth token**，选择 **Connect**，再在链接确认页点击
   **Open conversation**。生成的链接使用服务器确认的工作目录选择
   系统会话，不包含令牌；也可以在电脑上打开这个链接。

令牌可控制 Agent 及其工具。复制动作由原生代码执行，应用脚本和服务返回值都不会获得令牌。
仅使用你信任的 Web 客户端。

桌面 OctosCode 使用 `--endpoint`、`--profile-id _main` 和
`--session '_main:api:octosense#system'` 参数，并通过它的环境变量或配置传入令牌，关闭
stdio 模式。直接启动 `octoscode` 通常会另起自己的内核。本次**未验证 TUI 启动**；
已经使用真实内核和本地模拟模型验证原生客户端与浏览器类型客户端同时连接；也使用无界面
Chromium 打开生成的系统会话链接，发送消息并看到模型回复。

### 电脑连接 Android 手机

服务只监听 `127.0.0.1`，手机上的其他 APK 仍需令牌。电脑需要隧道。以下 ADB 示例在本次
修改中**未经过设备验证**：把 `SERIAL` 换成获授权的设备序列号，`PORT` 换成面板显示的端口。
两端使用相同端口时，电脑可以直接使用面板中的服务器地址。

```sh
adb -s SERIAL forward tcp:PORT tcp:PORT
```

在电脑托管 Web 客户端，在手机面板允许该精确来源地址，再在电脑打开系统会话链接并填写
转发后的服务器地址与令牌。这个 APK 内置服务无需 root、Termux、PRoot 或 Ubuntu，但本次
修改也不会添加 Linux 编程工具链。浏览器可能要求授予本地网络访问权限。

## 运行方式与生命周期

- 桌面和 Android 启动 `octos serve --host 127.0.0.1 --host-managed`。
  Android 执行 APK 中的 `liboctos.so`，桌面使用 `OCTOS_APP_CORE_BIN`。
- 首个原生使用方或连接面板启动服务。首次分配空闲端口，只接受固定版本内核的监听地址公告。
  原生接口保持不变，显式协商原 stdio 默认支持的协议能力。
- 系统工作目录解析后保存在 `system-workspace.txt`，原生客户端打开系统会话时复用它。
  Web 为会话绑定目录后，即使 Shell 重启也可继续访问；应用 peer 的目录仍相互独立。
- 更改模型或 Web 来源时，等待旧进程退出后重启，保留端口和令牌以便客户端重连。
  关闭全部原生应用后服务继续运行；关闭 Shell 时停止。这不是 Android 常驻前台服务，
  Android 杀死应用进程后 Agent 也会停止。
- `<core_dir>/client-connection.json` 保存连接信息和秘密令牌，以 `0600` 权限原子写入，
  正常停止时删除。重新创建 Shell 内核服务会更换令牌。不要发布此文件，也不要把令牌写进
  日志或命令行参数。
- core 目录按顺序选择：`Options::core_dir`、`OCTOS_APP_CORE_DIR`、手机上的
  `<应用数据目录>/octos-home/.octos`，或桌面的 `~/octos-home/.octos`。
  AI providers 写入 `<core_dir>/profiles/_main.json`。
- OpenHarmony 继续使用进程内 `serve_io`，暂不支持外部客户端；iOS 没有本地内核。
  `Options::stdio()` 为测试替身和嵌入宿主保留私有管道及空闲停止行为。

共享服务器不等于共享会话；客户端必须打开相同 session。应用 peer 仍有独立的受限会话。
浏览器断开 WebSocket 时，它发起的运行中任务仍可能被中断（上游 Octos issue 2167）；
本次没有实现脱离客户端的任务所有权。

## 构建与验证

可执行文件必须包含 [octos-runtime-patches.lock.json](../../octos-runtime-patches.lock.json)
锁定的补丁：强制宿主令牌认证、禁用免密 solo 登录和本地 profile 请求头冒充，并在服务器
进程内运行 profile。没有 `--host-managed` 的普通上游内核会启动失败，不会回退到免认证模式。
应用补丁前检查固定版本及哈希。Android 打包工具自动应用它。

以下命令已在仓库根目录运行：

```sh
python3 tools/kernel-artifact.py --host --plan
python3 -m unittest discover -s tools -p 'test_kernel_artifact.py'
cargo test --locked -p octosense-kernel
```

`python3 tools/kernel-artifact.py --host` 在 `target/octos-kernel/target/release/octos`
生成桌面 release 内核（**该 release 命令未验证**；已构建并测试同版本加补丁的 debug 内核）。
启动 Shell 时把 `OCTOS_APP_CORE_BIN` 指向该文件。

真实内核测试使用 `OCTOS_CORE_TEST_KERNEL=<带补丁的二进制>` 和
`cargo test --locked -p octosense-kernel --test real_kernel -- --nocapture`，覆盖认证、
来源拒绝、原生与浏览器共享对话、模型重启、令牌隐藏和关闭，不调用外部模型。
app-peer 的真实内核测试也使用新传输。

见 [ADR 0003（英文）](../../docs/adr/0003-shared-octos-client-access.md)。
本次修改的 Android APK 打包及真机行为仍**未验证**。
