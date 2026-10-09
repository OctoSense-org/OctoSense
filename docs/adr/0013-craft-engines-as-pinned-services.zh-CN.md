# ADR 0013：Craft 引擎作为固定修订版服务接入

[English](0013-craft-engines-as-pinned-services.md) | 简体中文

状态：提议中。下述测量于 2026 年 10 月 8 日针对真实引擎（photocraft 的
测试套件与 CLI、gridcraft 的解析器与求值器）以及 makepad `32d6415f` 的
Splash 计算内核完成；各服务均尚未实现。

## 背景

storytold 的「craft」系列是用纯 Rust 对专业工具做的洁净室重实现，许可证
为 Apache-2.0：photocraft（Photoshop 级栅格引擎：自研
JPEG/PNG/WebP/TIFF/EXR 编解码器、817 条已注册命令、无头 CLI）、
gridcraft（电子表格：公式解析器、求值器、依赖驱动的重算、xlsx）、
pdfcraft、soundcraft、vectorcraft 等。它们的引擎与 UI（egui，我们不会
使用）分层清晰；我们实测过的两个引擎是真实可用的：photocraft 通过
1,203 项引擎测试并能无头转换、滤镜真实文件；gridcraft 解析出的公式
AST 可以机械地降级到我们的 Splash 计算内核。

OctoSense 想要这些能力——代理可以重算的电子表格、可以运行的图像/PDF
处理——既不必自写引擎，也不把未固定修订版的第三方代码纳入信任基。
这类代码适配的形态我们早已具备：带类型化工具的宿主服务（mail、
calendar、news）、注册给 Octoscript 的原生组件（makepad-d3 的图表）、
经 App Hub 分发的脚本应用，以及带有准入控制的计算内核 JIT。

同组织的旗舰 artcraft 没有任何许可证，完全排除在外。

## 决定

1. **三层三归属。** 一项 craft 能力进入 OctoSense 的方式是：**引擎**
   ——他们的 crate，原样不动——置于带类型化工具的**宿主服务**之后
   （`sheet.*`、`photo.*`），这是代理所调用的；任何**重型界面**
   （表格网格、栅格画布）作为 Octoscript-Makepad 或 makepad fork 中的
   **原生 Makepad 组件**，像 `d3.*` 一样对脚本可见；**应用**则是经
   App Hub 分发的纯 Octoscript bundle，其代理在 `tools.json` 中声明。
   egui 界面不做移植；面板与对话框以声明式重写，只有确实需要 Rust 的
   少数界面才成为组件。
2. **固定在 `ymote` 名下的 fork。** 引擎以 `ymote` 账号下 fork 的 git
   依赖形式消费（`ymote/gridcraft`、`ymote/photocraft`，后续采用的同
   理），在根 `Cargo.toml` 中按修订版固定，与所有外部依赖一致
   （ADR 0001 的纪律）。这些 fork 是只读镜像：我们需要的修复先提交给
   storytold 上游，再通过移动固定修订版采纳。初始固定：gridcraft
   `c6b6f4177cbf`，photocraft `eec4af65513b`。
3. **先无头，后界面。** 每个引擎的第一个交付物是宿主服务及其代理工具，
   桌面优先——不写任何 UI，所有代理即获得表格重算与图像操作。各服务
   暂不进入手机外壳，待逐引擎权衡其二进制体积后再定。
4. **内核 JIT 是差异化所在，准入按作者映射。** gridcraft 的重算改为混
   合式：数值型下拉填充列从其公式 AST 降级为 f64 计算内核（按公式缓
   存），其余仍走其求值器。photocraft 的调整与曲线公式作为内核用于预
   览与批处理。人写的公式或滤镜以用户来源运行；模型写的以 `Ai` 准入
   运行，受其单元素时限与预算约束。
5. **审批与文件一如既往。** 这些服务不拥有任何新访问权：文件读写走现
   有的 files 宿主工具与审批流程，每个代理可见的工具都像 mail 与
   calendar 的那样按应用授予。

## 测量

- gridcraft，`=@A:A*1.05+SIN(@B:B)*0.5+EXP(-@A:A*0.01)` 下拉填充 100
  万行，数值已交叉核验：其求值器 595.7 ms（596 ns/格）；同一 AST 降级
  为 Splash 内核后单线程 4.9 ms（122×），八线程 2.5 ms（243×）；惯用
  Rust（libm）10.0 ms——内核内联的多项式 `sin`/`exp` 比逐格调用 libm
  快 2×。内核编译 2.9 ms，每公式一次。
