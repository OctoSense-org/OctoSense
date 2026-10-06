# 新闻卡片

[English](README.md) | 简体中文

News 通过 `news.list/read` 读取已经收集的新闻。打开新闻，选择 **Add to Glance**
或 **Research topic**。在 Glance 展开摘要后可阅读完整卡片并使用原生 Chat；
Open News 会回到对应的已保存新闻。

研究需要启用 News 助手并配置模型。`news.research` 在新闻应用声明的范围内运行
octos 的 `topic-brief`：英语或中文、新闻类别、最近七天。请求立即返回 `running`，
宿主完成后在同一卡片展示经过校验的摘要、最多十二条完整要点及来源引用。
不完整和失败结果保持明确标记；代理可通过 `news.research_result` 回答后续提问。
收集一条新闻本身不会调用模型或提醒用户。

服务先从自己的资料库解析新闻 ID，再调用 Shell，调用者不能通过参数替换标题
或网址。共享工具由 `bundle/tools.json` 声明，Shell 提供执行服务、调用方授权和
可信 Glance 操作。调用路径、隐私和验证范围见
[情境卡片指南](../../docs/contextual-app-cards.zh-CN.md)。
