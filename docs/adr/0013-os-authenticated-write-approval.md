# ADR 0013: OS-authenticated approval for reviewed writes

English | [简体中文](0013-os-authenticated-write-approval.zh-CN.md)

Status: implementation under review; OS-authenticated user acceptance pending. Linux/Windows physical-pointer provenance remains unsupported. Browser and vault tests do not establish write approval.

## Problem and decision

The host-owned review in `crates/shell/src/connected_review.rs` displays the exact Gmail reply, GitHub change, Calendar event or declared backend mutation. Its native `oauth-service` capability requires trusted pointer down and up. The Android/macOS provenance adapters support this; Linux/Windows currently reject genuine users too. Synthetic input must not become trusted to make the buttons work.

Keep existing physical approval unchanged. Add **OS-authenticated approval** for Linux/Windows: the person reviews the content, activates **Authenticate & Send/Save** in the native sheet, and completes the OS's authentication UI. A normal, remote or synthetic click alone can never submit a write.

This evidence is distinct from `trusted_user_input()`. It must not set that flag, manufacture `down/up=true`, or enter script/tool arguments. Both evidence types feed one internal, single-use execution claim. Provider login, token storage and ordinary browser use remain separate operations.

## Binding and lifecycle

The native review owner creates a challenge containing a random nonce, short monotonic deadline, admitted app/bundle identity, connection and active-account generation, review ID, canonical SHA-256 digest of the fully displayed operation, and live native-window generation plus host-sheet/isolate owner. The digest includes destination, content and resource version. No app-supplied digest, command, callback or authentication result is trusted.

Only the visible foreground native review can start authentication. Closing/replacing it, changing account/draft, expiry, disconnect, withdrawal or window destruction/reuse cancels the challenge. On completion, recheck every binding and the current admitted operation before atomically consuming its nonce. Late/repeated results and changed drafts fail. Existing provider/account/resource conflict checks still run when submitting.

The state machine is `Reviewing → Authenticating → Claimed → Finished`; cancellation/expiry is terminal. Failure can return to Reviewing only with a fresh challenge. Authentication is asynchronous. Cancel never authorizes a write. The OS prompt uses a short host-generated description; full mail bodies and credentials do not enter polkit details or logs.

## Linux

Call the system polkit authority for a dedicated `org.octosense.approve-business-action` action. Supported installers ship its root-owned policy: `allow_any=no`, `allow_inactive=no`, `allow_active=auth_self`, with no retained authorization. No privileged command/helper is executed. This approves a same-user business action, not a privileged system operation.

A noninteractive preflight must report a fresh challenge, not already-authorized/cached permission. Interactive `CheckAuthorization` binds the current process PID/start time/UID and unique cancellation ID. Reject retained/temporary authorization, dismissal, unavailable authority/agent/policy and unexpected replies. Cancel outstanding checks when their owner disappears. See [polkit policy semantics](https://polkit.pages.freedesktop.org/polkit/polkit.8.html) and the [authority interface](https://polkit.pages.freedesktop.org/polkit/eggdbus-interface-org.freedesktop.PolicyKit1.Authority.html).

The machine administrator remains trusted. AppImage/source launches must explicitly report missing policy; they cannot silently install it or fall back to synthetic approval. This task does not authorize installing a system policy on the shared test host.

## Windows

Use `UserConsentVerifier.CheckAvailabilityAsync` and desktop `IUserConsentVerifierInterop::RequestVerificationForWindowAsync`, bound to the exact native HWND. Accept only `Verified`. Cancellation, busy/unconfigured/disabled devices, unavailable API, destroyed windows and HRESULT failures deny authorization. OctoSense receives a verification result, not the user's PIN or biometric data. Microsoft's [desktop guidance](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.ui.userconsentverifier?view=winrt-26100) specifies this interface; its [requirements](https://learn.microsoft.com/en-us/windows/win32/api/userconsentverifierinterop/nf-userconsentverifierinterop-iuserconsentverifierinterop-requestverificationforwindowasync) list Windows build 22000 as the minimum. Older hosts report unavailable unless a separately reviewed adapter is added.

## Implementation and acceptance

1. Add a native-only `oauth-service` approval module: immutable bindings, challenge/cancellation state and opaque evidence without public construction or serialization. Refactor Gmail, connector and backend mutation claims to consume typed evidence while preserving physical-input semantics.
2. Put platform adapters behind that boundary. They mint evidence only for an outstanding bound challenge and never execute a business write directly.
3. Extend the existing native review widget with authentication state and availability messages, reusing its complete-content display and submission worker. Add no app-callable approval API.
4. Include the Linux action in supported package payloads and check policy/ownership. Missing registration stays unavailable.
5. Test changed account/digest/window, replay, late results, cancellation and expiry. Actual native negative tests must reject missing policy/Hello. Then use disposable accounts to verify OS-authenticated writes and rejection of synthetic-only attempts on each supported platform.

Keep distinct receipts for login, native vault, ordinary embedded browser, OS-authenticated approval and the resulting remote write. Compile, unit, Xvfb and credential-storage passes do not prove user approval. A physical-positive claim needs an actual person/device. Approval and business-write acceptance remain unverified until those native interactions complete.

The implemented boundary, package prerequisites and unverified native positives
are recorded in [desktop write approval](../os-authenticated-approval.md).