- photocraft：41 个套件共 1,203 项引擎测试通过；经其 CLI 在真实文件上
  验证了 PSD→PNG、JPEG→TIFF、PNG→WebP 与高斯模糊。默认构建中 HEIF 被
  特性门关闭；转换时 EXIF 与 DPI 会被丢弃。

## 后果

- 代理很快获得电子表格与图像/PDF 操作，引擎通过移动固定修订版保持可升
  级。
- 同一领域会存在两个引擎谱系（makepad 自带应用与 craft 引擎）。表格试
  点对此做了刻意处理：现有原生 Sheets 应用改用 gridcraft 的
  formula/calc 栈，而不是出现第二个电子表格。
- `ymote` 名下的 fork 集合属于供应链，纳入固定修订版清扫纪律；与
  storytold 的分歧比照我们 makepad fork 与上游的分歧处理——有意为之、
  有清单、能上游的尽量上游。
- 内核降级只覆盖数值子集；文本、引用与动态数组仍由引擎求值器处理，因
  此除内核可证明算出相同数值之处（f64 内核；f32 探针不是生产形态）
  外，重算结果与引擎完全一致。

## 代理工具（2026 年 10 月 8 日）

十个引擎服务（word、deck、cad、light、sound、design、film、effect、
vector、pdf）现已有代理工具，系统代理可以调用：

- **随服务一起声明。** `apps/<family>/host-service/tools.json`，采用
  App Hub 的 `tools.json` 形态（即该 crate 的 `TOOLS_JSON`），输入与输出
  都是对象 schema。每个 crate 的测试都用 App Hub 自己的加载器读取它。
- **每个引擎一个虚拟所有者。** 目前还没有应用发布这些工具，因此 Shell
  以 `os.<family>` 的名义声明它们（`crates/shell/src/host_tools/engines.rs`），
  并以该系统身份在引擎的服务上运行每个工具，调用与工具同名的方法，
  文件都在引擎自己的区域 `<apps root>/.host/<family>` 内；应答和错误都以
  相对该区域的路径指称文件，从不出现宿主自己的路径。虚拟所有者不
  是应用：没有 bundle、没有应用代理、设置里也没有它的条目；它作为
  Shell 自己编译进来的服务通过准入。当某个引擎以应用形式发布时（决定
  1），其 bundle 的 `tools.json` 接管该命名空间，虚拟所有者随之退出。
- **只授予系统代理。** `ENGINE_TOOLS`
  （`crates/shell/src/system_chat/grants.rs`）是一份与日历同类、经过审查
  的窄授权：共 43 个工具，包括每个读工具，以及只在其引擎自己区域内写入
  的每个 act 工具。没有一个是 destructive、outward 或 shareable 的，因此
  任何应用的代理都无法被授予。通用命令入口 `vector.run` 和 `effect.run`
  已声明，但留待单独审查，暂不授予。design、effect 与 vector 的
  `commands` 目录没有声明，因为它们返回 JSON 数组，而 octos 只接受对象
  结果。
- **文件仍是缺口。** 决定 5 预期文件访问走现有的 files 宿主工具。但这些
  工具到不了引擎的区域，系统代理的工作区也到不了。在经过审查的暂存路径
  出现之前，引擎只能看到它自己的工具写出的文件（`word.new`、`deck.new`
  以及各种转换），每个工具的描述都说明其路径相对于该引擎的工作区。
  designcraft 会按其打开的文档里写的路径解析数据合并源，vectorcraft 也
  会这样解析链接图片；在这一点被收敛之前，暂存路径不得把外部文档交给
  这两个引擎。
- **内核的上限。** octos 一次注册最多接受 64 个宿主工具，超出则整组拒绝。
  加上引擎工具后，系统会话可能的最大集合为 63 个。Shell 把引擎工具放在
  最后，超限时最先舍去，并有测试保证整份授权不超过上限。
- **长调用。** 引擎调用在派发它的线程上运行（工具调用时是 Shell 的 UI
  线程），运行期间占用 App Hub 的服务注册表，应用调用这些引擎时一直如此。
  一次长导出会让 Shell 在这段时间内停顿。内核等待宿主工具应答 30 秒（之后
  act 调用以结果未知结束），App Hub 在 60 秒后让请求超时。把引擎工作移到
  工作线程并按方法设置超时，是后续工作。

