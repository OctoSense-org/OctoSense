# Backend WebView acceptance

English | [简体中文](backend-webview.zh-CN.md)

`backend_webview.py` runs the signed fixture app through the real macOS host and WKWebView. Its temporary HTTP server has no preloaded account, authorization code or token. The driver enters fictional credentials into the native engine's form; the host receives the intercepted redirect, exchanges the PKCE code and stores the resulting session in the normal platform vault.

This is a fixture acceptance test, not a Google/GitHub sign-in test. Google continues to use the external browser. The earlier [browser acceptance](backend-login/README.md) remains separate.

## Run

Provide built `connected-app-host` and `connected-install` examples, plus the App Hub CLI. The host must include the non-default `mobile-apps,acceptance-fixtures` features. Use a new, private output directory; do not use an existing user profile.

```sh
python3 tools/connected-e2e/backend_webview.py \
  --binary "$HOST_BINARY" \
  --installer "$INSTALLER_BINARY" \
  --hub "$HUB_BINARY" \
  --out "$NEW_PRIVATE_RUN_DIRECTORY"
```

The native journey checks:

1. Cancel destroys the login view without exchanging a code or creating an account.
2. Normal process exit during pending login creates no account or exchange; a new process remains disconnected.
3. Host Back returns from a real same-origin page to the form.
4. A real connection failure shows the host error; Retry starts a fresh isolated view.
5. Registration, wrong-password feedback, login, fixed callback interception, PKCE exchange and protected identity work through the real native engine.
6. A second app receives no cookie from the first app and cannot use its connection handle, either before or after its own login.
7. The server receives its own HttpOnly cookie on real form submissions. The driver never reads or injects cookie values.
8. A new host process restores the connection, protected reads work, and logout revokes local handles and denies subsequent reads.

The fixture-only native command file permits DOM form input and inspection only when both `--backend-fixture` and `--webview-control` are explicitly supplied. Production auth views deny inspection. The fixture control does not supply callbacks, codes, cookies, tokens or accounts. Commands report only evaluation success; arbitrary JavaScript return values are not exported.

## Evidence and cleanup

The driver records binary/source hashes, server event names/status codes, input acknowledgements and state checks. Real WK snapshots and Makepad host-chrome captures are separate original images: Makepad's render-target capture does not include the operating system's WK subview. They must be reviewed separately; do not combine them and present the result as a screenshot.

A functional PASS is not a visual-review result. Inspect the original captures before publishing a sanitized receipt. Never publish the entire output directory: it contains private fixture control files, temporary connection metadata and native logs. The account and endpoint are fictional, but authorization material still belongs in the private run directory.

On success or failure the driver attempts host disconnect for remaining fixture accounts, stops its native processes and stops the server. The receipt distinguishes confirmed local-handle revocation from requested platform-vault deletion; it does not claim an independent vault readback. Preserve failed attempts and their exact source/binary association.

The final macOS run passed all eight checks, and the external-browser backend regression passed all seven checks on the same binary. See the [reviewed receipts and original captures](evidence/backend-webview-20261007/README.md). Three earlier native runs remain recorded separately: two preliminary runs and the acceptance run before the final Glance lifecycle correction.

Each app uses a separate native host process against the same isolated profile. Retry additionally verifies a fresh cookie jar within one process; simultaneous app windows are not covered. Pending-auth process exit is tested before submitting the form. Within-process sheet replacement is covered by separate host lifecycle tests. Android uses its own native activity/device driver and is not claimed by this macOS test.
