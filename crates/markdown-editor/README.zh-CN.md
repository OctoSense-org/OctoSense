# Markdown 编辑器

[English](README.md) | 简体中文

`octosense-markdown-editor` 将 Rinx 的通用文章组件作为 `MarkdownEditor`
提供给 OctoSense 隔离应用。组件本身不能访问文件系统、网络、账户或发布服务。
草稿由应用保存，GitHub 提交通过独立的宿主审核流程完成。

组件与原生 Rinx 模块共用工作区固定的 Rinx v1.1.0 版本，复用 `article-core`
与 `article-makepad` 的文档模型、保留原始 Markdown 的导入、撤销历史、原生
富文本输入与输入法/剪贴板、跨段落选择、样式以及完整 Markdown 渲染器。
可视化模式暂不支持的结构保留为可编辑 Markdown 块；写作、源码、预览共享一份
文档。无法解析的源码仍保留在源码页，修正前不能切换到可视化写作。

这里没有嵌入 Rinx 的 Matrix 发布控制器，也未包含图片选择器、二进制附件上传、
Matrix 作者/房间字段和发布工作流。图片链接语法可以保留，但不会自动联网加载
远程图片。当前编辑器采用 Rinx 的 512 KiB 文档上限，GitHub 宿主读取上限则为
1 MiB，超限文件加载会返回错误并保留原草稿。

宿主在创建隔离应用之前调用 `register()`。组件会在绘制和输入时进入自己的
脚本隔离环境，避免 Rinx 延迟计算样式时读到其他应用的堆。
`ui.editor.load(markdown)` 成功返回空字符串，失败返回错误且不替换文档；
`text()` 读取当前 Markdown，`on_change` 接收用户修改。程序加载不会伪装成
用户编辑。

构建与测试命令见英文说明。`editor-host` 是无真实账户的原生 UI 测试宿主，
默认拒绝所有服务操作；`--fixture-provider` 仅提供虚构仓库和文件供 UI 测试，
仍然拒绝提交。它不能证明 App Hub 安装、OAuth、宿主授权或真实 GitHub 提交
已经通过，这些需要在集成后的 OctoSense 中验证。

## Rinx 写作界面

主界面复用 Rinx 真正的文章写作布局：紧凑图标栏、Markdown 源码、桌面分栏预览、
逐块渲染的纸张预览、桌面格式栏和手机底部格式栏。宽度小于 960 点时，源码与预览切换显示。
悬停或长按图标可查看用途。桌面样式侧栏、手机预览中的样式底部面板提供主题，
以及辅助的块编辑器、撤销和重做入口；插入表格使用 Rinx 原生尺寸选择器。
源码、分栏预览和块编辑器使用同一份 Markdown。切换视图、预览主题和设置文件名不会改写文件。
无法解析的源码仍是当前草稿，修正后才可进入块编辑器。主题仅为组件内的显示状态，
不会向 GitHub 文件插入元数据。图标和代码来源见 [NOTICE](resources/NOTICE.md)。

组件占满工作区，外层不要再叠加标题、保存栏或模式按钮行：

```splash
editor := MarkdownEditor {
    width: Fill height: Fill
    on_change: |markdown| save_local_draft(markdown)
    on_repository: || choose_repository_and_file()
    on_save: || request_host_review()
}
```

`ui.editor.set_destination(filename)` 设置只读文件名；`set_status(message)` 显示简短保存状态，
长通知在内容下方换行。两个方法都没有文件或网络副作用。返回/仓库、保存图标只调用应用回调，
远程写入仍经过宿主审核。`load`、`text` 和 `on_change` 的原始 Markdown 合约不变。

原生测试通过 ID 定位图标：`repository_button`、`save_button`、`source_mode`、
`split_mode`、`preview_mode`、`markdown`。桌面样式按钮是 `palette_button`，手机预览是
`style_fab`；辅助控件桌面为 `rich_mode`/`undo_button`/`redo_button`，手机为
`mobile_rich_mode`/`mobile_undo_button`/`mobile_redo_button`。
测试宿主 `--wide` 使用 1200×820，默认 430×850；桌面窗口证据不等于 Android 输入法验收。

