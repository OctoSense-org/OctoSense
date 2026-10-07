# Connected accounts and App Hub samples

English | [简体中文](README.zh-CN.md)

OctoSense holds provider credentials for an installed app. The person signs in
to GitHub or Google, approves that app's scopes, and gets an app-bound connection.
There is no OctoSense account or central login backend. This implements the
shared-service part of [ADR 0010](../../docs/adr/0010-shared-oauth-and-connected-apps.md).

## Current delivery boundary

The Rust protocol, connectors, native review, account lifecycle and sample UI
are implemented. Deterministic transport tests and hidden macOS UI checks do
not prove a real provider operation. Live GitHub/Google sign-in, repository writes, Gmail sends and Calendar writes
remain **unverified**. A real DeepSeek peer processed synthetic incoming mail
through the installed app’s admitted tools and updated its saved reply through
chat. A Calendar peer also read the selected synthetic event through its own
tool and answered its title, time and location. This proves model/tool integration, not Google delivery. The ordinary
samples have not been tested on the OnePlus 6.

| Platform | Provider authorization | Credential storage | Gmail send approval |
| --- | --- | --- | --- |
| macOS | GitHub device flow; Google browser/PKCE loopback | Mail's platform Keychain adapter, separate OAuth namespace | Native pointer provenance; remote clicks refused; physical acceptance unverified |
| Windows | Same desktop flows; platform execution unverified | Windows Credential Manager; unverified on Windows | Unsupported: fails closed |
| Linux | Protocol tests and host compilation passed on Linux; browser login and GUI unverified | Secret Service; unlocked service required, no plaintext fallback; native vault test refused the locked/unavailable build-host store | Unsupported: fails closed |
| Android | GitHub flow present but unverified; **Google connection refused until its native adapter is implemented** | Mail's Android platform vault, separate namespace | Existing physical-touch provenance; this sample unverified |

This change does not remove or migrate the built-in Mail or Calendar apps.

## Sign in as a user

A distributor-configured build supplies OctoSense's provider registration.
Choose **Connect GitHub** or **Connect Google** in the app, review the requested
access, and complete sign-in in your browser. You do not need a developer
account, a Google Cloud project, or a JSON configuration file. Your own tokens
remain in the host's platform credential store, bound to the requesting app.

If this build has no registration for the provider, its sign-in sheet explains
that sign-in is unavailable and directs you to the distributor or an update.
Adding this resolver does not register OctoSense with either provider: a release
is ready for sign-in only after its maintainer supplies and validates the
registration. Existing beta.2 downloads contain no registration defaults.

## Identity, provider access and an app's own backend

These are separate choices; none requires an OctoSense account.

| Purpose | Current contract |
| --- | --- |
| Identify a GitHub user inside an app | Grant `auth` and request `read:user`. The host verifies GitHub's numeric user ID and login, then returns an app-bound handle plus `app_id`, `provider`, `subject`, `label`, `scopes` and optional `expires_at`. Repository access is not required. This does not provide a verified email address. |
| Identify a Google user inside an app | `auth` also admits identity-only `openid`, `email` and `profile` scopes without Gmail or Calendar capabilities. The host verifies the provider subject and uses the email as its label only when Google reports it verified. The same platform authorization limitations apply. |
| Access provider data | GitHub repositories additionally require the `github` capability and repository scopes. Google Gmail and Calendar require their own `gmail` / `gcalendar` capabilities and scopes, regardless of which identity an app uses for login. |
| Register or log in to an app's own backend | A reusable host-managed backend login/session service is **proposed, not implemented**. A local connection handle or returned profile is not a backend-verifiable SSO assertion. |

The proposed backend flow lets the developer's HTTPS login page offer GitHub
sign-in or its own registration and login. The backend verifies identity and
issues its own session; the host would store that separate session for the app.
The shared connector's GitHub or Google tokens are not exported to app backends.
A developer's backend may obtain its own separately consented GitHub token
through its own OAuth flow. Existing network access does not turn local GitHub
metadata into proof that a remote backend can trust. Apps must not collect
passwords or provider secrets themselves.

A backend-owned login page could use a dedicated host authentication WebView,
if its identity provider permits embedding. That adapter is **not implemented**:
the current Makepad reader WebView lacks the callback interception and isolated
session contract needed here. Google authorization uses a supported browser
flow, including when a backend offers a Google button. Browser-based backend
login is also a valid design; it does not require an embedded WebView.

## Configure a release (maintainers)

