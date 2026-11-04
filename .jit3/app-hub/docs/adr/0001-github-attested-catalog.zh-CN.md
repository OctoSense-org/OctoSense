# ADR 0001：通过 GitHub 管理员身份认证目录

[English](0001-github-attested-catalog.md) | 简体中文

状态：已接受、已实现，并已投入生产。App Hub #150 实现了这项决定。受保护工作流发布了签名目录第 11 版（[运行 37736098082](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37736098082)）至第 15 版。OctoSense 桌面版 0.1.0-rc.1 是兼容的宿主发行版，默认选用 `catalog-v2.json` 通道。

## 背景

管理员希望通过 GitHub 身份批准目录发布，不再维护另一把私有签名密钥。现有目录使用
离线 Ed25519 信任锚、经过认证的工作密钥，以及发布者签名。这些职责不同：目录认证
决定 Hub 提供哪些应用；发布者连续性则确定应用更新前后的所有者。

GitHub 产物证明使用短期 Sigstore 证书。本公开仓库的证明包含公开透明日志证据。
证明认证的是来源和字节，不代表应用已通过准入或具有良好 UX。应用审核仍是发布流程的一部分。

## 决定

增加独立的 `catalog-v2.json` 通道，使用明确的封装：

```json
{
  "schema": 2,
  "kind": "github-attested-catalog",
  "catalog": "<catalog-v2.payload.json 原始字节的 base64>",
  "attestation": { "<Sigstore v0.3 bundle>": "..." }
}
```

内部目录沿用条目结构，不包含旧版目录密钥或签名。Base64 保留被证明的精确字节。
上例只是结构示意，不是有效证明。现有 `catalog.json`、信任锚、历史条目和发布者签名
不会被改写或替换。

[`github_catalog`](../../crates/app-hub/src/github_catalog.rs) 使用
`sigstore/sigstore-rust` 维护的 `sigstore-verify` 0.14.0，关闭默认联网特性。
验证使用宿主内置的公共 Sigstore 信任快照，检查证书链、证书透明度、签名、经过认证的
签名时间、日志包含证明与检查点，以及签发者和身份策略。手机不运行 `gh` 子进程，也不
使用自制证书验证器。缺失信任根、不支持的证明或任何检查失败都会拒绝目录，不会转用较弱模式。

宿主还要求经过认证的声明满足：

- 签发者为 `https://token.actions.githubusercontent.com`；
- 仓库为 `OctoSense-org/OctoSense-App-Hub`，不可变仓库 ID 为 `1378439410`，
  所有者 ID 为 `328148893`；
- 证书身份和工作流声明都指向
  `.github/workflows/publish-catalog.yml@refs/heads/main`；
- 触发方式为 `workflow_dispatch`，仓库公开，使用 GitHub 托管运行器，
  部署环境为 `app-hub-catalog`。

签名语句必须采用 in-toto v1 / SLSA provenance v1，且恰好包含一个名为
`catalog-v2.payload.json` 的 subject 及其准确 SHA-256。GitHub workflow/v1 predicate
中的工作流、仓库、分支、事件和不可变 ID 必须相同。限制为目录字节 8 MiB、证明 2 MiB、
封装 14 MiB、4,096 个条目。这些限制不会赋予应用任何宿主权限。

## 发布边界

管理员工作流从 `main` 运行可信实现。它通过 GitHub API 检查发起人和重新运行发起人的
管理员权限，并在请求 OIDC 身份前通过受保护环境。它将已批准的不可变候选 commit
和摘要作为数据读取，不运行候选中的工作流、构建脚本或应用代码。发布串行执行，
在非强制更新前拒绝已过时的 main/base 目录。

启用发布前，维护者必须将环境审批限制为当前管理员，并禁用绕过环境保护。
管理员必须审核准确的工作流 commit、候选摘要及发布记录，不能仅凭工作流名称批准。
发起发布的管理员可以自行批准，无需第二人。证书绑定的是工作流路径和环境，不是某次
经过审核的实现 commit 或审批者角色。因此，受保护环境及该审核构成授权边界。
建议将分支保护与强制工作流审核作为治理措施；若强制要求拉取请求审核，也需要调整
当前直接执行的非强制发布事务。

