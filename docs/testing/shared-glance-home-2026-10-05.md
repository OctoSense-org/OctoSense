# Shared Glance in normal Home — 2026-10-05

English | [简体中文](shared-glance-home-2026-10-05.zh-CN.md)

The initial [Mail background test](mail-background-2026-10-05.md) exercised a
separate test package. It did not populate the person's normal launcher. This
follow-up checks the actual Android Home entry and multiple publishing apps.

## Deployment and preserved state

The user requested integration with normal Home. On the assigned OnePlus 6
(API 35), `dev.makepad.octosense` retained the Home role and was upgraded from
2026100216 to 2026100516 with its existing signing identity. No ROM or Bridge
was flashed. Both packages and the old Home APK were backed up before migration;
MailTest was stopped to avoid duplicate collection.

The destination had no Mail account. Its existing provider profile and other
apps were retained. The migration preserved the existing Mail account metadata,
mail cache, event policy, account workspace, publications, notification outbox
and previously granted Mail consent. All 271 selected regular-file hashes
matched after copying; all 11 saved draft files also remained unchanged after
opening a card and its Chat tab. No reply was approved or sent. Temporary stay-awake was restored to its
previous value and ADB returned to non-root mode before credential entry.

**Credentials were not successfully migrated.** The copied encrypted password
has an `OSK1` header and depends on the source package's Android Keystore key.
It cannot be decrypted under Home's UID. A matching file hash therefore does
not establish a usable login. Gmail collection initially remained blocked until the person
signed in within normal Home, as recorded below. The Reconnect account action opens the existing
host-owned sign-in sheet; using the same address, username and incoming-server settings
updates the existing account without deleting drafts. No key was exported and
no plaintext-vault workaround was used.

## Observed device checks

| Check | Result |
| --- | --- |
| Android Home intent → right swipe | Normal Home showed four saved Mail cards |
| Saved reply card | Full-screen Email/Chat workspace, native editor and Review reply visible; saved transcript retained |
| System agent → News agent | DeepSeek invoked `news.list`, then `news.notify`, under News's `own_agent` identity |
| Mixed feed | News and Mail visible together in the same normal Home Glance |
| News expansion | Generic full-screen Card/Chat workspace opened from the summary |
| Reconnect on 2026100517 | Reachable Inbox action opened the host-owned sign-in sheet; credential entry left to the person |
| Shared feed ordering test | Existing `published_cards_lead_the_feed_by_priority_then_recency` regression passed |

Codex drove the phone through ADB and reviewed platform screenshots. DeepSeek
`deepseek-v4-flash` supplied the News notice content from an actual cached story;
`news.notify` used the host's fixed notice template. This was not a newly
model-authored L0 source. The model published twice with the same test card id,
replacing the notice. Existing Mail card sources were not rewritten. This run
contains no fresh MiniMax, Calendar, Photos, Maps, Camera or YouTube acceptance.

## Build receipt and remaining checks

Home 2026100516 used source `e42ac4ee`, `mobile-apps`, and the pinned kernel.
Its APK SHA-256 is
`2607d21f0c2bab06b1a593b3f34d9b60dd56d67c5f22f4dc00a3fd23e7a4ce35`.
Home 2026100517 was then built and installed with the Reconnect action. Its APK
SHA-256 is `379bc439a4aa836ad0d769a1ee603b27b19666ad5961b28e8b1c3e22aa55e685`; Mail bundle SHA-256 is
`f2429306414abc929e64e9f2ea6e1b6116d433f73fbe8a0b4a42c8ad51afd3de`. The person subsequently reauthenticated successfully; collection resumed as
recorded below.

## Reauthentication and unattended follow-up

A read-only check after the person's sign-in found the same account identity,
unchanged importance instructions and all 11 original draft files unchanged.
The sign-in completed at 2026-10-06 01:09:30 UTC; a successful collection was
recorded at 01:15:07 UTC with no collection error. The mailbox contained 21
additional messages relative to the migration snapshot, including the person's
new delivery test.

DeepSeek's incoming-event lane had eight additional `mail.skip_event` receipts
with `no_action`, covering routine promotional, newsletter and statement mail.
The latest delivery test was still pending among 16 queued events; there was no
new successful publication receipt. Android logs explicitly rejected other
publication attempts because Mail already had four Glance cards. Thus silent
filtering was observed, but important-mail card delivery has **not passed**.
A queued message must not be counted as an importance decision.

Android recorded a periodic Home Mail job ending after its four-minute lease;
the phone was subsequently dozing and the next normal window was scheduled.
This follow-up did not force a job, wake the phone, prompt the model, delete
cards or send mail. It demonstrates an unattended job occurrence, not a full
Doze/reboot or latency guarantee. ADB was returned to non-root mode after the
read-only snapshot. Message contents and identifiers are excluded here.

The [scrolling follow-up](glance-scroll-2026-10-05.md) removes the small card-count
quotas and records successful automatic publication after the phone update.

This establishes a shared feed and two workspace paths, not a complete UX or
performance score. This phone run used the light appearance; dark appearance was not separately
exercised. Longer periodic/Doze/reboot delivery is still unverified.
At the 2026100517 checkpoint, Mail's four-live-card cap constrained further
publications; old cards were preserved. Generic News notices are in-memory publications: this check
does not establish durable restoration for every app. Raw mail, drafts,
screenshots, accounts and provider configuration remain private.