## 引擎技能（2026 年 10 月 9 日）

系统代理现在通过按需读取的 octos 技能来了解每个引擎。按方法划分的工具每个
回合都要发送完整 schema（43 个引擎工具约 29 KB），却仍让每个引擎的大部分能力
够不着：仅 photocraft 就有 817 条命令。

- **每个已链接引擎一个技能。** `apps/<family>/host-service/skill/` 中有手写的
  `SKILL.md`：frontmatter 为 `name: <family>-engine` 和一行不超过 200 字节的
  `description`，正文写引擎能做什么、系统代理可用的工具、文件规则和示例。旁边是
  按引擎固定版本生成的参考文件：有命令目录的十个引擎（photo、word、deck、cad、
  light、design、film、effect、vector、pdf，共 4,557 个 id）各有一份
  `commands.md`，每个 id 一行；light 有 `controls.md`，sheet 有 `functions.md`。
  每个服务都嵌入自己的技能（`src/skill.rs`），因此构建发布的技能总与其引擎一致。
  Shell 用与服务相同的条件注册已链接引擎的技能
  （`crates/shell/src/system_chat/skills.rs`）：sheet 和 photo 随 `app-hub`，
  另外十个随 `craft-engines`。
- **每次内核启动前安装。** 内核服务（`crates/kernel/src/skills.rs`）把它们写入
  octos 为系统代理的 profile 读取的技能目录 `<core dir>/profiles/_main/data/skills`，
  每个都带 `.octosense-managed` 标记。它刷新有变化的技能，删除不再注册的受管理
  技能，从不触碰用户自己的技能。profile 运行时启动时，octos 把每个技能的名称、
  描述和位置列入该 profile 的系统提示词（`build_skills_summary`）。代理用
  `read_file` 读取 `SKILL.md`，因为该目录是每个会话文件工具的只读区域。十二条
  摘要每个回合约占 4.6 KB。
- **`_main` 的每个会话都能看到。** octos 按 profile 划定技能范围，应用代理的
  peer 也运行在 `_main` 上。它们没有任何引擎工具，而每条描述都写明在使用
  `<family>.*` 工具之前先读该技能。
- **生成并校验。** `crates/skill-gen` 根据引擎的实时目录和手写的
  `safety-rules.json` 生成 `commands.md` 与 `safety.json`。若有未分类的 id、
  过时的规则或与引擎不一致的文件，各服务的 `tests/skill.rs` 就会失败；用
  `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-<family>-service --test skill`
  重新生成。Shell 的测试检查每个技能的 `## Tools` 恰好等于系统代理对该引擎的
  授权，且示例符合工具的 schema。
- **安全类别。** 每个目录 id 都依据其实现归入 `safe`、`file`、`code`（插件、
  脚本、会运行其他命令的命令）、`network`、`device` 或 `host`（窗口、视图、
  偏好设置、剪贴板）。每份 `safety.json` 都有该引擎的统计。
- **下一步（尚未完成）。** 按调用方划分的区域落地后，`<family>.info` 加上每个
  有目录的引擎一个经过审查的 `<family>.run` 入口，将取代这 43 个按方法划分的
  工具。每个入口拒绝调用方区域之外的 `file`，以及所有 `code`、`network`、
  `device` 和 `host` id。无法加以围栏的引擎保留其精选工具。

## 待决问题

- 第一个服务写出后，服务 crate 的归属（`crates/craft-*` 还是各应用的
  `apps/<name>/host-service`）。
- soundcraft 的引擎（以及我们 makepad fork 尚未收录的 `audio_aot`）是
  否足以支撑本季度的可脚本化音效线。
- 手机打包。自 2026 年 10 月 9 日起，系统 Agent 背后的十个引擎（word、deck、
  cad、light、sound、design、film、effect、vector、pdf）仅限桌面：由桌面默认
  特性 `craft-engines` 引入，若其中任一进入 Home 的依赖图，
  `tools/check-shell-graph.sh` 即报错。sheet 与 photo 引擎仍随 Home 发布，因为
  原生 Sheets 应用和 Photos 的 Agent 工具在手机上依赖它们。是保留它们（二进制
  成本尚未评估），还是同样改为仅限桌面（并在手机上撤下这些工具），仍待决定。
