# octosense-news-service: the `news` host service

News's data service ([ADR 0002](https://github.com/OctoSense-org/OctoSense/pull/58),
milestone M1). Code, not a model, collects the stories: on a timer, while the
shell runs and whether or not News is open. The News app (`os.news`,
[../bundle](../bundle)) reads them through `host.request`, so it opens from
the cache; from M2, News's agent gets the same calls as tools.

## Sources

| Source | What | Host |
|---|---|---|
| `hn` | Hacker News front page (Algolia JSON) | `hn.algolia.com` |
| `techmeme` | TechMeme RSS | `www.techmeme.com` |
| `google` | Google News top stories RSS (en-US) | `news.google.com` |
| `bbc-world`, `npr`, `guardian-world`, `ars` | the default RSS list | `feeds.bbci.co.uk`, `feeds.npr.org`, `www.theguardian.com`, `feeds.arstechnica.com` |
| `topic-<hash>-google` | a followed topic: Google News RSS search (or a section, `topic:BUSINESS`) in its language and region | `news.google.com` |
| `topic-<hash>-gdelt` | the same topic in GDELT DOC 2.0 (`mode=ArtList&format=json`, `sourcelang:`, last day) | `api.gdeltproject.org` |
| `feed-<hash>` | feeds imported from OPML (`news.feeds.import`) | must be declared |

The first three are the bundle's own tabs, with the same URLs (a test checks
it). Every request goes to a host in the bundle's manifest `network.hosts`,
redirects included: the manifest is the service's allow-list too (compiled in
from `../bundle/manifest.json`). An OPML feed on any other host is reported
as skipped, not fetched.

Politeness: `User-Agent: OctoSense-News/1.0`; `If-None-Match` and
`If-Modified-Since` from the last answer; 10 s to connect, 30 s in all;
bodies over 4 MB refused; 2 s between requests to one host (6 s for GDELT,
which asks for 5); each source at most every 10 minutes on the timer (every
15 minutes) and every 2 minutes on `news.refresh`; a failure (a 429 or 5xx
included) backs the source off (twice the interval, doubling, at most 6 hours)
instead of retrying.

## Stories and the ledger

An item is `{id, title, url, source, feed, lang, published, fetched, summary,
image?, topics, discussion?, points?, comments?, also?}`. `id` is 16 hex
digits of the SHA-256 of the canonical URL (https, no `www.`, no `utm_*`,
`fbclid` and other tracking parameters, the rest sorted, no trailing slash).
Times are Unix seconds. `topics` holds the feed's category (`tech`, `world`)
and the queries of the followed topics that found it. `also` lists the other
sources that carried the same story.

The **ledger** remembers every story seen, by id and by title key, for 60
days (at most 50,000). A story is not new again when its canonical URL was
seen, when its title matches a seen one after normalisation, or when it shares
at least 80% of its words (five or more) with a story fetched in the last three
days: then the stored copy gains the source in `also`, its topics, and (from
its own feed) fresher points and comments. The store keeps 3,000 stories for
14 days.

Files, under the host's directory, outside every app's jail
(`<host_dir>/news/`): `items.json`, `ledger.json`, `sources.json` (per source:
ETag, Last-Modified, last success and error, failures, next due, the ids of its
latest fetch), `topics.json`, `feeds.json`.

## Tools

