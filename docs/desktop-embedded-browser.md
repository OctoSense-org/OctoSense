# Desktop embedded browser

English | [简体中文](desktop-embedded-browser.zh-CN.md)

Since desktop 0.1.0-rc.1, `WebReader` works on Linux and Windows desktops. `WebReader` is the Splash widget that shows a web page inside a script app. On these platforms it embeds the system's own browser engine as a child view of the Makepad window:

- **Linux under X11 or XWayland** (XWayland is the X server that Wayland desktops run for X11 programs): WebKitGTK.
- **Windows:** Microsoft Edge WebView2.

It never opens an external browser and does not use CEF (the Chromium Embedded Framework). Earlier releases, such as `desktop-v0.1.0-beta.2`, cannot show embedded pages on Linux or Windows. The feature is part of the host's runtime, so installing a newer app does not add it to an older host.

## Runtime requirements

| Platform | What it needs, and what happens without it |
| --- | --- |
| Linux, X11 or XWayland | GTK 3 and WebKitGTK 4.1, or 4.0 when 4.1 is missing. The host loads them only when a page opens, so building OctoSense needs no WebKit development headers. WebKit's own sandbox stays on. Without the libraries, the reader reports an error that names what to install. |
| Linux, native Wayland | Not supported: the reader reports an error that suggests relaunching with `--linux-backend=x11`. When `DISPLAY` is set, OctoSense's desktop entry point already prefers X11 or XWayland, unless `--linux-backend` chooses otherwise or the build uses Vulkan, which needs native Wayland. OctoSense never starts an X server. |
| Windows | An installed Microsoft Edge WebView2 Runtime that provides `ICoreWebView2_27`. Without it, or with an older runtime, the reader reports an error; the host never downloads an engine. The adapter uses `webview2-com` 0.39.1. |
| macOS, Android | Unchanged: they keep their existing native readers. |

The `.deb` package lists GTK 3 and WebKitGTK 4.1 or 4.0 as dependencies, because `dpkg-shlibdeps` cannot see libraries that are loaded at run time. With the AppImage, install these libraries yourself. On Windows, install the WebView2 Runtime. OctoSense bundles neither engine. The Windows adapter requires the WebView2 interfaces that let it block the native Save As dialog and screen capture; on an older runtime it fails rather than show a page without those protections.

## What a page can do

Apps use `WebReader` as before, with the same capability checks. After a page opens, the adapter checks every navigation again, redirects and subframes included:

- A reader without the app's `web` grant stays on the document it opened. It can only move to another `#fragment` of that document.
- A reader with the `web` grant can also go to public HTTPS pages.
- Both refuse local files, other apps' URL schemes, URLs that carry a user name or password, and malformed addresses.

The first page may be plain HTTP when the app's capability checks admitted it. The acceptance test relies on this to serve its page from loopback; the exception grants no other local URL.

Each reader that opens gets a new private browser session. Closing the reader stops the page's scripts and destroys the native view; opening it again starts a new session, with no cookies from the last one. Hiding or clipping the reader only takes the native view off the screen and keeps the page. The native view keeps the page's full size inside a clipping parent window, so scrolling the app does not re-lay out the page to its visible part.

A page gets no bridge into OctoSense: it cannot call the app's tools or host services. Both adapters deny the page's permission requests, downloads and popups, so a page cannot use them to reach the app's Splash camera, microphone or file APIs. Linux also cancels file pickers and printing. Windows does not intercept the HTML file picker or printing; how those native dialogs behave there is unverified, so do not describe them as disabled. A file that a person picks for a website has nothing to do with the app's host-service grants.

The URL checks are not a network sandbox: a public host name can resolve to a private address. Do not rely on a reader to keep a page away from private networks (SSRF).

## macOS frozen captures and resizing

On macOS, a phone-style shell can freeze an app's texture and detach its drawing
pass from the window. The reader's watchdog must still hide its native WebView.
The adapter now detaches hidden readers, and readers without an attached window,
instead of ignoring their updates. Showing the app again attaches the same page
at its current layout size, preserving its form state and private session.
The runtime overlay is `tools/runtime-patches/makepad-macos-webview-placement.patch`.
Authentication inspection retains its existing host-only, explicit-fixture gate.

The macOS regression uses a hidden native window, a local fictional page and an
opt-in texture surface. It checks wide, narrow and short windows, freezing the
surface before resizing, and restoring it without a reload or lost draft.
`nativeAttached` in the existing native inspection receipt distinguishes a page
that still exists from an overlay that is still mounted.

```sh
cargo build --locked -p octosense-browser-smoke
python3 tools/test-browser-resize.py --out /tmp/octosense-reader-resize-check
```

Use a new output directory for each run. This regression passed on macOS;
Windows, Linux, iOS and Android device behavior for this change is **unverified**.

## Sign-in stays in the external browser

