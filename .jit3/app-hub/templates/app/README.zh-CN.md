# Hub 应用起步模板

[English](README.md) | 简体中文

按照 [开发你的第一个 Hub 应用](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/FIRST-APP.zh-CN.md)，把这个目录复制到新的应用仓库中。这是一个卡片应用的**元数据脚手架**，不是可运行或可发布的演示。要开发脚本应用（`main.splash`），请改用 [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)（原 Design Flow）的 `tools/octo new`，不要复制这个目录。

模板中的文件分为两组：

| 位置 | 文件 |
| --- | --- |
| `bundle/` 内（发布产物） | `manifest.json`（schema 1）、字段齐全的 `listing.json`，以及示例图标 `assets/icon.svg` |
| `bundle/` 外 | `README.md` 和 `README.zh-CN.md`；`AGENTS.md` 及 `CLAUDE.md`、`GEMINI.md`；`.gitignore`；`.gitattributes`（让 Git 原样保留 `bundle/` 的字节） |

提交之前：

1. 替换 `manifest.json` 中的应用 id 和名称，以及 `listing.json` 中的所有示例值（包括发布者信息）。
2. 把 `listing.json` 中的 `platforms` 设为你实际测试过的平台。模板声明的是 `["macos"]`；只有在 macOS 上测试过，才保留它。
3. 把 `listing.json` 中的 `license` 设为应用实际使用的许可证。模板中的 `Apache-2.0` 只是示例。
4. 用你的应用图标替换 `bundle/assets/icon.svg`。
5. 使用 [图像到卡片流程](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/image-to-card/FLOW.md)，生成并评审 `bundle/page.card`、可选的 `page.data.json`、卡片的 `kit/` 目录和本地素材。
6. 在 [`card-host`](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/DEVELOPMENT.zh-CN.md#在本地运行应用包card-host) 中运行未签名的应用包，并截取 `bundle/screenshots/01-main.png`。
7. 按照 [向 App Hub 提交应用](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/SUBMITTING.zh-CN.md) 的说明，为完成后的 `bundle/` 目录写入摘要，再检查和评审，然后通过 App Flow 的 `tools/octo publish-github` 安装的 GitHub 工作流生成 Release 并提交。

清单中的初始摘要只是占位符，`hub stamp` 会写入真实值。模板既没有 `page.card`，也没有截图，所以只写入摘要、不做其他修改的副本，在准入检查中会得到两项拒绝：

```sh
hub stamp bundle
hub check bundle --allow-unsigned
```

```text
my-app 0.1.0 — REFUSED
  [refused] entry (main.splash): the bundle has no entry: main.splash (a script app) or page.card (a card)
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] listing: screenshots/01-main.png is named by the listing but is not in the bundle
  grants: capabilities {}, hosts {}, storage none, agent none
hub: the bundle was refused
```

不要为了通过检查而加入占位卡片或截图。准入检查不能替代原生渲染和输入测试。

除应用本身以外，其他内容都放在 `bundle/` 之外：本 README、`AGENTS.md`、密钥、工具、构建产物、应用数据，以及 `hub scan` 生成的审核包。
