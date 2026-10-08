# Connected accounts and App Hub samples

English | [简体中文](README.zh-CN.md)

OctoSense holds provider credentials for an installed app. The person signs in
to GitHub or Google, approves that app's scopes, and gets an app-bound connection.
There is no OctoSense account or central login backend. This implements the
shared-service part of [ADR 0010](../../docs/adr/0010-shared-oauth-and-connected-apps.md).

## Current delivery boundary

The Rust protocol, connectors, native review, account lifecycle and sample UI
are implemented. Live identity-only GitHub and Google sign-in passed on macOS
through the native host and provider browser flows. GitHub requested `read:user`;
Google used a dedicated test account with `openid email profile`. The synthetic
backend passed browser registration/login, protected identity, refresh recovery,
native restart, logout and isolation between two installed apps using the real
platform vault. Native backend WebView acceptance also passed eight macOS
checks, including real form input, cancellation, retry, app/session isolation and
process restart; the desktop browser regression passed seven checks on that
same final binary. A separate OnePlus 6 backend fixture completed actual form
login, Glance handoff/cancellation, protected identity, native-vault cold restore
and logout. Its [Android receipt](../../tools/connected-e2e/evidence/backend-android-20261007/README.md)
records limited visual evidence and unrun cases; it is not full phone UX acceptance.
Repository writes and Gmail sends remain
**unverified**; identity-only sign-in does not grant or prove those operations.
In a later Mac session, the installed signed Calendar app completed real Google
authorization after the dedicated test account was added to the OAuth project's
tester list. The user confirmed the calendar list appeared, saved a test event
through the app's review flow, and saw it after Refresh. This was manual
verification, without independent API readback; editing/deletion and a
production-verified Google release remain unverified. The [sanitized receipt](../../tools/connected-e2e/evidence/calendar-login-20261007.json) separates these observations.
A real DeepSeek peer processed synthetic incoming mail
through the installed app’s admitted tools and updated its saved reply through
chat. A Calendar peer also read the selected synthetic event through its own
tool and answered its title, time and location. This proves model/tool integration, not Google delivery. Of the
three samples, only GitHub Notes has been checked on the OnePlus 6: its local
editing, in a standalone test APK that left the regular Home in place
([OnePlus Notes check](../../tools/connected-e2e/evidence/notes-oneplus-20261006/README.md));
Inbox Assistant and Google Calendar have not run there.

| Platform | Provider authorization | Credential storage | Gmail send approval |
| --- | --- | --- | --- |
| macOS | GitHub device flow; Google browser/PKCE loopback | Mail's platform Keychain adapter, separate OAuth namespace | Native pointer provenance; remote clicks refused; physical acceptance unverified |
| Windows | Same desktop flows; platform execution unverified | Windows Credential Manager; unverified on Windows | Unsupported: fails closed |
| Linux | Protocol tests and host compilation passed on Linux; browser login and GUI unverified | Secret Service; unlocked service required, no plaintext fallback; native vault test refused the locked/unavailable build-host store | Unsupported: fails closed |
| Android | GitHub flow present but unverified; **Google connection refused until its native adapter is implemented** | Mail's Android platform vault, separate namespace | Existing physical-touch provenance; this sample unverified |