| method | args | answer | risk |
|---|---|---|---|
| `news.list` | `{since?, topic?, lang?, feed?, current?, limit?, offset?}` | `{total, offset, updated, items: [item]}`, newest first. `feed` narrows to one source's stories; with `current: true`, exactly its latest fetch in the source's own order, plus `feed_error` (the last error or null). `limit` defaults to 50, at most 500 | Read |
| `news.read` | `{id, full?}` | `{item, text, full_text}`: `text` is the stored summary, or the article's main text when `full` and the shell granted a reader | Read |
| `news.topics.get` | – | `{topics: [{query, lang, region}]}` | Read |
| `news.topics.set` | `{topics: [{query, lang, region}]}` | `{topics}` as stored: whitespace collapsed, `lang` lower case (ISO 639-1), `region` upper case (ISO 3166-1), duplicates dropped, at most 20; fetched on the next run | in-app Act |
| `news.refresh` | – | the run's report (below), or `{busy: true}` while a run is going | in-app Act |
| `news.sources` | – | `{sources: [{id, label, kind, host, lang, topics, last_success, last_error, failures, next_due, items}]}` | Read |
| `news.feeds.import` | `{opml}` | `{added: [id], skipped: [{url, reason}]}` | in-app Act |

The service answers system apps (`os.*`) only. Errors come back as the
request's error string.

**Full text is a separate capability.** Article hosts are arbitrary, so the
service does not fetch articles itself. A shell that has a safe reader (a
browser that renders under its own policy and reduces a page to its main
text, ADR 0002 §6) passes it as `Options::reader`; without one,
`news.read {full: true}` returns the summary and `full_text: false`.

## The event

After each run that fetched anything, the service calls the shell's hook with
a `FetchReport`:

```json
{"at": 1789570000, "new": 12, "total": 640,
 "clusters": [{"topic": "tech", "count": 7}, {"topic": "electric cars", "count": 3}],
 "sources": [{"id": "hn", "status": "ok", "new": 4}, {"id": "techmeme", "status": "not_modified", "new": 0},
             {"id": "npr", "status": "error", "new": 0, "error": "the source answered 503"}]}
```

`status` is `ok`, `not_modified`, `skipped` (not due) or `error`. The hook
runs on the fetch thread. It is the "N new items" event that wakes News's
agent in M3; nothing is wired to it yet.

## Registering it (shells)

```rust
// Defaults: the network, the manifest's hosts, a 15-minute timer that starts
// at the first request (whose host_dir it takes).
let news = octosense_news_service::register();

// What a shell normally passes: the same host directory it gives the Card
// runner, so fetching starts with the shell rather than with News, and the
// event hook.
let news = octosense_news_service::register_with(
    octosense_news_service::Options::default()
        .host_dir(host_dir)                       // <host_dir>/news
        .on_fetch(|report| forward_to_news_agent(report)),
);
```

`register_with` returns a `News` handle for calling the tools directly
(`list`, `read`, `topics`, `set_topics`, `refresh`, `source_status`,
`import_opml`). Other options: `fetcher` (tests), `reader`, `clock`,
`timer`, `interval`, `source_intervals`, `spacing`, `gdelt`,
`default_feeds`, `hosts`, `retention`.

The bundle asks `host.has("news")`: where the shell grants `news`, it lists
each tab with `news.list {feed, current: true, limit: 30}` and asks for a
`news.refresh`; elsewhere (and where a granted `news` gets no answer from a
service) it fetches in its script, as before. App Hub's admission knows only
the capabilities in `octosense_app_policy::KNOWN_CAPABILITIES` and refuses any
other, so `news` has to be added there (beside `mail` and `llm`) before the
manifest can ask for it; until then News keeps its own fetch in every shell.

## Tests

```sh
cargo test -p octosense-news-service                   # fixtures, no network
cargo clippy -p octosense-news-service --all-targets --no-deps -- -D warnings
cargo test -p octosense-news-service -- --ignored live # the real feeds, once
```

Parsing (RSS 2.0, RSS 1.0/RDF, Atom, Hacker News, GDELT, OPML) is tested from
[tests/fixtures](tests/fixtures); runs, the ledger, near-duplicates, the list
filters, topics, the host allow-list, back-off, `news.read` and App Hub's
dispatch path through a fixture fetcher and a moved clock
([tests/service.rs](tests/service.rs)); the bundle's agreement with the
service and its fallback in [tests/bundle.rs](tests/bundle.rs).
