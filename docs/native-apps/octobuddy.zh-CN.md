# OctoBuddy 在 `native-apps.json` 里的条目

[English](octobuddy.md) | 简体中文

[OctoBuddy](https://github.com/tyreseluo/OctoBuddy) 是编码应用：外环把工作拆成切片，inner 并行实现，宿主检查验收。它的条目申请的权限比其他任何模块都多，所以本页逐条说明：每项授权用来做什么、受什么约束、拿掉会怎样。OctoBuddy 保存和发送哪些数据，见它自己的 [PRIVACY.zh-CN.md](https://github.com/tyreseluo/OctoBuddy/blob/main/PRIVACY.zh-CN.md)。

## 运行在哪里

- **各平台都是模块**（`hosting`），桌面 shell 默认编入，手机 shell 不编（`shells`）。
- **在 shell 的进程里。** `sandbox` 块只作用于进程启动（`sandbox::Policy::for_app`，ADR 0004 §3），所以目前没有 OS 沙箱约束 OctoBuddy：shell 能做的它都能做，Rinx、App Hub、AppCard 也一样。这个块写的是它实际做的事，也是它的 `hosting` 将来改成 `process` 时会拿到的策略。

## 各项授权

| 授权 | 用途 | 约束 | 拿掉会怎样 |
| --- | --- | --- | --- |
| `sandbox.network: "any"` | 用户启用的模型 provider（经 OctoBuddy 的 loopback 代理，由代理加 key）、agent 的下载（npm registry、octos 的 GitHub release，都按钉死的摘要校验）、应用模板仓库。 | 只连用户的 provider 和 OctoBuddy 钉死的来源；没有遥测（见 PRIVACY）。 | 模型无法回答，agent 无法安装。 |
| `sandbox.processes: true` | 双环就是 OctoBuddy 启动的 agent 程序：`claude -p`、`codex app-server`、`pi --mode rpc`、它自己的 `octos serve`；还有 `git` 和项目自己的检查，宿主在验收切片前运行。 | 子进程只拿到占位 key，拿不到 provider 的真 key；agent 在用户选定的项目目录（或其 git worktree）里工作。 | 没有外环和 inner，OctoBuddy 只剩聊天。 |
| `storage.agent_workspace: "account"` | 它的 app agent（`octos.*` 服务，系统 agent 的 peer）在 OctoBuddy 的账户目录里工作，与其他带文件的 app agent 相同（ADR 0004 §11）。 | 只限这个目录；request context 只看到 `contexts/<id>/`。`storage.accounts: false`：整台设备一个目录。 | app agent 不能读文件。 |
| `agent.octos`：`session.open`、`session.history`、`turn.start`、`turn.interrupt` | 在 OctoSense 自己的 agent 上聊天，以及系统 agent 与之对话的 peer。 | 与 Rinx 相同的四个服务。 | 不能用 OctoSense 的 agent 聊天。 |
| `agent.generic_tools`（12 个） | 聊天时它的 agent 拿到的 octos 内核工具：文件（`read_file`、`write_file`、`edit_file`、`glob`、`grep`、`list_dir`）、网页（`web_search`、`web_fetch`）、`view_image`、`ask_user_question`、`memory_search`、`save_memory`。 | 限于它的账户目录；永远拿不到 octos 的 shell（`tools/native_apps.py` 对任何 app 都拒绝）。 | 聊天 agent 只能对话。 |
| `agent.grants`：`terminal.run` | 聊天 agent 运行命令的唯一途径：在终端里跑，用户看得见。 | `terminal.run` 属于 `destructive`，由宿主确认；没有常设规则能代答（`auto_approvable: false`）：每条命令都要用户当场确认。只在终端作为沙箱进程运行的地方提供（ADR 0004 §10、§12）。双环不使用它。 | 聊天 agent 不能运行命令；双环不受影响。 |
| `agent.tools`：`octobuddy.status`（读）、`octobuddy.send`、`octobuddy.report`、`octobuddy.request`（写） | 系统 agent 能向 OctoBuddy 要的东西：当前在跑什么、给某个会话的外环发消息、报告它发布的应用出了问题、请求做一个新应用。 | `status` 可共享且只读。三个写工具由宿主确认：每次调用都要用户确认。`request` 只是排队：用户按下 Build 才会开始构建。 | 系统 agent 看不到、也调不动 OctoBuddy。 |

## 为什么不能更窄

- **`network: "any"` 而不是白名单。** provider 是用户自己的：OctoSense 的 AI providers 应用允许添加任意 endpoint，审核条目时无法预知主机。
- **inner 跑在 OctoBuddy 自己的 `octos serve` 上，而不是 shell 的内核。** app 的 peer 目前还不能在用户选的目录里工作、不能让多个 loop 共享目录、不能运行命令、不能被 steer。OctoBuddy 的 [`docs/upcr-draft-coding-peers.md`](https://github.com/tyreseluo/OctoBuddy/blob/main/docs/upcr-draft-coding-peers.md) 起草了内核需要补的能力；有了这些，`processes` 可以收窄到只跑检查。
- **`terminal.run`** 拿掉也不影响双环；保留它，是为了让 OctoSense agent 上的聊天能运行用户看得见、并逐条确认的命令。

## 怎么改

条目钉在 OctoBuddy 的某个 revision（`source.rev`）。升级就是改 `native-apps.json` 再跑 `python3 tools/native_apps.py`；改任何一项授权都在这里审核，并同步修改本页。
