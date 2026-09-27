# Android API-call and Binder telemetry

Date: 16 September 2026. Status: capability reference and proposed capture workflow.
This document adds no new phone validation, tracing session, hook installation
or device configuration change. Exact event availability and decoded-method
coverage must be checked on the target ROM.

Related: [ADR 0001](../adr/0001-hybrid-android-launcher-and-system-bridge.md),
[system integration](system-integration-plan.md), and
[performance measurement contract](performance-plan.md).

## Purpose

Observe which APIs an app actually calls, which Android services it contacts,
and where execution waits. For OctoSense, compare equivalent app-launch,
app-to-Home, Recents and system-control interactions with Trebuchet/SystemUI.
Correlate calls with input and rendered frames before attributing a delay to a
particular service or renderer.

Root can expand access to application and system processes, but it does not
automatically produce a complete API-call log. SELinux can restrict root, and
tool compatibility depends on the ROM. The earlier bench root probe is recorded
in the [system integration plan](system-integration-plan.md#1-what-is-verified-today);
it does not establish every tracing capability below.
[Android SELinux](https://source.android.com/docs/security/features/selinux).

## What each layer exposes

| Layer / tool | Observable details | Limits |
|---|---|---|
| Binder IPC — Perfetto | Caller/server process and thread, correlated requests/replies, synchronous or one-way calls, durations and scheduling delays; AIDL interface/method names when emitted | Kernel events alone do not expose complete method arguments or semantic return values. Some names remain unresolved. |
| Binder IPC — live ftrace stream | Events as they arrive: timestamp, emitting thread, transaction ID/code, destination process/thread where known, reply indicator and flags | Needs available/enabled tracepoints and access to tracefs. Correlating events into calls requires analysis. |
| Java/Kotlin — targeted Frida hooks | Selected app/framework methods, arguments, return values, exceptions and caller stacks | Requires instrumentation. Coverage depends on loaded classes, class loaders, optimization and tool/ROM compatibility. |
| Native/JNI — targeted Frida hooks | Selected native functions, arguments, return values and backtraces | Raw pointers need type/signature knowledge; stripped symbols and inlining can limit identification. |
| Linux system calls — strace | File/socket access, synchronization and other syscalls, arguments and errors | Does not automatically reconstruct Android API names or decode Binder Parcels and encrypted network content. |
| Window transitions — Winscope | WindowManager and SurfaceFlinger state/layer changes during transitions | Provides visual-system context, not a complete API-call or payload log. |

Sources: [Perfetto Binder analysis](https://perfetto.dev/docs/analysis/stdlib-docs#android-binder),
[Binder event fields](https://perfetto.dev/docs/reference/trace-packet-proto#BinderTransactionFtraceEvent),
[ftrace live streams](https://www.kernel.org/doc/html/latest/trace/ftrace.html#trace-pipe),
[Frida Android](https://frida.re/docs/android/),
[Frida instrumentation](https://frida.re/docs/javascript-api/),
[strace](https://strace.io/),
[Winscope](https://source.android.com/docs/core/graphics/winscope/overview).

Some Perfetto collection is available through ordinary ADB access. Root is useful
for additional sources and process access; it is not a prerequisite for every
row in the table. Tool availability on this bench is not implied by listing it.
[Perfetto system recording](https://perfetto.dev/docs/getting-started/system-tracing).

## Watching Binder calls

### Recorded interactive timeline

Use Perfetto for a bounded interaction capture, then inspect the trace in its
timeline viewer. Enable the available Binder transaction/receive events and
collect scheduler events plus process/thread metadata. The Android
`binder_driver` tracing category provides Binder context on supported builds;
AIDL/userspace annotations can supply readable interface and method names.
Select a transaction to follow the participating threads and request/reply flow.
[Android Binder trace example](https://source.android.com/docs/core/tests/debug/systrace),
[scheduler and process metadata](https://perfetto.dev/docs/data-sources/cpu-scheduling).

Representative kernel events are `binder/binder_transaction` and
`binder/binder_transaction_received`. Check their availability before configuring
a capture. A timestamp or transaction code is not evidence of a decoded method
name, successful operation, or complete end-to-end response.
[Binder event schema](https://perfetto.dev/docs/reference/trace-packet-proto#BinderTransactionFtraceEvent).

Example analysis query for a compatible Trace Processor version, after loading
a capture containing Binder events:

```sql
INCLUDE PERFETTO MODULE android.binder;

SELECT
  client_ts / 1e6 AS timestamp_ms,
  client_process,
  client_thread,
  server_process,
  server_thread,
  interface,
  method_name,
  is_sync,
  client_dur / 1e6 AS client_wall_ms,
  server_dur / 1e6 AS server_wall_ms
FROM android_binder_txns
WHERE client_process GLOB 'dev.makepad.octosense*'
   OR server_process GLOB 'dev.makepad.octosense*'
ORDER BY client_ts;
```

This is a documented-schema example, not a query validated on a new phone
capture. Preserve null/unresolved fields. Synchronous client wall time includes
waiting for the server and can include scheduling delays; do not add overlapping
client and server durations as independent costs. A one-way submission does not
prove the requested operation has completed.
[Perfetto Binder tables and delay breakdown](https://perfetto.dev/docs/analysis/stdlib-docs#android-binder).

### Live terminal stream

With supported kernel events and permissions, ftrace's `trace_pipe` streams
Binder events while the interaction runs. A parser can correlate transaction
IDs and resolve threads to processes for a live display.

Reading `trace_pipe` **consumes events**. Use a dedicated tracing instance and
its own buffer where supported; do not attach a reader to a buffer already used
by another capture. If isolation is unavailable, schedule a separate capture.
Preserve existing tracing configuration and clean up only the session's own
instance. Separate buffers do not eliminate tracing overhead.
[Kernel live-stream and tracing-instance documentation](https://www.kernel.org/doc/html/latest/trace/ftrace.html#instances).

## Obtaining API names and arguments

A Binder transaction code identifies an operation within a particular interface;
it is not a globally unique API name. Resolve it against the matching AIDL or
generated Stub/Proxy implementation for the installed build, or use emitted
AIDL trace annotations. Do not reuse numeric mappings across ROM revisions
without checking them.
[Android Binder model](https://source.android.com/docs/core/architecture/ipc/binder-overview).

When arguments or semantic results are needed, instrument selected client or
service methods before/after Parcel serialization. Decoding raw Parcels needs
the matching interface and types; Binder handles and file descriptors are not
ordinary JSON values. Capture exceptions and observed state separately from the
fact that a Binder reply arrived. For HTTP APIs, application-layer hooks may
expose requests/responses; a socket or packet trace alone does not automatically
decode HTTPS content. These are additional instrumentation tasks, not output
promised by the standard Binder trace.
[Frida Java and native instrumentation](https://frida.re/docs/javascript-api/).

## Proposed OctoSense workflow and evidence

1. Record ROM/build identity, APK hashes, process identities, tool versions,
   available events, and the exact capture configuration. Check existing trace
   sessions before enabling collection.
2. Capture one controlled interaction with Perfetto. Include relevant Home,
   Bridge, Quickstep, SystemUI, system_server and SurfaceFlinger activity; filter
   the analysis without losing their cross-process relationships.
3. Identify repeated calls, synchronous work on the UI thread, queue/scheduling
   delays and relevant frame boundaries. Correlation alone is not proof of cause.
4. Add narrowly targeted hooks only where names/arguments remain necessary.
   Prefer controlled fixture data and metadata over collecting user content.
5. Recheck behavior and performance with intrusive hooks removed. Retain raw
   traces, queries, unresolved mappings, dropped-event statistics, cleanup
   results and the instrumentation used. Follow the existing paired-run
   performance contract for any parity claim.

Keep this telemetry opt-in and outside the production render/input path. Do not
add periodic shell polling or per-frame synchronous IPC to obtain diagnostics.
An observed call establishes neither permission for OctoSense to perform it nor
successful device behavior; validate actual grants and observed effects through
the ADR's integration tests.
