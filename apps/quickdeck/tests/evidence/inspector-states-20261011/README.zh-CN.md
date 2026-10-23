# Quick Deck 检查器状态 — 2026-10-11

[English](README.md) | 简体中文

deck 服务现在会报告检查器要显示的内容（#507，SERVICE.md 的“Inspector states”），Quick Deck 也照此显示：文本框的垂直对齐和自动调整，图表的图例、数据标签、网格线和配色，表格的选项，每组配色方案的颜色，以及演示文稿的配色方案、字体搭配和页脚。撤销或重做之后，无需重新打开，各控件就会再次显示演示文稿自己的设置。[ui.py](../../ui.py) 在本分支上运行了这些检查，[receipt.json](receipt.json) 记录了摘要、命令、每次运行检查的内容、此前失败的各轮次以及未验证的内容。

**App Hub 的 `card-host`**（macOS 26.6.2，Apple 芯片，运行时默认的 64 ms 预算），使用像服务一样应答的开发夹具：

- [fixture/](fixture)：发生变化的页面与设计图并排，先浅色后深色（文件名以 `d` 开头；只有 10 有深色设计图）：02（文本框的“溢出时缩小文字”开关打开、顶端对齐被选中，与设计图一致）、03（图表的标题、数据标签和网格线打开，图例关闭，彩色配色）和 05（标记了 Harbor 配色方案和 Classic 字体搭配；每组配色方案都显示其颜色）。
- [before-after/](before-after)：main 上的应用（左）与本分支（右）并排，使用同一个夹具，覆盖 02、03、05、表格的选项（11）和页脚（12），浅色与深色。在 main 上，文本框的开关显示为关闭，图表没有图例却显示图例打开，表格只显示六个选项中的三个且都是默认值，页脚的幻灯片编号显示为关闭、文字为空，配色方案和字体搭配由主题推测。
- 这些运行还把每个开关和标记与替身核对，并在改动及撤销后核对一个表格选项和页脚的日期（这些截图没有保留：它们与 11 和 12 相同）。

**隐藏运行、使用真实 `deck` 引擎的桌面 Shell**（Ubuntu 26.04 LTS，x86_64，72 个 CPU，没有 GPU；与之前一样使用无头 weston）：Shell 由基于 main eb2fd5d1 的本分支构建，main 已包含 #503 和 #507 的服务。每张截图都是应用窗口，1292 x 662 点，一像素对应一点。在流程的浅色部分，每个状态都先读取，再改动，然后从引擎读回（重新选中形状，或重新查询演示文稿的设计），再撤销并再次读回（[linux/states.json](linux/states.json) 记录了引擎最初报告的内容）：

- [linux/10-box.png](linux/10-box.png)、[10b-box-undone.png](linux/10b-box-undone.png)：Agenda 文本框，引擎把它画在顶端并缩小文字以适应；改为底端对齐并关闭缩小（引擎把文字移到下方），然后两项都撤销。
- [11-chart.png](linux/11-chart.png)、[11b-chart-undone.png](linux/11b-chart-undone.png)：新图表的数据标签打开（引擎在柱子上方画出数值），然后撤销。
- [12-table.png](linux/12-table.png)、[12b-table-undone.png](linux/12b-table-undone.png)：新表格的汇总行打开，然后撤销。
- [13-colours.png](linux/13-colours.png)、[13b-colours-undone.png](linux/13b-colours-undone.png)：选择 Ocean 配色方案，然后撤销：Ember 自己的配色方案再次被标记。
- [14-footer.png](linux/14-footer.png)、[14b-footer-undone.png](linux/14b-footer-undone.png)：打开幻灯片编号（幻灯片显示其编号），然后撤销。

流程的其余部分（深色流程、重启）与 [v2-20261011](../v2-20261011/README.zh-CN.md) 一样通过。

每张截图都经过检查。
