# Mail to Calendar validation — 2026-10-05

English | [简体中文](mail-calendar-2026-10-05.zh-CN.md)

Mail now requests Calendar's shareable `calendar.events`, `calendar.add_event`
and `calendar.notify`. The system agent has its own explicit grant for the same
three tools. App Hub's per-app host admission offer is separate from those
requests and the relay's caller checks. Calendar's executor retains ownership
of the service call; Mail does not receive raw access to the Calendar directory.

Cold Mail startup registers the Calendar service and lazily loads its admitted
tool catalog/executor without opening Calendar or preparing its agent. This is
covered by a fresh-process test, distinct from the foreground phone run below.

Events accept an IANA timezone and a stable `request_id`. Exact retries reuse a
saved event; changed payloads with the same key are refused. Unknown end times
are omitted. Missing/ambiguous daylight-saving times are refused. Calendar
cards display the event's zone. Scheduling requires a human request or an
explicit provisioned policy; an email alone is not authorization. This stores
local events, not Google Calendar sync, invitation emails or reminder alarms.

## Automated checks

The shared Rust test run passed 1,203 tests, with two optional Mail tests ignored.
This includes 939 Shell, 55 Mail, five Calendar and 17 Appstore unit tests, plus
App Contract/Policy/Hub tests and documentation examples. Coverage includes
cold admission/service registration, denied ungranted callers, narrow Mail and
system grants, owner routing, actual Calendar persistence, exact-retry
idempotency, changed-payload refusal, legacy events and named timezones.

From `phone/`, with the prepared pinned workspace:

```sh
cargo test --offline --locked --features mobile-apps \
  -p octosense-shell -p octosense-mail-service -p octosense-calendar-service \
  -p octosense-appstore -p octosense-app-policy -p octosense-app-hub \
  -p octosense-app-contract
```

All nine required checks passed: desktop default/mobile-apps, phone, both shell
graphs, pins, native catalog, private paths and whitespace. The 114 build-tool
unit tests also passed. Optional real-kernel tests that return early without a
configured binary are not evidence of a live model; phone evidence is separate.

## Phone validation

Normal OnePlus 6 Home was upgraded in place to `2026100525`, preserving its
existing Gmail/provider configuration. Source: `33ef60804758d3ba09321eb42312a5e1e42acb1c`;
App Hub: `db7ef46aea22e6f1e8b5c7ed7a72548bf94feb7f` ([PR #93](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/93)).
The release APK is not debuggable; developer mode was not enabled. Its SHA-256
is `ac245c48893d5fa6b947b2854bb80c3499a5ba50a0ec642c4f50f834c28dedb1`.
The native catalog was aligned and regeneration produced no code changes.

Before the update, a private backup contained all 15 draft files and no Calendar
event file existed. Private mail, addresses, account ids, provider settings,
transcripts and screenshots are excluded from this repository.

The existing profile selects `deepseek-v4-flash`. The live system agent delegated to the existing Mail peer. The host audit
records Mail as caller (`app/os.mail`) and Calendar as owner for read, add,
read-back and notify; all returned success. Exactly one event was saved with
the requested Pacific timezone, no end, a stable retry key, and address
components matching a previously accepted reply. All 15 draft files remained
byte-identical. The system agent retried after gathering an older peer result;
Mail read the saved event and notified again, producing two banners for one
event. Notification retry deduplication is not implemented. Its final answer
also included unrelated prior-session work; this run does not establish clean
conversation summaries.

The startup test shortcut restored history without submitting the request; the
driver then entered and submitted it through the visible Assistant composer.
This was an explicit human-authorized scheduling request, not a new incoming
email trigger or a changed autonomous scheduling policy. After a clean restart without the test action, the event file remained
byte-identical and contained one event. The Calendar Glance card did not
survive process restart: generic Calendar publications are currently in-memory,
whereas Mail has its own durable restoration. Republishing the saved event is
needed to show its card again. This is an observed limitation, not a persistence
pass. Background-job Calendar publication and MiniMax were not tested here.
The driver uses Android/ADB input and platform captures, not Makepad headless
widget instrumentation. No email-send approval is automated.
