# Recoverable host actions

English | [简体中文](README.zh-CN.md)

A portable, optional native-host journal for actions that must survive a lost acknowledgement without repeating their external effect. This implementation is used by [BuWei](https://github.com/WeiR-h/buwei) for activity invitations, participation replies, community publication and articles.

The library is a standalone workspace, excluded from the default shell build. It changes no existing host service, shell permission or UI. Integrating another service requires a reviewed adapter.

## Contract

1. The trusted host binds `Authority` to its authenticated SDK account and issues an opaque, non-serializable `Grant` after consent.
2. `prepare` persists the exact action, identity, revision, digest and confirmation lifetime.
3. `confirm` checks the current grant and adapter's business revision.
4. `execute` claims the durable row before dispatch. The adapter uses the original operation ID as its service transaction key.
5. A verified receipt must match the operation ID, account, target and complete action digest. An uncertain result remains pending.
6. `reconcile` only looks up the original effect. A new consent for the same account can recover an older operation, without resending it.

Account changes and revoked grants invalidate old confirmations. Queue order and capacity remain application rules. SQLite indexes keep recovery scoped to the application and account even with a large history.

## Reproduce

Requires Rust 1.98.0 and a C compiler for bundled SQLite:

```sh
cargo test --manifest-path crates/action-receipts/Cargo.toml --locked
cargo run --manifest-path crates/action-receipts/Cargo.toml --locked --example benchmark_recovery
```

The benchmark compares the same production queries over 50,008 synthetic rows before and after the index migration and checks identical results. It reports local database latency, with no network or model calls.

Adapters in the unit tests are scripted. Real SDK behavior is separately checked in BuWei's two-account runner; passing these unit tests does not establish another host's integration. The journal uses a host-supplied clock; the host adapter must recheck live identity and permission immediately before an external effect. It must verify server evidence, not treat a successful socket write as a receipt.

Apache-2.0. No credentials, sessions or raw user records are included.

The additional `benchmark_owned_history` example keeps all 50,008 records under one account, with 50,005 verified historical results and three unresolved actions. This separates recovery-index gains from cross-account filtering. Run it with the same command and `--example benchmark_owned_history`.
