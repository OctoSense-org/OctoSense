# octosense-toolbox: workflow templates

The system toolbox's library of **OctoScript workflow templates** ([ADR 0002](../../docs/adr/), section 6, "Workflow templates", proposed in OctoSense PR #77). A template is a fixed, bounded procedure (a news digest, a multi-language topic brief, a plan from the weather) written in OctoScript with a manifest that says what it may call. An app's agent picks one and fills its parameters: one model call to choose, instead of a multi-call tool loop. The host then runs the independent steps concurrently.

This crate holds the library, the runner, forks, evaluation, the `mod.research` v1 host module, and the `workflow.*` tool surface as a Rust API. **The shells do not link it yet.** See [What remains](#what-remains).

The first templates are ported from the AppCard research experiment ([`apps/appcard/tools/splash-research`](../../apps/appcard/tools/splash-research)). Its composition harness (Python) was not used; what was needed is in Rust here.

## Templates

| id | What it does | Host calls (max) | Model calls (max) |
|---|---|---|---|
| `news-digest` (1.2.0) | Optional query translation; search; read the top N articles concurrently, skipping results the backend cannot read; pages that do not mention the topic (`on_topic: false`) and articles the model marks off topic are counted in `off_topic` and left out; one digest in the requested language with citations | 8 | 2 |
| `topic-brief` (1.2.0) | Translations per language, all started together; one search per language (asking for `max(per_language, read_top)` results); reads in rounds that give every language a read first whenever it found a readable article, take at most `per_language` from one language until no language has a candidate left within that share, then give the unused share of `read_top` to the languages that still have readable results; skip results the backend cannot read and replace a failed or off-topic read with that language's next candidate (at most `read_top` + one attempt per language); one brief, without the articles the model marks off topic. Each language's `queries` entry counts `found`, `unreadable`, `read` and `off_topic` | 20 | 5 |
| `weather-plan` | Forecast and air-quality searches started together; reads run concurrently; one plan with citations. Air quality is optional (the result is `partial` without it); a forecast is required (the run fails without it) | 7 | 1 |
| `market-brief` | One news search per ticker, started together; per-symbol reads run concurrently; one brief (a comparison for several symbols). Research, not advice: v1 has no quote method, so prices appear only as the sources state them | 13 | 1 |
| `briefing` | Up to four topics searched together; the top articles of each are read concurrently; one briefing | 17 | 1 |
| `compare` | The same aspect of two subjects searched together; reads run concurrently; one comparison. It needs sources for both sides | 9 | 1 |

`weather-plan` and `market-brief` adapt the experiment's `weather` and `stock` workflows. Those called `forecast`, `air_quality`, `quote` and `baseline` methods, which `mod.research` v1 does not have. Here they read published forecasts and news. Structured weather and market data will come with the research engine's providers. `briefing` and `compare` come from the experiment's composition set. Its `travel`, `outdoor` and `market` families need places and quotes, which the research module does not provide, so they are deferred.

Every template ships recorded fixtures in `templates/<id>/fixtures/`, with the expected `status` and `data`. The 20 cases include ready, partial and failed runs, off-topic pages dropped by the host and by the model (`news-digest/off-topic-dropped`), read budget moving to the language with readable results (`topic-brief/budget-moves-to-readable`), and Traditional-script pages matching a Simplified search (`topic-brief/traditional-script`).

## Template format

```
templates/<id>/
  template.octoscript    the procedure (canonical OctoScript, workflow profile)
  template.json          the manifest
  fixtures/*.json        recorded cases (params, fixture, expected)
templates/library.lock.json   the digest of every library template
```

`template.json`:

```json
{
  "id": "news-digest", "version": "1.0.0", "title": "…", "description": "…",
  "params": { JSON Schema: the parameters the script sees as `request` },
  "modules": [{"module": "research", "methods": ["query", "search", "article", "digest"]}],
  "budget": {"max_calls": 8, "max_model_calls": 2, "max_pages": 7, "max_ms": 60000, "max_concurrency": 4},
  "output": { JSON Schema of `data` },
  "provenance": true,
  "lineage": {"parent_id", "parent_version", "parent_digest"}   (forks only)
}
```

The schemas use OctoScript's executable subset (`octoscript-schema`): types, `properties`, `required`, boolean `additionalProperties`, `items`, and min/max bounds with `enum`. There is no `anyOf`. A nullable field (`digest` when the model failed) is written without a `type`.

A script returns `{status: "ready" | "partial", data}`. Every optional parameter needs a `default`, because a script that reads an absent field fails. The runner fills defaults before it validates.

## Admission

`Library::builtin()` and `Library::load_dir()` load each template, validate it and pin it:

- **Manifest**: unknown fields are refused. The id is a directory-safe name. Only known modules and methods are allowed, and each only once. The budget must stay within the toolbox ceiling (64 calls, 8 model calls, 32 pages, 300 s, concurrency 8). A module that reads external sources requires `provenance: true`. Each file is capped at 32 KiB.
- **Static check** (`check::check_source`): the source must pass OctoScript's canonical syntax check. Imports may name only `mod.std.{array,assert,json,math,object,text}` and the declared host modules; `mod.tool` is always refused. OctoScript's scope-resolved call report checks every `module.method` call, local aliases included, against the declared methods. `mod` may appear only in a `use` line. An incomplete report counts as a refusal.
- **Pin**: the digest is SHA-256 over both files, with line endings normalized. Each digest must match `library.lock.json`, and the lock must name nothing else.

At run time the VM also installs **only** the declared methods, so a call the check cannot see finds nothing to call.

## Runner

`runner::run(template, app, params, host, options)`:

1. Refuses the run before it starts if the app lacks a grant the modules need (`research`) or the parameters do not match the schema.
2. Builds a `CapabilityRuntime` whose only host module is the declared one. Each method is a deferred external tool with its input and output contract from `modules.rs`. The parameters become `request`.
3. Evaluates the script. Every deferred call is claimed and dispatched to the caller's **`ToolboxHost`**, together with a `CallContext`: the app's identity, grants, scope and folder (`AppContext`), the run and template, the effective budget and what is left of it. Independent calls run concurrently, up to `max_concurrency`.
4. **Budget**: the run gets the template's budget, narrowed by the app's own budget and its scope's page limit. A call over `max_calls`, `max_model_calls` or `max_pages` is refused before dispatch. `max_pages` counts **articles read** (each `article` call, whether or not it succeeds). A `search` is one call and is not charged to `max_pages`: how many feeds and APIs it fetches is the backend's configuration (one for the fixture backend, four for the interim adapter below), and charging it let two searches starve the reads (on 27 Sep 2026 a two-language `topic-brief` used 8 of its 10 pages on feeds and read one article). The fan-out is capped at `research::MAX_SEARCH_FETCHES` (8) per search and reported as `stats.search_fetches`. The script sees a failed call and can continue (`try … catch`). When `max_ms` elapses, in-flight calls are cancelled and fail as timed out.
5. **Output**: the value must be exactly `{status, data}`, and `data` must match the output schema. Any URL in `data` must be one the host retrieved. Otherwise the run is `failed` and no data is published. A `ready` result is downgraded to `partial` when any call was refused, failed or timed out.
6. **Provenance**: the host returns provenance records with each reply (URL, title, source, retrieval time, evidence hash). The runner keeps them outside the VM and attaches every record whose id or URL `data` refers to.
7. With `write_result`, the result goes to `<app folder>/toolbox/runs/<template>/<run>.json`.

The result also carries `diagnostics`, `stats` (calls, model calls, pages, search fetches, denied, failed, peak concurrency, elapsed) and a `trace` of start, complete, denied and timed-out events. The future is not `Send`, because the VM stays on the calling thread.

## `mod.research` v1

| Method | Input | Output | Charged |
|---|---|---|---|
| `query` | `{query, language?}` | `{query, language}`: search terms in `language` | 1 model call |
| `search` | `{topic, language?, region?, limit?, max_age_hours?}` | `{items: [{id, title, url, source, language, published_at, readable}], source: {partial, providers, queried_at}}`, readable items first | 1 call; its fetches are reported, not charged to `max_pages` |
| `article` | `{id}` (a search result's id from **this run**) | `{id, title, url, source, language, published_at, excerpt, chars, truncated, evidence_sha256, on_topic}` | 1 page |
| `digest` | `{task: digest\|brief\|plan\|compare, language, article_ids, focus?}` (articles read in **this run**) | `{task, language, summary, points: [{text, citations, label?}], off_topic: [id]}` | 1 model call |

`research::ResearchHost` implements `ToolboxHost` for this module over two parts: a `ResearchBackend` (finds and reads sources) and a `ModelClient` (supplied by the host). The policy is enforced here, once, whatever the backend:

- **Scope**: every call is checked against the app's scope. Languages and regions are refused when out of scope. Results on denied domains, or outside the allowed ones, are dropped. Recency is capped at the scope's limit, and `max_pages` narrows the run's page budget. The scope has no depth limit (`max_depth` was declared and never enforced, and is removed): `mod.research` reads only this run's search results and never follows a link from a page, so every read is at depth one. A scope that still carries `max_depth` loads, and the field is ignored.
- **Ids**: search results get host-assigned ids derived from their URLs. `article` reads only this run's ids, so a template cannot fetch an arbitrary URL.
- **Evidence**: article text stays in the host, capped at 6000 bytes on a paragraph boundary and hashed. The script sees a 400-byte excerpt and the hash.
- **Relevance** (`research::relevance`, in the host so it survives the engine swap): topics are split into terms, stop-words ("of", "the", "news", "de", "新闻" …) ignored; words match whole words, case- and accent-insensitively, with a light suffix stemmer; runs of Han, kana, Hangul or Thai match as substrings after both sides are folded to Simplified Chinese with a small character table (common news vocabulary, about 540 characters; a character outside it matches only its own script).
  - `search` lists results whose headline mentions every term first.
  - `article` judges the whole page against the terms of the searches that found it, in the page's language (a page in another language is left to the model): every short name (under four letters, written with a capital: `EU`, `AI`, `Act`) and one of the longer names (`Hormuz`, `Nvidia`) must appear; a topic without names needs a third of its terms; a run of four or more unspaced characters also counts by its first or last half. A page that fails is returned with `on_topic: false` and a diagnostic (`s…: off topic, the page does not mention strait + hormuz; not digested`).
  - `digest` leaves out any off-topic article whatever the template passes, and refuses when none is left. For the `digest` task with a `focus`, the same model call also returns `off_topic`: the ids it finds are not about the focus. Their citations are removed (a point citing only them is dropped) and the templates drop them from `sources`. No extra model call.
  - This is a cheap floor, not a judge: `Taiwan Strait` passes it for `Strait of Hormuz`, which the model check then catches.
- **Readability**: a backend marks an item `readable: false` when it knows a read would fail (the interim adapter cannot resolve Google News links). The host lists readable items first, before it applies `limit`, and the templates skip unreadable ones instead of spending a read on them.
- **Digests**: the host owns the prompts for each task, and each prompt states the length limits. Validation is per point. The digest is refused only when its summary is missing, longer than 1200 characters or contains a URL, when the reply is not JSON (a code fence or a line of prose around the object is tolerated), or when no valid point is left. A point with no text, text over 400 characters, a URL, no citation, more than 8, or a citation to an article it was not given is dropped, and the rest are kept; a label over 40 characters or with a URL is dropped from its point; points beyond 12 are dropped. Each drop is a diagnostic (`research.digest (call N): digest point 3 dropped: it has no text`) that never quotes the model. So every point kept cites only articles read in this run, and no model text carries a URL. Lengths are counted in characters, as the output contract's `maxLength` is.

Backends:

- **Fixture** (`fixture::FixtureBackend` and `fixture::FakeModel`): replays recorded searches and pages with their delays. A recorded search can say how many feeds it fetched (`fetches`), carry the backend's `notes`, and mark items `readable: false`. The fake model is deterministic and extractive: it builds each point from an article's first sentence and cites it. Its config can inject a URL into the summary (`inject_url`), a point citing an unknown article (`cite_unknown`), three invalid points (`malformed_points`), or mark articles off topic by URL (`off_topic`; it still writes their points, so the host must remove them). Tests and evaluation use this backend.
- **Interim live adapter** (`research::live`, feature `live`). **Interim**: it is replaced once the octos research engine (octos#2568) and metasearch (octos#2576) land. It fetches only free structured sources: Google News RSS search, the GDELT DOC API and configured RSS/Atom feeds. It never scrapes results pages. Pages are read over plain HTTP, and `dom_smoothie` (MIT, a port of Mozilla's readability.js) extracts the main text. Pages that need JavaScript fail as partial results; Google News article links are among them, so search marks them `readable: false`. Items from GDELT (with a `sourcelang:` filter) and Google News are tagged with the query's language.
  - **Google News editions** per language (`google_news_locale`): `zh`, `zh-CN`, `zh-Hans` → `hl=zh-CN&gl=CN&ceid=CN:zh-Hans`; `zh-TW`, `zh-Hant` → `TW:zh-Hant`; `zh-HK` → `hl=zh-HK&gl=HK&ceid=HK:zh-Hant`; `en` → `hl=en-US&gl=US&ceid=US:en` (`en-GB` → `GB:en`); `es` → `ES:es`, `es` elsewhere → `es-419`; `pt` → `BR:pt-419`, `pt-PT` → `PT:pt-150`; other languages their main edition. The earlier `hl=zh&gl=US&ceid=US:zh` returned no items.
  - **Configured feeds are filtered by topic**: an item is kept only when its headline and summary (`<description>`/`<summary>`, tags stripped) mention **every** significant term of the query. The count skipped is a note (`feeds: 21 items skipped as off topic (headline and summary do not mention strait + hormuz)`). Feeds are matched to the query by primary language (`zh` feeds serve `zh-CN` searches).
  - **One slow provider does not block a search.** Providers run concurrently, each under a 12 s deadline (`LiveConfig::provider_deadline`; GDELT's 429 arrives after about 10.5 s). The shared APIs, GDELT and Google News, are asked once without retries; a 429 or 503 from one trips a breaker for its host **for the rest of the process**, so later searches, and a search already waiting for its 5 s turn, skip it at once instead of paying for it again. Every provider that failed, timed out or was skipped is a note naming it and the reason, which the runner puts in `diagnostics`: `research.search (call 0): gdelt rate-limited (429); not queried again in this process; results from other sources`, then `gdelt skipped: rate-limited (429) earlier in this process; …`. Results that are Google News links are counted too (`google-news-rss: 5 results are Google News links, which need a browser to read`). `source.partial` stays true when a provider failed or was skipped.
  - **robots.txt is an operator setting, off by default.** OctoSense agents are personal assistants that read on behalf of one person, so robots.txt is not applied by default. That holds for feeds, reads a person starts and autonomous research alike. When it is off, robots.txt is never fetched. An operator turns it on with `LiveConfig { respect_robots: true, .. }` or `OCTOSENSE_TOOLBOX_ROBOTS=1` (read by `LiveConfig::from_env()`). When it is on, the rules are RFC 9309: our product token's group, else `*`; the longest match wins; they are cached per origin; an unreachable robots.txt means the host is not fetched.
  - **Always on, whatever the setting:**
    - an honest User-Agent: `OctoSense-Toolbox/0.1 (octos research for one person; +https://github.com/OctoSense-org/OctoSense)`;
    - a minimum interval per host: 1 s, or 5 s for GDELT, as GDELT asks;
    - backoff on 429 and 503: up to 2 retries, honouring `Retry-After` in seconds or as an HTTP date; a wait over 30 s fails at once;
    - a 15 s timeout and a 2 MiB response cap;
    - no cookies, credentials or proxies, and 401/402/403 are failures, so there is no paywall or login bypass;
    - **SSRF blocking** on every fetch and every redirect hop. Loopback, private, link-local (including cloud-metadata 169.254.169.254 and `fd00:ec2::254`), unique-local, shared (CGNAT), multicast, reserved and IPv4-mapped forms are refused, both as URL literals and in DNS answers. A resolver filters the addresses the connection actually uses, so DNS rebinding cannot get past the check. `localhost` names are refused.
  - **Licenses**: `dom_smoothie`, `dom_query`, `gjson`, `html-escape` (MIT), `flagset` (Apache-2.0), `quick-xml` (MIT), and Mozilla's `cssparser` and `selectors` (MPL-2.0, unmodified; the workspace already links them through `scraper`). All of these are behind the `live` feature.

## Forks

`fork::fork(library, id, app.templates_dir(), new_id)` copies a library template into `<app folder>/toolbox/templates/<new_id>/`, adding `lineage {parent_id, parent_version, parent_digest}`. The default id is `<id>.fork`. `fork::load_fork` loads an edited fork with the same manifest validation and static check. It refuses a fork that:

- declares a module or method its parent does not (`Widening`);
- raises any budget field above the parent's (`Widening`);
- turns provenance off;
- takes a library id;
- has no lineage.

The run-time grant check still applies, so a fork never gains capabilities beyond the app's grants. `workflow.list` reports whether each fork's parent still has the digest it was copied from (`lineage_current`).

## Evaluation

`evaluate(a, b, cases, app, scorer)` runs both templates on each case's **recorded inputs**, with a fresh fixture host per run. It compares:

- whether the output is valid against its schema;
- recursive data equality with the expected `status` and `data`, where given (the first differing path is reported);
- calls, model calls and pages;
- latency;
- an optional `QualityScorer` for text output (default: none).

`adopt` returns `better`, `worse` or `equal` for `b` against `a`, with reasons. The criteria are applied in order, and the first one that differs decides:

1. schema-valid outputs
2. expected matches
3. `ready` results
4. mean quality, when both sides are scored
5. model calls
6. host calls
7. latency: only a difference above 25% and 20 ms counts

The tests show this rule deciding real cases:

- A fork that awaits each read before starting the next is `worse`, on latency alone.
- A fork that reads fewer articles is `worse` on expected matches, even though it uses fewer calls.
- A scorer decides between two templates that are otherwise equally correct.

## Tool surface

`api::Toolbox::new(library, host)` provides:

- `list(app)`: the library and the app's forks, with parameters, output schema, modules, effective budget and whether the app may run each one. Broken forks are listed as `refused`, not dropped.
- `run(app, {id, params, run_id?})`: forks are resolved before library templates. The result is written into the app's folder.
- `fork(app, {id, new_id?})`
- `evaluate(app, {a, b, cases})`

`handle_json(app, {"tool": "workflow.run", "arguments": {…}})` routes the same calls as JSON and returns errors as `{"error": {kind, message}}`. `api::tool_descriptors()` gives the four tools' names, risk and input schemas for registration with a peer. The risk levels are the ones octos#2567 and App Hub's `tools.json` accept: `workflow.list`, `workflow.run` and `workflow.evaluate` are `read`, and `workflow.fork` is `act` (it writes only into the calling app's folder). The descriptors carry no `confirm` field.

## How an app agent will use it

This flow needs the wiring in [What remains](#what-remains):

1. The app's manifest asks for `research` with a scope. App Hub pins the request and the person grants it.
2. The kernel offers the app's peer `workflow.list` and `workflow.run` (and `workflow.fork` / `workflow.evaluate` if granted). The host fills `AppContext` from the peer's identity and the app's grants, never from the model.
3. The agent picks a template, fills its parameters in one model call, and calls `workflow.run`. The host runs it within the app's scope and budget and writes `toolbox/runs/<id>/<run>.json`. The agent gets back the structured result, including provenance.
4. To improve a procedure, the agent forks it, edits the fork, and evaluates it against the parent on recorded cases. It adopts the fork only if the verdict is `better`.

## Commands

All of these were run on 27 Sep 2026 (the test counts after the relevance, Google News and GDELT fixes).

```sh
cargo test --locked -p octosense-toolbox                     # 52 tests, fixtures only
cargo test --locked -p octosense-toolbox --features live     # 66 tests (4 ignored): + adapter tests on a local server
                                                             # (robots.txt never requested by default; honoured when on; SSRF; backoff;
                                                             # Google News editions; the GDELT breaker; provider deadlines; the feed filter)
cargo clippy --locked -p octosense-toolbox --all-targets --features live --no-deps -- -D warnings
cargo fmt --check -p octosense-toolbox
# The live smoke test: Google News RSS, GDELT and the BBC and Guardian technology
# feeds, the extractive stand-in model. Latest: ready, 3 of 3 sources read,
# 4 search fetches, 28.7 s.
cargo test -p octosense-toolbox --features live --test live -- --ignored --nocapture
# The templates with a real model: DeepSeek deepseek-v4-flash, the same feeds
# plus BBC 中文. The key is read only from DEEPSEEK_API_KEY; the tests skip
# without it. Results below.
DEEPSEEK_API_KEY=… cargo test -p octosense-toolbox --features live --test live_model -- --ignored --nocapture --test-threads=1
# Only the validation's twelve runs (six topics, both templates); LIVE_OUT keeps
# each run's result, the pages read and token usage as JSON.
DEEPSEEK_API_KEY=… LIVE_OUT=/tmp/live cargo test --locked -p octosense-toolbox --features live --test live_model c_validation_topics -- --ignored --nocapture
# After changing a template or a fixture: rewrite the lock and the expected
# results from the current output, then review the diff.
TOOLBOX_BLESS=1 cargo test -p octosense-toolbox --test templates
```

A workspace-wide `cargo fmt --check` reports files outside this crate (the shell, `phone/`, the apps), which this crate's changes do not touch.

### Real-model runs (27 Sep 2026)

`tests/live_model.rs`, topic "OpenAI" (the default; `LIVE_TOPIC` changes it). `topic-brief` searched en (as is) and zh (translated), `per_language` 3, `read_top` 4. GDELT answered HTTP 429 to this network throughout, so the readable sources were the configured feeds.

| Run | news-digest | topic-brief |
|---|---|---|
| Before these fixes | `partial`, 2 of 3 read (one Google News link failed); on a second run `digest: null`: "model output rejected: a point has no text" | `partial`, pages 10 of 10, 1 read refused (`max_pages`), 2 reads failed on Google News links; zh not read; the brief rested on 1 article |
| "OpenAI", after | `partial` (a provider failed), 3 of 3 read (BBC, Guardian ×2), 12 points, no drops, pages 3, search fetches 4 | `partial`: en read 3 (BBC, Guardian ×2); zh found 3, all Google News links, skipped as unreadable, so zh had nothing to read. Pages 3, denied 0, failed 0, 12 points, no drops |
| "Trump", after | `partial`: all 3 results were Google News links, skipped; nothing read, no model call | `partial`: zh ("特朗普") read 3 from BBC 中文; en found 3, all Google News links, skipped. Pages 3, denied 0, failed 0, 12 points, no drops |

Earlier runs of the same code, before the prompt stated the length limits, dropped 1 to 4 points per digest as "longer than 400" and kept the rest; one dropped a thirteenth point. Two limits of the data noted then are now handled: Google News is asked for the `CN:zh-Hans` edition, and the BBC 中文 feed's Traditional-script titles (中國, 颱風) match Simplified terms through the relevance module's character folding.

### The validation topics, before and after the relevance fixes (27 Sep 2026)

The deep-research validation of 27 Sep 2026 ran both research templates on six topics ("OpenAI" en+zh, "Nvidia earnings", "Strait of Hormuz" en+zh, "EU AI Act", "台风" zh+en, "Vucic resignation"): twelve runs, `news-digest` with `limit` 5, `topic-brief` with `per_language` 3 and `read_top` 4 (5 with two languages), 72 hours, the same feeds and model as above. `c_validation_topics` repeats them. On-topic counts are by reading each page read; the "before" column is the validation's model judge.

| | Before (0fcb511) | After |
|---|---|---|
| Runs with a digest | 7 of 12, 3 of them on topic | 2 of 12 ("OpenAI"), both on topic |
| Articles read | 26, 8 on topic (the rest Guardian and BBC technology stories let through by "of", "ai", "act") | 10, 10 on topic (one is a live blog whose OpenAI entry is the relevant part); 0 off-topic reads, 0 dropped by the gate or the model |
| Feed items skipped as off topic | none | 13 to 21 per search, each counted in `diagnostics` |
| zh search results | 0 (`hl=zh&gl=US`) | 5 per zh search in all 4 runs that search zh, all Google News links, so none readable |
| `topic-brief` read budget | "OpenAI" read 3 of `read_top` 5 | "OpenAI" read 5 en: zh had nothing readable, so its share moved to en |
| Status and diagnostics | `partial` in 11 of 12, `diagnostics` empty | `partial` in 12 of 12, every one explained: GDELT `rate-limited (429)`, then `skipped … earlier in this process`; the feed skips; `unreadable` counts |
| Wall time | 15–57 s per run, 420 s in all; searches 15–46 s | 14.6 s and 30.1 s for the two runs that read and digested, 0.3–2.8 s for the rest; 55 s in all |
| Model calls, cost | 10, about $0.011 | 5, about $0.0045 (DeepSeek off-peak prices) |

What still limits the interim adapter: GDELT answered 429 from the first request (a direct probe also got 429 after 10.2 s), and every Google News result is a link that needs a browser. So outside the configured feeds' own stories nothing was readable, and 10 of 12 runs honestly read nothing. The octos research engine (octos#2568) reads Google News links through headless Chrome; the relevance gate, translation and citation checks stay in the host and apply to it unchanged.

`octoscript-schema` turns on `serde_json`'s `arbitrary_precision` feature for any build that includes this crate. The shells do not link it today. Check this before they do.

## What remains

- **Peer tool wiring**, after octos#2567 (host-registered peer tools): register `tool_descriptors()` for granted peers in `crates/ai-host`, route each call to `Toolbox::handle_json` with the peer's `AppContext`, and supply the real `ModelClient` from the person's providers.
- **Engine swap**: replace the interim live adapter with the octos research engine (octos#2568) and metasearch (octos#2576) behind `ResearchBackend`, adding browser reading, SearXNG, structured weather and market sources, and `deep_crawl`.
- **Durable execution** through `octoscript-workflow` (checkpointed, resumable runs; queueing and batching by the system agent).
- **App Hub**: the `research` and `crawl` capabilities with a scope in the manifest; pinning forks shipped in a bundle.
- **Back upstream**: offering a winning fork to the library, with the person's consent.
