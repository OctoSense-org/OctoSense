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
  文件都在引擎自己的区域 `<apps root>/.host/<family>` 内（自 2026 年 10 月
  9 日起改为调用方自己的文件夹，见下文）；应答和错误都以
  相对该区域的路径指称文件，从不出现宿主自己的路径。虚拟所有者不
  是应用：没有 bundle、没有应用代理、设置里也没有它的条目；它作为
  Shell 自己编译进来的服务通过准入。当某个引擎以应用形式发布时（决定
  1），其 bundle 的 `tools.json` 接管该命名空间，虚拟所有者随之退出。
- **只授予系统代理。** `ENGINE_TOOLS`
  （`crates/shell/src/system_chat/grants.rs`）是一份与日历同类、经过审查
  的窄授权：共 43 个工具，包括每个读工具，以及只在其引擎自己区域内写入
  的每个 act 工具。没有一个是 destructive、outward 或 shareable 的，因此
  任何应用的代理都无法被授予。通用命令入口 `vector.run` 和 `effect.run`
  已声明，但留待单独审查，暂不授予。（自 2026 年 10 月 9 日起，这份授权是
  27 个工具：七个引擎各一个 `info` 加一个经过审查的 `run` 入口，见下文的
  “命令入口”。）design、effect 与 vector 的
  `commands` 目录没有声明，因为它们返回 JSON 数组，而 octos 只接受对象
  结果。
- **文件仍是缺口**（已于 2026 年 10 月 9 日补上，见下文）。决定 5 预期文件
  访问走现有的 files 宿主工具。但这些工具到不了引擎的区域，系统代理的工作
  区也到不了。在经过审查的暂存路径
  出现之前，引擎只能看到它自己的工具写出的文件（`word.new`、`deck.new`
  以及各种转换），每个工具的描述都说明其路径相对于该引擎的工作区。
  designcraft 会按其打开的文档里写的路径解析数据合并源，vectorcraft 也
  会这样解析链接图片；在这一点被收敛之前，暂存路径不得把外部文档交给
  这两个引擎。
- **内核的上限。** octos 一次注册最多接受 64 个宿主工具，超出则整组拒绝。
  加上引擎工具后，系统会话可能的最大集合为 63 个（有了命令入口后为 47 个）。Shell 把引擎工具放在
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
- **下一步**（已于 2026 年 10 月 9 日完成，见下文的“命令入口”）。按调用方划分的
  区域落地后，`<family>.info` 加上每个有目录的引擎一个经过审查的 `<family>.run`
  入口，将取代这 43 个按方法划分的工具。每个入口拒绝调用方区域之外的 `file`，以及所有 `code`、`network`、
  `device` 和 `host` id。无法加以围栏的引擎保留其精选工具。

## 引擎在调用方自己的文件夹里工作（2026 年 10 月 9 日）

引擎是 OctoSense 全局共用的工具，在应用的文件夹里运行，而不是在自己的
文件夹里。这补上了上文的「文件仍是缺口」。每个引擎私有的文件夹
`<apps root>/.host/<family>` 只在服务脱离 Shell 运行时使用（它自己的测试、
App Hub 的 card-host）。

- **每个服务一个解析器。** 十二个引擎服务（sheet、photo 以及上文的十个）
  都向一个解析器询问一次调用在哪里工作（`set_area_resolver`；共用的规则在
  `crates/engine-area`）。Shell 在注册这些服务时装上自己的解析器
  （`crates/shell/src/host_tools/areas.rs`）。一个区域包括根文件夹、在那里
  写入时能否替换已有文件，以及这次调用还能增加多少字节。解析器只依据可信
  的宿主数据作出决定，从不看调用的参数：
  - **系统 Agent 调用 craft 引擎的工具：** 它的工作区，即它的会话所在、它
    自己的文件工具（`read_file`、`list_dir`）看到的文件夹。内核在系统会话
    打开时给出它（`session/open` 的 `workspace_root`）；在此之前，以核心
    目录中保存的工作区为准。两者都未知时，调用被拒绝（`no_workspace`）。
  - **应用 Agent 调用 craft 引擎的工具：** 该账户在应用 jail 里的文件夹
    `accounts/<hash>/`，也就是该 Agent 自己的工作区。这些工具都不是
    shareable 的，所以目前没有任何应用的 Agent 能调用它们（开发者模式跨应用
    也只授予 shareable 工具）；这条规则是为将来允许时准备的。
  - **任何 Agent 调用应用自己的引擎工具**（Sheets 的 `sheets.*`、Photos 的
    `photos.info`）：该应用的 Agent 文件夹；若调用者是它自己的 Agent，用该
    Agent 的账户，否则用该应用当前所代表的账户。每个应用工具都处理该应用
    的数据，因此系统 Agent 的 `sheets.get` 读取的是 Sheets 的 Agent 打开的
    工作簿，位于 Sheets 的文件夹中。
  - **应用自己通过 `host.request` 调用引擎**（目前还没有 bundle 这样做）：
    该应用的存储，即它的 `fs.*` 看到的 jail。应用必须持有 `storage` 能力。

  已登出、已挂起或被拒绝的账户会被拒绝（`signed_out`、
  `workspace_refused`）；指向任何其他文件夹的请求也一样。