原生 `hub` 命令是工作流的验证边界：

| 命令 | 职责 |
| --- | --- |
| `catalog-prepare` | 认证现有旧版/v2 基础目录；要求序号 +1 和当天日期；保留历史；对每个新的签名应用包执行准入并比对 pack/index；输出精确待签字节和绑定基础、候选、输出摘要的 JSON 记录。 |
| `catalog-envelope` | 验证这些字节的正式 GitHub 证明，生成明确的 v2 文档。此命令不能签名或绕过验证。 |
| `catalog-verify` | 使用与设备相同的原生策略独立验证 v2 文档。 |

既有条目只允许从提供状态转为带理由的撤回状态。重新提供、删除、改写清单、更换发布者密钥、pack/index 不一致或可变源码 ref 都会失败。这些命令不会用目录证明替换发布者签名。经 GitHub 证明的发布者身份由 [ADR 0002](0002-github-attested-publisher-identity.zh-CN.md) 定义；密钥签名的应用不会迁移到这一身份。

## 客户端和缓存边界

[`Store::with_github_catalog`](../../crates/app-hub/src/client.rs) 是宿主选定的通道要求。
即使尚未成功获取过 v2，它也从第一次调用起就拒绝旧版、无签名和无效 v2 文档。
旧版 Store 保留现有信任锚验证。下载的文档不能选择信任模式，v2 请求失败后也不能重试旧版通道。

`accept_catalog_and_cache` 获取非阻塞缓存锁，先验证磁盘中的目录，再验证新目录，
原子替换完整文档后才更新内存。证明、载荷和序号始终保存在一起。较低序号或相同序号下
内容不同的目录会被拒绝。调用方必须为旧版和 v2 使用不同缓存路径，并在 UI 事件处理器之外
执行验证。现有的 14 天新安装时效规则保持不变。
`for_install_root` 为暂存安装克隆已认证状态，不改变所选通道、已准入目录、宿主限制或 API 版本。

新宿主可在发布目录前先携带 v2 验证器和信任策略。随后用隔离通道中的合成证明测试同一策略，
无需修改公开目录。只有原生和手机验收通过后，兼容版本才应选择 v2 通道。
旧客户端需要更新宿主；没有旧私钥，就无法把无密钥证明转换成其 Ed25519 信任链。
旧目录的时效限制仍然有效。

## 验证与限制

本地测试验证真实、公开的上游 Sigstore 样本，再篡改字节、签名和透明日志证据。
测试还覆盖错误身份、信任根与认证机构有效期、精确 subject/工作流策略、实际签名包准入、
发布者历史、序号回退、缓存竞争和写入失败。上游样本**不是**正式 App Hub 证明。
缓存策略测试单独验证认证后的状态机，不能替代真实 GitHub 签名运行。

正式发布会经过受保护环境的实际审批，并使用正式格式的 App Hub 证明。正式证明的命令行检查已在 macOS 和一部 Android 手机上通过（[证据](../GITHUB-PUBLISHING.zh-CN.md#验证)）。读取 `catalog-v2.json` 的手机商店和任何 iOS 宿主仍为**未验证**。信任根轮换需要兼容宿主更新；验证器不会从目录下载信任材料，也不会静默启用在线 TUF。这些限制都不是接受无签名目录的理由。

## 参考

- [GitHub 产物证明](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
- [官方 actions/attest](https://github.com/actions/attest)
- [GitHub provenance 生成器](https://github.com/actions/toolkit/blob/main/packages/attest/src/provenance.ts)
- [sigstore-verify 0.14.0](https://docs.rs/crate/sigstore-verify/0.14.0)
- [既有 App Hub 信任设计](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/home/0003-app-hub-and-store.md)
