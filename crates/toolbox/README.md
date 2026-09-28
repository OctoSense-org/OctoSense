# octosense-toolbox: workflow templates

The system toolbox's library of **OctoScript workflow templates** ([ADR 0002](../../docs/adr/), section 6, "Workflow templates", proposed in OctoSense PR #77). A template is a fixed, bounded procedure (a news digest, a multi-language topic brief, a plan from the weather) written in OctoScript with a manifest that says what it may call. An app's agent picks one and fills its parameters: one model call to choose, instead of a multi-call tool loop. The host then runs the independent steps concurrently.

This crate holds the library, the runner, forks, evaluation, the `mod.research` v1 host module, and the `workflow.*` tool surface as a Rust API. **The shells do not link it yet.** See [What remains](#what-remains).

The first templates are ported from the AppCard research experiment ([`apps/appcard/tools/splash-research`](../../apps/appcard/tools/splash-research)). Its composition harness (Python) was not used; what was needed is in Rust here.

## Templates

| id | What it does | Host calls (max) | Model calls (max) |
|---|---|---|---|
| `news-digest` (1.3.0) | Optional query translation; search; read the top N articles concurrently, skipping results the backend cannot read; pages that do not mention the topic (`on_topic: false`) and articles the model marks off topic are counted in `off_topic` and left out; one digest in the requested language with citations | 8 | 2 |
| `topic-brief` (1.3.0) | Translations per language, all started together; one search per language (asking for `max(per_language, read_top)` results); reads in rounds that give every language a read first whenever it found a readable article, take at most `per_language` from one language until no language has a candidate left within that share, then give the unused share of `read_top` to the languages that still have readable results; skip results the backend cannot read and replace a failed or off-topic read with that language's next candidate (at most `read_top` + one attempt per language); one brief, without the articles the model marks off topic. Each language's `queries` entry counts `found`, `unreadable`, `read`, `off_topic`, `failed` (reads that failed) and `left` (readable results not tried) | 20 | 5 |
| `weather-plan` (1.1.0) | Forecast and air-quality searches started together; reads run concurrently; one plan with citations. Air quality is optional (the result is `partial` without it); a forecast is required (the run fails without it) | 7 | 1 |
| `market-brief` (1.1.0) | One news search per ticker, started together; per-symbol reads run concurrently; one brief (a comparison for several symbols). Research, not advice: v1 has no quote method, so prices appear only as the sources state them | 13 | 1 |
| `briefing` (1.1.0) | Up to four topics searched together; the top articles of each are read concurrently; one briefing | 17 | 1 |
| `compare` (1.1.0) | The same aspect of two subjects searched together; reads run concurrently; one comparison. It needs sources for both sides | 9 | 1 |

`weather-plan` and `market-brief` adapt the experiment's `weather` and `stock` workflows. Those called `forecast`, `air_quality`, `quote` and `baseline` methods, which `mod.research` v1 does not have. Here they read published forecasts and news. Structured weather and market data will come with the research engine's providers. `briefing` and `compare` come from the experiment's composition set. Its `travel`, `outdoor` and `market` families need places and quotes, which the research module does not provide, so they are deferred.

Every template ships recorded fixtures in `templates/<id>/fixtures/`, with the expected `status`, `reasons` and `data`. The 20 cases include ready, partial and failed runs, off-topic pages dropped by the host and by the model (`news-digest/off-topic-dropped`), read budget moving to the language with readable results (`topic-brief/budget-moves-to-readable`), and Traditional-script pages matching a Simplified search (`topic-brief/traditional-script`).

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
  "budget": {"max_calls": 8, "max_model_calls": 2, "max_reads": 7, "max_ms": 60000, "max_concurrency": 4},
  "output": { JSON Schema of `data` },
  "provenance": true,
  "lineage": {"parent_id", "parent_version", "parent_digest"}   (forks only)
}
```

The schemas use OctoScript's executable subset (`octoscript-schema`): types, `properties`, `required`, boolean `additionalProperties`, `items`, and min/max bounds with `enum`. There is no `anyOf`. A nullable field (`digest` when the model failed) is written without a `type`.

A script returns `{status: "ready" | "partial" | "failed", data, reasons}`, where `reasons` (optional, at most 16 strings of at most 300 characters) says why the status is not `ready`; see [Status](#status). Every optional parameter needs a `default`, because a script that reads an absent field fails. The runner fills defaults before it validates.

## Admission

`Library::builtin()` and `Library::load_dir()` load each template, validate it and pin it:

- **Manifest**: unknown fields are refused. The id is a directory-safe name. Only known modules and methods are allowed, and each only once. The budget must stay within the toolbox ceiling (64 calls, 8 model calls, 32 reads, 300 s, concurrency 8). A module that reads external sources requires `provenance: true`. Each file is capped at 32 KiB.
- **Static check** (`check::check_source`): the source must pass OctoScript's canonical syntax check. Imports may name only `mod.std.{array,assert,json,math,object,text}` and the declared host modules; `mod.tool` is always refused. OctoScript's scope-resolved call report checks every `module.method` call, local aliases included, against the declared methods. `mod` may appear only in a `use` line. An incomplete report counts as a refusal.
- **Pin**: the digest is SHA-256 over both files, with line endings normalized. Each digest must match `library.lock.json`, and the lock must name nothing else.

At run time the VM also installs **only** the declared methods, so a call the check cannot see finds nothing to call.

## Runner

`runner::run(template, app, params, host, options)`:

1. Refuses the run before it starts if the app lacks a grant the modules need (`research`) or the parameters do not match the schema.
2. Builds a `CapabilityRuntime` whose only host module is the declared one. Each method is a deferred external tool with its input and output contract from `modules.rs`. The parameters become `request`.
3. Evaluates the script. Every deferred call is claimed and dispatched to the caller's **`ToolboxHost`**, together with a `CallContext`: the app's identity, grants, scope and folder (`AppContext`), the run and template, the effective budget and what is left of it. Independent calls run concurrently, up to `max_concurrency`.
4. **Budget**: the run gets the template's budget, narrowed by the app's own budget (`AppContext::budget`). The budget is the template's and the app's, never the grant's: the app's scope says what it may reach, not how much one run may do. A call over `max_calls`, `max_model_calls` or `max_reads` is refused before dispatch. `max_reads` counts **articles read** (each `article` call, whether or not it succeeds); it was called `max_pages` until 27 Sep 2026 and was renamed so it is not confused with the `crawl` scope's `max_pages` (pages of one crawl). A template or fork that still says `max_pages` in its budget is refused as an unknown field. A `search` is one call and is not charged to `max_reads`: how many feeds and APIs it fetches is the backend's configuration (one for the fixture backend, four for the interim adapter below), and charging it let two searches starve the reads (on 27 Sep 2026 a two-language `topic-brief` used 8 of its 10 pages on feeds and read one article). The fan-out is capped at `research::MAX_SEARCH_FETCHES` (8) per search and reported as `stats.search_fetches`. The script sees a failed call and can continue (`try … catch`). When `max_ms` elapses, in-flight calls are cancelled and fail as timed out.
5. **Output**: the value must be exactly `{status, data, reasons?}`, and `data` must match the output schema. Any URL in `data` must be one the host retrieved. Otherwise the run is `failed` and no data is published. The template's `status` and `reasons` become the result's `status` and `status_reasons`; the runner adds its own reasons (below).
6. **Provenance**: the host returns provenance records with each reply (URL, title, source, retrieval time, evidence hash). The runner keeps them outside the VM and attaches every record whose id or URL `data` refers to.
7. With `write_result`, the result goes to `<app folder>/toolbox/runs/<template>/<run>.json`.

The result also carries `status_reasons`, `diagnostics`, `stats` (calls, model calls, reads, search fetches, denied, failed, peak concurrency, elapsed) and a `trace` of start, complete, denied and timed-out events. The future is not `Send`, because the VM stays on the calling thread.

## Status

A run's `status` says whether its output is complete, and `status_reasons` says why it is not, one line each, for the app to show. Failures a template recovered from are not held against it: they stay in `diagnostics` and in the templates' counts. Until 28 Sep 2026 any failed call downgraded a run, and a provider that was suspended or rate-limited (GDELT, which answered 429 all through validation run 2) marked every search partial, so 12 of 12 validation runs were `partial` and the status carried no signal.

- **`ready`**: the template got everything its output needs.
- **`partial`**: usable output that is materially incomplete.
- **`failed`**: nothing usable came back; `data` is `null` and no provenance is attached.

Each template judges its own output:

| Template | `ready` | `partial` (the reasons) | `failed` |
|---|---|---|---|
| `topic-brief` | every language with a readable result was read; `read_top` articles were kept, or the candidates ran out; the brief kept every point | `language zh: the search failed`; `language zh: the query translation failed; searched the topic as given`; `language en: no article read (2 failed, 1 readable left untried)`; `read 1 of 2 articles while 1 readable candidates remained`; `the digest failed; …`; `the digest dropped 2 invalid point(s)` | nothing on the topic was read |
| `news-digest` | articles on the topic were read and the digest kept every point | the translation or the digest failed; the digest dropped points | the search failed, found nothing, or nothing on the topic was read |
| `briefing`, `market-brief` | every topic (symbol) was read; the digest kept every point | a topic found nothing, its search failed or none of its reads succeeded; the digest failed or dropped points | nothing was read |
| `compare` | both sides were read; the comparison kept every point | a side found or read nothing; the comparison failed or dropped points | nothing was read |
| `weather-plan` | a forecast and an air-quality report were read; the plan kept every point | no air-quality report; the plan failed or dropped points | no forecast was read |

Not held against a run: a failed read that another candidate replaced, a failed read when others of the same topic succeeded, results the backend cannot read (`readable: false`, counted in `unreadable`), off-topic pages (`off_topic`), a provider that failed or was suspended while others answered (`source.partial` and the provider's note in `diagnostics`), a point dropped because it cited only articles marked off topic, and summary sentences the host dropped or flagged ([below](#modresearch-v1)).

The runner adds what a template cannot see: `the budget refused N call(s)`, `the time budget (N ms) ran out`, `the template's script failed`, `the template's output was refused`. A budget refusal or an expired time budget makes a `ready` run `partial`. A `partial` or `failed` result without a reason gets `the template gave no reason`; a `ready` one carries none. A reason containing a URL is withheld.

## The app's grant: octos's `Scope`

The scope an app is granted with `research` and `crawl` (ADR 0002 section 6) is `octos_research::toolbox::Scope` (octos#2585): the same JSON, field names and units, with unknown fields refused and empty lists meaning no limit.

```json
{"langs": ["en", "zh"], "regions": ["US"], "domains_allow": [], "domains_deny": ["example.com"],
 "max_age_days": 3, "categories": ["news"], "max_results": 10, "max_depth": 0, "max_pages": 0}
