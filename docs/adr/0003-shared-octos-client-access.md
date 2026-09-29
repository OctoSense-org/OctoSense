# ADR 0003: Talk to Octos: one kernel for native and external clients

Status: Implemented in source (desktop verified; Android **unverified**).
Depends on octos `serve --host-managed` (octos#2591, UPCR-2026-036).

## Context

OctoSense runs one octos kernel per shell process as a private
`octos serve --stdio` child (the embedded core on OpenHarmony). Native apps
share it through `crates/kernel`, and apps reach the assistant only through
the app-peer broker (`crates/app-peers`). People also want to talk to the
same assistant from a web client or a terminal UI. Starting a second kernel
does not work: octos holds a single-writer lock on its data directory, and a
separate kernel would be a different assistant with different memory.

## Threat model

- **Loopback is not a boundary.** On Android every installed app can connect
  to `127.0.0.1`. On a shared computer, every other local user can. Web pages
  reach loopback through the person's browser (cross-site WebSocket, DNS
  rebinding).
- **The kernel's authority is large.** A client that controls the system agent
  controls its tools, and through the app peers, work done for apps.
- **The shell can die at any time**: a crash, a kill, an OOM. A kernel that
  outlives it keeps its data-directory lock and its port.
- **Ports are first come, first served.** Between two kernel generations,
  another local process can bind a port that clients still trust.

## Decision

1. **Opt-in listener.** The kernel stays the private stdio child unless the
   person turns on **Talk to Octos** in AI providers (Settings → AI
   providers). Only then does the shell restart it as
   `octos serve --host-managed --host 127.0.0.1`. Turning it off restarts the
   kernel on the pipe: nothing listens, and the connection file is removed.
