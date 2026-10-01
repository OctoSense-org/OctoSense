# octosense-app-peers: host-owned octos app peers

> **Where this fits.** The broker is the host connection for app agents: one host-owned peer per (app, account), owned by the system agent. It starts the system agent's `peer/input` turns on the peer's session (`#peer-<app>`), runs the person's turns (a separate `share_history` request context `#peerctx-<app>.<id>` is in progress), registers the app's tools and hands every `peer/tool/call`, approval and question to the shell, and applies the 10-minute prompt deadline and the person's Stop. Diagrams of the processes, an app agent's two lanes and a tool call with its approval: [How it fits together](../../README.md#how-it-fits-together); the details: [docs/architecture.md](../../docs/architecture.md) and [ADR 0004](../../docs/adr/0004-native-apps-hosting-and-peers.md).

Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md):
an OctoSense shell runs ONE octos kernel and ONE provider profile
([`crates/kernel`](../kernel)). A native app that declares assistant
services (the exact `octos.*` names App Hub publishes) and that host policy
grants gets ONE octos peer owned by the shell's system agent, and a scoped
service handle injected at module creation. The app talks with its agent in
its conversation (`open_conversation`): the person's lane, a request context
opened with `share_history` that runs in parallel with the peer's own session
(the system agent's lane, `peer/input`); each lane's model sees the other's
recent turns read-only, each turn carries who is speaking, the app follows
both lanes and its history merges them (octos UPCR-2026-034). It also
opens plain request contexts of that peer for per-client work
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
shell keeps each peer's token and the workspace it was created with in one
record beside its kernel's core dir (`<core_dir>/../app-peers/<namespace>.peer`,
written at once, mode 0600 in a 0700 directory; `src/peer_record.rs`), outside
every app's reach. A standalone app sets `BrokerConfig::state_dir` to its own
data dir. A new peer's workspace is the account's folder the host names
(`ToolHost::agent_workspace`), else the kernel's own provisioned one; a resume
names the recorded one, made again first if the account's folder was removed.
A peer recorded without a workspace (older `.token` files) resumes with the
account folder, else the kernel's, and the one the kernel takes is recorded.
Their memory namespace is `app/<app>/acct-<hash>`.

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
  next launch. The app's last instance then releases the peer's route
  (`peer/tools/unregister`, octos#2658): the shell's consumers share one
  kernel connection that stays open, so without it the kernel would still
  accept the system agent's input for the closed app; now the system
  agent's `peer_send_input` fails ("not connected"). An input that reaches
  the released broker first is refused (`other`, "the app was closed").
  The next launch registers the route again.
- **Nobody answers**: an approval or question on the peer's session or a
  context expires after `BrokerConfig::prompt_deadline` (10 min;
  `OCTOSENSE_PROMPT_DEADLINE_SECS` overrides it): denied or declined with
  the reason, never approved; the app hears `prompt/expired`. A turn still
  running `expiry_grace` (30 s) later is interrupted, and the peer's next
  queued turn starts, whether the broker or the host expired it first (a
  deny with the expiry note, `host_tools::expired_note`, is an expiry, not
  an answer).
- **Stop**: `ContextOp::Interrupt` on a conversation stops whatever turn
  runs on the peer, the system agent's included (the person owns the
  device); the shell's own surfaces use `broker::interrupt_where`, and the
  "Ask <app>" panel `broker::interrupt_lane_where` (one lane: its Stop is
  the person's own turn, the system agent's has its own control). A turn
  that ends before the host answered its `host_tool` approval withdraws it
  from the host (`ToolHost::host_tool_approval_closed`), as its questions
  are closed (`ToolHost::user_question_closed`).

## Testing

From the repository root:

```sh
cargo test --locked -p octosense-app-peers --features octos-core,ws   # unit + scripted-kernel tests
# The real kernel (UPCR-2026-034) with a scripted local model (python3):
OCTOS_APP_PEERS_TEST_KERNEL=/path/to/octos cargo test -p octosense-app-peers --features octos-core --test real_kernel -- --nocapture
```
