# News cards

English | [简体中文](README.zh-CN.md)

News reads its collected stories through `news.list/read`. Open a story and
choose **Add to Glance** or **Research topic**. Expand its Glance summary for
the full card and native Chat. Open News returns to that exact stored story.

Research requires an enabled News assistant and a configured model provider.
`news.research` starts octos's `topic-brief` workflow under News's declared
English/Chinese, news-category, seven-day scope. It returns `running` promptly.
The host publishes the validated summary, all twelve possible points and
source citations in the same card. Partial and failed runs remain labeled;
`news.research_result` lets the agent answer a later follow-up. Fetching a
headline alone does not start a model call or alert the person.

The service resolves the stored story ID before calling the shell. A caller
cannot replace its headline or URL with tool arguments. The shared tools are
declared in `bundle/tools.json`; the shell supplies the executable service,
per-caller grants and trusted Glance actions. See the
[contextual-card guide](../../docs/contextual-app-cards.md) for the call path,
privacy, test status, and limitations.
