# makepad run --allow-c: cargo rewrites Cargo.lock on every native build (the order of `[[patch.unused]]`), so the native key changes and the C code is rebuilt on every run

## What happens
`native.rs build()` keys the native library on the workspace's Cargo.lock bytes, then runs `cargo check --release -p <package>` (no `--locked`) to build the native packages. In a workspace with several `[patch]` entries cargo rewrites the lockfile on each of those runs: the entries are the same but the `[[patch.unused]]` blocks come back in a different order (diff of two consecutive runs: only those blocks move). The next run hashes the rewritten lock, gets a new key, finds no library under it and builds the C code again from scratch (aws-lc + blake3: about 2 minutes, every launch), while `target/makepad-run/native/` fills with one directory per run (17 here after a day).

## Suggestions
- Run the native cargo with `--locked` (the program's unit graph was already resolved from that lockfile; a lock that needs changing should fail loudly, not drift), or
- key on a canonical form of the lock (the `[[package]]` entries only, sorted), not its bytes.