- **实现方式。** 运行 Agent 工具的执行器知道是谁在调用（broker 盖章的应用、
  账户和调用者类型），也知道它以哪个所有者的身份运行该工具。它在派发前据此
  解析出区域，并把它登记为一项授予，一直保留
  到调用得到应答或被取消。它把区域的根放进 `ServiceCall` 的 `host_dir`，
  这是唯一只有宿主代码才能设置的字段。解析器对已授予的根返回对应区域，对
  App Hub 共用的 `<apps root>/.host` 返回调用方应用的存储。
- **Agent 从不覆盖文件。** Agent 的调用（`may_prompt` 为 false）从不替换已
  有文件。写入时，只要路径上已有任何条目（文件、文件夹或链接）就拒绝；同一
  时刻新建的文件也不会被替换（`create_new`，或从暂存文件夹建立硬链接）。
  错误信息会要求换一个新名字。应用自己的前台请求仍可原子地替换文件。自己
  写文件的引擎（PDF 拆分、连同附属文件的批量显影、多画板 SVG、视频导出）
  先写进区域内一个隐藏的暂存文件夹，再一次性全部移到位，否则一个也不移。
- **配额。** 写进应用 jail 的输出必须放得进该应用剩余的存储配额：jail 的
  上限（脚本应用是 App Hub 准入的 `storage` 上限，原生应用是
  `storage.max_bytes`）减去调用开始时 jail 已占用的量。一次调用的所有写入
  共用这份额度，每次写入前都会检查。系统 Agent 的工作区除了各服务自己的
  单次调用上限之外没有配额。
- **限制在区域内。** 每个路径都相对于区域。`..`、绝对路径以及通向区域外的
  符号链接都会被拒绝，任何写入都不会穿过链接。应答和错误都以相对于区域的
  路径指称文件。
- **文档内部的路径。** 每个引擎自己会打开的文件都逐一检查过；引擎技能（见上文）
  的逐命令安全审查也检查了文档还能让引擎做什么。每一项都被限制在区域内，并且
  在各自服务的测试里都有一个恶意样本：
  - **design：** 图形以链接方式引用的 IDML（任何链接，包括 `file://`）在引擎
    打开之前就被拒绝：先用引擎自己的导入器配一个只记录、不读取每个链接的读取
    器跑一遍。文档旁边的 `Document Fonts` 文件夹必须留在区域内。`.designcraft`
    文档的数据合并源和资源链接在任何输出之前都会被清空，因此其他位置的路径不会
    进入输出。以 `<image href>` 链接文件的置入 SVG 会让任何调用都拒绝该文档，
    因为 usvg 一解析该 SVG 就会读取这个链接。
  - **vector：** 先运行引擎自己的 SVG 导入器和原生加载器，列出文档链接的每个
    文件（绝对路径、`file://`、`../`，以及导出的 SVG 或 PDF 里的编辑载荷）。
    链接了区域外文件的文档会被拒绝。
  - **effect：** 素材以及图像序列的每一帧都必须在区域内，3D 模型被拒绝。引擎会
    越过所有关卡、用 `std::fs` 按参数自身的路径读取文件的效果参数（Apply Color
    LUT、Lumetri 的输入 LUT 与 look、OCIO 文件变换或配置、mocha 形状数据），
    除非直接存放文件的文本，否则会让工程被拒绝；这类参数上有表达式也一样，因为
    表达式可能在渲染帧时生成路径。
  - **photo：** 引擎自己的授权工作区就是该区域。智能对象链接了区域外文件的文档
    在打开时即被拒绝，嵌套的智能对象也一样：引擎会按链接自身的路径读取它，导出
    PSD 时还会嵌入该文件的原始字节。
  - **pdf：** 读写都以区域为根，文档自带的脚本从不运行：每个服务会话都关闭引擎
    的 JavaScript（引擎默认会在打开 XFA 表单时在其沙箱中运行表单脚本）。
  - **film：** 工程指向区域外的媒体保持离线。**light：** 原图必须是普通文件，
    其 XMP 附属文件必须在区域内。
  - **word、deck、cad、sound、sheet：** 只进出字节，引擎不打开任何其他文件。
    word、deck 和 cad 按字体族名使用系统字体，gridcraft 拒绝指向其他工作簿
    的链接，sound 从不打开音频或 MIDI 设备。有了命令入口后，`word.run` 会运行
    wordcraft 的命令，而 `review.readAloud`（会启动语音程序）归为 `device`，
    任何入口都不运行它。

  十二个引擎全部受到限制，没有一个留在私有文件夹里。
