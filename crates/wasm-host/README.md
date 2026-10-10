# OctoSense Wasm runtime

English | [简体中文](README.zh-CN.md)

`Runtime` checks and compiles an app's verified Wasm bytes, then creates
isolated module or component instances. See the [Wasm guide](../../docs/wasm.md)
for the guest ABI, allowed imports, storage and execution limits.

Compiled code is an optional local cache, keyed by the source digest and engine
compatibility hash. A cache read gets at most 50 ms of waiting time. Across all
runtimes in one process, at most two reader threads may remain in flight; there
is no waiting queue. A timed-out reader keeps its slot until it finishes, then
drops any late result. Cache reads accept at most 64 MiB per artifact. This is a
cache optimization limit, not a new limit on valid Wasm source.

If the cache is busy, slow, oversized or unreadable, both module and component
loads compile the verified source and skip cache writes for that load. They do
not rewrite a file whose read is still blocked. A promptly missing entry may
still be populated atomically; a promptly read but incompatible entry may be
recompiled and replaced. The cache directory is host-owned: serialized code is
never accepted from an app bundle as a trusted compiled artifact.

Cache publication does not spawn warm-up reader threads. `precompile` uses the
same bounded reader path as ordinary loads. Invocation deadlines, cancellation,
guest memory limits and the host request deadline remain unchanged. Source
compilation still takes time; this change bounds waiting for cache reads, not
compilation or cache publication.

The runtime tests cover deterministic blocked readers, saturation, slot release,
read errors and size limits; execution after fallback; and actual module and
component deserialization from a ready cache.
