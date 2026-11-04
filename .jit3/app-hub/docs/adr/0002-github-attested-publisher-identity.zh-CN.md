# ADR 0002：通过 GitHub 证明发布者身份

[English](0002-github-attested-publisher-identity.md) | 简体中文

状态：已接受、已实现，并已投入生产。App Hub #153 在应用契约 1.8.0 中实现了这项决定，该版本新增 `publisher-github-v1` 宿主要求。一个测试应用的两个 Release 均由标签推送生成，并通过了原生商店的安装、更新和启动检查（[证据与限制](../PUBLISHING.zh-CN.md#github-发布者来源证明)）。公开签名目录第 15 版提供三个参考应用（GitHub Notes 为 0.2.2 版，其余为 0.2.1 版）和 Camera Card Demo 的 1.1.1 版，全部以 GitHub 来源证明发布，每个应用在更新前后都保持同一 GitHub 身份。OctoSense 桌面版 0.1.0-rc.1（下称 RC1）可以在 macOS 上安装这些应用，并在安装和更新时检查它们的证明和发布者连续性。[2026-10-08 修订](#2026-10-08-修订)：App Hub 只接受带 GitHub 证明的 Release；准入检查尚未执行这项规则。

## 背景

在契约 1.8.0 之前，Ed25519 发布者密钥是应用更新权限的唯一来源。用这把密钥签名的版本一旦进入签名目录，之后的每个版本都必须带有它的签名。没有任何命令能替换丢失的密钥，Hub 也没有密钥轮换机制：密钥一旦丢失，应用就无法再更新。参加黑客松的开发者也曾卡在签名上：有人提交的是最终签名之前的 commit，也有人因为缺少公钥，无法对已签名的应用包运行 `hub check` 或 `hub scan`。

[ADR 0001](0001-github-attested-catalog.zh-CN.md) 引入了签名目录的 GitHub 管理员证明，并把发布者身份（包括密钥签名的应用是否迁移）留待另行决定。

应用的代码就是 OctoScript：Splash 脚本和 L0 卡片，以可读文本的形式随应用包交付。应用包唯一能携带的编译代码是 `fns/` 中的 WebAssembly。OctoSense 只在启用 `wasm-lab` 特性的构建中运行它，而目前没有任何 OctoSense 发行版启用这一特性。App Hub 在公开的 `artifacts/` 中保存每个已准入应用包的原样副本。因此，OctoScript 应用默认开放：任何人都能阅读已准入应用的源码。

