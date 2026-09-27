# octosense-toolbox: workflow templates

The system toolbox's library of **OctoScript workflow templates** ([ADR 0002](../../docs/adr/), section 6, "Workflow templates", proposed in OctoSense PR #77). A template is a fixed, bounded procedure (a news digest, a multi-language topic brief, a plan from the weather) written in OctoScript with a manifest that says what it may call. An app's agent picks one and fills its parameters: one model call to choose, instead of a multi-call tool loop. The host then runs the independent steps concurrently.

This crate holds the library, the runner, forks, evaluation, the `mod.research` v1 host module, and the `workflow.*` tool surface as a Rust API. **The shells do not link it yet.** See [What remains](#what-remains).

The first templates are ported from the AppCard research experiment ([`apps/appcard/tools/splash-research`](../../apps/appcard/tools/splash-research)). Its composition harness (Python) was not used; what was needed is in Rust here.

## Templates

| id | What it does | Host calls (max) | Model calls (max) |
|---|---|---|---|
| `news-digest` | Optional query translation; search; read the top N articles concurrently; one digest in the requested language with citations | 8 | 2 |
| `topic-brief` | Translations per language, all started together; one search per language; round-robin merge so every language is represented; read the top N concurrently; one brief | 16 | 5 |
| `weather-plan` | Forecast and air-quality searches started together; reads run concurrently; one plan with citations. Air quality is optional (the result is `partial` without it); a forecast is required (the run fails without it) | 7 | 1 |
| `market-brief` | One news search per ticker, started together; per-symbol reads run concurrently; one brief (a comparison for several symbols). Research, not advice: v1 has no quote method, so prices appear only as the sources state them | 13 | 1 |
| `briefing` | Up to four topics searched together; the top articles of each are read concurrently; one briefing | 17 | 1 |
| `compare` | The same aspect of two subjects searched together; reads run concurrently; one comparison. It needs sources for both sides | 9 | 1 |

`weather-plan` and `market-brief` adapt the experiment's `weather` and `stock` workflows. Those called `forecast`, `air_quality`, `quote` and `baseline` methods, which `mod.research` v1 does not have. Here they read published forecasts and news. Structured weather and market data will come with the research engine's providers. `briefing` and `compare` come from the experiment's composition set. Its `travel`, `outdoor` and `market` families need places and quotes, which the research module does not provide, so they are deferred.

Every template ships recorded fixtures in `templates/<id>/fixtures/`, with the expected `status` and `data`. The 14 cases include ready, partial and failed runs.

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
4. **Budget**: the run gets the template's budget, narrowed by the app's own budget and its scope's page limit. A call over `max_calls`, `max_model_calls` or `max_pages` is refused before dispatch. The script sees a failed call and can continue (`try … catch`). When `max_ms` elapses, in-flight calls are cancelled and fail as timed out.
5. **Output**: the value must be exactly `{status, data}`, and `data` must match the output schema. Any URL in `data` must be one the host retrieved. Otherwise the run is `failed` and no data is published. A `ready` result is downgraded to `partial` when any call was refused, failed or timed out.
6. **Provenance**: the host returns provenance records with each reply (URL, title, source, retrieval time, evidence hash). The runner keeps them outside the VM and attaches every record whose id or URL `data` refers to.
7. With `write_result`, the result goes to `<app folder>/toolbox/runs/<template>/<run>.json`.

The result also carries `diagnostics`, `stats` (calls, model calls, pages, denied, failed, peak concurrency, elapsed) and a `trace` of start, complete, denied and timed-out events. The future is not `Send`, because the VM stays on the calling thread.

## `mod.research` v1

| Method | Input | Output | Charged |
|---|---|---|---|
| `query` | `{query, language?}` | `{query, language}`: search terms in `language` | 1 model call |
| `search` | `{topic, language?, region?, limit?, max_age_hours?}` | `{items: [{id, title, url, source, language, published_at}], source: {partial, providers, queried_at}}` | the pages the backend fetched |
| `article` | `{id}` (a search result's id from **this run**) | `{id, title, url, source, language, published_at, excerpt, chars, truncated, evidence_sha256}` | 1 page |
| `digest` | `{task: digest\|brief\|plan\|compare, language, article_ids, focus?}` (articles read in **this run**) | `{task, language, summary, points: [{text, citations, label?}]}` | 1 model call |

`research::ResearchHost` implements `ToolboxHost` for this module over two parts: a `ResearchBackend` (finds and reads sources) and a `ModelClient` (supplied by the host). The policy is enforced here, once, whatever the backend:

- **Scope**: every call is checked against the app's scope. Languages and regions are refused when out of scope. Results on denied domains, or outside the allowed ones, are dropped. Recency is capped at the scope's limit.
- **Ids**: search results get host-assigned ids derived from their URLs. `article` reads only this run's ids, so a template cannot fetch an arbitrary URL.
- **Evidence**: article text stays in the host, capped at 6000 bytes on a paragraph boundary and hashed. The script sees a 400-byte excerpt and the hash.
- **Digests**: the host owns the prompts for each task. A model reply is refused when it cites an article it was not given or contains a URL.

Backends:

- **Fixture** (`fixture::FixtureBackend` and `fixture::FakeModel`): replays recorded searches and pages with their delays. The fake model is deterministic and extractive: it builds each point from an article's first sentence and cites it. Tests and evaluation use this backend.
- **Interim live adapter** (`research::live`, feature `live`). **Interim**: it is replaced once the octos research engine (octos#2568) and metasearch (octos#2576) land. It fetches only free structured sources: the GDELT DOC API and configured RSS/Atom feeds. Fetching is polite. The User-Agent is `OctoSense-Toolbox/0.1 (…; +https://github.com/OctoSense-org/OctoSense)`. robots.txt is honoured on every host; when robots.txt is unreachable, the host is not fetched. Each host gets a minimum interval: 1 s, or 5 s for GDELT, as GDELT asks. Redirects are followed by hand and each hop is checked. Responses are capped at 2 MiB, with a 15 s timeout. Pages are read over plain HTTP, and `dom_smoothie` (MIT, a port of Mozilla's readability.js) extracts the main text. Pages that need JavaScript fail as partial results.
  - **Google News RSS is off by default**: news.google.com's robots.txt disallows `/rss` for general agents, and the adapter honours it. With it turned on, each search reports it as refused.
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

`handle_json(app, {"tool": "workflow.run", "arguments": {…}})` routes the same calls as JSON and returns errors as `{"error": {kind, message}}`. `api::tool_descriptors()` gives the four tools' names, risk and input schemas for registration with a peer.

## How an app agent will use it

This flow needs the wiring in [What remains](#what-remains):

1. The app's manifest asks for `research` with a scope. App Hub pins the request and the person grants it.
2. The kernel offers the app's peer `workflow.list` and `workflow.run` (and `workflow.fork` / `workflow.evaluate` if granted). The host fills `AppContext` from the peer's identity and the app's grants, never from the model.
3. The agent picks a template, fills its parameters in one model call, and calls `workflow.run`. The host runs it within the app's scope and budget and writes `toolbox/runs/<id>/<run>.json`. The agent gets back the structured result, including provenance.
4. To improve a procedure, the agent forks it, edits the fork, and evaluates it against the parent on recorded cases. It adopts the fork only if the verdict is `better`.

## Commands

All of these were run on 27 Sep 2026.

```sh
cargo test --locked -p octosense-toolbox                     # 35 tests, fixtures only
cargo test --locked -p octosense-toolbox --features live     # + adapter unit tests; the smoke test stays ignored
cargo clippy --locked -p octosense-toolbox --all-targets --features live --no-deps -- -D warnings
# The live smoke test: GDELT plus the BBC and Guardian technology feeds, the
# extractive stand-in model. On 27 Sep 2026: ready, 3 sources read, 15.2 s.
cargo test -p octosense-toolbox --features live --test live -- --ignored --nocapture
# After changing a template or a fixture: rewrite the lock and the expected
# results from the current output, then review the diff.
TOOLBOX_BLESS=1 cargo test -p octosense-toolbox --test templates
```

`octoscript-schema` turns on `serde_json`'s `arbitrary_precision` feature for any build that includes this crate. The shells do not link it today. Check this before they do.

## What remains

- **Peer tool wiring**, after octos#2567 (host-registered peer tools): register `tool_descriptors()` for granted peers in `crates/ai-host`, route each call to `Toolbox::handle_json` with the peer's `AppContext`, and supply the real `ModelClient` from the person's providers.
- **Engine swap**: replace the interim live adapter with the octos research engine (octos#2568) and metasearch (octos#2576) behind `ResearchBackend`, adding browser reading, SearXNG, structured weather and market sources, and `deep_crawl`.
- **Durable execution** through `octoscript-workflow` (checkpointed, resumable runs; queueing and batching by the system agent).
- **App Hub**: the `research` and `crawl` capabilities with a scope in the manifest; pinning forks shipped in a bundle.
- **Back upstream**: offering a winning fork to the library, with the person's consent.
