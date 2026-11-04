# 通过 GitHub 发布目录

[English](GITHUB-PUBLISHING.md) | 简体中文

App Hub 仓库管理员批准目录的确切摘要。GitHub Actions 使用短期身份
证书为这些字节签名，并把签名记录到公开透明日志中。管理员不需要
保存额外的 Hub 私钥。这是 GitHub 管理的制品签名，不是 SSH 签名，
也不是 commit 上的绿色 Verified 标记。

**交付状态**：[生产工作流运行 37736098082](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37736098082) 已在 commit `27eeec5b3abc3f9c2aa06a3894bab2f15228f116` 发布序号 11 的签名目录。此后的运行又发布了序号 12 到 15 的签名目录：序号 13 由[工作流运行 37755718288](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37755718288) 在 commit `3842c5ec503a8e9124cbbe99655556ffe24c41e1` 发布；序号 14 由[工作流运行 37852340163](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37852340163) 在 commit `14135444bb2ded83cee3458553287aa60387f795` 发布，撤回了六个用密钥签名的 `org.octosense.samples.*` 条目；序号 15 由[工作流运行 37902582113](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37902582113) 在 commit `18cd41d91b326db199fbed4129484a9ba1a8c63d` 发布，新增 GitHub Notes 0.2.2。本源码让新兼容宿主默认选择 GitHub 渠道；2026-10-08 推出的 OctoSense 桌面版 0.1.0-rc.1 和 2026-10-09 推出的 [0.1.0-rc.2](../README.zh-CN.md#下载兼容宿主) 都是兼容的宿主发行版，默认读取 `catalog-v2.json`。已安装的旧宿主不受影响，参考应用的应用包字节也保持不变。序号 11 的证明和原生消费者检查结果见[验证](#验证)。

## 谁能批准发布

只有当前仓库的 **admin** 可以在 `main` 上手动启动或重新运行
[`publish-catalog.yml`](../.github/workflows/publish-catalog.yml)。工作流
检查原发起者的数字身份，以及原发起者和重跑者当前是否都是管理员。
拥有写权限的协作者或普通应用发布者不能通过此工作流签署目录。

启用发布前，为 GitHub 的 `app-hub-catalog` 环境设置：仅允许从
`main` 部署、要求 App Hub 管理员审核、禁止绕过保护。管理员成员
变化时，同步更新审核人。用仓库审核规则保护工作流和发布工具的修改。
原生验证器要求签名证书中包含正确的环境声明；修改工作流并删除环境
不能绕过此检查。

批准的对象是完整 commit、候选名称和准备好的 SHA-256，不是应用
名称或可变分支。管理员仍需完成[常规准入审核](SUBMITTING.zh-CN.md#8-审核检查什么)。

## 准备待审核候选

把候选保存在 `main` 上可审核的 commit 中：

```text
catalog-candidates/<name>/catalog.json
catalog-candidates/<name>/artifacts/<app>-<version>.bundle/...
catalog-candidates/<name>/artifacts/<app>-<version>.bundle.pack.json
catalog-candidates/<name>/index/<app>-<version>.json
catalog-candidates/<name>/artifacts/<component>-<version>.wasm
catalog-candidates/<name>/index/components/<component>-<version>.json
```

候选 `catalog.json` 是未签名载荷：schema 为 1，不带 `key` 或
`signature`，sequence 比已验证的基准恰好大 1，发布日期为当天
UTC 日期。保留全部历史条目。已有条目只能从 offered 改为带理由的
withdrawn；不能删除历史、替换既有字节或重新上架已撤回版本。
仅刷新有效期的更新不需要新增制品文件。

对于新版本，附上审核时的原样应用包、对应 pack 和 index。
`hub catalog-prepare` 重新运行准入和发布者连续性检查，比较每个
派生条目和打包文件。

共享组件（[ADR 0003](adr/0003-shared-components.zh-CN.md)）记录在签名目录的
`components` 列表中，遵守相同的历史规则。对于新的组件版本，附上审核时的
`.wasm` 文件和 `hub component-entry` 写出的 index 条目。`hub catalog-prepare`
在要求 GitHub 来源证明的前提下重新运行组件的准入检查，把重建的条目与候选
中的条目和 index 比较，并在回执中为每个新组件版本列出
`{"component", "index"}`。同一份候选中的新应用可以固定其中的新组件。工作流仅把候选解压为数据，不运行其中的脚本
或构建命令。

[准备原生工作区](FIRST-APP.zh-CN.md#1-准备工具和应用仓库)后：

```sh
cargo build --locked --release -p octosense-app-hub
# 使用当前已验证的 v2 目录作为基准。
target/release/hub catalog-prepare --base catalog-v2.json \
  --candidate catalog-candidates/<name>/catalog.json \
  --artifact-root catalog-candidates/<name> \
  --out /tmp/catalog-v2.payload.json
```

记录 JSON 回执中的 `payload_sha256` 和候选完整的 40 位 commit。
批准摘要之前，审核生成的载荷及所有新增文件。基准改变或 UTC 日期
跨天时重新准备。命令本身有原生集成测试；上面的生产派发是该已审核候选的独立证据。

## 签名、验证和发布

打开 **Actions → Publish catalog → Run workflow**，选择 `main`，
填写候选名称、完整 commit 和确切的 `payload_sha256`。先使用
`dry_run: true`，再用管理员账号批准受保护环境。

第一个任务验证管理员身份、构建可信工具、验证候选，并把准备包绑定
到审核的摘要。受环境保护的任务使用 GitHub OIDC 和 Sigstore 签署
`catalog-v2.payload.json`，随后用原生验证器验证返回的证明。
试运行把验证后的 `catalog-v2.json` 信封和回执保留为工作流制品，
不改变公开目录。

消费者验收该证明后，以 `dry_run: false` 运行同一审核事务。工作流
只提交信封及新增的准入制品和 index。非强制 push 必须从已检查的
确切 main commit 向前推进。遇到并发修改时停止发布，不自动 rebase
或重试。再次派发需要对照新基准审核。旧 `catalog.json` 保持原样。

## OctoSense 验证什么

原生验证器检查载荷的确切 SHA-256、证书链、证书透明证明、已签名
日志条目及包含证明、GitHub 发行者、App Hub 仓库和所有者数字 ID、
工作流路径、`main` 引用、手动启动事件、GitHub 托管 runner 和受保护
环境。签名声明必须描述确切的目录对象和获准工作流。网络镜像无法用
其他仓库或工作流替换它。

新兼容宿主默认选择 `github-v2`，无需额外设置
`OCTOSENSE_HUB_CATALOG=github-v2`。自定义旧格式测试 Hub 必须显式设置
`OCTOSENSE_HUB_CATALOG=legacy`，并使用新的应用库。只要已有 v2 缓存，即使
损坏，显式选择旧格式也会拒绝继续。

旧 `catalog.json` 缓存不会离线转换成可信 v2 数据。第一次加载 v2 需要网络，
或者显式提供 v2 镜像，不能回退使用旧缓存。参考应用的包字节和发布者签名保持不变。

宿主在拉取之前选定 v2 客户端模式。它拒绝旧格式或畸形文档，
不自动回退。缓存必须原子保存整个信封及证明，并在重启后保留最大
已接受序号。相同序号但内容不同也会被拒绝。现有 14 天有效期限制
仍约束新安装。

## 迁移与应用发布者

旧发布版只理解信任锚签署的 `catalog.json`，需要更新宿主才能读取
`catalog-v2.json`；两个文件及缓存名称保持独立。改变默认值不会更新已经安装的
宿主，也不会转换它的离线旧缓存。

目录签名与应用发布者证明是不同边界。新应用使用 [GitHub 发布工作流](PUBLISHING.zh-CN.md#签名)，
无需开发者密钥；日常更新保留经过验证的仓库、所有者和工作流身份。既有参考条目
及签名作为历史版本保留。开提交 issue 仍是发布请求，目录发布仍由管理员批准控制。

## 验证

[生产回执](evidence/catalog-v2-production-37736098082.json)记录了对不可变公开信封的
独立拉取：59,745 字节，SHA-256 为
`90576462177341de69eae4737f1e36bd16efce5e474223c1f74182b4b150d5e2`，与 Actions
产物一致。目录序号为 11，载荷 SHA-256 为
`d7a43c63ca3219691f0879b3637aef7088963c579812228a9afa82967437de0c`。

| 原生消费者 | 确切 App Hub 源码 | 结果 |
| --- | --- | --- |
| macOS CLI | `af0cf7f3c8e4e8eb91829bd8ac44219508ba7f0c` | 6/6 通过 |
| OnePlus 6 上的 Android arm64 CLI | `6e3b8ffefff018269efb45f7d2c9b08fe75c4d74` | 6/6 通过 |

两者均接受真实生产证明，拒绝载荷篡改、签名篡改、透明日志篡改、缺失日志和旧格式
文档。回执包含各二进制摘要及完成清理的记录。这是所记录源码的原生证明验证，
不是应用安装、GUI/账户验收，也不是对之后这个默认渠道构建的测试。

运行 `python3 -m unittest discover -s tools -p test_catalog_publish.py`，
检查管理员授权、输入和路径拒绝、准备包的确切绑定，以及真实 Git
并发更新拒绝。PR 工作流运行这些测试时没有签名或写权限。
原生证明、防篡改、准入、防回退和缓存测试位于 `crates/app-hub`。
正式证明和设备验收结果应与这些自动检查分开记录。
