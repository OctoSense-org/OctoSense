# App capabilities and execution boundaries

English | [简体中文](capabilities.zh-CN.md)

Capabilities describe an app's intended use. Public calls run under the
app's verified identity, private storage boundary, account scope and actual
user consent; the manifest's family and destination lists are disclosures.

This policy ships in [desktop RC4](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4),
built from `9266b008`. [Source and archive acceptance](../tools/fixtures/wasm-phone-lab/README.md#rc4-release-evidence)
are recorded separately. RC2 and Home beta.2 predate this policy; isolated
phone tests do not upgrade installed Home.

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

At source `8b09e05d`, the full desktop's App Hub/component rehearsal passed
11/11 checks, and native Host API Lab passed 31/31. On the assigned OnePlus 6,
shared components passed 28/28 native assertions plus 13/13 driver checks;
Host API Lab passed 31/31 native assertions and all 45 driver checks. See the
[component evidence](../tools/fixtures/wasm-phone-lab/README.md#final-source-acceptance)
and [Host API evidence](../tools/fixtures/host-api-lab/README.md#final-source-acceptance).
The phone used isolated test packages and left installed Home unchanged.
Live-model behavior and OpenHarmony device execution remain **unverified**.

Record unit tests, simulated connectors, platform tests, physical-device tests
and released artifacts separately. A successful build or a merged PR is not a
device acceptance result. See [WebAssembly in OctoSense](wasm.md) for component
support and [host OS API status](host-os-api-status.md) for device APIs.
