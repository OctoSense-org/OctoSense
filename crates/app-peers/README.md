# octosense-app-peers: host-owned octos app peers

> Where this fits: see the [OctoSense architecture](../../docs/architecture.md) (with diagrams).

Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md):
an OctoSense shell runs ONE octos kernel and ONE provider profile
([`crates/kernel`](../kernel)). A native app that declares assistant
services (the exact `octos.*` names App Hub publishes) and that host policy
grants gets ONE octos peer owned by the shell's system agent, and a scoped
service handle injected at module creation. The app talks in its peer's ONE
shared conversation (`open_conversation`: the peer's own session, which the
system agent drives too; each turn carries who is speaking, octos#2626), and
opens request contexts of that peer for per-client work
(`open_context`: one per client instance, e.g. a Rinx mini app). It never sees raw
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
- **Nobody answers**: an approval or question on the peer's session or a
  context expires after `BrokerConfig::prompt_deadline` (10 min;
  `OCTOSENSE_PROMPT_DEADLINE_SECS` overrides it): denied or declined with
  the reason, never approved; the app hears `prompt/expired`. A turn still
  running `expiry_grace` (30 s) later is interrupted, and the peer's next
  queued turn starts.
- **Stop**: `ContextOp::Interrupt` on a conversation stops whatever turn
  runs on the peer, the system agent's included (the person owns the
  device); the shell's own surfaces use `broker::interrupt_where`.

## Testing

From the repository root:

```sh
cargo test --locked -p octosense-app-peers --features octos-core,ws   # unit + scripted-kernel tests
# The real kernel (UPCR-2026-034) with a scripted local model (python3):
OCTOS_APP_PEERS_TEST_KERNEL=/path/to/octos cargo test -p octosense-app-peers --features octos-core --test real_kernel -- --nocapture
```
