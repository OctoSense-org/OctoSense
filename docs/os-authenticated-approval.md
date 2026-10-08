# Desktop approval of account writes

English | [简体中文](os-authenticated-approval.zh-CN.md)

Linux and Windows use a separate OS authentication step when the native review
asks to send Gmail, save a GitHub file or Calendar event, or execute a declared
backend mutation. This implements [ADR 0013](adr/0013-os-authenticated-write-approval.md).
Actual user authentication followed by a business write is **unverified** on both
platforms; a compiled adapter or a successful browser login is not that proof.

The app prepares the change and opens the existing host-owned review. The person
checks its account, destination and complete content, then selects **Authenticate
& Send** or **Authenticate & Save**. The OS asks for authentication. OctoSense
receives a result, never a PIN, password or biometric. A synthetic click can open
the authentication request but cannot approve a write by itself. macOS and
Android keep their existing physical-input approval path.

## Platform requirements

On Linux, a running system polkit authority, an authentication agent in the user's
desktop session, and OctoSense's exact policy are required. The `.deb` packaging
configuration includes `desktop/resources/org.octosense.policy` at
`/usr/share/polkit-1/actions/org.octosense.policy`. It uses `auth_self`, without
retained authorization, for an active session and denies inactive/other sessions.
The host checks the file bytes, root ownership and absence of group/other write
permission. It rejects missing or changed policy, cached authorization,
cancellation and an unavailable agent. It never starts a privileged command.

AppImage and source users need an administrator to install that identical policy
as a root-owned regular file with mode `0644` at the same system path. OctoSense
does not install it. This administrator installation and a successful polkit
dialog have **not been performed** on the shared acceptance machine. The package
mapping is tested; it does not prove that an installed desktop session can approve.

On Windows, the desktop Windows Hello interface requires Windows build 22000 or
later and a configured, enabled verification method. OctoSense checks availability
before opening the prompt, binds it to the exact owning HWND and accepts only
the native `Verified` result. An older or unconfigured host reports unavailable;
it does not fall back to an ordinary confirmation button.

## What the approval binds

Only native Rust holds the one-shot proof. It binds the admitted bundle and grant,
connection and account generation, review ID, native window generation and sheet
owner. Its canonical operation digest includes the complete mutation, including
GitHub's expected file SHA and Calendar event IDs, etags and create IDs. The
authentication deadline is at most 120 seconds and cannot extend the original
review deadline.

Cancellation, a changed draft/account/bundle, expiry, reused window or duplicate
result refuses the operation. The worker rechecks expiry and authorization after
acquiring its per-app execution lock; backend mutations recheck after token
refresh, immediately before their business request. Cancelling a review does not
undo a request that has already begun at the provider.

The review checks its rendered bounds and native owner. Its pending checks run on
a timer or completion signal, not on every mouse movement. OS authentication can
take focus from the application, so focus loss alone is not evidence of approval
or grounds for treating synthetic input as trusted.

## Acceptance boundaries

The deterministic tests cover one-shot claims, late results, cancellation, changed
account/bundle/digest/window, original resource versions, and approval expiry or
revocation while queued behind another account operation. Linux native tests also
verify that the real adapter refuses to start when the required system policy is
absent. No system policy is installed for that test.

The [native refusal receipt](../tools/connected-e2e/evidence/os-approval-native.json) records 97 passing Windows tests and the real OS result: Windows Hello was unavailable and no authentication was attempted. The opt-in availability test calls the actual OS API without requesting
verification. It records an availability result or API refusal; an `Available`
result is **not** authenticated approval. The Windows workflow runs this test and
the adapter's refusal/cancellation tests.

Still **unverified**: a person completing the polkit or Windows Hello dialog,
successful execution of the exact reviewed synthetic business write, and physical
cancellation while that dialog is visible. Those checks need a suitably configured
desktop and a person. Browser login, credential-vault persistence and platform
input provenance have separate acceptance records.