Backend sign-in is a separate, host-owned flow. **On Linux and Windows it still uses the external browser.** Reader support does not enable embedded sign-in (OAuth), and it does not give apps the login cookies. WebKitGTK cannot reliably confirm that a navigation is in the main frame before it requests the sign-in callback, so the Linux adapter refuses embedded sign-in outright. The Windows adapter can intercept the callback, but embedded sign-in stays off in production until its complete host-owned flow passes acceptance. Google and GitHub keep their existing provider flows. See [Connected accounts](../crates/oauth-service/README.md).

## How it works

- `desktop/src/main.rs` calls `Cx::prefer_x11_for_embedded_browser()` on Linux before Makepad picks a windowing backend. An explicit `--linux-backend` option still wins.
- `tools/runtime-patches/makepad-desktop-webview.patch` holds the runtime change. `runtime-patches.lock.json` pins the patch's SHA-256 and the resulting Makepad tree, and `tools/setup.py` applies it on top of the other runtime patches.
- Makepad's `system_browser::BrowserPolicy` parses each URL with the `url` crate and decides whether to allow it. `linux_webkit.rs` embeds a `GtkPlug` in an X11 child window (the socket) through XEmbed, and runs a bounded amount of GTK work on the existing UI thread. `windows_webview.rs` places a WebView2 controller in a clipping child window (`HWND`) and receives its asynchronous callbacks on the UI thread's single-threaded apartment (STA).
- `WebReader` keeps its overlay lifecycle and turns native loading, navigation and failure events into widget state. A blocked navigation is not fatal: a denied iframe does not hide the allowed page around it. Title changes reach the widget. Linux also reports URL changes, and it releases the native view after the callback for a page's `window.close()` returns. A missing engine takes the error path, so the widget never shows an invisible view that claims to have loaded.

## Acceptance tests

`crates/browser-smoke` is a native test host that mounts the real `WebReader` widget; it is separate from the shell and never published. `tools/browser-smoke.py` serves synthetic HTML on loopback and drives that host through a private control directory. It checks that:

- the page's JavaScript runs, and automated DOM edits take effect;
- title changes reach the widget;
- a blocked iframe leaves the parent page usable;
- a forbidden navigation stops before its HTTP request;
- the host accepts hide and show commands;
- closing stops the page's scripts, and reopening starts with fresh cookies;
- native network errors reach the widget.

The test uses no personal accounts and downloads no engine.

On Windows, the workflow also requires a PNG snapshot taken by the engine itself, and the driver reads back the native settings to prove that web messaging and host objects are off. To show that the message counter works, the test host opens a separate, disposable controller that enables messaging for one fixed synthetic page, confirms that the counter receives that page's message, then closes the controller and deletes its profile. The production controller never enables messaging; its counter only counts native deliveries, never reads a payload and never acts on one. The driver requires zero deliveries after a bounded wait and again just before the same view closes. This proves that nothing was delivered while the test watched.

The receipt keeps the JavaScript result of the page's message call, or its exception, only as a diagnostic. [Microsoft's documentation](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.iswebmessageenabled) says that the call throws when messaging is off, but the tested runtime returned normally with both native settings off. Neither the presence of `chrome.webview` nor a normal return shows that a message reached the host; only the native count does.

The GitHub Windows runner has no GPU. There, `--software-graphics` sets `MAKEPAD_D3D11_WARP=1`, which makes Makepad draw with Windows' built-in [WARP software rasterizer](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp), and the receipt records the mode. WebView2 is still the real engine, with its normal sandbox. The run says nothing about hardware GPU performance, and production builds choose their graphics adapter as before.

`cargo test --locked -p octosense-browser-smoke` runs the shared URL-policy tests: restricted documents, public HTTPS, malformed and private URLs, and the exact main-frame sign-in callback. On their own they do not prove that the native engines intercept navigation.

The `windows` job in `.github/workflows/embedded-browser.yml` fails when the runner's installed engine is missing or too old; it never turns that into a skipped pass. It uploads only synthetic receipts and the engine's own captures; full receipts stay with the pull request and its CI run, not in the repository. When a change touches the files that this workflow watches, `tools/ci-local-merge.sh` also requires the job to pass on the pull request's exact head commit. A local pass on macOS or Linux cannot replace it, and neither can a Windows cross-compile.

### Verified

- **Linux:** eleven native checks passed with WebKitGTK 2.52.6 under Xvfb 21.1.22, with WebKit's sandbox on. An `XQueryTree` check confirmed that the `GtkPlug` sits in the socket inside the Makepad window, with valid geometry; it did not capture the screen. The same run confirmed that a page calling `window.close()` removes the native view and stops the page's heartbeat requests.
- **Windows:** twelve native checks passed in the `windows` job, on a GitHub `windows-2022` runner with WARP.
- **Release:** the desktop 0.1.0-rc.1 release source also passed the Windows embedded-browser check ([release notes](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.1)).

### Unverified

- Physical input, accessibility, visual quality, the full shell experience and sign-in. The drivers use automated DOM input.
- Whether the native view is actually visible: the hide and show check only confirms that the host accepted the commands.
- HiDPI displays, and closing a view while Windows is still creating its controller.
- The HTML file picker and printing on Windows.
