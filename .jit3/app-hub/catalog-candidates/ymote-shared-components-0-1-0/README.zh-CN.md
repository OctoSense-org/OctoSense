# 共享组件安装演练

[English](README.md) | 简体中文

这个候选目录**仅用于 `dry_run: true`**。合并它不会修改正式目录，也不会把合成应用加入公开商店。

两个组件和两个消费应用来自 [ymote/octosense-component-demo v0.1.0](https://github.com/ymote/octosense-component-demo/releases/tag/v0.1.0)。源码提交为 `8faa66c3e4006d68dd4de16fba375688578814e2`。
[发布者工作流 38024551136](https://github.com/ymote/octosense-component-demo/actions/runs/38024551136)编译 Rust 组件，并验证四份真实 GitHub 证明，使用的 App Hub 提交为 `f0b9b22f521607d67c7e3fdc925a810c45d62a9b`。

提交 `6264190` 的 `shared_component_candidate` 工具以未改动的正式目录（序号 15）为基础生成本目录，验证真实基础目录及发布者证明、两个组件的固定版本、首次发布者身份连续性、全部制品和索引，以及完整候选载荷。第一个应用不声明能力；第二个声明 `wasm` 和 `storage`。在宿主当前能力策略下，两者都必须获得应用私有的运行状态和文件。

管理员应审阅的 `catalog-v2.payload.json` 精确 SHA-256 为：

```
87bde245807a5ff6a1b3297c409d4ef6684414e47b038519a196feab29f42a7e
```

`prepare-receipt.json` 记录全部新增路径、基础目录和候选目录摘要。审阅并合并后，App Hub 管理员可从 main 调用受保护的 `publish-catalog.yml` 工作流，提供本目录名、已审阅的完整提交、上述摘要，并明确使用 **`dry_run: true`**。将其验证后的证明封装与这些精确制品一起用于独立 Mac 和 OnePlus 6 的安装、执行验收。**在产生单独回执之前，原生执行和设备验收仍未验证**。本目录不含个人数据、账户凭据或开发者签名私钥。
