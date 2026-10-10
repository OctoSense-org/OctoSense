# App capabilities and execution boundaries

English | [简体中文](capabilities.zh-CN.md)

This describes the declaration-only capability implementation in this source
tree. It does not claim that a downloaded release contains these changes.
Desktop 0.1.0-rc.2 predates the component model and this policy.

## Declarations describe usage

An app's `capabilities` and `network.hosts` describe its intended use for
discovery, store disclosure and review. Omitting a public API family does not
deny a call. Adding one does not supply user consent, connect an account or
authorize a write. Keep declarations accurate so users can understand the app.

This applies to an app's Splash requests, its own Wasm functions and components,
and its agent's offered host-service tools. These entry points use the same app
identity, but retain their different interaction rules: an agent or component
cannot open a permission sheet or impersonate a person's input.

`requires` and `host_api.required` are compatibility requirements. They still
prevent an app from installing on a host that cannot run its ABI or methods.
Use `runtime.describe` to discover optional methods on the current device.
Neither a declaration nor a Wasm wrapper creates an API absent from the host.

## What still authorizes an operation

| Boundary | What the host checks |
| --- | --- |
| App identity | Verified installed bundle, current admission and the host-owned app ID and profile directory. Withdrawal, tampering or replacement invalidates pending work. |
| Device access | Per-app consent and the operating system's permission. OctoSense's OS permission does not authorize every app. Permission requests need a foreground app. |
| Account data | The app's own connected account and current account binding. Script or model arguments cannot choose another app's account. |
| Private writes | Host-owned review and trusted user input for mail sends, Calendar changes and GitHub saves. A background callback cannot approve them. |
| App files | The app's storage jail, active account, quota and resource lifetime. A component sees the app's folder, not the host filesystem. |
| App assistants | A genuine app assistant offer, user consent and a supported public method. Declarations alone do not create an assistant. |
| Tools shared between apps | The offered tool, its owner, sharing approvals and risk rules. Removing a family check does not grant another app's tools. |
| Resource and platform limits | Linked service, supported platform, bounded input and output, deadlines, quotas and cancellation. |

The runtime's private profile access and raw unowned agent notifications are
internal interfaces, not public app capabilities. Apps use the scoped host APIs
and the app-agent broker. Web content also keeps its own sandbox and request
validation; removing a manifest host list does not remove those boundaries.

Before evaluating a card, App Hub's `apply_device_consent` binds the verified
launch bundle's app ID to its isolate and applies the device-consent boundary.
A storage folder alone is insufficient: native file and media services require
that same isolate identity when obtaining its storage handle. A failed launch
clears the previous identity; scripts cannot supply a replacement app ID.

## Validation and release

The acceptance matrix pairs declared and undeclared calls, then separately
tests denied consent, a foreign profile, stale accounts, unapproved writes,
storage escape and quota exhaustion. Component acceptance also checks actual
store resolution, alias calls, isolated state and withdrawal.

Record unit tests, simulated connectors, platform tests, physical-device tests
and released artifacts separately. A successful build or a merged PR is not a
device acceptance result. See [WebAssembly in OctoSense](wasm.md) for component
support and [host OS API status](host-os-api-status.md) for device APIs.
