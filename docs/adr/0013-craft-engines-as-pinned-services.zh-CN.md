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

## 待决问题

- 第一个服务写出后，服务 crate 的归属（`crates/craft-*` 还是各应用的
  `apps/<name>/host-service`）。
- soundcraft 的引擎（以及我们 makepad fork 尚未收录的 `audio_aot`）是
  否足以支撑本季度的可脚本化音效线。
- 手机打包：哪些引擎（若有）进入 Home 而非仅桌面。