- **命令入口暂不开放**（直到 2026 年 10 月 9 日，见下文的“命令入口”）。
  `vector.run` 和 `effect.run` 从未授予，它们拒绝的
  id 列表也算不上围栏：包装命令（`command.batch`、`engine.batch`、
  `file.runScript`）、偏好设置（用 `prefs.set` 设置插件文件夹）或插件效果都能
  绕过去。装上 Shell 的解析器后，这两个服务直接拒绝 `run`，因此在审查之前这两
  个入口够不着任何调用方的文件夹。同样未授予的 `photo.run` 保留 photocraft 自己
  对文件命令和智能对象路径的允许列表。
- **工作簿。** Sheets 工作簿属于创建或打开它的区域。来自其他区域的调用看不
  到、改不了、导不出也关不掉它。每个区域最多 16 个打开的工作簿，所有区域
  合计最多 64 个。

## 命令入口（2026 年 10 月 9 日）

系统代理现在通过每个引擎一个经过审查的命令入口来驱动七个引擎，不再为每个方法
单独精选一个工具（#418）。

- **工具面。** word、deck、cad、light、film、effect 和 vector 各给系统代理两个
  工具：`<family>.info`（读）和 `<family>.run`：在 `path` 指向的文档（或一份新
  文档）上依次运行引擎目录中最多 64 条命令，再把结果写到 `out`，即调用方文件夹
  中的一个新文件。写出什么由 `out` 的扩展名决定，因此一个入口就涵盖了原先按方法
  划分的工具所做的事（转换、渲染、帧、导出、Lottie 导入与导出、显影）。sound、
  design 和 pdf 保留固定工具。引擎授权从 43 个工具减为 27 个，系统会话的最大
  集合从 63 个减为 47 个（octos 的上限为 64，octos #2737 加上重新固定版本后为
  96）。按方法划分的服务方法仍然保留，供应用自己的请求（`host.request`）使用，
  命令入口不改变它们。
- **允许列表，绝不用拒绝列表。** 每个服务用引擎生成的分类 `skill/safety.json`
  和自己的 `REVIEWED` 审查结论构建门禁（`crates/engine-area/src/door.rs`，
  `Door`）。一次调用中的每条命令都在任何命令运行之前先经过准入。只有归为
  `safe` 的 id 可以运行，另外还有审查者确认只读取其参数所指文件的 `file`
  命令：门禁在调用方的文件夹内解析该路径（相对路径、不含 `..`、跟随链接、
  必须是已有文件），再把绝对路径交给引擎。写入只经过入口自己的 `out`
  （`Area::write`：代理的写入从不覆盖已有文件，并受配额限制）。其他 id
  一律拒绝：`code`、`network`、`device` 和 `host`，未经审查的 `file` 命令，
  以及分类中没有的 id（只要引擎还有 id 没有类别，技能漂移测试就会失败）。
