# Connected accounts and App Hub samples

English | [简体中文](README.zh-CN.md)

OctoSense holds provider credentials for an installed app. The person signs in
to GitHub or Google, approves that app's scopes, and gets an app-bound connection.
There is no OctoSense account or central login backend. This implements the
shared-service part of [ADR 0010](../../docs/adr/0010-shared-oauth-and-connected-apps.md).

## Current delivery boundary

The Rust protocol, connectors, native review, account lifecycle and sample UI
are implemented. Deterministic transport tests and hidden macOS UI checks do
not prove a real provider operation. Live GitHub/Google sign-in, repository
writes, Gmail sends, Calendar writes and real model turns remain **unverified**.
The ordinary samples have not been tested on the OnePlus 6.

| Platform | Provider authorization | Credential storage | Gmail send approval |
| --- | --- | --- | --- |
| macOS | GitHub device flow; Google browser/PKCE loopback | Mail's platform Keychain adapter, separate OAuth namespace | Native pointer provenance; remote clicks refused; physical acceptance unverified |
| Windows | Same desktop flows; platform execution unverified | Windows Credential Manager; unverified on Windows | Unsupported: fails closed |
| Linux | Same desktop flows; platform execution unverified | Secret Service; unlocked service required, no plaintext fallback | Unsupported: fails closed |
| Android | GitHub flow present but unverified; **Google connection refused until its native adapter is implemented** | Mail's Android platform vault, separate namespace | Existing physical-touch provenance; this sample unverified |

This change does not remove or migrate the built-in Mail or Calendar apps.

## Register providers in the host

The host reads `<apps root>/.host/oauth/clients.json`. Keep this file outside
all bundles and source control. Example shape, with placeholders only:

```json
{
  "github": { "client_id": "REGISTERED_GITHUB_CLIENT_ID" },
  "google": {
    "client_id": "REGISTERED_GOOGLE_DESKTOP_CLIENT_ID",
    "client_secret": "GOOGLE_DESKTOP_REGISTRATION_VALUE_IF_REQUIRED"
  }
}
```

