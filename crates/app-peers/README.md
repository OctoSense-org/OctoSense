# octosense-app-peers: host-owned octos app peers

Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md):
an OctoSense shell runs ONE octos kernel and ONE provider profile
([`crates/kernel`](../kernel)). A native app that declares assistant
services (the exact `octos.*` names App Hub publishes) and that host policy
grants gets ONE octos peer owned by the shell's system agent, and a scoped
service handle injected at module creation. The app opens request contexts of
that peer (one per client instance, e.g. a Rinx mini app). It never sees raw
kernel protocol, provider settings or credentials, and it never starts a
kernel. An app without granted assistant services allocates no peer.

The kernel side is octos UPCR-2026-034 (`peer/prepare` host binding with an
app/account memory namespace and `resume`, `peer/context/open|close`,
`peer/model/set`). A kernel without it is refused, never substituted by an
ordinary session with the profile's memory.

| Feature | What it adds | Who links it |
| --- | --- | --- |
| (default) | `contract` (`OctosAppService`, `OctosContext`, `ContextOp`, …) and `injection` (`offer` / `claim` / `withdraw`) — serde_json only | a hosted app (Rinx with `octosense-module`) |
| `broker` | `broker::Broker`: peer binding, contexts, a lease check on every request and before every reply, event routing, stale-reply dropping | via the features below |
| `octos-core` | `connectors::CoreConnector` (the shell's kernel, or an owned one) and `hosted` (`HostPolicy`, `launch`, `offer`) | shells; a standalone app's local runtime |
| `ws` | `connectors::WsConnector`: an explicit remote octos server | a standalone app's remote mode |
| `peer-tools` | host-registered peer tools (octos UPCR-2026-035, octos#2567): `peer_tools::HostTools`, `BrokerConfig::host_tools`, `hosted::launch_with_tools`; registration after every prepare and reconnect, `peer/tool/call` routing | `crates/ai-host`'s `toolbox-peers`; off by default |

## A shell

```rust
use octosense_app_peers::hosted;
static POLICY: std::sync::LazyLock<hosted::HostPolicy> = std::sync::LazyLock::new(|| {
    let p = hosted::HostPolicy::default();
    p.allow("rinx", octosense_app_peers::OCTOS_SERVICES);
    p
});
// Creating an instance of `module`:
let broker = hosted::launch(module.id(), module.label(), module.capabilities().iter().copied(), &POLICY);
let scope = handles.scope.to_string();
if let Some(b) = &broker { hosted::offer(module.id(), &scope, b); }
let parts = module.create(vm, open, handles);
octosense_app_peers::injection::withdraw(module.id(), &scope);
// Keep `broker`; on instance shutdown: `broker.release()`.
```

The owner of every app peer is the system agent session
`_main:api:octosense#system`. The kernel mints a host token when it creates a
peer (octos UPCR-2026-034); every later control call on the peer needs it. The
shell keeps the tokens beside its kernel's core dir (`<core_dir>/../app-peers`,
mode 0600), outside every app's reach. A standalone app sets
`BrokerConfig::state_dir` to its own data dir. Apps get the kernel's own provisioned workspace
per app and account; their memory namespace is `app/<app>/acct-<hash>`.

## An app

```rust
let service = octosense_app_peers::injection::claim("rinx", &handles.scope.to_string());
// None: hosted without assistant access. Never fall back to a kernel.
service.set_account(Some(&user_id));
let ctx = service.open_context(ContextSpec { account, instance, services })?;
ctx.call(ContextOp::Turn { text }, sink)?;   // Data(..)* then Complete(..)
ctx.close();                                  // instance closed
service.release();                            // app closed
```

## Policy on the ADR's open questions

- **Approvals** reach the person through the app's native approval UI
  (`ContextOp::Approval`); the system agent never approves for an app.
- **Background work after close**: `release()` closes every context and
  interrupts the peer's running turn. The peer and its memory stay for the
  next launch.

## Host-registered tools (`peer-tools`)

With the feature, every broker follows OctoSense issue #62's rules for
octos#2567:

- **Register on every prepare and reconnect.** Right after each successful
  `peer/prepare`, on the same link that later opens contexts and starts
  turns, the broker calls `peer/tools/register` with the app's set
  (`BrokerConfig::host_tools`), or the **empty set** when it has none (Rinx
  today). A lost link unbinds the peer, so the next request prepares and
  registers again before any turn.
- **No turns without registration.** If registering fails the peer stays
  unbound: no context opens, no turn starts, and `availability()` is
  `Failed` with the reason.
- **Calls.** `peer/tool/call` for the bound peer runs the host's
  `HostTools::call`, at most once per `(session, turn, tool call, args
  digest)` (a repeat gets the first result), and answers `peer/tool/result`.
  A tool that was not registered is refused (`not_registered`) without
  running. `peer/tool/cancel`, the call's `timeout_ms`, a release or a lost
  link cancel the call: its future is dropped after its `Cancel` fires, and
  it is never answered.

**What changes for a peer that registers** (octos#2567, by design): its
turns get exactly the registered tools (the empty set means none, not the
profile's roster), app memory and context reach only turns driven by the
registering connection, and kernel-internal continuations (a system agent's
`peer_send_input`) get no tools. The four real-kernel tests of system agent
to peer exchanges are therefore ignored with the feature on; see
`tests/real_kernel.rs`.

Without the feature none of this is compiled and the broker behaves as
before.

## Testing

From the repository root:

```sh
cargo test --locked -p octosense-app-peers --features octos-core,ws   # unit + scripted-kernel tests
cargo test --locked -p octosense-app-peers --features octos-core,ws,peer-tools   # + registration and tool calls
# The real kernel (UPCR-2026-034) with a scripted local model (python3):
OCTOS_APP_PEERS_TEST_KERNEL=/path/to/octos cargo test -p octosense-app-peers --features octos-core --test real_kernel -- --nocapture
# A kernel with octos#2567 (host-registered peer tools), the feature on:
OCTOS_APP_PEERS_TEST_KERNEL=/path/to/octos cargo test -p octosense-app-peers --features octos-core,peer-tools --test real_kernel -- --nocapture
```

On 28 Sep 2026 against an `octos` built from octos#2567 (e270d039 and again at 31c51064): without
the feature 6 passed; with it 3 passed (the new
`a_registered_host_tool_is_called_by_a_real_turn` included) and 4 are
ignored as above.
