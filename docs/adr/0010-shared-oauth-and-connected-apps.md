# ADR 0010: Shared OAuth and independently installed connected apps

English | [简体中文](0010-shared-oauth-and-connected-apps.zh-CN.md)

Status: implementation in progress; macOS identity login and synthetic backend
acceptance passed. A user verified live Google Calendar login, event save and
refresh with a dedicated test account. A separate OnePlus backend fixture passed
login, Glance handoff and vault/logout checks, with visual limitations. Other
provider writes and complete device UX acceptance remain pending; see the
implementation guide for exact limits.

The [implementation guide](../../crates/oauth-service/README.md) records current
platform support, configuration and validation limits.

## Decision

OctoSense supplies a reusable OAuth host service, initially with GitHub and
Google adapters. It does not introduce an OctoSense user account. Provider
registrations and credentials belong to the host. Store apps receive opaque,
app-bound connection handles and authorized business results, never tokens.

The first consumers are three ordinary App Hub bundles maintained in App
Design Flow: GitHub Notes, Inbox Assistant and Google Calendar. Their identities
are not `os.*`, and installation must not require the system Mail or Calendar UI.

## Authentication boundary

- Provider adapters own fixed authorization/token/API origins. Bundles cannot
  supply a token endpoint, client secret or redirect destination.
- Host configuration supplies registered client IDs and provider options.
- The host shows the requesting app, provider and scopes before authorization.
- GitHub supports the device authorization flow. Google desktop uses the system
  browser, PKCE S256, a random state and an ephemeral loopback listener.
- Google Android requires a supported native authorization integration. Desktop
  loopback and custom-scheme flows must not be silently reused on Android.
- Authorization attempts expire, are app-bound and single use. Cancellation,
  logout and account replacement invalidate pending work.
- Refresh tokens and access tokens use host credential storage. Connections
  expose provider identity, scopes, expiry/status and an opaque handle only.
- Connectors validate the caller, provider, scope and selected resource on every
  operation. A granted `auth` capability alone does not grant GitHub, Gmail or
  Calendar business operations.
- Protected writes retain host review of the exact content. Model output is
  neither consent nor proof that a remote write succeeded.

## Consumers

### Developer-owned backend login

An app may use its own backend account instead of a provider identity. The host
provisions a per-app registration with exact, same-origin HTTPS authorization,
token, identity and logout endpoints. It opens the backend's registration/login
page in a host-owned WebView on macOS/Android or an external desktop browser,
exchanges a PKCE-bound one-time code and stores the resulting
session in the same app-bound vault. Apps never receive bearer credentials or
supply endpoints in a login request. The saved registration digest prevents a
configuration change from redirecting an existing token.

The adapter provides `auth.connect` with provider `backend` and
scope `app.session`, the shared account lifecycle, and `auth.backend.me` for
protected identity. Backend business APIs and bundle-driven registration require
separate implementations. Embedded sessions use nonpersistent WKWebView storage
on macOS and a private per-attempt WebView process/profile on Android 9+.
The host intercepts `https://octosense.invalid/auth/callback`, limits navigation
to the registered login origin, supplies Back/Cancel/retry, and exposes no page
bridge to contained apps. Provider authorization keeps its browser/device flows.
Windows/Linux retain the browser adapter; iOS backend login is unavailable.
Local revocation precedes remote logout; remote failure is reported separately.
The test backend must exercise real browser forms and code exchange, not seed a
logged-in account. Its HTTP-loopback exception exists only in acceptance builds.

### Connected App Hub samples

GitHub Notes reuses Rinx's extracted article editing components. Preserve
Markdown source, rich editing, selection/IME, preview and undo. Saving a note
creates or updates a chosen repository path/branch and records GitHub's commit
receipt. Use the remote blob SHA to detect conflicts; never silently overwrite
a remote edit. GitHub saves are distinct from local draft saves.

Inbox Assistant connects a mailbox, reads and filters new messages, publishes
important items, and shares authoritative draft state between chat, editing and
review. Sending requires the existing protected approval semantics. Inbox
content is untrusted data, not authority for agent actions.

Google Calendar uses Google's real Calendar API: calendar selection, event
listing, creation and editing, timezones/all-day events, paginated incremental
sync, token expiry/full resync, and ETag conflict handling. The app exposes
bounded tools to authorized peer agents and publishes cards that reopen the
same saved Google event. Local fixture events are explicitly labelled.

## Delivery and acceptance

Platform work belongs in OctoSense, capability/admission changes in App Hub,
and sample bundles and onboarding in App Design Flow. Reuse Rinx components
through a compatible, pinned dependency graph instead of copying its entire
messenger or introducing a second Makepad revision.

Test protocol/ownership/conflict behavior against deterministic transports.
Then install the exact bundles through a temporary signed catalog into a clean
OctoSense profile, test visible workflows with Makepad instrumentation, and
verify authorized remote effects against real providers. Test Windows/Linux
on those platforms and Android on the assigned OnePlus 6. Mark missing client
registrations, provider consent and unexecuted device tests as pending.

No sample contains a credential, personal mailbox, private repository content,
real calendar event or fabricated screenshot. Publishing the public catalog
is separate from local development and fixture installation.

## References

- [GitHub OAuth authorization](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [GitHub repository contents](https://docs.github.com/en/rest/repos/contents)
- [Google native OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)
- [Google Calendar synchronization](https://developers.google.com/workspace/calendar/api/guides/sync)
