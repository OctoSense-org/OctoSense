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
not establish a usable login. Gmail collection remains blocked until the person
signs in within normal Home. The Reconnect account action opens the existing
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
`f2429306414abc929e64e9f2ea6e1b6116d433f73fbe8a0b4a42c8ad51afd3de`. Successful
reauthentication and resumed Gmail collection are **pending**.

This establishes a shared feed and two workspace paths, not a complete UX or
performance score. This phone run used the light appearance; dark appearance was not separately
exercised. Natural periodic/Doze/reboot delivery is still unverified.
Mail's four-live-card cap currently constrains further publications; old cards
were preserved. Generic News notices are in-memory publications: this check
does not establish durable restoration for every app. Raw mail, drafts,
screenshots, accounts and provider configuration remain private.