2. **Two tokens, delivered on stdin.** The shell writes both as the first
   two lines of the kernel's stdin; they never enter an environment or a
   command line. `/proc/<pid>/environ` is readable by every process of the
   same user, and on Android that includes the kernel's own tools.

   | Token | Holder | Grants |
   | --- | --- | --- |
   | Host token | the shell process only | everything a native consumer had on stdio (octos admin) |
   | External token | a paired web client, or a terminal client of this user through the 0600 connection file | `/api/ui-protocol/ws` only, as user `_main`, and there only an allowlist of methods |

   The external token gets no REST route (403) and no admin route (401). On
   the socket it may call only these methods: `config/capabilities/list`,
   `session/status/read`, `system/status.get`, `session/open`,
   `session/hydrate`, `session/messages_page`, `session/status.get`,
   `turn/start`, `turn/interrupt`, `turn/steer`, `turn/state/get`,
   `approval/respond`, `approval/scopes/list`, `user_question/respond` and
   `diff/preview/get`. Every other method fails with `external_method_denied`.
   That includes provider, key and model configuration (a redirected
   `base_url` would otherwise receive the stored key), skills, snapshots,
   session fork and delete, every peer method and `server/shutdown`.
   Within those methods:

   - no session of an app's assistant (`peer-…`/`peerctx-…`, under any
     `*session*` key at any depth) and no profile but `_main`;
   - no `topic`, `cwd` or `sandbox` parameter and no local media. The web
     client's sessions stay in the workspace octos bound them to; for the
     system conversation that is the saved system workspace;
   - `turn/interrupt`, `turn/steer`, `approval/respond` and
     `user_question/respond` only for turns this connection started. On the
     shared system conversation the host's own turns are untouchable, and
     an external approval is once-only (no approval scope is recorded);
   - a turn it starts gets exactly this tool set, filtered on the finished
     per-turn registry: `read_file`, `write_file`, `edit_file`, `diff_edit`,
     `apply_patch`, `glob`, `grep`, `list_dir`, `code_structure`,
     `check_workspace_contract`, `web_search`, `web_fetch`,
     `ask_user_question`, `recall`, `recall_memory`, `memory_search`,
     `memory_load`, `view_image`, `view_video` and `tool_search` (those the
     kernel has registered). That means no shell or other code execution,
     no git, no `spawn` or delegation, no `peer_*`, `send_file`, task, MCP or
     plugin tool. The memory tools read the system agent's memory, not the
     apps'. `web_fetch` refuses loopback (this server's port included),
     private, link-local and metadata addresses;
   - it runs no background continuations; the host's connection does.

   The model therefore cannot drive the apps' assistants or read the host's
   processes on the external client's behalf.

   The external token is minted when Talk to Octos turns on, and again on
   **Revoke all clients** (which restarts the server, ending open
   connections). A new shell lifetime mints a new one. Tokens are never
   printed, logged or copied to the clipboard.

   **What the profile runs.** OctoSense configures neither the tool set nor
   the sandbox: the kernel runs octos's defaults. That means all built-in
   tools, file tools fenced to the session workspace (plus the a2app memory
   read-zone on Android), and sandbox `Auto`: Seatbelt on macOS, bubblewrap or
   Landlock on Linux when present, and on Android usually none, so the shell
   tool runs unconfined as the app's user. Hence the external turn has no
   shell at all. For every session, no file tool opens a process's private
   view (`/proc/self`, `/proc/<pid>/…`, `/dev/fd`), judged on the raw,
   normalized and canonical path. The shell policy's text check on
   `/proc/<pid>/environ` is only a backstop for the host's own turns.

   *Amended 2026-09-29:* since
   [#117](https://github.com/OctoSense-org/OctoSense/pull/117) OctoSense does
   configure the tool set, in one place: before every kernel start the shell
   writes a `tool_policy` denying `group:runtime` (`shell`, `bash`,
   `exec_command`, `write_stdin`) into the `_main` profile
   (`crates/kernel/src/system_tools.rs`, `enforce`), so no `_main` session
   (the system agent, the app peers) gets octos's shell either. It replaces
   only a policy OctoSense wrote and refuses a foreign policy or the person's
   own octos home; on such a refusal the kernel still starts without the
   denial (a known gap). The sandbox is still octos's default. See
   [ADR 0004 §12](0004-native-apps-hosting-and-peers.md#12-agent-tools-what-the-manifest-declares-and-the-person-grants).
3. **Pairing, not copying.** A web client gets the external token only
   through octos's pairing: an 8-character code shown on the trusted sheet
   (with a QR of the web client's link), valid for five minutes and one claim.
   It works only while the sheet is open; the shell turns pairing off when the
   sheet closes (including the phone's Back, which calls the sheet's
   `cancel()`), in order: a late "off" cannot cancel a newer code. Failed
   claims are rate-limited, not burned, and every claim is audited without the
   code. There is no "copy token" action.
4. **Browser guards.** The server answers only requests whose `Host` names
   its own loopback listener (DNS rebinding). It trusts only the web origin
   the person saved: `https`, or on a desktop also `http` for localhost,
   127.0.0.1 or [::1] (never on Android, where any app can serve localhost). A malformed saved origin counts as none, and the kernel still
   starts. Origin protects a browser that holds a token from other pages. It
   is not authentication; the token is.
5. **The host owns the lifecycle.** The kernel's stdin is its lifeline: when
   the shell exits, crashes or is killed, the kernel reads EOF and stops. On
   Linux and Android it also asks for SIGTERM when the parent dies. Clients
   cannot stop it.
6. **The port stays the host's.** On Unix the shell binds the listener once
   and passes it to every kernel generation (`--listen-fd`). A provider
   restart therefore keeps the port, and no other process can take it in
   between; clients that connect meanwhile wait in the backlog. Without
   descriptor passing (Windows), a restart reuses the port when it is free,
   and otherwise moves to a fresh port with a fresh external token and
   rewrites the connection file.
7. **Native consumers are unchanged.** They keep the same frame API over the
   WebSocket and request octos's own stdio feature set
   (`UI_PROTOCOL_STDIO_DEFAULT_FEATURES`). The system conversation is
   `_main:api:octosense#system`. Its workspace is saved so native opens and a
   web client's scoped session agree.
8. **Upstream, not an overlay.** `--host-managed` is octos code (octos#2591);
   OctoSense carries no patch to octos.

## Consequences

- With Talk to Octos off (the default), behaviour is exactly the private-pipe
  kernel: the app-peer real-kernel tests run on it.
- With it on, the kernel no longer stops when the last native consumer leaves.
  It stops when the person turns it off or the shell exits.
- The connection file (`<core_dir>/client-connection.json`, mode 0600) holds
  the external token. On Windows it is kept in `%LOCALAPPDATA%\OctoSense\`,
  whose default ACL admits this user, SYSTEM and administrators, and not
  other users. An administrator or a process of the same user can read it:
  that is the same trust as the user's own files.
- A computer reaches a phone's server through a tunnel that keeps the port
  number (for example `adb forward tcp:P tcp:P`), because of the `Host` check.
- Browser-owned turns can still be interrupted when their socket closes
  (upstream octos issue 2167).
- This is not an Android foreground service: when Android kills the shell,
  the kernel stops with it.

## Limits

OpenHarmony (embedded core) and iOS (no kernel) have no Talk to Octos. There
is no bundled web client. Android packaging and device behaviour, and a
terminal UI reading the connection file, are **unverified**.

## Acceptance

Real-kernel tests (a scripted local model, no external calls) check that:

- nothing listens while Talk to Octos is off;
- when it is on, the external token gets 401/403 on `/api/admin/*` and REST,
  cannot call `server/shutdown` (not advertised either), provider, skill or
  snapshot methods, or profile reads, cannot open an app peer's session, and
  cannot answer a peer approval;
- **Revoke all clients** ends a live external session;
- a foreign `Host`, a missing token, a spoofed profile header, solo login and
  untrusted origins are refused;
- pairing is single use and hands out the external token;
- a restart keeps the port and token, and rotation retires the old token;
- native and web clients share the system conversation;
- a shell killed with SIGKILL takes its kernel with it, in both modes.

The six app-peer real-kernel tests pass with Talk to Octos off. CI runs
these real-kernel tests (`apps.yml`, job `kernel-security`) against octos
built at the pinned revision. A
hidden-window desktop run turned Talk to Octos on and off from the sheet.
