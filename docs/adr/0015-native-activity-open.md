# Native activity verification and open proposal

English | [简体中文](0015-native-activity-open.zh-CN.md)

Status: proposed verification seam. Rinx [PR 89](https://github.com/hagency-org/Rinx/pull/89)
supplies a generic locator and SDK source context. This does not install BuWei,
enable an agent or grant membership. It does not change external pins.

`crates/shell/src/native_activity.rs` checks a host-issued SDK account/generation,
re-fetches the source event, compares room, event, sender and pointer, checks
current source/target membership, then asks the registered application's
authoritative resource adapter whether that sender may share the resource.
Queued work is checked again before display. Expiry is two minutes. Focus keys
include account, generation, application, room and resource; repeated cards
focus the same view without mixing accounts.

The host implements `ActivityBackend` on its worker using the Rinx SDK and native
application registry. `open_or_focus` schedules display/focus through the
existing `ModuleHost`, using only the verified key. It must not join rooms,
enroll, send messages or issue a lease. An existing module receives navigation
through `ModuleHost::send_custom`; the module validates its authoritative
resource again. A new module uses its registered `OpenSchema`. SDK identity
is assigned outside deserialized open arguments. There is no provider key,
kernel protocol, callback URL or arbitrary process command in this interface.

BuWei is the motivating example: `app=buwei`, opaque resource=activity ID,
target=activity room. The app checks its verified activity owner and state
before presenting enrollment; actual enrollment remains separately confirmed
through its prepare/confirm/execute/reconcile receipt path. Its existing v2
activity event protocol remains compatible until the generic carrier is adopted.

Validation: the portable seam tests were compiled with Rust 1.98 on Windows
GNU. They cover forged source, wrong room, unknown version, account switch,
same-account login generation change, revoked membership/authority, expiry,
repeat focus and separate account views. These use a fake SDK adapter, not real
network identity evidence. Full shell desktop/phone compilation and a concrete
Rinx SDK/ModuleHost adapter are unverified. Production routing stays unwired;
the draft requests review of this boundary before an external pin update.
