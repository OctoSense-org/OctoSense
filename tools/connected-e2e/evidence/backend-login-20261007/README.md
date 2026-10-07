# Native backend login evidence

English | [简体中文](README.zh-CN.md)

**PASS: seven functional checks and eight individually reviewed original screenshots.** This is a synthetic backend running on local HTTP, a real isolated Chrome context, and the real macOS native host, callback listener and platform vault. No accounts, authorization codes or tokens were pre-seeded. [Receipt](receipt.json) records binary/source hashes, signed installation, sanitized server events and image hashes. [Reproduction](../../backend-login/README.md) explains the test-only browser handoff and build.

Registration, sign-in, protected identity, refresh failure recovery, cold process restart, logout, repeated sign-in and isolation between two signed apps passed. Both local connection handles were revoked through the host; platform-vault deletion was requested but not independently read back. All owned native/server processes stopped. There were zero input replays and zero instrument read-only retries. Three browser console errors were automatic `/favicon.ico` requests; none was a CSP error or failed authorization callback.

The images below are unmodified originals. The email-like label is fictional. The backend loopback port is disposable. No provider registrations, personal accounts, callback query strings, passwords, codes, tokens, app profiles or raw logs are included.

| Original image | What was inspected |
| --- | --- |
| [Host consent](01-register-consent.png) | App/scope/backend origin and unobstructed Continue/Cancel controls |
| [Browser registration](01-register-browser-registration.png) | Fictional browser form; no password field in the contained app |
| [Forced identity failure](02-refresh-identity-failure.png) | Readable error after refresh and a synthetic HTTP 503 |
| [Recovery](02-refresh-recovered.png) | Protected identity loads after another genuine refresh |
| [Cold restart](03-cold-restored.png) | Same connection restored after native process replacement |
| [Logout denial](04-logout-denied.png) | No account/data and protected request rejected |
| [Second app denied](06-other-app-denied.png) | First app's handle cannot be used before second app signs in |
| [Second app independently signed in](08-other-app-own-login-isolation.png) | Own identity visible; first app's handle still rejected |

[Prior attempts](prior-attempts.json) preserves failure/source/binary hashes: attempts 1–2 found and confirmed a synthetic-server CSP defect; attempt 3 found a driver modal-settlement race; attempt 4 passed before the final browser-console classification run. No failed attempt is presented as a pass.

This proves neither Android backend callbacks nor embedded WebView login. Real GitHub/Google acceptance is separate. The fixture is not a production app UX design or a published App Hub listing.
