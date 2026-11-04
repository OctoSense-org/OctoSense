# 同时准备组件及其使用者应用

[English](SHARED-COMPONENT-REHEARSAL.md) | 简体中文

`shared_component_candidate` 将新的共享组件和依赖它们的应用放入同一个可审核的
目录事务。普通 `publisher-entry` 根据已有可信目录解析依赖，本工具也能解析
同一候选中刚加入的组件。

把真实 GitHub 来源证明的发布文件下载到一个目录中：每个组件的
`<id>-<version>.component.json`、匹配的 `<id>-<version>.wasm`，以及各应用的
`*.bundle.pack.json`。以当前真实的 `catalog-v2.json` 为基础，本地不使用凭证
或签名密钥。

```sh
cargo run --locked -p octosense-app-hub --example shared_component_candidate -- \
  catalog-v2.json downloaded-releases new-candidate
```

工具验证管理员目录证明、各组件和应用的发布者证明、精确依赖及身份连续性，
最后执行与 `hub catalog-prepare` 相同的验证。输出包括 `catalog.json`、所有
必要的发布文件与索引、规范化的 `catalog-v2.payload.json`，以及记录 SHA-256 的
回执。它不会执行应用或组件代码，不会使输出自动获得信任，也不会发布。已有
输出目录会被拒绝；失败只清理本次调用新建的目录。

审核完整候选后，遵循 [GitHub 管理员发布流程](GITHUB-PUBLISHING.zh-CN.md)。
受保护工作流的 `dry_run: true` 会产生真实验证过的目录信封，但不修改公开目录。
使用该信封和完全匹配的候选文件执行隔离安装。声称端到端验证成功时，不能用假
证明或旧密钥测试目录代替这个流程。

示例的三个拒绝与清理测试使用仓库中的真实公开基础目录证明。真实证明演练已使用
[合成 v0.1.0 发布](https://github.com/ymote/octosense-component-demo/releases/tag/v0.1.0)成功完成准备阶段：
[发布者工作流 38024551136](https://github.com/ymote/octosense-component-demo/actions/runs/38024551136)
构建并验证两个组件和两个消费应用。工具验证序号 15 的正式基础目录，并在
[#195](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/195)中生成序号 16 的组合候选，
载荷 SHA-256 为 `87bde245807a5ff6a1b3297c409d4ef6684414e47b038519a196feab29f42a7e`。
管理员证明、原生安装和执行仍需要各自的回执；准备阶段成功不能证明这些后续步骤。