已执行的无真实服务原生回归命令：

```sh
python3 crates/markdown-editor/tests/native-writer.py --output target/markdown-writer-native-final
```

## 文档页面

本 crate 还提供 Writer 在 Mac 上使用的页面 `DocumentEditor`，以及用于只读预览的
`DocumentScroller` 和 `DocMark`。`DocumentEditor` 复用 Rinx 的富文本输入和跨段落选择，
每行一个输入框，标题、列表编号、引用和强调都按打印效果显示。它的文本仍是应用的行格式
（`# Title`、`- item`、`1. item`、`> quote`、`**bold**`、`*italic*`）；只有用户改动的行会被重写，
因此文档的保存形式不会自行变化。这里不使用 `MarkdownEditor`：它的模型没有一级标题，
一次编辑会重写整个文档，而且自带界面外壳。

```splash
editor_text := DocumentEditor {
    width: Fill height: Fill
    on_change: |text| draft_changed(text)
    on_format: |style, bold, italic, line, selected| format_changed(style, bold, italic, line, selected)
    on_save: || save_doc()
}
```

`ui.editor_text.text()` 和 `set_text(s)` 读取和载入文本；`headings()` 返回 JSON
`[{level, text, line}]`；`bold()`、`italic()`、`set_style(id)`、`go_to_line(n)`、`focus()`、
`set_active(on)`、`cut()`、`copy()` 和 `paste()` 作用于光标或选区。
`find(words, step)` 选中这些词出现的一处（不区分大小写）并滚动到可见位置，键盘焦点保持不变：
`step` 为 0 时取选区处或其后的第一处，1 取下一处，-1 取上一处，到文档末尾后从头循环。它返回 JSON
`{index, count}`，`index` 从 1 开始计数，找不到时为 `{0, 0}`。`find_done()` 把键盘交还给页面，
停在匹配处。`replace_found(words, with, all)` 用 `with` 替换选中的匹配并前往下一处；`all` 为 true 时替换
所有匹配（作为一次可撤销的修改）。替换的文字沿用被替换文字的粗体和斜体，返回 JSON
`{index, count, replaced}`。Mac 编辑菜单的其余命令：`on_format` 的 `selected` 表示是否有选区（菜单行从脚本的状态读取它，而不是询问编辑器），`delete_selected()`
删除选区而不改动剪贴板，`paste_matching()` 即“粘贴并匹配样式”（把剪贴板的文字当作在该处键入：其中的
Markdown 保持为字符，之后的每一行按回车键的方式成为新的一行；在页面上按 Option-Shift-Command-V 亦可），`change_case(kind)` 即“转换”，取值
`"upper"`、`"lower"` 或 `"capitalize"`（每个词首字母大写，词以空格为界），保留选区及其粗体和斜体。
在选区之外右键点按时，会先选中指针下的词（或把光标放到那里），再打开页面外层的菜单。
`paper := Sheet{...}` 是文字下面的纸张，`page_gap` 是纸张上方的留白：纸张的上边缘随第一行一起
向上滚动，和真实页面一样。
`DocumentScroller{on_mark: |k| ...}` 用 `scroll_to_mark(k)` 和 `scroll_to_top()` 滚动含有
`DocMark{mark: k}` 标题的页面，用 `reveal_mark(k)` 让某个标记刚好进入视野（向上时，若标记在第一屏内，
页面回到顶部，让它上方的内容也可见），并报告哪个标记已到达顶部。页面以 `DocMark{end: true}`
结尾：滚动接近它时，`on_near_end: || ...` 会请求长页面的下一步；在最后一屏，末尾的标记虽然滚不到顶部，也会依次被报告。
