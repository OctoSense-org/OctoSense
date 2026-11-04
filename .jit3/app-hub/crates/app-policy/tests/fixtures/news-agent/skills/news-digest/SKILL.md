---
name: news-digest
description: Write a cited morning or evening digest from collected stories.
---

# Writing a digest

1. Group the stories from `news.list` by event, not by source.
2. For each group, read the most complete source with `news.read`.
3. Summarise each group in at most three sentences, citing every story id used.
4. Order sections by how many sources reported the event, then by recency.
5. Call `news.digest.write` once with the whole digest.
