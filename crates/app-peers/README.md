# octosense-app-peers: host-owned octos app peers

English | [简体中文](README.zh-CN.md)

> **Where this fits.** The broker is the host connection for app agents: one host-owned peer per (app, account), owned by the system agent. It starts the system agent's `peer/input` turns on the peer's session (`…#peer-<slug>`), runs the person's turns in a separate request context opened with `share_history` (`…#peerctx-<id>`; octos#2636), registers the app's tools and hands every `peer/tool/call`, approval and question to the shell, and applies the 10-minute prompt deadline and the person's Stop. Diagrams of the processes, an app agent's two lanes and a tool call with its approval: [How it fits together](../../README.md#how-it-fits-together); the details: [docs/architecture.md](../../docs/architecture.md) and [ADR 0004](../../docs/adr/0004-native-apps-hosting-and-peers.md).

Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md):
an OctoSense shell runs ONE octos kernel and ONE provider profile
([`crates/kernel`](../kernel)). An app with an agent gets ONE octos peer per
account, owned by the shell's system agent. A native app that declares
assistant services (the exact `octos.*` names App Hub publishes) and that
host policy allows gets its peer and a scoped service handle injected at
module creation. These declarations opt the native app in; they do not
limit it to a subset of the four public assistant methods. A script app with an agent (News, Mail, Calendar today)
gets its peer `card.<app id>` from the shell's `octos` host service
(`crates/ai-host/src/contained.rs`), which launches it through
`hosted::launch` as well. The app talks with its agent in
its conversation (`open_conversation`): the person's lane, a request context
opened with `share_history` that runs in parallel with the peer's own session
(the system agent's lane, `peer/input`); each lane's model sees the other's
recent turns read-only, each turn carries who is speaking, the app follows
both lanes and its history merges them (octos UPCR-2026-034). It also
opens plain request contexts of that peer for per-client work
(`open_context`: one per client instance, e.g. a Rinx mini app). It never sees raw
kernel protocol, provider settings or credentials, and it never starts a
kernel. An app without an agent opt-in or host consent allocates no peer.
For script apps, the host also verifies the bundle and exact host profile;
an agent block or admitted tools can opt in without `octos.*` declarations.

The broker still intersects each host-issued context's requested services
with its supported surface. This is a context lease, not a manifest
permission. Context/account ownership, cancellation and tool approvals
remain enforced; a manifest cannot add kernel methods or another app's tools.

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
Their memory namespace is `app/<id>/acct-<tag>` (`broker::app_namespace`;
`<id>` is the id the broker was launched with: `rinx` for Rinx,
`card.os.mail` for Mail's script-app peer; `<tag>` is 16 hex digits of an
FNV-1a hash of the normalized account, not the account folder's SHA-256
hash, #139).

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

- **Approvals**: a gated host tool's approval (`confirm: host`) goes to the
  shell's approval router through the `ToolHost`, and a `confirm: app`
  call to the owning app's own sheet; other octos approvals raised in an
  app's context reach the app (`ContextOp::Approval`). The system agent
  never approves for an app.
- **Background work after close**: `release()` closes every context and
  interrupts the peer's running turn. The peer and its memory stay for the
  next launch. The app's last instance then releases the peer's route
  (`peer/tools/unregister`, octos#2658): the shell's consumers share one
  kernel connection that stays open, so without it the kernel would still
  accept the system agent's input for the closed app; now the system
  agent's `peer_send_input` fails ("not connected"). An input that reaches
  the released broker first is refused (`other`, "the app was closed").
  The next launch registers the route again.
- **Removing an account or uninstalling the app** (ADR 0004 §11):
  `purge::purge_app` (the shell's storage lifecycle calls it through
  `purge::purge_in_background`, after deleting the folders) sends octos's
  `peer/purge` for each (app, account) peer the host recorded (its name and
  host token, owned by the system agent; octos#2649), retries
  `peer_purge_busy`, then drops the record (`<ns>.peer`) and makes every
  live broker of the app forget the peer. A record saved before records
  carried the peer's name is tried under each name the host can derive
  (`<label> <8 hex>` for the labels it passes and the label a broker of
  the app used in this process); `peer_not_found` there is a failure and
  the record is kept. Adding the account again makes a new peer. Signing
  out never purges.
- **The app's conversation reads the account folder** (ADR 0004 §11):
  where the host says the agent works in the account folder
  (`ToolHost::context_reads_account`; the shell: the manifest's
  `storage.agent_workspace` is `"account"` and the agent has that
  workspace), the person's lane is opened with octos's `read_parent`
  (octos#2647): read-only, never another context's folder. A kernel that
  ignores it is refused. A client's request context (`open_context`, a
  Rinx mini app) stays fenced to its own folder.
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

## Host-provisioned app guidance

The shell can call `contained::set_guidance(app_id, account, TrustedGuidance)`
before preparing a contained app's peer or between turns. The broker snapshots
that app/account's instructions and named skill texts for each request-context,
conversation, and system `peer/input` turn. Updates affect the next turn without
recreating the peer or deleting its history. The combined text is limited to
16 KiB and 16 skills; the host must check consent and persist any overlay itself.
Account changes and revocation clear the in-memory guidance.

This supplies host-provisioned **text**, not kernel-native skill installation or
discovery. Guidance and request data are separately serialized ordinary text
inputs; they are not separate kernel system-message roles. Tool grants and the
original `TurnTrigger` remain the authorization boundary, including for incoming
email. Incoming text cannot replace the host's structured guidance fields.

The guidance tests use scripted connectors, not a provider or phone. They
cover incoming approval provenance, unchanged request data, all three entry
points, account clearing and updates without peer recreation. The hosted
service tests cover the public method set after opt-in and host consent;
unknown methods and apps with no agent offer remain refused.