Register OctoSense once under the distributor's identity. Create a GitHub OAuth
app with device flow enabled. For Google desktop, create a Desktop app, enable
the Gmail/Calendar APIs used by the samples, and configure the consent screen.
Public access to sensitive/restricted scopes requires the applicable Google
verification; test users can authorize a testing registration. End users do
not repeat these setup steps. Follow
[GitHub's authorization instructions](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
and [Google's native-app instructions](https://developers.google.com/identity/protocols/oauth2/native-app).

Supply these environment variables when Cargo compiles the host, including when
Cargo is invoked by the desktop packager. They are not runtime environment
overrides. A packager's skip-build option cannot add them to an existing binary.

| Build variable | Native registration value |
| --- | --- |
| `OCTOSENSE_GITHUB_CLIENT_ID` | OctoSense's GitHub OAuth client ID; no GitHub client secret is used |
| `OCTOSENSE_GOOGLE_DESKTOP_CLIENT_ID` | OctoSense's Google Desktop client ID |
| `OCTOSENSE_GOOGLE_DESKTOP_REGISTRATION_VALUE` | Optional Desktop registration value sent as Google's `client_secret`, if required for that native client |

These values ship in the host executable and cannot be kept confidential there.
They identify the distributor's native application; they are not user passwords,
access/refresh tokens, signing keys, or confidential web-client secrets. Never
put those private credentials in build variables or app bundles. Keep real
registration values outside committed source and use only registrations owned
by the distributor. Do not reuse a TV/device or web client for Google desktop.

The same resolver supplies authorization and connector token refresh. Tests use
fictional registrations and do not establish live provider sign-in. Before
distribution, validate a real account's consent, refresh, cancellation and
revocation. Google sign-in uses the system browser with PKCE and a loopback
callback; an embedded WebView is not a substitute for supported authorization.
Google Android still requires its native adapter.

## Advanced operator override

An optional `<apps root>/.host/oauth/clients.json` replaces the complete set of
build registrations. Omitted providers are disabled; `{}` disables both. A
malformed, oversized or unreadable override refuses sign-in rather than silently
switching to another registration. Only an absent file uses build defaults.
Keep this operator file outside app bundles and source control. Placeholder
example (live registration is **unverified**):

```json
{
  "github": { "client_id": "REGISTERED_GITHUB_CLIENT_ID" },
  "google": {
    "client_id": "REGISTERED_GOOGLE_DESKTOP_CLIENT_ID",
    "client_secret": "NATIVE_DESKTOP_REGISTRATION_VALUE_IF_REQUIRED"
  }
}
```

Changing the client registration does not migrate existing provider tokens.
Reconnect affected accounts using the intended registration. An installed-app
registration value is not a substitute for PKCE or app ownership.

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
The model need not regenerate the Reply/Chat editor. Desktop script-card tiles
show the publication title and summary; opening one gives the script a bounded
app viewport, so its editor and scrolling regions do not collapse under a
content-sized ancestor. Template workspaces own their Email/Reply/Chat navigation;
the shell adds no duplicate Chat tab. Legacy scripts retain content measurement
and outer scrolling unless they opt into `viewport: true`. Foreground-published cards can restore before agent
consent, while removed Glance grants, explicit agent refusal, sign-out and
account changes still block restoration.

The event is acknowledged only after a successful turn and a durable quiet
decision or verified persisted card. Failed turns keep a retryable event and return an error to the scheduler, which
waits 60 seconds before retrying; a failed model turn cannot masquerade as a
successful two-second queue drain.
Chat edits and manual editing use the same revisioned draft. A successful send
withdraws its card. Notifications do not imply approval to send or create events.

Android's existing JobScheduler adapter also drives this collector during a
bounded job. A fresh job forces a poll despite the foreground interval; it
finishes only when both built-in Mail and connected collectors are settled.
Android may defer quiet periodic jobs. New Java code compiles, but natural
scheduling/cold-start notification delivery for these samples is **unverified**.

## Code and validation

Read `providers.rs` → `oauth.rs`/`authorize.rs` → `protocol.rs` → `store.rs` → `host.rs`.
`protocol.rs` adapts `oauth2` 5 to the host's bounded, fixed-origin transport;
the library constructs authorization/token requests and parses protocol
responses. The host still owns caller identity, cancellation, callback
validation, scope admission and credential storage. GitHub device polling stays
one request at a time so each attempt rechecks its owner, expiry and cancellation;
the library's built-in polling loop cannot replace those lifecycle checks.
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

The real macOS credential adapter was also exercised with a unique disposable
profile and fictional credentials. Store, reopen/read and logical revocation
passed, and neither access nor refresh data appeared in the profile's files.
This explicit opt-in test uses the actual OS vault and may require an unlocked
desktop session; it is ignored during ordinary test runs:

```sh
cargo test --locked -p octosense-oauth-service --features host host_vault_acceptance::platform_vault_persists_across_reopen_without_plaintext_credentials -- --ignored --exact
```

This does not test provider authorization, physical-send approval or prove the
legacy Mail vault's void-returning deletion operation removed an OS item.
The test creates no provider request and reads no existing account.

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