Register a GitHub OAuth app with device flow enabled. For Google desktop,
register a Desktop app and enable the Gmail/Calendar APIs needed by the samples;
configure consent/test users for that registration. Registration and live login
are **not executed by the fixture tests**. Follow
[GitHub's authorization instructions](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
and [Google's native-app instructions](https://developers.google.com/identity/protocols/oauth2/native-app).
An installed-app client secret is not a substitute for PKCE or app ownership.

The host presents app identity and scope descriptions before opening provider
authorization. Apps cannot supply endpoints, redirect URLs or client secrets.
GitHub device codes appear only in the host sheet. Google callbacks check
state, origin, path, expiry and single use. Provider errors are sanitized;
tokens never appear in app responses or the connection metadata JSON.

## App-facing contract

Declare only the services used in `manifest.json`. `auth` alone grants no
Gmail, GitHub or Calendar data access. Set `storage.accounts: true` so app
peers and account folders follow the selected connection.

| Service | Operations |
| --- | --- |
| `auth` | `connect`, `accounts`, `active`, `select`, `disconnect` |
| `github` | `repositories`, `files`, `read`, `review_save` |
| `gcalendar` | `calendars`, `sync`, `cached`, `refresh`, `get`, `prepare`, `review_save` |
| `gmail` | `labels`, `messages`, `message`, `draft.open/get/edit/review`, `events.status`, `event.status/decide` |

`auth.connect` accepts a provider and named scopes. GitHub: `read:user`,
`public_repo` or `repo`. Google: `openid`, `email`, `profile`, `calendar.list`,
`calendar.events`, `mail.read`, `mail.send`. Provider-specific scopes remain
separate from App Hub capabilities. Handles are private identifiers, not tokens.
Selection does not grant another app access to the same Google account.

Example request syntax (live authorization unverified):

```javascript
host.request("auth.connect", {
    provider: "google"
    scopes: ["openid" "email" "calendar.list" "calendar.events"]
}, fn(result) {
    // result.data.handle is this app's connection; credentials remain native.
})
```

`github.review_save` freezes repository/branch/path/content/base SHA.
`gcalendar.review_save` freezes calendar/event/ETag; a stale ETag is a conflict,
not permission to overwrite. `gmail.draft.review` freezes the durable draft
revision, recipient and body. The native review requires a physical human
activation; scripts, agents, remote instrumentation and JSON flags cannot send.
Unknown Gmail submission outcomes stay unknown and are not retried blindly.

## From an app peer to a shared service

An ordinary bundle uses its own tool namespace, such as `inbox.message`, and
an explicit `host_method: "gmail.message"` in `tools.json`. App Hub admits
only reviewed methods and enforces their minimum risk, private-data flag and
declared service capability. Secret management, review approvals and remote
writes are not available as tool aliases.

The shell reads the digest-checked bundle, registers the declaration and routes
the tool through `HostServiceExecutor`. It checks the actual target family,
injects the owner's active connection, and rejects a stale peer account or a
model-selected foreign connection. The service checks app/provider/scope again.
Cross-app access still requires the owner's `shareable` declaration and the
caller's grant; these three samples keep private reads non-shareable by default.

## New mail and Glance

`connected_events.rs` discovers installed apps with Gmail/auth capabilities,
agent consent, background permission and `<app namespace>.new_message`.
The collector establishes a forward-only Gmail history baseline, then normally
polls every five minutes while execution is allowed. After connecting and
allowing the app agent, refresh until `gmail.events.status` reports
`baseline_ready: true` **before sending a test email**. Old mail is not flooded
into notifications. Expired history uses a bounded recovery scan.

For each new message, the collector gives the account's peer an incoming turn
with admitted `AGENT.md`/skill guidance and an explicit untrusted-email boundary.
The model reads the email and decides quiet or important. Important mail can
publish the app's admitted `glance-workspace.splash` template with message data;
the host supplies the active connection and preserves the resolved source.
The model need not regenerate the Reply/Chat editor.

The event is acknowledged only after a successful turn and a durable quiet
decision or verified persisted card. Failed turns keep a retryable event.
Chat edits and manual editing use the same revisioned draft. A successful send
withdraws its card. Notifications do not imply approval to send or create events.

Android's existing JobScheduler adapter also drives this collector during a
bounded job. A fresh job forces a poll despite the foreground interval; it
finishes only when both built-in Mail and connected collectors are settled.
Android may defer quiet periodic jobs. New Java code compiles, but natural
scheduling/cold-start notification delivery for these samples is **unverified**.

## Code and validation

Read `providers.rs` → `oauth.rs`/`authorize.rs` → `store.rs` → `host.rs`.
`api.rs` implements provider requests; `calendar_cache.rs` makes paginated
snapshots atomic; `inbox.rs` owns draft/review/send state; `inbox_events.rs`
owns cursors, leases and decisions. The shell owns consented peer routing,
native review and Glance publication. Each app's peer identity is separate
from its worker thread or Tokio task.

These commands have been run from the OctoSense root:

```sh
cargo test --locked -p octosense-oauth-service
cargo check --locked -p octosense-oauth-service --features host
```

Tests use deterministic transports and synthetic accounts. They cover scope
and app isolation, revocation, callback replay, refresh, GitHub conflicts,
Calendar paging/410/ETags/DST, draft revisions, injected approval refusal,
send ambiguity, event retries and durable decisions. Native sample evidence
and authoring instructions live in Design Flow's
[connected-apps examples](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/tree/feat/connected-sample-apps/examples/connected-apps).

Do not treat `card-host` admission as a running provider service: plain
`card-host` has no OAuth, Gmail, Calendar or octos host. The separate
`connected-app-host` example exercises native services in a private fixture
profile; it does not start an agent kernel or prove production installation.
