# News, Photos, YouTube and private preferences

English | [简体中文](contextual-app-cards.zh-CN.md)

These cards use the shared Glance workspace: compact summary, expanded card,
and native Card/Chat. Expanding keeps the publication's original app/account
and item context. Open News, Open Photos and Play are in the card content.
The host supplies fixed L0 templates; the model supplies grounded research,
conversation and choices. These are not claims of model-authored template code.

| App | Try it | Actual data and behavior |
| --- | --- | --- |
| News | Open a story → Research topic → expand its Glance card | Existing octos research engine; scoped search, article reads and model synthesis; full validated summary and citations; explicit running/partial/failure state |
| Photos | Select photos → Glance; open the card → Open Photos | One bundled 75-photo **sample** catalog shared by UI and service, plus the app's saved albums/favorites; no Android Gallery access or pixel analysis |
| YouTube | Tap a music-time chip; open the resulting card → Play | Real public YouTube search and returned video IDs; existing WebReader player; content/provider restrictions may still prevent playback |

Enable each app's assistant in Settings to chat. YouTube's **Daily music** is
an opt-in schedule with defaults 07:00, 12:00, 15:00, 18:00 and 21:00 in the
device's current local timezone. Its agent can adjust slot hours and queries.
The scheduler runs while OctoSense's host is running; this change does not add
an Android background job for media. Retries are quiet and bounded. No timer
starts playback. A person’s explicit choice takes precedence over that slot’s
default. Saved cards restore quietly; dismiss and Undo preserve their state.

## One cross-app call

For example, the Photos agent can research a public topic when the person asks.
Its manifest requests `news.list/read/research/research_result/publish_card`.
The shell's system-app admission offer allows those exact names, loads News's
owner declarations and executor even when News has not opened, and the relay
checks consent, account, sharing, schemas and budget. The News host resolves
the story in its own store. Research runs through the scoped toolbox executor
and publishes as News. Photos never reads News files or gains unrestricted web
or model access through that grant.

News similarly has YouTube search/read/recommend/publish grants for requested
related listening. YouTube has Photos list/read/collections/publish_card grants
for a requested music-and-photos task. Granting an API does not authorize an
agent to browse private data without a relevant human request. System Assistant
can delegate through `agents.ask`; each app answers on its own peer.

## Private preference memory

After a successful human Card/Chat turn, a bounded worker can ask the configured
fast model to select short, explicit preference statements from that human
message. It never submits the whole transcript, article or assistant answer.
The host accepts only complete verbatim statements, adds hashed account/thread
provenance, deduplicates them, and checks consent/account again before writing.
Credentials, contact details, medical/financial facts and inferred preferences
are excluded. No extraction is promised for every message; failures, uncertainty
and exhausted budgets create no memory entry.

The system session alone offers `preferences.list`, `preferences.forget` and
`preferences.configure`. Ask System Assistant “What preferences have you saved?”,
“Forget that preference” or “Stop saving card-chat preferences.” Forget invalidates
pending writes and prevents a retry from recreating the same statement. It does
not erase the original chat or unrelated kernel memories.

Storage is an owner-only file under `apps/.host/system-memory/`, outside app
jails. The pinned octos Recall API has no deletion operation, so these preferences
are not duplicated there. System Assistant reads this authoritative store when
personalizing and may form a narrow app request from a relevant preference;
other app agents cannot read the global list.

## Validation and reproduction

Build both shells from this commit and its pinned App Hub dependency. No account,
provider key, private mailbox, photo library or captured user conversation is
included. Configure your own provider through the host Settings sheet.

The deterministic service/VM/relay tests exercise stored identities, denied
callers, admitted L0, routing, restored expiry, dismissal, schedule boundaries,
manual-choice precedence and private-memory deletion. Live public YouTube
search was also exercised through Makepad's platform networking. [Native, phone and model acceptance](testing/contextual-app-cards-2026-10-06.md)
records actual outcomes and build receipts; unit tests alone do not prove video
playback, keyboard UX or successful live model research.
