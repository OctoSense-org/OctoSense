# Native backend WebView acceptance — 2026-10-07

English | [简体中文](README.zh-CN.md)

**PASS: 8 macOS WebView checks and 7 external-browser regression checks.** Both used binary `8d7b1b0032486a8020ceafb2117f51cacfe6b285fbb6270706a700956750256e` with the as-tested native overlay tree `50371b7e54adf63fa8aa8c0b1acc53226dcff9ba`. [WebView receipt](receipt.json) and [browser regression](browser-regression.json) retain the relevant source hashes and signed-install proof. Build inputs and both acceptance source snapshots stayed unchanged through the refresh. The receipts additionally bind the final Glance lifecycle source hashes. This fixture uses the installed-app host, not the full shell Glance UI; the latter has separate tests. The locked tree includes the final Android inset changes, but these Mac runs do not validate Android behavior.

The WebView journey registered a fictional account using the actual WK form, displayed wrong-password feedback, signed in through the intercepted fixed callback and real PKCE exchange, and loaded protected identity. It also exercised Cancel, normal process exit before form submission, Back, transport-error/Retry, fresh HttpOnly cookie stores, cross-app handle denial, cold restoration and logout. No callbacks, codes, cookies, tokens or accounts were injected. The normal host and platform vault handled the session; only the disposable HTTP backend and its availability faults were synthetic.

The browser regression additionally exercised rotated refresh credentials after a temporary protected-data failure and repeat login. Its three recorded browser diagnostics concern favicon requests; no CSP/form-action diagnostic was recorded. These were scripted acceptance runs, not physical-user tests or live-provider tests. Their full run durations were 15.063 seconds and 11.128 seconds respectively, not UI latency or FPS measurements.

Eight original captures were inspected individually and copied without modification:

| Capture | Surface and observation |
| --- | --- |
| [Registration](04-registration-webview.png) | Native WK snapshot: fictional username, masked password and both form actions visible after scrolling. |
| [Wrong password](05-invalid-password-webview.png) | Native WK snapshot: backend credential error is readable. |
| [Retry](04-retried-webview.png) | Native WK snapshot: the login form loads again in the fresh view. |
| [Pending process reopened](02-pending-process-reopened.png) | Native Makepad image: no account or protected data after quitting during login. |
| [Protected identity](06-connected.png) | Native Makepad image: actual host response appears in the app. |
| [Cross-app denial](08-cross-app-denied.png) | Native Makepad image: a second app cannot use the first connection. |
| [Cold restore](09-cold-restored.png) | Native Makepad image: protected identity works after process restart. |
| [Logout](10-logout-denied.png) | Native Makepad image: no account/data and protected access denied. |

WK images capture the OS web view. Makepad images capture its render target and do not include the OS subview; none are composites or whole-screen photographs. Auth host-chrome images were reviewed privately but omitted because they display the temporary loopback endpoint. Native logs, widget snapshots, profiles, control files and authorization material are not published. Visible `example.invalid` identities are fictional.

Both runs stopped their native/browser/server processes and durably revoked all local fixture connection handles. Platform-vault deletion was requested through normal host disconnect, without independent vault readback. The [three previous runs](prior-attempts.json) remain historical. The preceding 8+7 run, including its original receipts and eight PNGs, is preserved [before the Glance refresh](historical/before-glance-refresh/README.md). None is relabeled as this refreshed build. Android, simultaneous app windows and physical typing are outside this receipt; within-process sheet replacement has separate lifecycle tests. Reproduce with the [native driver guide](../../backend-webview.md).
