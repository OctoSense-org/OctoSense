# Repo Radar 0.1.2 目录候选

[English](README.md) | 简体中文

此候选对应[提交 #202](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/202)，
取代尚未准入的 0.1.0 提案，不改变公开目录或历史版本。正式发布须等待消费端验收，
以及包含 D3 和 WASI HTTPS 修复的兼容 OctoSense 正式发布包。

## 已验证的版本

[GitHub 工作流 38094354008](https://github.com/ymote/octosense-repo-radar/actions/runs/38094354008) 从标签中的 Rust 源码重新构建，
通过 17 项单元测试、1 项文档测试、14 项验收工具测试、fmt 和 clippy，
并为[版本 0.1.2](https://github.com/ymote/octosense-repo-radar/releases/tag/v0.1.2)生成证明；提交为 `e18eb6a88b74398c5df2472d6fd5c0a3d6a1d50b`。

- 发布包 SHA-256：`62fb0262fa9139675abbd286f71d23efd385eb16697ec6341c12893d216067c3`。
- Bundle BLAKE3：`da4272cead19e613638d8381c47662ef4cff3716838c7e719916d779f3c9edf9`。
- 组件 SHA-256：`d0ee1835711ed4b1c952f564789d96c160d02035b1d2104db1388e6affdbc757`。

独立下载验证通过了指定 GitHub 身份、远端标签、发布者证明、准入门禁、完整文件集合
比对和候选准备。组件为 497,521 字节，提供 9 个类型化导出，列出 18 个 crate。
安全公告扫描未发现受影响的已列 crate；数据库版本和检查详情见 `review.json`。
源码标签与封装结果仅有封装元数据、默认值序列化和 CI 重建组件的差异，未改动封装文件。

## 功能与限制

公开 GitHub 搜索、私有关注列表、近期 PR 样本、原生 D3 图表、版本说明和主动发布的
Glance 摘要均不需要账号。0.1.2 沿用重新设计的 0.1.1 运行代码，新增商店说明：
可选 Chat 属于预览，尚未验证真实模型回复和 agent 工具执行。
全部 9 项工具仅在前台运行，不与其他 agent 共享；agent 取消关注须经宿主审核。
商店仅声明 macOS，要求 `charts.d3@1`。

`review.json` 链接源码 UX 证据；这些证据不代表此封装版本已执行、跨版本更新已通过，
或已从公开商店安装。以上结果需要独立 receipt。RC4 无法安装本应用。

## 已准备的事务

候选原样保留现有 15 条记录，以已验证的公开序列 15 为基础提议序列 16。
准备后的载荷 SHA-256：

`9925772acb6995ed13e292f22e9f97dfdcb2ee397163007d85818e4ae4390b41`

审核合并后，用 `ymote-repo-radar-0-1-2`、准确提交和摘要触发受保护的目录工作流，
先执行 `dry_run: true`。验证签名试运行并完成消费端验收后才能正式发布。
公开基础目录或 UTC 日期变化时须重新准备、审核；保留其他候选及完整历史。
参见 [GitHub 发布](../../docs/GITHUB-PUBLISHING.md)。
