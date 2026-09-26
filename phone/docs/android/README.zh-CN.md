# Android 性能与启动器记录

[English](README.md) | 简体中文

这些是手机 Shell 在 OnePlus 6 测试台上的工程记录（2026 年 9 月 15–16 日）。其中引用的原始运行数据、trace、APK 和辅助脚本存放在测试台机器的 `target/perf-artifacts/` 中（被 git 忽略）；记录本身保留了哈希值和每次运行的数据表。

- [perf-gap-analysis.md（英文）](perf-gap-analysis.md)：帧时间都花在哪里：按场景的诊断、成本排序、kgsl GPU/频率测量、Vulkan 构建为何更慢，以及已落地的两轮候选优化（场景缓存、扁平材质、面板截取、延迟应用截取、淡入淡出浮层）。
- [performance-plan.md（英文）](performance-plan.md)：目标、测量约定，以及标注日期的状态段落。
- [launcher-plan.md（英文）](launcher-plan.md)：Home 角色的决策、公开 API 能提供和不能提供的能力、第 4 阶段（特权）探测，以及桌面页面的下拉手势。
- [system-integration-plan.md（英文）](system-integration-plan.md)：通过配套 APK 和 root 服务实现通知、短信、网络和电源集成；手机的实际能力、权限边界和实施顺序。
- [api-call-telemetry.md（英文）](api-call-telemetry.md)：实时 Binder 流、Perfetto 调用计时、针对性的 Java/原生 API 检查、解码限制，以及建议的采集流程。
- [ADR 0001：混合式 Android 启动器与系统桥接（英文）](../adr/0001-hybrid-android-launcher-and-system-bridge.md)：已采纳的架构，涵盖与原生启动器对齐、软件包/接口契约、权限边界、恢复、发布节奏和验收标准。
- [adr-0001-implementation-record.md（英文）](adr-0001-implementation-record.md)：Home/桥接的真机证据、真实通知夹具测试、原生 Quickstep 实验及其回滚、工具版本，以及剩余的对齐里程碑。
- [systemui-shade-replacement.md（英文）](systemui-shade-replacement.md)：全局 OctoSense 通知/控制面板、真机验证、确切的部署产物和恢复方法。
- [octosense-systemui-build.md（英文）](octosense-systemui-build.md)：原生 SystemUI 分支、设备控制页预览证据、构建流程，以及发布签名部署的边界。
- [perf-findings-oneplus-6t.md（英文）](perf-findings-oneplus-6t.md)：较早的 OnePlus 6T / Android 11 测试结果；请勿将其数据与 OnePlus 6 的数据混在一起。
- [validation-record.md（英文）](validation-record.md)：测试台的验证日志：确切的补丁和 APK 哈希、运行组、被否决的方案。
- [vulkan-probe-record.md（英文）](vulkan-probe-record.md)：未经修改的 Vulkan 后端在同一台手机上的表现，附带其 kgsl trace。
- [../build-tool.md（英文）](../build-tool.md)：为什么分支版的 `cargo-makepad` 是框架的一部分：其 activity 与上游有何不同、原版工具会破坏什么，以及如何让 `cargo makepad` 始终使用分支版工具。
