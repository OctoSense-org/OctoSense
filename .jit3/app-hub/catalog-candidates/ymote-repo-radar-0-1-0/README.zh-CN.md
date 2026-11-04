# Repo Radar 0.1.0 目录候选

[English](README.md) | 简体中文

**此目录仅为草稿候选，尚不可正式发布。** 它不会修改公开目录、安装应用或批准发布。
[提交问题 #202](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/202)
请求发布 `org.ymote.reporadar` 0.1.0，来自
[ymote/octosense-repo-radar v0.1.0](https://github.com/ymote/octosense-repo-radar/releases/tag/v0.1.0)，
源码提交为 `ec202f7d6c8a192a80353a1640c19786db3f77e1`。

## 已核验的准确字节

[发布工作流 38088072936](https://github.com/ymote/octosense-repo-radar/actions/runs/38088072936)
构建并证明了该版本。下载的 pack 按原样复制：

- Pack SHA-256：`54ed1fe4fc2d885defe5fe9f8a507f3ac29104629d9c10e4b0f597de35b63f83`。
- Bundle BLAKE3：`90463f61990f62f1610bbcd9e3be76b7ca4bd046c05c9f2c222cbd92267bf136`。
- 已封装 Rust 组件 SHA-256：`d0ee1835711ed4b1c952f564789d96c160d02035b1d2104db1388e6affdbc757`。

原生验证器通过了序号 15 基准目录的认证、发布者证明、bundle 准入、首次发布者身份、
候选条目和完整目录载荷检查。`review.json` 与 `prepare-receipt.json` 记录了这些绑定。
没有生成开发者签名密钥，也没有重新 stamp 或修改已封装的发布文件。

## 功能与验收边界

Repo Radar 搜索公开 GitHub 项目，在应用私有目录保存关注列表，用原生 D3 图表展示
有限范围的近期 PR 和版本数据，并向 Glance 发布项目摘要。可选助手使用九个 Rust 工具；
助手取消固定项目须由宿主确认。无需 GitHub 账号。
商店清单目前仅支持 macOS，并要求 `charts.d3@1`。

开发宿主的完整原生场景通过了 94 步，包括真实公开 HTTPS、图表绘制、固定／选择／排序、
Glance 发布及重启持久化。**该场景使用本地构建的组件。** GitHub 重新构建了
`fns/radar.wasm`；上面列出的已封装组件仍须完成执行验收。
源码与发布包仅在重建组件和已证明的 manifest 上不同。RC4 缺少 `charts.d3@1`，会正确拒绝安装。
正式发布前必须提供兼容的公开宿主，验证已封装组件执行，并完成发布宿主的安装验收。
真实模型回答、升级和其他平台仍未验证。

## 候选事务

这是基于已认证公开序号 15 的独立序号 16 提案，不修改已有共享组件的序号 16 演练候选。
新载荷按原样保留全部历史条目。若其他候选先发布，或 UTC 日期变化，必须针对新的基准
重新准备本候选；不得覆盖或直接复用其他候选的事务。

已准备的 `catalog-v2.payload.json` SHA-256：

```
83867e6f8197dff3d7fb630ba50a408a44322c53fa03c619a6fd01fcba0c91bf
```

完成审查并另行决定合并后，App Hub 管理员可在受保护工作流中填写候选名
`ymote-repo-radar-0-1-0`、准确的审查提交、上述摘要和 **`dry_run: true`**。
准备候选不等于获得已签名的目录证明。应先验证工作流产生的 envelope 并完成消费端检查，
再考虑正式发布。详见[GitHub 发布流程](../../docs/GITHUB-PUBLISHING.md)。