```

| Field | Meaning | How `mod.research` applies it |
|---|---|---|
| `langs` | BCP-47 languages (normalized: `zh-hant` → `zh-Hant`) | searches, translations, digests and results outside it are refused or dropped (primary subtags compared) |
| `regions` | ISO 3166-1 alpha-2 | a search for another region is refused |
| `domains_allow`, `domains_deny` | domains, subdomains included | results outside are dropped; `article` checks the link and, with the octos engine, the page it ended on |
| `max_age_days` | oldest material, in days | a search's `max_age_hours` is clamped to it; a search without one gets it |
| `categories` | metasearch categories | `mod.research` searches `news`; a grant without it refuses every search |
| `max_results` | results per search (default 20) | caps a search's `limit` |
| `max_depth`, `max_pages` | the `crawl` capability: depth and pages of one crawl (0: not granted) | unused: `mod.research` does not crawl |

`scope::parse` reads a grant and `AppContext` deserializes its `scope` through it; `scope::unrestricted()` is the empty grant (`Scope::default()` is not: its `max_results` is 0).

- **With `octos-engine`**, `Scope` is octos's type. A grant is parsed by `Scope::from_grant`, each search is narrowed by `Scope::search_args` and each read checked by `Scope::check_domain`. There is no toolbox definition of the scope.
- **Without it**, `Scope` is `scope::compat::Scope`, a thin parser of the same JSON with the same validation, normalization, domain matching and narrowing, so the fixture and interim backends enforce the same grant. It is not a second definition to maintain: a test built with the feature (`scope::tests::the_thin_parser_matches_octos`) runs both on the same grants, URLs, languages and searches and fails on any difference. Gating the scope on the feature instead would leave the default build, the fixtures and the evaluation with no scope checks at all.
- **The run budget is not in the grant.** Articles read per run are the template's `max_reads`, narrowed by the app's own budget. The scope's `max_pages` counts pages of one crawl.
- **Grants in the old shape are refused, not converted.** The toolbox's own scope (`languages`, `allowed_domains`, `denied_domains`, `recency_hours`, and `max_pages` as articles per run) is refused with an error naming each replacement (`` `languages` is now `langs` ``, …). Converting would round hours up to days, widening what the person granted, and would read the old `max_pages` as a crawl limit. No grant in the old shape was ever stored (the toolbox is not wired into App Hub yet), so App Hub pins and the person grants the scope again in the new shape.

## `mod.research` v1

| Method | Input | Output | Charged |
|---|---|---|---|
| `query` | `{query, language?}` | `{query, language}`: search terms in `language` | 1 model call |
| `search` | `{topic, language?, region?, limit?, max_age_hours?}` | `{items: [{id, title, url, source, language, published_at, readable}], source: {partial, providers, queried_at}}`, readable items first | 1 call; its fetches are reported, not charged to `max_reads` |
| `article` | `{id}` (a search result's id from **this run**) | `{id, title, url, source, language, published_at, excerpt, chars, truncated, evidence_sha256, on_topic}` | 1 read (`max_reads`) |
| `digest` | `{task: digest\|brief\|plan\|compare, language, article_ids, focus?}` (articles read in **this run**) | `{task, language, summary, points: [{text, citations, label?}], off_topic: [id], dropped_points, summary_check: {sentences, dropped, flagged}}` | 1 model call |

`research::ResearchHost` implements `ToolboxHost` for this module over two parts: a `ResearchBackend` (finds and reads sources) and a `ModelClient` (supplied by the host). The policy is enforced here, once, whatever the backend:

- **Scope**: every call is checked against the app's grant, octos's `Scope` (see [The app's grant](#the-apps-grant-octoss-scope)). A search in a language, region or category outside it is refused; its recency is clamped to `max_age_days` and its `limit` to `max_results`, with a note in the run's diagnostics. Results on denied domains, outside the allowed ones or in a language outside `langs` are dropped, and `article` refuses them. `query` and `digest` refuse an output language outside `langs`. The scope does not limit how many articles a run reads; that is the budget's `max_reads`. `mod.research` reads only this run's search results and never follows a link from a page, so every read is at depth one and it needs no crawl grant.
- **Ids**: search results get host-assigned ids derived from their URLs. `article` reads only this run's ids, so a template cannot fetch an arbitrary URL.
- **Evidence**: article text stays in the host, capped at 6000 bytes on a paragraph boundary and hashed. The script sees a 400-byte excerpt and the hash.
- **Relevance** (`research::relevance`, in the host so it survives the engine swap): topics are split into terms, stop-words ("of", "the", "news", "de", "新闻" …) ignored; words match whole words, case- and accent-insensitively, with a light suffix stemmer; runs of Han, kana, Hangul or Thai match as substrings after both sides are folded to Simplified Chinese with a small character table (common news vocabulary, about 540 characters; a character outside it matches only its own script).
  - `search` lists results whose headline mentions every term first.
  - `article` judges the whole page against the terms of the searches that found it, in the page's language (a page in another language is left to the model): every short name (under four letters, written with a capital: `EU`, `AI`, `Act`) and one of the longer names (`Hormuz`, `Nvidia`) must appear; a topic without names needs a third of its terms; a run of four or more unspaced characters also counts by its first or last half. A page that fails is returned with `on_topic: false` and a diagnostic (`s…: off topic, the page does not mention strait + hormuz; not digested`).
  - `digest` leaves out any off-topic article whatever the template passes, and refuses when none is left. For the `digest` task with a `focus`, the same model call also returns `off_topic`: the ids it finds are not about the focus. Their citations are removed (a point citing only them is dropped) and the templates drop them from `sources`. No extra model call.
  - This is a cheap floor, not a judge: `Taiwan Strait` passes it for `Strait of Hormuz`, which the model check then catches.
- **Readability**: a backend marks an item `readable: false` when it knows a read would fail (the interim adapter cannot resolve Google News links). The host lists readable items first, before it applies `limit`, and the templates skip unreadable ones instead of spending a read on them.
- **Digests**: the host owns the prompts for each task, and each prompt states the length limits. Validation is per point. The digest is refused only when its summary is missing, longer than 1200 characters or contains a URL, when the reply is not JSON (a code fence or a line of prose around the object is tolerated), or when no valid point is left. A point with no text, text over 400 characters, a URL, no citation, more than 8, or a citation to an article it was not given is dropped, and the rest are kept; a label over 40 characters or with a URL is dropped from its point; points beyond 12 are dropped. Each drop is a diagnostic (`research.digest (call N): digest point 3 dropped: it has no text`) that never quotes the model. So every point kept cites only articles read in this run, and no model text carries a URL. Lengths are counted in characters, as the output contract's `maxLength` is. `dropped_points` counts the invalid points dropped (not those removed for citing only off-topic articles), so a template can tell a complete digest from one the host had to cut.
- **The summary only restates the points** (`research::summary`). Validation run 2 found the points clean and the summaries adding statements no point backs (4 in the topic briefs, 2 of them backed by no source: "日本气象厅呼吁防范狂风、巨浪和暴雨", "预计9月30日减弱"). The digest prompt now says the summary adds no fact, number, date, name, cause, forecast, quote or conclusion of its own. The host then checks each summary sentence against the kept points, with no model call:
  - its **key terms** are its numbers (`1,032` = `1032`), its names (words written with a capital or a digit, other than an ordinary capitalized first word, which backs a sentence but never counts as new) and its two-character sequences in scripts written without spaces (without a function character such as 和 or 的, folded to Simplified); a sentence with no key term is judged on its other words of four letters or more;
  - a sentence none of whose key terms is in any point is **dropped**, with a diagnostic (`digest summary: sentence 2 of 4 dropped: no point backs it`); a summary with no sentence left is refused;
  - a kept sentence carrying a number, a name, or a run of five or more unspaced characters that no point carries is **flagged**: kept, counted in `summary_check.flagged`, with a diagnostic. The check cannot tell a new fact from a paraphrase, so it does not drop these.
  - It is deliberately conservative. On validation run 2's 24 digests (81 summary sentences) it drops none and flags 10. Five of those carry a statement the judge found no point backs: the invented date "27 September 2026" (twice), "Dev Day", "Gulf front", and "狂风、巨浪和暴雨" (the run 巨浪和暴雨 shares no content pair with any point). The other five are paraphrases (`Saudi Arabia` for `Saudi`, `Q1` for `first-quarter`, `DPIAs`). A sentence built from the points' own words that says something they do not ("预计9月30日减弱" combines the points' "30日 … 变性" and "逐渐减弱"; framings such as "enforcement is expanding") is beyond it, and left to the prompt. The unit tests run it on the drift examples from run 2's judged files.

Backends:

- **Fixture** (`fixture::FixtureBackend` and `fixture::FakeModel`): replays recorded searches and pages with their delays. A recorded search can say how many feeds it fetched (`fetches`), carry the backend's `notes`, and mark items `readable: false`. The fake model is deterministic and extractive: it builds each point from an article's first sentence and cites it. Its config can inject a URL into the summary (`inject_url`), a point citing an unknown article (`cite_unknown`), three invalid points (`malformed_points`), mark articles off topic by URL (`off_topic`; it still writes their points, so the host must remove them), or add a summary sentence no point backs (`summary_drift`). Tests and evaluation use this backend.
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
- **The octos research engine** (`research::octos::OctosResearch`, feature `octos-engine`). It depends on `octos-research` through the workspace's one octos pin, and needs octos#2568 (reader), #2582 (metasearch), #2585 (`octos_research::toolbox`) and #2590 (read-failure reasons, posts marked as posts), all in the pin (octos e200b072). It only finds and reads; everything under "The policy" above stays in the host unchanged.
  - **One definition of an app's reach.** The app's grant **is** `octos_research::toolbox::Scope`; nothing is mapped. Each search is narrowed by octos's `Scope::search_args`, in the host (above) and again here, so a language, region, category or domain outside the grant is refused there and the recency is clamped. The search asks the engines for a candidate pool of three times the app's `limit` (at most 30, `candidate_pool`) so the host's filters still leave `limit`; `max_results` bounds what the app gets, not the pool. Each read is checked with `Scope::check_domain` twice: the link, then the page the browser ended on, the publisher for a Google News link.
  - **Search** is the octos metasearch (`octos_research::metasearch`), category `news`: Google News in the language's own edition, GDELT, publisher feeds, Hacker News and Mastodon, plus keyed engines when their keys are set.
    - Octos fans the query out under a 15 s deadline, then merges, deduplicates and ranks the results. It spaces requests per host, honours `Retry-After`, caches with ETags, and suspends a failing engine with doubling backoff. That backoff is how GDELT's 429s are handled.
    - No results page is scraped.
    - The toolbox's own `research.query` translation goes in as `query_by_lang`, so octos makes no model call.
    - The diagnostics name every engine call (`engines: gdelt (zh) suspended, google_news (zh) 15, mastodon (zh) 13, publisher_feeds (zh) 0`) and each engine that failed, timed out, was suspended or was rate limited. They also count the results filtered out and give each result's engines and score by host id (`ranked: s1a2… google_news+gdelt 0.83`). The score is also in the search provenance's `via`.
    - `source.partial` means an engine failed or timed out in this search. An engine octos had already suspended is reported but does not make the search partial.
    - An item's language keeps the searched tag when the engine's is the same language (`zh-CN` for a `zh` search stays `zh`).
  - **Reading** is octos's polite reader (`octos_research::reader`): SSRF check and DNS pinning on every hop, 1 s per host, one backoff on 429/503 honouring `Retry-After`, content-type and size caps, readability extraction, and octos's honest User-Agent. robots.txt is off unless the operator sets `OCTOS_RESPECT_ROBOTS=1`. When plain HTTP yields no main text (every Google News link, script-built pages), the page is rendered in headless Chrome and the reader re-checks where the browser went; a page the browser ends on outside the app's allowed domains is refused.
    - A failed read says why, in octos's words (octos#2590): the tool error, and so the run's diagnostics, is `<code>: <detail> (final URL: …)` with a code such as `bot_challenge`, `consent_page`, `paywall`, `login_wall`, `http_403`, `redirect_unresolved`, `render_timeout` or `no_main_text`. Walls are reported, never worked around. SSRF, scope and robots.txt refusals (`blocked`, `robots`, `robots_unreachable`) are denials; every other reason is a failure.
  - **Posts are not evidence.** Octos marks Mastodon posts (and discussion threads without a linked article) `kind: post`. The backend leaves them out of the results it returns, so they are never read or cited, and notes how many it left out (`3 results are social posts, not articles; not used as evidence`). A story found by both Mastodon and a news engine is an article and stays.
  - **`readable`**: Google News links are readable only when a browser is available. Without Chrome (a phone, a server, or `OCTOSENSE_TOOLBOX_RENDER=off`) they say `readable: false`, as with the interim adapter.
  - **Headless Chrome** (`research::chrome`), following octos `deep_crawl`'s discipline: `--headless=new`, a throwaway profile, no automation-hiding switches or scripts, Chrome's own User-Agent with the `octos-research/1.0` product token appended, and a bot challenge ends the attempt. Every request the tab makes is paused and continued only when `octos_research::net::check_url` passes (public address, DNS fail closed); images, media and fonts are not fetched. **Process hygiene**: one browser per process (`Chrome::shared`), at most 2 tabs at a time, 30 s per render, the tab closed however the render ends. Chrome runs in its own process group, and the guard that owns it kills the whole group and deletes the profile when it is dropped: when the last backend using it goes away, after 60 s idle, on `Chrome::shutdown`, or when the browser stops answering. A process killed with SIGKILL runs no destructor and leaves its browser. The tests start Chrome and check the group is gone after a drop, a hung render times out without a relaunch, an idle browser closes, four renders share one browser, and private addresses are refused inside the browser.

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
- calls, model calls and reads;
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

`peer` is what an app's peer is actually offered (octos#2567): `peer::tool_decls(app, library)` gives the `tools.json` entries from the app's grants alone (`research`: `workflow.run`, `workflow.fork`, `toolbox.search`, `toolbox.web_read`; `crawl` with crawl limits in the scope: `toolbox.deep_crawl`; octos's `deep_research` never), each `background`, not `outward`, `confirm: host`. `peer::PeerToolbox::call(app, name, args)` checks the name against the grants again (`not_granted` otherwise) and runs it: the two workflow tools through `handle_json` (the agent gets the data, sources and the result's path; the trace stays in the file), the single tools on the same `ResearchBackend` as `mod.research`, narrowed by the scope (`scope::narrow_search`, `check_domain`, `scope::narrow_crawl`), with their items saved under `research/` in the app's folder. `toolbox.deep_crawl` reads pages with `ResearchBackend::read_links` (the octos engine keeps each page's HTML for its links) and stays on the start's site, under `path_prefix`, inside the domains and within `max_depth` and `max_pages`.

## How an app agent will use it

This flow needs the wiring in [What remains](#what-remains):

1. The app's manifest asks for `research` with a scope. App Hub pins the request and the person grants it.
2. The kernel offers the app's peer `workflow.list` and `workflow.run` (and `workflow.fork` / `workflow.evaluate` if granted). The host fills `AppContext` from the peer's identity and the app's grants, never from the model.
3. The agent picks a template, fills its parameters in one model call, and calls `workflow.run`. The host runs it within the app's scope and budget and writes `toolbox/runs/<id>/<run>.json`. The agent gets back the structured result, including provenance.
4. To improve a procedure, the agent forks it, edits the fork, and evaluates it against the parent on recorded cases. It adopts the fork only if the verdict is `better`.

## Commands

All of these were run on 28 Sep 2026 with octos's merge of #2585 (7bec0918) in place of the pin, through an uncommitted override (the pin's octos has no `octos-research` toolbox, so every build needs it until the pin moves, and `--locked` needs the lock file updated locally for it): `--config 'patch."https://github.com/octos-org/octos.git".octos-research.path="<octos checkout>/crates/octos-research"'` (**unverified** against the pin until it moves).

```sh
cargo test --locked -p octosense-toolbox                     # 67 tests, fixtures only (the thin scope parser)
cargo test --locked -p octosense-toolbox --features live     # 81 tests (4 ignored): + adapter tests on a local server
                                                             # (robots.txt never requested by default; honoured when on; SSRF; backoff;
                                                             # Google News editions; the GDELT breaker; provider deadlines; the feed filter)