`desktop-v0.1.0-beta.2` is the first release with the connected-account
services (`auth`, `github`, `gmail`, `gcalendar`); no Home (phone) release can
install connected apps yet. In beta.2, `auth` has no backend sign-in, and
provider registrations come only from `clients.json`
([Advanced operator override](#advanced-operator-override)). Beta.2 also
predates [#356](https://github.com/OctoSense-org/OctoSense/pull/356), which is
on `main` but in no release yet. So in beta.2, only a Gmail send checks for a
physical press, while GitHub and Calendar saves use a host sheet that does not;
an agent's `glance.publish` still accepts a `script` card; and Calendar syncs
the full event history with `gcalendar.sync` and sync tokens instead of the
bounded window below. The earlier
releases, `desktop-v0.1.0-beta.1` and `home-v0.1.0-beta.1`, use app contract
1.1.0, which has no `auth` capability. Their stores list apps that declare
`auth` but refuse to install them. These services do not replace or migrate
the built-in Mail and Calendar apps.

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

If Google displays **403: access_denied** and says only developer-approved
testers may access the app, the provider registration was found, but the account
is not on that OAuth project's test-user list. The maintainer adds the dedicated
test account under **Google Auth Platform → Audience → Test users**, then starts
a fresh sign-in. This unblocks testing; it does not verify the app for public
distribution. Keep personal accounts out of isolated acceptance runs.

## Identity, provider access and an app's own backend

These are separate choices; none requires an OctoSense account.

| Purpose | Current contract |
| --- | --- |
| Identify a GitHub user inside an app | Grant `auth` and request `read:user`. The host verifies GitHub's numeric user ID and login, then returns an app-bound handle plus `app_id`, `provider`, `subject`, `label`, `scopes` and optional `expires_at`. Repository access is not required. This does not provide a verified email address. |
| Identify a Google user inside an app | `auth` also admits identity-only `openid`, `email` and `profile` scopes without Gmail or Calendar capabilities. The host verifies the provider subject and uses the email as its label only when Google reports it verified. The same platform authorization limitations apply. |
| Access provider data | GitHub repositories additionally require the `github` capability and repository scopes. Google Gmail and Calendar require their own `gmail` / `gcalendar` capabilities and scopes, regardless of which identity an app uses for login. |
| Register or log in to an app's own backend | A host-owned login WebView on macOS/Android uses the admitted bundle's app-bound registration, PKCE code exchange and the backend's protected identity endpoint. An external-browser option remains available on desktop. |

The backend flow lets the developer's HTTPS login page offer its own
registration and login. A desktop backend flow that also offers GitHub sign-in
uses the external-browser presentation so it can visit the provider's origin. The backend verifies identity and
issues its own session; the host stores that separate session for the app.
The shared connector's GitHub or Google tokens are not exported to app backends.
A developer's backend may obtain its own separately consented GitHub token
through its own OAuth flow. Existing network access does not turn local GitHub
metadata into proof that a remote backend can trust. Apps must not collect
passwords or provider secrets themselves.

Backend login reuses the native WebView engine in a dedicated authentication
mode. On macOS each attempt has a nonpersistent WKWebView store. Android 9+
uses a non-exported Activity in a separate process and a unique WebView data
directory, removed after the process exits. The reader's existing cookies are
untouched. Navigation stays on the registered login origin; the exact callback
is intercepted before loading, and login pages have no app-tool JavaScript bridge.
Back, Cancel, loading and retry controls belong to the host. Contained apps
cannot open this authentication mode directly or inspect its page.

GitHub and Google retain their existing provider authorization flows. A backend
login that needs to visit another provider's origin must use the desktop browser
option; the embedded mode does not silently open external sites. Windows/Linux
keep desktop browser login, and iOS backend login remains unavailable.
On Linux/Windows, ordinary `WebReader.open` also refuses explicitly because the
pinned runtime has no embedded SystemBrowser adapter. The separate optional CEF
Browser widget is not a drop-in authentication adapter: its current global
persistent profile and disabled Chromium sandbox do not provide our per-app
session and navigation boundaries. Shipping embedded support still requires an
isolated adapter and native OS acceptance; an external browser does not count.

## Developer backend contract

Declare `auth` and `storage.accounts: true`. Connect with
`auth.connect` arguments `{"provider":"backend","scopes":["app.session"]}`.
On macOS/Android this defaults to the embedded login page. Set
`"presentation":"webview"` to require that mode, or `"presentation":"browser"`
for the desktop external-browser flow. Unsupported combinations fail explicitly.
Use the ordinary `auth.accounts`, `auth.active`, `auth.select` and
`auth.disconnect` lifecycle. `auth.backend.me` takes this app's active
`connection` handle and returns
`{"connection":"…","backend_id":"…","identity":{"sub":"…","label":"…"}}`,
containing the backend's verified identity.
Apps may also declare bounded business operations and call `auth.backend.request`.
The host supplies the bearer credential; the app receives only the operation's JSON
result. This is not an open HTTP proxy: callers cannot choose a URL, method, header,
or a path outside their admitted declaration.

The shell can supply a digest-verified manifest `backend` declaration through
`host::set_backend_resolver`. It is re-read on every credential use; a resolver error
fails closed. Its JSON shape matches the registration below without `app_id`, which
always comes from the admitted bundle. The manifest requires `backend-api-v1`,
`auth`, and `storage.accounts: true`. Without a bundle declaration, the existing
operator registration remains available. The shell wires this resolver to its signed
catalog and digest-checked bundle loader. Each resolution records the binding in
host-private metadata; a changed, removed, withdrawn or invalid declaration revokes
backend handles before returning. This durable observation prevents restoring an
older declaration from reviving a session. Installation/update/removal notifications
also revoke existing backend handles, requiring reconnect even when a new version
keeps the same declaration. A quiet five-second local metadata watcher performs
eager revalidation without provider HTTP.
`host::invalidate_backend_registration` remains available for explicit lifecycle
revocation. Each request also checks the registration binding and authorization epoch before
network access and before accepting its response.

For operator-managed integrations, an operator provisions `<apps root>/.host/oauth/backends.json`, outside app
bundles and source control. Example configuration only; the example domain
does not host a service:

```json
{
  "schema": 1,
  "apps": {
    "com.example.notes": {
      "id": "notes-backend",
      "app_id": "com.example.notes",
      "client_id": "registered-public-native-client",
      "authorization_url": "https://login.example.test/authorize",
      "token_url": "https://login.example.test/token",
      "me_url": "https://login.example.test/me",
      "logout_url": "https://login.example.test/logout",
      "scopes": ["app.session"],
      "operations": {
        "notes.list": {"method":"GET", "path":"/api/notes", "query_keys":["tag"]},
        "notes.create": {"method":"POST", "path":"/api/notes"}
      }
    }
  }
}
```

Call the declared operations with an active connection:

```json
{"connection":"opaque-host-handle","operation":"notes.list","query":{"tag":"work"}}
```

```json
{"connection":"opaque-host-handle","operation":"notes.create","body":{"text":"A fictional note"}}
```

Both objects are arguments to `auth.backend.request`. GET operations execute on a
worker. POST/PUT/PATCH/DELETE open the same native immutable review used for
GitHub/Calendar; a physical activation is required, and scripts/agents cannot
approve through `auth.backend.sheet.save`. Background mutations return a request
to open the app. Cancellation or expiry before approval performs no write. Once an
approved HTTP request begins, cancellation cannot promise to undo the server's
operation; errors are not automatically retried.

Operations have exact ASCII paths on the login origin, up to 32 declared query keys,
and no path templates or auth-endpoint aliases. JSON requests and responses are
bounded to 64 KiB, transport time to 30 seconds, and redirects are refused.
Only declared query keys are accepted and URL-encoded by the host. Connections
remain app-bound and active-account-bound; account selection, logout, withdrawal,
or grant changes invalidate pending work. Per-app serialization prevents account
changes crossing a remote write without holding the global vault metadata lock.
Servers remain responsible for their own user authorization and idempotency.

The backend must implement a public-client authorization-code flow with S256
PKCE, state echo and one-time codes. For embedded login, allow exactly
`https://octosense.invalid/auth/callback`: this is a host-intercepted return
address, never a network service. The external-browser desktop flow uses the
host's ephemeral loopback callback instead.
The token endpoint accepts code and refresh grants and returns OAuth bearer
tokens. `GET /me` returns `{sub,label}`; `POST /logout` revokes the session and
acknowledges `{"logged_out":true}`. Registration and password entry belong to
the backend's host-presented web page, never the contained app.

Each endpoint is a distinct exact HTTPS URL on the same origin and port 443;
queries, fragments, URL credentials and HTTP redirects are refused. The host
binds each saved connection to the normalized registration. Changing the
registration requires reconnecting; it cannot redirect an existing token.
Logout revokes the local handle before attempting remote logout and reports
the remote result separately. Embedded callbacks retain caller, state, expiry
and single-use checks. Android versions below 9 and iOS refuse embedded backend
login. Windows/Linux execution remains unverified.

The synthetic backend uses real browser forms, HTTP code exchange and protected
requests. HTTP loopback is available only in the non-default acceptance build,
with explicit isolated host registration; it is not a release configuration
override. See the [browser acceptance driver](../../tools/connected-e2e/backend-login/README.md) and [native WebView acceptance](../../tools/connected-e2e/backend-webview.md).

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
example; replace the values with the distributor's registered native clients:

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
| `auth` | `connect`, `accounts`, `active`, `select`, `disconnect`, `backend.me`, `backend.request` |
| `github` | `repositories`, `files`, `read`, `review_save` |
| `gcalendar` | `calendars`, `cached`, `refresh`, `get`, `prepare`, `review_save` |
| `gmail` | `labels`, `messages`, `message`, `draft.open/get/edit/review`, `events.status`, `event.status/decide` |

`auth.connect` accepts a provider and named scopes. GitHub: `read:user`,
`public_repo` or `repo`. Google: `openid`, `email`, `profile`, `calendar.list`,
`calendar.events`, `mail.read`, `mail.send`. Backend login uses `app.session`.
Provider-specific scopes remain
separate from App Hub capabilities. Handles are private identifiers, not tokens.
Selection does not grant another app access to the same Google account.

Example Calendar request syntax (production Google approval remains unverified):

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
revision, recipient and body. All three writes require a physical activation
of the native host review control, with native provenance checked on press and
release before the one-use capability crosses into the worker. Scripts,
agents, remote instrumentation and JSON flags cannot approve a save or send.
Dismissal cancels an unsubmitted review; a selected-account change is checked
again before writing. Unknown Gmail submission outcomes stay unknown and are
not retried blindly.

`gcalendar.refresh` atomically replaces a finite agenda: from UTC midnight
30 days before today to UTC midnight 366 days after today. Google expands
recurring series into actual occurrences inside that window; instance IDs,
ETags and exceptions are preserved, and cancelled occurrences are excluded.
Every page uses the same bounds. A later refresh moves the window; a failed or
incomplete refresh preserves the last complete cache, including its recorded
`window`. Cached older schema-1 snapshots remain readable until a successful
bounded refresh replaces them.

This agenda uses full **window snapshots**, not incremental history sync.
Google forbids `timeMin`/`timeMax` with `syncToken`, so this path never stores
or reuses `nextSyncToken`. Raw `gcalendar.sync` is no longer exposed; use
`refresh` and `cached`. See the [Google events.list contract](https://developers.google.com/workspace/calendar/api/v3/reference/events/list).

Provider HTTP and app-local draft/cache changes serialize per host profile and
app. An unrelated app can operate while a provider is slow. Account selection,
disconnect, uninstall and final connection admission share the same app lock;
the process-wide metadata lock covers only short load/commit sections. Token
refresh reloads current metadata before committing, so it cannot overwrite
another app's account changes or restore a revoked connection.

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
agent consent, background permission and `<app namespace>.new_message`, where
the app namespace is the last segment of the app id. The collector establishes a forward-only Gmail history baseline, then normally
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
For developer backends, read `backend.rs` (registration validation, PKCE and
bounded HTTP requests) → `host_backend.rs` (consent, callbacks, refresh and
logout) → `store.rs` (app ownership and registration binding). Google token
responses normalize only its two documented identity-scope URI aliases;
missing permissions still fail authorization.
`api.rs` implements provider requests; `calendar_cache.rs` makes paginated
snapshots atomic; `inbox.rs` owns draft/review/send state; `inbox_events.rs`
owns cursors, leases and decisions. The shell owns consented peer routing,
native review and Glance publication. Each app's peer identity is separate
from its worker thread or Tokio task.

These commands have been run from the OctoSense root:

```sh
cargo test --locked -p octosense-oauth-service
cargo check --locked -p octosense-oauth-service --features host
cargo test --offline --locked -p octosense-oauth-service --features host,acceptance-fixtures --lib
```

At [#353](https://github.com/OctoSense-org/OctoSense/pull/353), the last command
passed 75 tests with one explicit platform-vault test ignored. #356 added tests;
the count on `main` has not been recorded.
The separate native backend acceptance used the actual vault, including a cold
process restart. Live provider acceptance covered identity login, connection
metadata restoration after restart and local disconnect, not provider refresh
or remote revocation. Registrations, account details and raw evidence
remain outside the repository. Windows, Linux and phone login execution are not
covered by these macOS results. The [sanitized provider receipt](../../tools/connected-e2e/evidence/provider-login-20261007.json)
records the exact scopes, native binary and limits.

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
Calendar bounded-window paging/rollover/recurrence/ETags/DST, cross-app availability and refresh-commit races, draft revisions, injected approval refusal,
send ambiguity, event retries and durable decisions. Native sample evidence
and authoring instructions live in Design Flow's
[connected-apps examples](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/tree/main/examples/connected-apps).

Do not treat `card-host` admission as a running provider service: plain
`card-host` has no OAuth, Gmail, Calendar or octos host. The separate
`connected-app-host` example exercises native services in a private fixture
profile; it does not start an agent kernel or prove production installation.

### Backend business-request validation

Run on macOS for this implementation:

```sh
cargo test --locked -p octosense-oauth-service --features host
cargo test --locked -p octosense-oauth-service --features acceptance-fixtures backend
```

The fixture performs real local HTTP signup/login, notes create/list, distinct-account
isolation, logout, redirect refusal, payload limits and accidental credential-echo
refusal. Host tests cover active-handle/scope invalidation, durable registration
revocation, background-write refusal, and cancellation of the native review. These
are synthetic protocol/host tests with an isolated memory vault, not a claim of a
physical approval, a rendered installed-app journey, live provider integration, or
Android/Windows/Linux acceptance. The separate OS-vault test remains opt-in.