- **组合命令与间接命令。** 批处理、宏和脚本（`command.batch`、`engine.batch`、
  `tools.macros`、`file.runScript`）归为 `code`，整条拒绝。按键名修改应用级
  状态的设置命令，只有使用其服务审查过的键名时才能运行；目前没有任何键名
  经过审查，所以 cad 的 `setvar` 被拒绝。指名另一条命令的命令（vector 的
  `perspective.draw {command}`）会让被指名的 id 连同其参数再次经过准入，最多
  嵌套四层。指名效果的命令（`effect.apply`、vector 的 `appearance.addEffect`）
  只运行引擎内置的效果，因此效果插件（`plugin.<id>`）绝不会经由入口运行。
- **上限（2026 年 10 月 9 日决定）。** 在 #399 之前，引擎工作都在 Shell 的 UI 线程上运行，
  因此任何单次调用都不能无限放大工作量或内存。每个服务都审查了会放大工作量的参数（阵列和
  复制的数量、行数和列数、画布、页面和渲染尺寸、帧范围和帧率、迭代次数，以及会成倍增加绘制
  工作的小比例和小间距），并为每个参数设一个 `Limit`：对单个参数或若干参数乘积的上限，理由
  写在旁边，由门禁在任何命令运行之前检查，内层命令也不例外。复制在一次调用内相乘（阵列再
  阵列），受每次调用的预算约束。每个服务还在每条命令之后把文档保持在尺寸上限以内（这能拦住
  没有数量参数的复制粘贴循环），并限制自己的 `out`（渲染的像素、导出的帧数）。
- **每条命令之后的围栏。** 命令可能写进文档、之后又会被后续命令、渲染或 `out`
  读取的内容，会在每条命令之后检查，不通过则调用在写出任何东西之前失败：
  vector 的链接图片；effect 的素材、LUT、OCIO 和 mocha 参数（包括 Essential
  Graphics 取值）以及效果插件；film 的效果参数、暂存盘、采集文件夹和排队的导出
  （区域外的媒体照旧保持离线）；photo 的链接智能对象和 Color Lookup 文件。
- **#418 的几条路径。** 包装 `plugin.install` 的批处理、用 `prefs.set` 设置插件
  文件夹（vector：`code`；effect 和 film：`host`），以及
  `effect.apply {effect: "plugin.<id>"}` 都会被拒绝，每条都在其服务的测试中有
  恶意样例，并在 Shell 的中继里再验证一次
  （`every_command_door_refuses_what_its_review_does_not_admit`）。#419 的围栏
  保持不变：design 的 IDML 链接和 pdf 的脚本（两者都没有入口），effect 的 LUT
  与色彩文件以及 photo 的 `.psd` 链接（每条命令之后都检查），以及 word 的
  `review.readAloud`（现归为 `device`）。

每个引擎的工具面：