cargo test --locked -p octosense-toolbox --features octos-engine        # 81 tests (4 ignored): + octos's Scope, the thin parser against it, engine notes,
                                                             # and headless Chrome (skipped without Chrome): the process group is gone after
                                                             # a drop, a hung render times out without a relaunch, an idle browser closes,
                                                             # four renders share one browser, private addresses are refused in the browser
cargo test --locked -p octosense-toolbox --features live,octos-engine   # 95 tests (5 ignored)
cargo clippy --locked -p octosense-toolbox --all-targets --features live --no-deps -- -D warnings
cargo clippy --locked -p octosense-toolbox --all-targets --features live,octos-engine --no-deps -- -D warnings
cargo fmt --check -p octosense-toolbox
# The octos engine live: 台风 in the zh edition, Google News links read through
# headless Chrome, the extractive stand-in model. Latest: 2 of 3 read, 23.2 s.
cargo test -p octosense-toolbox --features octos-engine --test octos_engine -- --ignored --nocapture
# The live smoke test: Google News RSS, GDELT and the BBC and Guardian technology
# feeds, the extractive stand-in model. Latest: ready, 3 of 3 sources read,
# 4 search fetches, 28.7 s.
cargo test -p octosense-toolbox --features live --test live -- --ignored --nocapture
# The templates with a real model: DeepSeek deepseek-v4-flash, the same feeds
# plus BBC 中文. The key is read only from DEEPSEEK_API_KEY; the tests skip
# without it. Results below.
DEEPSEEK_API_KEY=… cargo test -p octosense-toolbox --features live --test live_model -- --ignored --nocapture --test-threads=1
# The same with the octos research engine (LIVE_BACKEND=interim picks the
# interim adapter when both features are on).
DEEPSEEK_API_KEY=… cargo test -p octosense-toolbox --features octos-engine --test live_model -- --ignored --nocapture --test-threads=1
# Only the validation's twelve runs (six topics, both templates); LIVE_OUT keeps
# each run's result, the pages read and token usage as JSON.
DEEPSEEK_API_KEY=… LIVE_OUT=/tmp/live cargo test --locked -p octosense-toolbox --features live --test live_model c_validation_topics -- --ignored --nocapture
# A subset of the topics, on the octos engine (the check of 28 Sep 2026 below).
DEEPSEEK_API_KEY=… LIVE_TOPICS=openai,hormuz,typhoon,vucic cargo test -p octosense-toolbox --features octos-engine --test live_model c_validation_topics -- --ignored --nocapture
# After changing a template or a fixture: rewrite the lock and the expected
# results from the current output, then review the diff.
TOOLBOX_BLESS=1 cargo test -p octosense-toolbox --test templates
```

A workspace-wide `cargo fmt --check` reports files outside this crate (the shell, `phone/`, the apps), which this crate's changes do not touch.

### Real-model runs (27 Sep 2026)

`tests/live_model.rs`, topic "OpenAI" (the default; `LIVE_TOPIC` changes it). `topic-brief` searched en (as is) and zh (translated), `per_language` 3, `read_top` 4. GDELT answered HTTP 429 to this network throughout, so the readable sources were the configured feeds.

| Run | news-digest | topic-brief |
|---|---|---|
| Before these fixes | `partial`, 2 of 3 read (one Google News link failed); on a second run `digest: null`: "model output rejected: a point has no text" | `partial`, pages 10 of 10, 1 read refused (`max_pages`, now `max_reads`), 2 reads failed on Google News links; zh not read; the brief rested on 1 article |
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

### The validation topics on the octos research engine (27 Sep 2026)

These are the same twelve runs with the same model, on `OctosResearch` (feature `octos-engine`) instead of the interim adapter. The host and the templates are unchanged. Grounding was judged as in the first validation: `deepseek-v4-pro` against the evidence the host kept, one call per run.

The engine ran in two versions:
- the first searched octos-research's providers directly (Google News, GDELT, the feeds above);
- the current one searches with the octos metasearch.

| | Interim, before (0fcb511) | Interim, relevance fixes (d60891f) | octos deep-search (#2568) | Engine, providers | **Engine, metasearch (current)** |
|---|---|---|---|---|---|
| Runs with output | 7 of 12 | 2 of 12 | 6 of 6 | 12 of 12 | **12 of 12** |
| Runs on topic | 3 of 12 | 2 of 12 | 6 of 6 | 10 of 12 | **12 of 12** |
| Articles read, on topic | 26, 8 | 10, 10 | 12 (16 of 19 sources on topic) | 41, 36 | **39, 38** (26 Google News links read in Chrome, 10 direct, 3 Mastodon posts) |
| Languages read | en | en | en, zh | en and zh in all 4 two-language runs | **en and zh in all 4 two-language runs** |
| Strict / lenient grounding | 92–98% / 100% | not judged | 84% / 94% | 97–98% / 100% | **96–98% / 100%** (130 claims) |
| Unsupported or wrongly cited | 0 | – | 6 | 0 | **0** |
| Supported and on topic | 25–31% | – | 62% | 97% / 84% | **96% (news-digest), 98% (topic-brief)** |
| Mean wall time | 34–36 s | mostly 0.3–2.8 s (nothing read) | 49 s | 26–28 s | **28–32 s** (search median 2.2 s) |
| Model calls, cost | 10, $0.011 | 5, $0.0045 | 6, $0.006 | 15, $0.021 | **15, $0.020** |

What still fell short (the first two are fixed since, see the check below; the reader now says why a read failed and Mastodon posts are no longer read, since the octos pin at e200b072):
- **Every run was `partial`.**
  - With the metasearch, a search is partial only when an engine fails in that call. GDELT answered 429 or timed out on the first searches and then stayed suspended.
  - Every run also had a failed read, and the runner downgraded `ready` to `partial` for any failed call.
- **23 of 49 Google News reads failed** as `no_main_text`.
  - 2 of them were bot challenges, which were not bypassed.
  - The rest were Reuters, NYT, MarketWatch and similar pages with no extractable article.
  - The reader does not say why.
- **Mastodon posts are read as sources.** They are short and one was off topic (a game post in an "Nvidia earnings" run).
- **Summaries added 11 statements no point backs** (2 unsupported by any source).
- **Without Chrome, Google News links go back to `readable: false`.** That covers a phone, a server, or `OCTOSENSE_TOOLBOX_RENDER=off`.

After the run, no Chrome process or profile was left.

### The status rules and the summary check, live (28 Sep 2026)

After the [status rules](#status) and the [summary check](#modresearch-v1): four of the six topics ("OpenAI" en+zh, "Strait of Hormuz" en+zh, "台风" zh+en, "Vucic resignation"), both templates, eight runs, on the octos engine (octos 7bec0918 through the override above) with `deepseek-v4-flash`, the same parameters as the validation. GDELT answered 429 on the first search and stayed suspended, as before.

| Run | Status and reasons | Reads (failed) | Summary: sentences, dropped, flagged |
|---|---|---|---|
| news-digest OpenAI | `ready` | 5 (2) | 3, 0, 0 |
| topic-brief OpenAI en+zh | `ready`: en 2, zh 3 | 6 (1) | 9, 0, 0 |
| news-digest Hormuz | `ready` | 5 (0) | 7, 0, 1 |
| topic-brief Hormuz en+zh | `ready`: en 3, zh 2 | 5 (0) | 7, 0, 1 |
| news-digest 台风 | `ready` | 5 (2) | 3, 0, 0 |
| topic-brief 台风 zh+en | `partial`: "read 4 of 5 articles while 3 readable candidates remained" (zh 2, en 2; 3 reads failed, and the attempts ran out at `read_top` + 2) | 7 (3) | 5, 0, 0 |
| news-digest Vucic | `ready` | 5 (2) | 5, 0, 1 |
| topic-brief Vucic | `ready`: 3 of `read_top` 4, the candidates ran out | 4 (1) | 5, 0, 0 |

- **Status**: 7 `ready`, 1 `partial` with its reason, where validation run 2 had 12 of 12 `partial`. The suspended GDELT and the 11 failed reads are in the diagnostics; only the 台风 brief's shortfall counts against a run.
- **Summaries**: no sentence dropped; 3 of 44 flagged, each for a name no point carries: `IRGC` (the points say Revolutionary Guards), `SNS` (the points spell out the party) and `Beijing` (the points say China). Not judged this time.
- 8 runs, 11 model calls, 262 s in all, about $0.018 at the logged off-peak prices.

`octoscript-schema` turns on `serde_json`'s `arbitrary_precision` feature for any build that includes this crate. The shells do not link it today. Check this before they do.

## What remains

- **Peer tool wiring** is in place behind `crates/ai-host`'s `toolbox-peers` feature (the `peer` module here: the tools per grant, `PeerToolbox::call`, and `toolbox.deep_crawl` over `ResearchBackend::read_links`). Turning it on waits for octos#2567 on the octos pin and App Hub's `research`/`crawl` capabilities.
- **Engine**: the octos research engine is behind `ResearchBackend` (`octos-engine`). Still to do:
  - drop the interim adapter once the shells use it;
  - add metasearch (octos#2582) and publisher feeds (octos#2585);
  - add structured weather and market sources;
  - report why a read had no main text (final URL, consent page, bot challenge);
  - stop a provider that the breaker skipped from making every run `partial`.
- **Durable execution** through `octoscript-workflow` (checkpointed, resumable runs; queueing and batching by the system agent).
- **App Hub**: the `research` and `crawl` capabilities with a scope in the manifest; pinning forks shipped in a bundle.
- **Back upstream**: offering a winning fork to the library, with the person's consent.
