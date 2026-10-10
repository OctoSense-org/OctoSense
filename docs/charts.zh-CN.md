# OctoScript 应用的原生图表

[English](charts.md) | 简体中文

Shell 将固定版本的 Makepad D3 图表注册到普通 Splash 隔离 VM 中。
已安装应用及其 Glance 卡片使用同一套图表，同时保留各自的应用身份、存储、
用户授权和生命周期。图表由 Makepad 原生矢量渲染器绘制，不依赖浏览器或 JavaScript。

这项集成晚于桌面 RC4；RC4 **没有**注册 `d3`。必须使用图表的应用可以在
`"requires":["host-api-v1"]` 之外声明
`"host_api":{"required":{"charts.d3":1}}`。这是兼容性检查，不是权限。
`runtime.describe` 将 `charts.d3` 描述为不可通过请求调用的运行时 ABI。
如果应用也提供完整的非图表界面，可以不声明这一要求，并使用
`try { d3.BarChart != nil } catch { false }` 判断是否显示图表。

完整组件名称是 `d3.LineChart`、`d3.BarChart` 和 `d3.Heatmap`：

| 组件 | 数据 | 脚本方法 | 事件 |
| --- | --- | --- | --- |
| `LineChart` | 数字（以索引为 x）、`[x y]` 数对或 `{x: … y: …}` 对象 | `set_data`、`set_domain(y_min, y_max)`、`data`（只返回 y 值） | `on_click`、`on_hover`：x 最接近的点的索引 |
| `BarChart` | 数字数组，以及独立的字符串 `labels` 数组 | `set_data`、`set_labels`、`set_domain`、`data` | `on_click`、`on_hover`：柱的索引 |
| `Heatmap` | 由数字数组组成的行数组 | `set_data` | `on_click`、`on_hover`：行、列 |

```text
line := d3.LineChart {
    width: Fill height: 240
    data: [[0 3], [1 7], [2 5]]
    on_click: |index| ui.status.set_text("Selected point " + index)
}
```

相邻的嵌套数组之间需要逗号。`Heatmap` 的 `colormap` 支持 `viridis`、
`plasma`、`inferno`、`magma`、`coolwarm`、`turbo` 和 `gray`，还支持 `cell_gap`。
颜色根据当前网格的最小值、最大值归一化；没有固定颜色值域或内置日期标签。
折线图的 x 轴是数字，不会自动格式化日期。这些脚本组件不提供多序列、缩放或
框选 API。请用普通组件补充标签及选中项的说明。

空数据会触发图表库的演示数据。加载中、缺少数据或请求失败时，必须显示明确的
状态消息；仅在真实数据准备好以后创建图表，不能用空图表代替状态界面。

Shell 同时注册该固定版本中的其他原生 D3 图表系列，但不提供旧的嵌套 VM 容器
`d3.Octoscript`；应用继续使用宿主管理的普通 Splash 生命周期。图表库 README 中
关于坐标轴标签的历史警告针对较早的 Makepad 开发版本，不能当作当前验收结论。
每个发布平台都需要实际检查标签及图形的点击区域。

原生测试程序为 `crates/shell/examples/charts-host.rs`，驱动脚本为
`tools/test-native-charts.py`。它检查绘制区域、点击、数据替换及脚本错误，并保存
截图供视觉检查。组件树快照可能早于原生字体和矢量资源上传完成，因此首次截图
前会明确等待三秒。测试宿主的 `--script=<path>` 参数可加载本地 Splash 界面用于
排查 UI；该示例不注册账号或宿主服务代理。

macOS 原生测试的八项检查全部通过，包括三个图表的点击回调，以及折线图和柱状图
的数据替换、读回。人工检查了初始与更新后的截图，坐标轴、数字刻度、分类标签和
热力图单元格均可见。Glance 单元回归使用真实的工作区渲染路径；它与普通隔离 VM、
冷启动兼容性回归均通过。这些是开发宿主的验证结果，不代表已发布的 Shell 已支持
图表。Android、Windows、Linux 的原生图表渲染仍为**未验证**。