| 引擎 | 系统代理的工具 | 入口在 `safe` id 之外还运行什么 | 上限（单次调用） |
| --- | --- | --- | --- |
| word | `word.info`、`word.run` | 389 个 id 中的 328 个。读取：`insert.picture`、`picture.change`（`path`）。 | 表格 ≤ 10,000 个单元格；页面每边 72–1584 pt；一次替换使文本最多增长 1,000 倍（连续替换合计 ≤ 10,000 倍）；文档 ≤ 500,000 个字符、50,000 个段落、128 MiB 图片；PDF ≤ 10,000 页。 |
| deck | `deck.info`、`deck.run` | 222 个中的 203 个。读取：`insert.picture`、`insert.audio`、`insert.video`、`picture.change`（`path`）。 | 表格 ≤ 5,625 个单元格；图表 ≤ 10,000 个数据点；单张幻灯片面积 ≤ 1920 × 1080 pt；演示文稿 ≤ 500 张幻灯片、20,000 个形状、1,000,000 个字符；每次调用的栅格化 ≤ 160 MP，每张 ≤ 4096²。 |
| cad | `cad.info`、`cad.run` | 295 个中的 288 个。`setvar` 被拒绝：它按名称设置变量，而没有任何名称经过审查。 | 阵列和复制 ≤ 10,000 份，一次调用内相乘 ≤ 10,000；多边形 ≤ 1,024 条边；样条拟合点 ≤ 2,000；填充和线型比例 ≥ 0.0001；图形 ≤ 200,000 个对象；渲染约 ≤ 一秒的绘制工作量，先估算再绘制。 |
| light | `light.info`、`light.run` | 239 个中的 189 个。没有其他。 | 原图 ≤ 64 MP；导出 ≤ 16 MP（AVIF ≤ 4）；≤ 16 张照片（含虚拟副本）；≤ 16 个蒙版、256 笔画笔、64 个污点；裁剪每边 ≥ 1%。 |
| film | `film.info`、`film.run` | 675 个中的 525 个。读取：`captions.import`（`path`）。只运行内置项：`effects.apply`、两个转场命令、`effects.setDefaultTransition`、`mixer.addInsert`、`presets.apply`、`lumetri.applyPreset`、`essentialSound.applyPreset`。 | 序列每边 ≤ 4096 且 ≤ 9.4 MP，≤ 120 fps，≤ 96 kHz；速度 1%–10,000%；时长 ≤ 24 小时；一次调用最多添加 5,000 个元素；分析 ≤ 18,000 帧；导出 ≤ 18,000 帧。 |
| effect | `effect.info`、`effect.run` | 665 个中的 460 个。只运行内置项：`effect.apply`。 | 合成 ≤ 8.85 MP（4096 × 2160）、≤ 36,000 帧、1–240 fps；中继器副本 ≤ 1,000（每次调用 ≤ 10,000）；约 120 个效果参数设了上限；项目 ≤ 5,000 个项目、图层和效果。表达式归为 `code`：含表达式的项目只保存，不渲染。 |
| vector | `vector.info`、`vector.run` | 679 个中的 574 个。只运行内置项：`effect.apply` 和 `appearance.addEffect`（经 `effect` 或 `id`）；`perspective.draw` 只运行 `shape.*` 命令，每条再经过准入。 | 形状 ≤ 1,000 个点；混合 ≤ 1,000 步；重复、马赛克和网格 ≤ 10,000 份（每次调用 ≤ 10,000）；变换效果 ≤ 1,000 份；文档按绘制计 ≤ 20,000 个节点、100,000 个对象；栅格 `out` 每边 ≤ 8192 px 且 ≤ 16 MP。 |
| sound | `sound.info`、`peaks`、`convert`、`trim`、`mix` | 没有入口：soundcraft 没有命令目录。 | — |
| design | `design.info`、`render`、`export` | 按决定不设入口（#418）。 | — |
| pdf | `pdf.info`、`text`、`render`、`merge`、`split` | 没有入口：只有几个固定操作。 | — |
| photo | 无（照片应用自己的 `photos.info`） | `photo.run` 只供应用自己的请求使用：817 个 id 中的 692 个，并且每个还要通过 photocraft 自己的工作区检查。 | 尚未设上限（只供应用自己的请求使用）。 |
| sheet | 无（Sheets 应用自己的 `sheets.*`） | 没有入口：公式求值。 | — |

这次审查重新归类了这些 id：word 的 `review.readAloud`（`code` 改为 `device`），vector 的
`effect.apply` 和 `appearance.addEffect`（`code` 改为 `safe`，并检查其效果），effect 的两个
媒体浏览器收藏命令（`safe` 改为 `host`），effect 中设置或链接表达式的 14 个命令（`safe` 改为
`code`：effectcraft 运行表达式时没有时间、步数或内存预算），以及 photo 的
`layer.smartFilter.setParams`（`safe` 改为 `file`：它可能写入一个 Color Lookup 文件路径）。审查还堵上了两条 id 检查看不到的路径：
预合成中通过 Essential Graphics 取值设置效果的 LUT 文件（effect），以及指名文件的 Color Lookup
智能滤镜（photo）；两个服务的围栏现在都会在打开时和每条命令之后拦下它们。即使在上限之内，
一次调用仍可能占用 UI 线程数秒（一次 4K 导出、一组参数都在上限内的重型效果）；把引擎工作
移到带超时的工作线程上是 #399 的事。

只能由应用自己的 `host.request` 调用的 `photo.run` 现在也经过同一个门禁。每个
有入口的引擎的 `SKILL.md` 都列出 `info` 和 `run`，说明入口的规则，并用示例教
命令；其服务的测试会运行每个示例（`the_skill_examples_run`），Shell 则对照工具
的 schema 检查它们。

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
