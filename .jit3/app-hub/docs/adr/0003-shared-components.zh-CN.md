# ADR 0003：共享组件

[English](0003-shared-components.md) | 简体中文

状态：提议中，2026 年 10 月 9 日。这是 [OctoSense ADR 0014](https://github.com/OctoSense-org/OctoSense/pull/436)（提议中）第 4 阶段在 App Hub 一侧的部分，在 App Hub 中实现，基于 [#186](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/186) 和 [#188](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/188)；这两项改动让准入检查接受应用自己的 WebAssembly 组件。目前签名目录中还没有任何组件，仍需发布兼容宿主；源码支持不等于发布验收。

## 当前源码策略：声明与授权

`capabilities` 和 `network.hosts` 用于向用户和审核人员说明应用预计使用的
API 与网络目的地。遗漏某个服务族或主机，不会阻止调用宿主已支持的公开 API。
这是为下一个兼容版本准备的源码策略；RC2 的历史行为不代表该修改已经发布。

每个准入应用都有独立的存储沙箱、已解析的配额和网络模块。设备访问仍需逐应用
同意和系统权限；连接账户仍按应用、账户和提供商 scope 隔离。外部写入保留
原生审阅，Agent 保留用户启用和工具审核，访问其他应用的数据或工具仍需共享
授权。宿主私有用户资料和不带调用方身份的 `agent.notify` 不是公开 API。

`requires` 和 `host_api.required` 仍用于检查兼容性，不是能力授权。组件的
ABI 与导入验证、确切摘要、发布者证明、目录撤回、配额和平台支持检查继续生效。
遗漏使用声明可以产生审核警告。`AppPolicy::allows` 与 `allows_host` 是历史
声明查询接口；宿主不能把它们当作执行授权。请用 `runtime.list` 和
`runtime.describe` 查询实际方法及其平台、同意要求。


## 背景

ADR 0014 让应用把普通的 Rust crate 作为 WebAssembly 组件放在应用包的 `fns/` 中。它的第 4 阶段要求支持**共享组件**：App Hub 签名目录中经过审核、带版本的组件，可供多个应用使用，就像共享 npm 包一样。安装程序会验证这些组件，而每个应用仍然以自己的授权运行自己的实例。

需要满足以下约束：

- **代码只随应用更新而改变**：审核人员批准的是应用连同它运行的代码。共享组件绝不能在已安装的应用下面悄悄改变：不允许版本范围，也不允许与审核时不同的文件。
- **像应用一样审核**：组件通过使用它的应用访问时钟、文件或网络，所以 App Hub 要审核它，像验证应用自己的组件一样验证它（只允许特定导入），并且只接受带 GitHub 证明的 Release（[ADR 0002](0002-github-attested-publisher-identity.zh-CN.md)）。
- **只有一个签名目录**：宿主接受的是一份带证明的签名目录文档，它只有一个序号和一份证明（[ADR 0001](0001-github-attested-catalog.zh-CN.md)）。撤回组件，必须像撤回应用一样迅速到达设备。
- **宿主严格解析签名目录**：`Catalog`、`Entry` 和应用清单都拒绝未知字段，并且每个宿主都会用自己认识的特性检查每个条目的 `requires`（`CatalogPublishers::from_catalog`）。只要有一个条目用到宿主不认识的内容，宿主就会拒绝整个签名目录。对要求 `wasm-components-v1` 的应用来说，情况已经如此。

## 决定

### 1. 与应用并列的组件条目

签名目录在 `entries` 旁边新增一个 `components` 列表，其中每一项是一个组件版本：

```json
{
  "component": {
    "schema": 1, "id": "org.example.markdown", "version": "1.2.0", "name": "Markdown",
    "wasm_blake3": "<64 hex>", "bytes": 320538,
    "imports": ["wasi:clocks/wall-clock@0.2.12"],
    "exports": [{"name": "to-html", "params": [["markdown", "string"]], "result": "string"}],
    "publisher": {"name": "…", "support": "…", "privacy_policy_url": "https://…"},
    "license": "MIT OR Apache-2.0",
    "integrity": {"github": {"repository": "…", "repository_id": "…", "owner_id": "…",
      "workflow": "…", "tag": "v1.2.0", "commit": "…", "attestation": {}}}
  },
  "listing": {"subtitle": "…", "description": "…", "keywords": []},
  "artifact": "artifacts/org.example.markdown-1.2.0.wasm",
  "publisher": "github:<repository_id>",
  "source": {"repository": "https://github.com/…", "commit": "<40 hex>"},
  "status": {"state": "offered"},
  "admitted": "2026-10-09"
}
```

`component` 和 `listing` 是发布者发布、GitHub 证明所覆盖的内容。`artifact`、`publisher`、`source`、`status` 和 `admitted` 是 Hub 的记录，与应用条目中的相同。列表为空时不写出。

我们选择这种方式，是因为它对现有结构的改动最小：

- 没有组件的签名目录，序列化、签名和证明的结果与以前逐字节相同。测试会重新序列化已发布的 `catalog.json` 和带证明的 `catalog-v2.json` 载荷，并比较字节。
- `entries` 仍然只是应用列表，所以读取应用的地方都不用改：商店的列表和搜索、发布者连续性、v2 历史规则，以及 OctoSense 在测试中构造的 `Entry` 结构。
- 仍由一份文档承载全部内容，只有一个序号和一份证明。撤回组件的信息，与安装时解析组件所用的已验证签名目录一起送达。

其他方案都不如它合适。在 `entries` 中放入形如应用的组件条目，旧版商店会把它们当作应用列出，而每条处理应用的代码路径都要另加组件分支。另建一份带证明的文档，发布工作流就要增加第二个证明对象、信封和序号；设备还可能同时持有两份对同一组件说法不一的文档。两种方案都帮不了旧宿主：只要某个应用固定了组件，旧宿主就会拒绝签名目录，就像它们拒绝任何带有未知特性的条目一样。

### 2. 组件 Release

一个 Release 由 `.wasm` 文件和 `<id>-<version>.component.json` 组成，后者包含 `component` 和 `listing`。发布者填写 ID、版本、名称、发布者、许可证和商店信息。`hub component-prepare` 从文件中得出 `wasm_blake3`、`bytes`、`imports` 和 `exports`。给出 GitHub 身份时，它写出要证明的对象 `octosense-component.json`：不含证明的规范化 Release（键排序、无空白）。这个对象带有文件的摘要，所以证明绑定了文件，正如应用的规范化清单绑定了它的应用包。`hub component-pack` 附加证明、验证全部内容，并写出两个 Release 文件。

### 3. 审核

`hub component-check` 和发布组件时的签名目录准备运行同一套准入检查。出现以下情况时，它拒绝这个 Release：

- 文件不是有效组件，或导入了 `wasi:cli`、`wasi:clocks`、`wasi:filesystem`、`wasi:http`、`wasi:io`、`wasi:random`、`wasi:sockets` 和 `octosense:host` 以外的任何内容，与应用自己的组件相同；
- 文件的 BLAKE3 摘要、大小、导入或导出与 Release 不一致；
- ID 违反应用 ID 规则、以 `os.` 开头，或是签名目录中某个应用的 ID；
- 版本不是单个确切的语义版本、已经发布过，或不高于记录中最新的版本；
- 名称、发布者、许可证或商店信息违反相应规则；
- Release 没有 GitHub 来源证明，或证明验证失败。开发检查可以用 `--allow-unsigned` 接受未签名的 Release。

更新必须来自与早期版本相同的仓库、所有者和工作流。审核人员还会看到组件能访问什么，例如：`org.example.markdown 1.0.0 is a component that reaches the clock, random numbers and files in its app folder, but no network or other app`。

套接字导入现在使用可扩展的宿主 API 要求 `wasm.sockets@1`。固定这类组件的应用必须要求此 ABI，商店在安装前和启动时检查实际宿主支持。这不会新增使 RC5 拒绝整个目录的清单特性；当前规则见[共享组件发布规则](../PUBLISHING.zh-CN.md#共享组件)。RC5 不包含套接字支持。

### 4. 应用契约

清单新增可选字段 `components`，它需要契约特性 `wasm-shared-components-v1`：

```json
"components": [{"as": "markdown", "id": "org.example.markdown", "version": "1.2.0", "blake3": "<64 hex>"}]
```

`as` 是应用给组件起的名字：`[a-z][a-z0-9_]{0,31}`，且不能重复。一个应用最多指定 8 个组件，每个都固定到一个确切版本**和**摘要。没有所需宿主特性却写了这个字段时，解析器会拒绝；不要求 `wasm` 使用声明。字段为空时不写出，所以现有清单及其签名字节不变。不认识该特性的宿主会拒绝这个应用。

### 5. 准入固定了组件的应用

提供签名目录时，准入检查会解析每个组件：签名目录中必须有这个确切版本，它必须仍在提供，并且哈希值等于固定的摘要。支持的导入不依赖 `storage`、`net` 或 `wasm` 使用声明；执行仍遵守应用沙箱、配额和宿主服务的实际同意要求。`octosense:host` 不需要授权。每个组件都有一行给审核人员看的说明，例如 `component markdown (org.example.markdown 1.2.0) reaches the clock, but no files, network or other app`。

商店应用的应用包绝不携带 `components/`：它的组件来自签名目录。随构建发布的系统应用没有签名目录，它把固定的每个组件放在应用包的 `components/<blake3>.wasm`，系统应用检查会验证每个文件。

### 6. 发布与撤回

组件使用 ADR 0001 规定的 GitHub 管理员发布流程。候选内容新增 `artifacts/<id>-<version>.wasm` 和 `index/components/<id>-<version>.json`。`hub catalog-prepare` 按与应用相同的规则保持组件历史只增不改：条目保持原有顺序和字节，只能从“提供”改为“撤回”，并且必须写明原因。它对每个新版本运行准入检查，要求 GitHub 来源证明，并把准入检查重建的条目与候选内容中的条目和 index 文件比较。同一份候选内容中的新应用，可以固定其中的新组件。回执为每个新组件版本列出 `{component, index}`。

### 7. 在设备上

- **安装与更新**：在安装应用之前，商店从同一份已验证的签名目录中解析每个组件，并像获取应用包一样获取它的产物。它检查大小、摘要以及文件是否为有效组件，然后把每个摘要只保存一份、只读，放在 `<apps root>/.components/<blake3>.wasm`。组件不在时，`Store::install_staged` 会拒绝安装应用。
- **启动**：`Store::may_run` 和 `prepare_launch` 把应用的组件与应用包一起检查。组件缺失、被改动或被撤回，应用就不能运行，就像应用包被改动或应用版本被撤回一样。
- **加载**：宿主用 `octosense_appstore::components::resolved(app_id)` 加载组件。它返回每个组件的别名、ID、版本、摘要和已验证的路径。系统应用的组件来自其应用包；已安装应用的组件来自共享存储，依据缓存的已验证签名目录解析。每次调用都会对每个文件计算哈希，不依赖任何“写入时已验证”的标记，也从不下载。
- **回收**：卸载（`Store::remove`）以及每次通过 App Hub 商店的安装或更新，都会删除已没有任何已安装应用固定的组件。

## 影响

- **旧宿主**：一旦签名目录中有组件，或有固定组件的应用，早于本决定的宿主就会拒绝整个签名目录。这包括 OctoSense 桌面版 RC1 和 RC2，以及在更新 App Hub 版本之前的 OctoSense `main`。在兼容的宿主发行版发布之前，App Hub 不得发布这两种内容。对要求 `wasm-components-v1` 的应用来说，情况已经如此。
- **撤回**：撤回一个组件，会让固定它的每个已安装应用停止运行，直到各应用的发布者发布改用其他版本的更新。
- **磁盘**：无论有多少应用固定同一个组件，它都只存一份；没有应用固定时就会被删除。
- **构建使用方**：以 git 方式使用 App Hub 的项目，在契约发行版包含 `components` 之前，需要使用尚未发布的契约（[crates.io 上的版本](../../crates/app-contract/README.md#versions-on-cratesio)）。
- **开销**：每次启动检查和每次加载，宿主最多对 8 个、每个最大 8 MiB 的组件计算哈希。

## 未决事项与未验证部分

- **未验证**：还没有任何 GitHub 工作流为组件 Release 生成过证明，App Flow 也没有相应的工作流模板。证明检查使用应用证明的验证器，只是换成组件的证明对象。测试覆盖了它的各种拒绝，但还没有任何生产证明通过它。新组件的签名目录准备只在关闭证明检查的情况下测试过。
- **未验证**：还没有任何 OctoSense 构建调用 `octosense_appstore::components::resolved` 或加载共享组件。为每个应用运行各自的实例，是 OctoSense 的工作。
- **运行时支持**：如果宿主认识这个特性，但它的 `wasm` 服务不能加载共享组件，它仍会接受这个应用。OctoSense 必须在采用这个 App Hub 版本的同一改动中提供加载功能，否则就需要增加一个宿主 API 标记。
- **并发**：安装和回收依次进行时，回收是安全的；Shell 的 App Hub 就是这样，它在两者期间都持有签名目录锁。如果两个进程同时向同一个应用根目录安装，其中一个可能回收另一个尚未完成的安装写入的组件。下一次启动时，应用会被拒绝，并提示重新安装。
- **其他卸载路径**：宿主若不通过 `Store::remove` 删除应用，之后必须调用 `octosense_app_hub::components::collect`，否则组件会留在磁盘上。
- **商店界面**：商店不单独列出组件。应用的权限说明会列出它固定的组件。
- **设备**：还没有任何手机或桌面宿主安装过固定了组件的应用。

## 考虑过的替代方案

- **像 npm 那样解析版本范围**：不采用。版本范围会让已安装应用的代码在没有应用更新的情况下改变。
- **把组件复制进每个应用的应用包**：应用本来就可以通过自己的 `fns/` 这样做。这样什么也没有共享：没有统一审核的产物，不能统一撤回，磁盘上每个应用一份副本。
- **另建一份带证明的组件签名目录**：见第 1 节。
- **只按摘要固定**：用户和审核人员都看不出应用固定的是什么，而撤回是按 ID 和版本决定的。
- **组件拥有自己的授权**：不采用。组件能访问的，只能是使用它的应用可以访问的内容。

## 参考

- [OctoSense ADR 0014](https://github.com/OctoSense-org/OctoSense/pull/436)
- [App Hub #186](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/186) 和 [#188](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/188)
- 发布参考中的[共享组件](../PUBLISHING.zh-CN.md#共享组件)
- [ADR 0001](0001-github-attested-catalog.zh-CN.md) 和 [ADR 0002](0002-github-attested-publisher-identity.zh-CN.md)
