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
2. **Two tokens.**

   | Token | Holder | Grants |
   | --- | --- | --- |
   | Host token | the shell process only, passed in the child's environment | everything a native consumer had on stdio (octos admin) |
   | External token | a paired web client, or a terminal client of this user through the 0600 connection file | `/api/ui-protocol/ws` only, as user `_main` |

   The external token gets no REST route (403), no admin route (401), no
   `server/shutdown` (never offered on a host-managed server), and no answers
   to approvals or questions of host-owned app peers (`peer-…`/`peerctx-…`
   sessions). Those belong to the person in the app, as UPCR-2026-034 already
   requires of the system agent. The external token is minted when Talk to
   Octos turns on, and again on **Revoke all clients** (which restarts the
   server, ending open connections). A new shell lifetime mints a new one.
   Tokens are never printed, logged, copied to the clipboard or put in a
   command line.
3. **Pairing, not copying.** A web client gets the external token only
   through octos's pairing: an 8-character code shown on the trusted sheet
   (with a QR of the web client's link), valid for five minutes and one claim.
   It works only while the sheet is open; the shell turns pairing off when the
   sheet closes. There is no "copy token" action.
4. **Browser guards.** The server answers only requests whose `Host` names
   its own loopback listener (DNS rebinding). It trusts only the web origin
   the person saved: `https`, or `http` only for localhost, 127.0.0.1 or
   [::1]. A malformed saved origin counts as none, and the kernel still
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
  cannot call `server/shutdown` (not advertised either), and cannot answer a
  peer approval;
- a foreign `Host`, a missing token, a spoofed profile header, solo login and
  untrusted origins are refused;
- pairing is single use and hands out the external token;
- a restart keeps the port and token, and rotation retires the old token;
- native and web clients share the system conversation;
- a shell killed with SIGKILL takes its kernel with it, in both modes.

The six app-peer real-kernel tests pass with Talk to Octos off. A
hidden-window desktop run turned Talk to Octos on and off from the sheet.
