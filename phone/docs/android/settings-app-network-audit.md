# Per-app network policy

Audited against the ROM's pinned `packages/apps/Settings` revision
`0f0669fc699f70adb20fe6ed4b2ff1c600da6f86` and `frameworks/base` revision
`ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b` on 2026-09-25.

The built-in Apps → app details → Network access page uses the same native
`NetworkPolicyManager` operations as `AppDataUsage` and `DataSaverBackend`.
Background data is the inverse of `POLICY_REJECT_METERED_BACKGROUND`; changing
it also removes `POLICY_ALLOW_METERED_BACKGROUND`. Changing the Data Saver
exception removes the background rejection bit. Unrelated policy bits are
preserved through native add/remove operations. The UI observes effective
choices and dependencies, rather than interpreting every policy as a boolean.

Lineage's all-network, Wi-Fi, mobile and VPN rejection controls are offered only
when the running framework exposes all four audited constants with the expected
values. Their constants are looked up at runtime to prevent the Lineage build
compiler from making these features appear available on an AOSP emulator.
These controls require a Lineage runtime for behavior acceptance.

The owner-only Settings broker checks native enterprise metered-data policy,
the financed-device kiosk/controller roles, package membership and Internet
permission. That enterprise read requires system UID, so it belongs to the
existing broker; Home and the Agent receive no new broad policy privileges.
The native Settings host and OctoSense service packages remain protected. Shared
UID membership is disclosed, and every member participates in the identity and
policy checks because Android applies these network policies to the entire UID.

Only a finite field and boolean cross the UI boundary. A positive correlated
read supplies a random 256-bit, single-use capability valid for 20 seconds.
The broker checks the package UID, source path/inode, install/update/version
identity, full raw UID policy, shared membership, global Data Saver and current
authority again before any write. A stale or denied attempt consumes the
capability. The Home client and trusted Rust Settings root also check observed
identity and foreground focus. Android accessibility actions are tied to the
exact offered request, and held touch controls cannot target a replacement read.

Native background/exception changes consist of two operations and are not
atomic. Exceptions or mismatching full-policy readback report an unconfirmed
change; the client refreshes state without retrying or rolling back. Native
enforcement can also depend on app foreground state, metering, administrator
policy and VPN restrictions. The page does not claim that an enabled switch
guarantees Internet access.

Validation receipts are recorded in the emulator validation log after execution.
This change does not implement traffic history, billing cycles, quotas, proxy
configuration or VPN provider setup.
