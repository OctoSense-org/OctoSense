# Backend login acceptance

English | [简体中文](README.zh-CN.md)

This internal fixture tests the normal signed installation, host consent, browser callback, credential vault, protected identity, logout, and per-app connection boundary. It is not an App Hub submission. Its generated listing artwork is explicitly a placeholder; native and browser screenshots from each run are separate evidence.

`main.splash` contains no password field. The synthetic backend serves its own registration and sign-in form in an isolated Chrome context. The host uses authorization code + PKCE and its ordinary platform vault. Tokens and accounts are never injected into the host. The server is HTTP loopback only; that exception and the endpoint registration API are absent from ordinary builds.

Build the development examples:

```sh
cargo build --locked --release -p octosense-shell \
  --features mobile-apps,acceptance-fixtures \
  --example connected-app-host --example connected-install
```

Run with an existing Chrome executable and a Python environment containing Playwright. Set `CHROME`, `HUB`, and `RUN_DIRECTORY` to local paths; `RUN_DIRECTORY` must not exist. This recipe was run on macOS with the existing local Chrome and Playwright environment.

```sh
python tools/connected-e2e/backend_login.py \
  --binary target/release/examples/connected-app-host \
  --installer target/release/examples/connected-install \
  --hub "$HUB" --chrome "$CHROME" --out "$RUN_DIRECTORY"
```

The driver uses Makepad instrument for native input and original PNG captures. It opens the exact authorization URL from the host-owned LinkLabel in its isolated browser instead of clicking the OS-default-browser link. The example’s explicit `--capture-browser-url` flag writes that URL to a new private file with mode 0600; it is available only with `acceptance-fixtures`. Authorization and callback handling remain unchanged. Input events are never retried. Read-only frame-capture retries are recorded separately.

The run checks new browser registration, sign-in, protected data, host-process restart, logout, repeat sign-in, and two separately admitted apps attempting to use each other’s handles. Successful cleanup disconnects both fictional accounts through the host, stops owned native processes, and stops the synthetic server. On failure the driver also attempts host-mediated cleanup and records whether local connection metadata was removed. Platform-vault deletion is requested, not independently verified. If cleanup fails, resume the same isolated host to disconnect the fictional account before deleting its profile.

Keep the run directory private. It contains temporary authorization URLs, host metadata and diagnostic logs. Publish only reviewed original synthetic screenshots and a sanitized receipt; never copy profiles, callback URLs, registrations or raw logs. Functional assertions do not establish visual acceptance. Synthetic acceptance does not prove live GitHub or Google authorization, Android callbacks, or backend WebView support.


The 2026-10-07 native run passed seven checks on binary SHA-256 `7798c1fd23ae092a323177d1af7d3118d3f117941711140616617f3969fd37c7`:

1. New fictional account registration, browser sign-in, real PKCE callback, and protected identity.
2. A 45-second token forces refresh; one protected-data HTTP 503 is followed by successful refresh and identity retry using the persisted rotated credential.
3. Closing and reopening the native process restores the same app connection and reloads protected data through the normal platform vault.
4. Logout removes the local handle, and a subsequent protected-data request is denied.
5. The existing fictional user signs in again in the browser and receives a new connection.
6. A second signed app cannot use the first app's handle, before or after its own browser sign-in.
7. Both local handles are revoked through the host. Vault deletion was requested, not independently verified.

See the [sanitized receipt and original synthetic screenshots](../evidence/backend-login-20261007/README.md). Two earlier attempts exposed the fixture's overly restrictive browser form policy; a third exposed a driver click before host-modal closure. Those failed attempts remain recorded separately. This driver does not automate real GitHub or Google sign-in.
