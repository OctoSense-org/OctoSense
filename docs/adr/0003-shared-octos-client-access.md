# ADR 0003: One Octos server for native and external clients

Status: Implemented in source; desktop integration verified; Android acceptance pending.

## Context

OctoSense started `octos serve --stdio`. That mode disables the HTTP listener;
a second server cannot open the same single-writer data directory. Starting
OctosCode without an endpoint instead creates an unrelated kernel. Users need
to talk to the system agent from their desktop terminal or a computer's web
browser while native app peers remain connected.

Loopback alone is not an Android authentication boundary. Upstream solo login
and the trusted-loopback `X-Profile-Id` path are inappropriate for a host-owned
agent that other APKs can reach.

## Decision

Desktop and Android start one host-managed HTTP/WebSocket Octos child bound to
127.0.0.1. The native frame router connects to that server with a random host
token and explicitly negotiates the capabilities previously enabled by stdio.
External clients use the same UI Protocol endpoint and independently negotiate
features. We do not add simultaneous stdio and WebSocket listeners upstream.

A small executable overlay, locked by source revision and SHA-256, introduces
`--host-managed`. It requires local mode, loopback binding and a nonempty token;
uses one in-process profile runtime; disables solo login; and rejects local
profile-header impersonation and alternate credentials. The normal executable
build applies the overlay; incompatible prebuilts fail closed. The workspace's
Octos crate pin is unchanged, including OpenHarmony's embedded implementation.

The shared server starts on demand and survives native consumer closure. A
provider/origin restart waits for the previous child to exit, preserving port
and token; shell shutdown ends it. The private descriptor is atomically written
with mode 0600 and removed on orderly stop. A new service lifetime rotates the
token. Pipe fixtures and OpenHarmony retain stop-when-idle semantics.

AI providers exposes **Talk to Octos** through a trusted host sheet. Only that
sheet may request connection controls. Native code copies the token or opens
the browser; scripts never receive the token. An explicit HTTP(S) web origin
is saved as the allowlist. The generated credential-free web link identifies
the system session and server-confirmed workspace. The client still asks for
server origin and token. Web assets remain separately hosted.

The system conversation is `_main:api:octosense#system` in profile `_main`.
The server-confirmed system workspace is canonicalized and persisted. Native
opens reuse that explicit cwd so Web workspace scoping does not break native
resumption after a full shell restart.
Sharing the runtime does not collapse app-peer sessions or grant raw protocol
access to ordinary apps. A computer reaches the phone's loopback listener over
an authorized tunnel; this change adds no public listener or ROM Binder API.

## Limits

This is not an Android foreground service. Shell/process death stops the
kernel. Browser-owned turns can still be interrupted when their socket closes
(upstream issue 2167); detached task ownership is a separate concern.
OpenHarmony has no external listener here; iOS has no local kernel. We do not
bundle a web client, add an embedded chat view, or provide a Linux toolchain.

## Acceptance

Real-kernel tests with a local scripted model verify native/browser turns in
one system session, simultaneous app peers, rejection of missing tokens and
spoofed profile headers, disabled solo login, rejection of untrusted origins,
provider restart with stable access, continued service after native closure,
private descriptor permissions and orderly shutdown. Hidden desktop UI checks
exercise the sheet and origin save. A headless Chromium run against the
shell-owned server opened the saved system-conversation link, sent a prompt
and displayed the local model reply. Full host restart preserves the scoped
workspace for native resumption. Artifact tests enforce overlay idempotence
and rejection of changed hashes/revisions. Android device/packaging and actual
TUI acceptance are pending; the OnePlus is untouched.