注（2026 年 10 月 8 日）：自 [OctoSense #400](https://github.com/OctoSense-org/OctoSense/pull/400) 起，OctoSense `main` 的标准桌面版和 Home 构建在 macOS、Linux 和 Android 上运行 `fns/` 中的 WebAssembly。目前还没有任何 OctoSense 发行版包含它。

## 决定

应用不使用 Ed25519 发布者密钥，而是用 GitHub 来源证明（即 GitHub Actions 运行生成的证明）来证实发布者身份。无论是新应用还是更新，App Hub 都只接受带 GitHub 证明的 Release。

- **证明**：推送 `v<manifest.version>` 标签后，应用公开仓库中的工作流在 GitHub 托管运行器上运行。`hub publisher-prepare` 在 `integrity.github` 中记录仓库、所有者、工作流、标签和 commit，并添加 `requires: ["publisher-github-v1"]`。接着，它写出规范化清单，也就是证明所覆盖的确切字节。`actions/attest` 通过 Sigstore 为这份清单签名，随后工作流附加证明，并打包成 Release pack。
- **验证**：规范化清单带有应用包摘要，因此证明绑定了应用包中的每个文件。Hub 的准入检查和宿主都用内置的 Sigstore public-good 信任快照离线验证证明。它们逐项核对仓库 ID、所有者 ID、工作流路径、标签和 commit，并要求由推送触发、使用 GitHub 托管运行器，且仓库在签名时处于公开状态。
- **更新**：签名目录把发布者记录为 `github:<repository_id>`。每次更新都必须来自相同的仓库名、仓库 ID、所有者 ID 和工作流路径，且语义版本更高。准入检查和宿主都会拒绝重放和回滚。
- **只用 GitHub 来源证明**：Release 带 GitHub 来源证明，不带 Ed25519 签名。审核人员不批准用密钥签名的 Release。
- **批准**：提交 issue 仍是发布请求。仅有标签或 GitHub Release，既不构成提交，也不代表批准：每个版本都要由 App Hub 管理员批准，再通过 ADR 0001 规定的受保护工作流发布到签名目录。
- **既有应用**：App Hub 已在签名目录第 14 版撤回六个用密钥签名的参考条目，改由带 GitHub 证明的后继应用取代（见[修订](#2026-10-08-修订)）。GitHub 来源证明不能接管已登记为未签名或密钥签名的应用；以 GitHub 来源证明发布的应用也不能退回密钥签名或未签名版本。
- **不迁移**：应用只能换用新 ID 改走 GitHub 来源证明；参考应用就是这样做的，新 ID 为 `io.github.ymote.*`。已安装的应用及其数据不会迁移。

## 影响

- 开发者无需管理任何密钥：不用 `hub keygen`，也不用仓库签名 Secret。GitHub 为每次工作流运行签发短期身份。
- 仓库成为更新权限的来源。任何能向仓库推送标签的人都能生成有效的 Release，但发布仍需管理员批准。重命名或转移仓库，或者重命名工作流，都会让该应用无法再更新。
- 仓库必须公开。这与 OctoScript 默认开放的模式相符：公开仓库暴露的内容，几乎都已由 App Hub 发布。
- OctoSense App Flow（原 Design Flow）的工作流模板由 `tools/octo publish-github` 安装，只处理位于仓库根目录的 `bundle/`。应用包在其他位置时（例如在 monorepo 中），没有可用的模板。
- 安装需要支持 `publisher-github-v1` 的宿主，例如 RC1；旧宿主会拒绝这类应用。

## 未决事项

- 准入检查目前还不会拒绝用密钥签名的 Release（见[修订](#2026-10-08-修订)）。
- 私有仓库中的应用和闭源应用，有待今后另行决定。
- 仓库重命名、转移或删除后，没有任何办法能恢复更新，应用只能换用新 ID 继续发布。
- 与签名目录的证明一样，轮换 Sigstore 信任根需要更新宿主。
- iOS、Windows 和 Linux 上的商店安装仍未验证。目前还没有任何已发行的手机版本支持 `publisher-github-v1`；Android 上只有一个调试构建在本地测试签名目录中通过了安装和更新检查。

## 考虑过的替代方案

- **继续使用 Ed25519 发布者密钥**：“背景”一节中的问题都会保留，即密钥保管、丢失密钥后无法恢复，以及让新开发者受阻的签名步骤。最初的决定为已用密钥发布的应用保留了这条路径；这次修订将其关闭，因为没有任何第三方应用用过它。
- **未签名版本**：经 GitHub 证明的签名目录不会准入未签名的应用包，任何已发行的商店也都不会安装未签名的应用。
- **GitHub commit 签名或 Verified 标记**：commit 签名需要开发者自行管理 GPG、SSH 或 S/MIME 密钥。它只覆盖该 commit，而工作流稍后才生成规范化清单。Verified 标记只是展示 GitHub 自己对该签名的检查结果，宿主无法离线验证。
- **由 Hub 托管构建**：证明中的身份会变成 Hub 的工作流，而不是开发者的仓库，因此无法承载更新权限。发布者连续性还会并入签名目录的授权，而 ADR 0001 把二者分开。
- **私有仓库**：GitHub 用自己的 Sigstore 实例为私有仓库的证明签名；这个实例没有公开透明日志，也不在宿主的信任快照之中。私有仓库也藏不住多少东西：应用包附带应用源码，App Hub 还保留了公开副本。

## 2026-10-08 修订

App Hub 对新应用和更新关闭了 Ed25519 发布者密钥路径。此前没有任何第三方应用用密钥发布过。

- App Hub 只接受带 GitHub 证明的 Release（`publisher-github-v1`）。
- 2026-10-08 发布的签名目录第 14 版（[工作流运行 37852340163](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37852340163)）在 `catalog-v2.json` 中把六个用密钥签名的参考条目标记为已撤回；这些条目仍留在签名目录中，`status.state` 为 `withdrawn`：发布者 `ymote` 的 `org.octosense.samples.githubnotes`、`org.octosense.samples.inbox` 和 `org.octosense.samples.googlecalendar`，各有 0.1.0 和 0.1.1 两个版本。接替它们的是带 GitHub 证明的 `io.github.ymote.*` 应用。
- 准入检查尚未执行这项规则。要让准入检查拒绝新的用密钥签名的 Release，还需要一项改动；这项改动由 [App Hub #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168) 跟踪，尚未合并。在它合并之前，准入检查仍会让用密钥签名的 Release 通过，只能靠审核让这条路径保持关闭。`hub keygen`、`hub pubkey` 和 `hub sign-manifest` 的去留也由这个 issue 决定。

## 参考

- [App Hub #153](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/153)
- [App Hub #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168)
- [OctoSense 桌面版 0.1.0-rc.1 发行说明](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.1)
- [GitHub 发布者来源证明](../PUBLISHING.zh-CN.md#github-发布者来源证明)
- [App Flow 的 Release 工作流模板](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/tools/publish-app.template.yml)
- [GitHub 产物证明](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
