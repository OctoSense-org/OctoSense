# Markdown 编辑器

[English](README.md)

`octosense-markdown-editor` 将 Rinx 的通用文章组件作为 `MarkdownEditor`
提供给 OctoSense 隔离应用。组件本身不能访问文件系统、网络、账号或发布服务。
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

构建与测试命令见英文说明。`editor-host` 是无真实账号的原生 UI 测试宿主，
默认拒绝所有服务操作；`--fixture-provider` 仅提供虚构仓库和文件供 UI 测试，
仍然拒绝提交。它不能证明 App Hub 安装、OAuth、宿主授权或真实 GitHub 提交
已经通过，这些需要在集成后的 OctoSense 中验证。
