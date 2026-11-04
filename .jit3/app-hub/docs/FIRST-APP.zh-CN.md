# 开发你的第一个 Hub 应用

[English](FIRST-APP.md) | 简体中文

你将从模板起步，最终得到一个未签名的应用包：它能在 `card-host` 中运行，并能通过准入检查（由 `hub check` 执行）。之后，[向 App Hub 提交应用](SUBMITTING.zh-CN.md)会带它走完提交的四个阶段，从请求发布到正式发布。

Hub 应用分为两类，提交方式相同：

| 类型 | 入口文件 | 说明 |
| --- | --- | --- |
| 脚本应用 | `main.splash` | 一个 Splash 程序，有自己的状态、处理函数、存储和请求。 |
| 卡片应用 | `page.card` | 一张 L0 卡片，附带自己的数据和 kit（渲染这张卡片的控件套件）。宿主把卡片转换为套件里的控件。卡片本身没有逻辑。 |

原生应用则随 Shell 版本发布（[选择合适的交付路径](DEVELOPMENT.zh-CN.md#选择合适的交付路径)）。

## 1. 准备工具和应用仓库

1. 按[快速上手第 1 节](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.zh-CN.md#1-前置条件)的说明搭建工作区 `~/octosense-ws`：把 App Hub 和 OctoSense App Flow（原 Design Flow）都克隆到这个目录下，然后运行 App Flow 的 `tools/setup-native.py`。这个脚本会添加 `makepad`、`octoscript-makepad` 和 `octoscript` 三个检出目录，App Hub 的 `Cargo.toml` 以补丁方式引入它们。

   ```sh
   cd ~/octosense-ws/OctoSense-App-Flow
   python3 tools/setup-native.py
   python3 tools/setup-native.py --check
   ```

   成功时，`--check` 以退出码 0 退出，并输出锁定的版本。如果某个检出目录不在锁定的版本上，或者有本地改动，`--check` 会报错停止：`RuntimeError: …/makepad differs from the unified runtime source lock`。如果检出目录只是版本不同，运行 `python3 tools/setup-native.py --update`，它会把没有本地改动的检出目录切换到锁定的版本。如果检出目录有本地改动，保留这些改动，按同样的方法另建一个工作区，例如 `~/octosense-ws2`。

2. 构建 `hub` 和 `card-host`：

   ```sh
   cd ~/octosense-ws/OctoSense-App-Hub
   cargo build --release -p octosense-card-host -p octosense-app-hub
   ```

   成功时，输出以 ``Finished `release` profile [optimized] target(s)`` 结尾。生成的可执行文件位于 `target/release/`；如果设置了 `$CARGO_TARGET_DIR`，则位于 `$CARGO_TARGET_DIR/release/`。如果构建失败并报 `no variant … TextInputStateQuery`，请看 [`card-host` 构建失败](DEVELOPMENT.zh-CN.md#card-host-构建失败)。

3. 把两个工具加入 `PATH`，然后检查 `hub`：

   ```sh
   export PATH="${CARGO_TARGET_DIR:-$PWD/target}/release:$PATH"
   hub
   ```

   成功时输出用法，第一行是 `hub — the OctoSense app hub command (ADR 0003)`。这条 `export` 只对当前 shell 有效；要长期生效，把这一行写入 shell 配置文件，并把其中的路径换成绝对路径。

4. 现在就确定应用 ID，以后不能再改。ID 由 1 到 64 个 `[a-z0-9.-]` 字符组成，不能以 `os.` 开头，最后一段不能是[保留名称](PUBLISHING.zh-CN.md#id-与保留名称)。详见[第 3 节](#3-选择-id能力和素材)。然后在一个新目录中，从模板创建应用仓库：

   - **脚本应用**：运行 App Flow 的 `tools/octo new`。用 `--platform` 指定每个要测试的平台；有多个平台时，重复这个参数。

     ```sh
     cd ~/octosense-ws/OctoSense-App-Flow
     tools/octo new ~/apps/my-app --platform macos --id com.example.mynotes --name "My Notes"
     ```

     ```text
     created …/apps/my-app
       id com.example.mynotes, name 'My Notes', version 0.1.0, bundle stamped
       target platforms: macos; verify each before publishing
     ```

     App Flow 的 [`templates/script-app/`](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/templates/script-app) 是一个小型笔记应用，带有清单、商店信息和图标。`tools/octo new` 复制它的 `bundle/`，添加 `AGENTS.md`、`CLAUDE.md`、`GEMINI.md` 和 `.gitignore`，并把 `--platform` 的值写入商店信息的 `platforms`。然后为应用包写入摘要，即把摘要记录到 `manifest.json` 中。在创建任何文件之前，它就会拒绝以 `os.` 开头的 ID（除非传入 `--system`），也会拒绝最后一段是保留名称的 ID：`octo: id 'com.example.notes' uses reserved native/host namespace 'notes'; choose an app-specific name`。

   - **卡片应用**：复制 App Hub 的[应用起步模板](../templates/app/README.zh-CN.md)：

     ```sh
     mkdir -p ~/apps && test ! -e ~/apps/my-app && cp -R ~/octosense-ws/OctoSense-App-Hub/templates/app ~/apps/my-app
     ```

     模板只有元数据和一个示例图标，有意不带 `page.card`、kit 和截图。在你补上这些文件之前，准入检查会拒绝它（[第 5 节](#5-对应用包运行准入检查)）。

   - **已有仓库**：手动合并模板文件，并保留仓库原有的指引和元数据。

   提交的只有 `bundle/`。`AGENTS.md`、设计提示词、源码与工具、日志、密钥和测试状态都放在 `bundle/` 之外（[安排仓库结构](SUBMITTING.zh-CN.md#1-安排仓库结构)）。

5. 把这个目录初始化为 Git 仓库：

   ```sh
   cd ~/apps/my-app && git init
   ```

## 2. 设计并构建应用

先写一份简短的需求说明，包括应用的用途、界面、操作、数据源、要保存的状态，以及错误状态和空状态。

### 脚本应用

按照 App Flow 的[脚本应用流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/script-app/FLOW.md)（英文）开发。程序可以调用的接口（`fs`、`net`、`host.request`、控件句柄）见[脚本 API](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/SCRIPT-API.md)（英文）。[系统应用](https://github.com/OctoSense-org/OctoSense/tree/main/apps)是同一格式的完整示例。

```text
my-app/
  AGENTS.md
  README.md
  bundle/
    manifest.json
    listing.json
    main.splash          # 程序本身；有了它，这个应用包就是脚本应用
    assets/              # 本地素材和你的图标
    screenshots/
      01-main.png        # 真实截图，在第 4 节添加
```

- 引用应用包自带的素材时，使用 `{{assets}}`，例如 `Image{src: http_resource("{{assets}}/assets/logo.png")}`。宿主会把 `{{assets}}` 替换为一个本地回环地址，这个地址只提供本应用包的内容。不要自己写这个地址，也不要写 `file://` 路径或 `../` 路径。
- 在 `main.splash` 中使用 HTTPS，并在 `network.hosts` 中准确披露预计访问的目的地。当前策略不把这份列表作为允许访问的白名单；准入检查仍拒绝 `http://`。
- 不要让用户输入密码、PIN 或一次性验证码。准入检查会拒绝这类输入框，即使到了运行时，它们也不接受输入。登录由宿主服务负责，它在自己的面板上收集这些机密信息。面板是宿主覆盖在应用之上绘制的界面（详见[面板](PUBLISHING.zh-CN.md#面板应用从不收集机密信息)）。
- 除了用于发现宿主 API 的 `runtime`，`card-host` 不提供任何宿主服务：其他 `host.request` 都只会得到 `no service answers "<family>" on this device`。用到宿主服务的界面，请在 [兼容的桌面发行版](../README.zh-CN.md#下载兼容宿主) 中测试（[开始之前](SUBMITTING.zh-CN.md#开始之前)）。

### 卡片应用

用 App Flow 的[图像到卡片流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/image-to-card/FLOW.md)（英文）创建并评审卡片。应用的数据绑定仍需你自己实现：截图或流程产出的界面图集，都不是能运行的应用。[L0 参考文档](https://github.com/OctoSense-org/OctoSense/blob/main/apps/appcard/a2app-l0/framework/l0.md)（英文）定义了数据、状态和事件。

```text
my-app/
  AGENTS.md
  README.md
  bundle/
    manifest.json
    listing.json
    page.card
    page.data.json       # 可选；卡片不需要数据时省略
    kit/                 # 这张卡片的 kit 需要的所有文件
    assets/              # 运行时用到的本地素材和你的图标
    screenshots/
      01-main.png        # 真实截图，在第 4 节添加
```

卡片的 `font_src` 可以引用应用包中的字体文件，或三种已随运行时提供的字体：Inter、LXGW WenKai Regular 和 LXGW WenKai Bold。历史 OctoSense 桌面版 0.1.0-beta.2 不加载打包字体。设置字体或显示中文之前，请先阅读[字体](PUBLISHING.zh-CN.md#字体)。

### 两类应用都适用

- 所有素材引用都只能指向应用包内的文件。检查导出的 kit 文件和数据文件，确保其中没有作者本机的路径，也没有开发服务器的 URL。
- 开发笔记放在 `bundle/` 之外；所用素材要求保留的许可证和来源说明应以 `.txt` 或 `.md` 随包提供。文档链接不会授予网络权限，卡片数据中的 URL 仍受素材检查约束。应用 Agent 的文件与 `main.splash` 一样，拒绝不安全的引用（`http://`、`file://`、`../`），但不要求 HTTPS 主机出现在 `network.hosts` 中。
- 如果某个流程依赖外部的 Python 或浏览器控制器，提交之前先改造它，让它能在应用的隔离环境中运行。

## 3. 选择 ID、能力和素材

编辑 `bundle/manifest.json`：

- **`id`** 永久不变：由 1 到 64 个 `[a-z0-9.-]` 字符组成。准入检查会拒绝以 `os.` 开头的 ID，也会拒绝最后一段是保留名称的 ID。[保留名称](PUBLISHING.zh-CN.md#id-与保留名称)共 23 个，例如 `notes`、`weather` 和 `terminal`。如果应用以后可能附带工具，最后一段要能用作工具命名空间，即符合 `[a-z0-9_]{1,24}`：用 `com.example.mynotes`，而不是 `com.example.my-notes`。
- **`version`** 从 `0.1.0` 开始，每次发布都要用新的版本号。
- **`capabilities`** 只列出界面实际用到的能力。查看[能力](PUBLISHING.zh-CN.md#能力)一节的**目前由谁提供**列：有些能力（例如 `llm` 和 `calendar`）能通过准入检查，但只向系统应用提供；`prompt` 则没有任何宿主提供。请在 `network.hosts` 中披露预计访问的目的地；这两个字段都不会授权访问。兼容的公开 API 仍检查应用及账户范围、用户同意和资源上限。

把 `bundle/listing.json` 中的占位内容全部换掉：`subtitle`、`description`、`category`、`publisher`（包括 `support` 和 `privacy_policy_url`）、`platforms`、`release_notes` 和 `license`。模板中的 `example.com` URL 都是占位内容。App Hub 的应用起步模板声明了 `"platforms": ["macos"]`，`tools/octo new` 写入的是你指定的平台；两者都不代表测试结果。只声明你实际运行过应用的平台。

按照[应用图标与随包素材](ICONS.zh-CN.md)替换示例图标。商店信息中的图标路径必须与导出的文件一致。

## 4. 运行未签名的开发版应用包并截图

可编辑的应用包始终保持未签名；推送标签时，Release 工作流会封存它的一份副本。`card-host` 没有发布者验证器，所以即使加了 `--allow-unsigned`，它也会拒绝已封存的 Release。

1. 为应用包写入摘要：`hub stamp` 计算应用包的摘要，并写进 `manifest.json`。应用包每次改动之后都要做这一步。

   ```sh
   cd ~/apps/my-app
   hub stamp bundle
   ```

   成功时输出应用包摘要，共 64 个十六进制字符。如果输出 `hub: manifest is not valid: unknown field …`，删除消息中指出的字段。

2. 在 App Hub 检出目录中用 `card-host` 启动应用，与 `tools/octo run` 的做法相同。`MAKEPAD_REMOTE=8151` 在 8151 端口上开启远程控制路由，供第 3 到 5 步使用。

   ```sh
   cd ~/octosense-ws/OctoSense-App-Hub
   MAKEPAD_REMOTE=8151 card-host --bundle ~/apps/my-app/bundle \
     --app-data ~/apps/my-app/.local-state --allow-unsigned &
   ```

   成功时，日志显示应用已准入，窗口中显示应用界面。

   ```text
   card-host: com.example.mynotes 0.1.0 admitted — capabilities {"storage"}, hosts {}, storage 16777216 bytes, agent none
   ```

   `card-host` 拒绝应用包时，日志显示 `card-host: refused: <reason>`，窗口显示“card-host refused this bundle”及原因。修复原因中指出的问题，重新写入摘要，再重启 `card-host`。

3. 通过远程控制路由把每个界面都操作一遍。`/snap` 列出可见控件及其矩形和文本；`/click`、`/t` 和 `/k` 发送输入（见 [`card-host` 参考](DEVELOPMENT.zh-CN.md#在本地运行应用包card-host)）。

   ```sh
   curl --silent 127.0.0.1:8151/snap
   ```

   成功时输出一行 JSON，开头是 `{"s":[{"i":"main_window","ty":"Window","r":[0,0,412,892],…`。查看日志中 `admitted` 行之后有没有错误。要用脚本发送输入，见[原生测试工具指南](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/core/NATIVE-INSTRUMENT.md)（英文）。

4. 截取一帧画面：

   ```sh
   mkdir -p ~/apps/my-app/bundle/screenshots
   curl --fail --silent --show-error -o ~/apps/my-app/bundle/screenshots/01-main.png '127.0.0.1:8151/g?raw=1'
   ```

   成功时，`01-main.png` 是一张与窗口像素尺寸相同的 PNG：在 2× 显示屏上，默认的 412 × 892 窗口对应 824 × 1784。如果截取失败并报 `curl: (22) The requested URL returned error: 404`，说明第一帧还没有绘制出来；等几秒再截取一次。

5. 打开 PNG 检查画面，不要提交显示错误的截图。然后退出 `card-host`：

   ```sh
   curl --silent 127.0.0.1:8151/quit
   ```

   成功时，`quit` 返回 `{"ok":1}`。

需要宿主服务的界面如何截取，见[截图](SUBMITTING.zh-CN.md#4-截图)一节。

## 5. 对应用包运行准入检查

截图改变了应用包，所以先重新写入摘要，再对未签名的应用包运行准入检查：

```sh
cd ~/apps/my-app
hub stamp bundle
hub check bundle --allow-unsigned
```

较早 CLI 的历史输出（当前 CLI 将概要标为 `declarations:`，不再使用 `grants:`）：

```text
com.example.mynotes 0.1.0 — PASSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  grants: capabilities {"storage"}, hosts {}, storage 16777216 bytes, agent none
```

这个阶段出现未签名警告是正常的。请对照实际用途核对当前的 `declarations:` 行，补齐遗漏或删除未使用的声明。每个准入应用都有带配额的私有隔离目录，即使未声明 `storage` 也是如此；下方旧输出中的 `storage none` 不再描述当前策略。

结果为 `REFUSED` 时，`hub check` 以退出码 1 退出。逐条修复 `[refused]` 行（[常见拒绝原因及修复](SUBMITTING.zh-CN.md#常见拒绝原因及修复)），然后重新写入摘要并再次检查。未经修改的应用起步模板会得到两项拒绝，分别针对入口文件和截图：

```text
my-app 0.1.0 — REFUSED
  [refused] entry (main.splash): the bundle has no entry: main.splash (a script app) or page.card (a card)
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] listing: screenshots/01-main.png is named by the listing but is not in the bundle
  grants: capabilities {}, hosts {}, storage none, agent none
hub: the bundle was refused
```

`PASSED` 表示应用包符合准入规则。`hub check` 不运行应用，发现不了占位文字，也不检查你的隐私政策。

## 6. 生成 Release 并提交

接下来按[向 App Hub 提交应用](SUBMITTING.zh-CN.md)的第 5 到 9 步继续：

| 步骤 | 要做的事 |
| --- | --- |
| [5. 生成最终字节](SUBMITTING.zh-CN.md#5-生成最终字节) | 扫描应用包，回答审核问题，并加入 GitHub Release 工作流。 |
| [6. 冻结并验证发布](SUBMITTING.zh-CN.md#6-冻结并验证发布) | commit 后推送新标签，再验证工作流生成的 Release pack。 |
| [7. 开提交 issue](SUBMITTING.zh-CN.md#7-开提交-issue) | 请求发布，然后补充标签、完整的 commit SHA、Release pack 和验证输出。 |
| [8. 审核检查什么](SUBMITTING.zh-CN.md#8-审核检查什么) | 了解审核人员检查什么；发现的问题会出现在你的 issue 中。 |
| [9. 提交之后](SUBMITTING.zh-CN.md#9-提交之后) | 了解 App Hub 管理员如何批准、Hub 如何发布，并以新版本推出更新。 |

整个过程都要遵守两条规则：

- **测试可编辑源码，绝不修改已封存的 Release。** 工作流会封存它放进 Release 的字节；任何改动都需要新版本和新标签（[GitHub 发布者来源证明](PUBLISHING.zh-CN.md#github-发布者来源证明)）。
- **你不需要发布者密钥。** App Hub 只接受带 GitHub 证明的 Release，而应用的 GitHub 工作流会为每个 Release 生成证明（[签名](PUBLISHING.zh-CN.md#签名)）。

要在 App Hub 发布之前用 OctoSense 桌面版试用某个 Release，请用从源码构建的 Shell，从本地测试签名目录安装它（[在本地演练商店流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/PUBLISHING.zh-CN.md#4-在本地演练商店流程)）。未验证：用带 GitHub 证明的 Release 进行演练；已记录的结果来自一个用密钥签名的测试应用。

## 故障排查

| 现象 | 原因与修复 |
| --- | --- |
| `cargo build` 停止并报 `no variant … TextInputStateQuery` | 见 [`card-host` 构建失败](DEVELOPMENT.zh-CN.md#card-host-构建失败)。 |
| `card-host: refused: this host has no GitHub publisher verifier` | 应用包是已封存的 Release。请运行可编辑源码。 |
| `no service answers "<family>" on this device` | 除了用于发现宿主 API 的 `runtime`，`card-host` 不提供任何宿主服务。在 [兼容的桌面发行版](../README.zh-CN.md#下载兼容宿主) 中测试这个界面。 |
| 请求 `/g?raw=1` 时报 `curl: (22) The requested URL returned error: 404` | 还没有绘制出任何一帧。等几秒再截取一次。 |
| `[refused] identity: app id "…" ends in "…", which is reserved` | ID 的最后一段是[保留名称](PUBLISHING.zh-CN.md#id-与保留名称)。在首次发布之前换一个 ID。 |
| `[refused] digest: the bundle hashes to …, the manifest claims …` | 应用包在 `hub stamp` 之后有改动。重新写入摘要。 |
