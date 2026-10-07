# Desktop embedded browser

English | [简体中文](desktop-embedded-browser.zh-CN.md)

The source build includes native `WebReader` adapters for Linux X11/XWayland
(WebKitGTK) and Windows (Microsoft Edge WebView2). These are child views inside
the Makepad window. They do not launch an external browser or use CEF. A release
binary must contain the new runtime overlay before it can offer this feature;
installing an app bundle alone cannot upgrade an older host.

## Runtime requirements

| Platform | Requirement and behavior |
| --- | --- |
| Linux X11 / XWayland | GTK 3 and WebKitGTK 4.1, with 4.0 as a compatible fallback. Libraries are loaded at runtime, so ordinary host compilation does not require WebKit development headers. The engine sandbox stays enabled. |
| Native Wayland | Embedded child views are not implemented. The view reports an explicit error. OctoSense's desktop entry point prefers an existing X11/XWayland display when available, unless an explicit backend option or Vulkan build requires another backend. It does not start an X server. |
| Windows | An installed Microsoft Edge WebView2 Runtime exposing `ICoreWebView2_27`. Missing engines or older runtimes fail explicitly; the host does not download an engine. The adapter uses `webview2-com` 0.39.1. |
| macOS / Android | Existing native reader adapters are unchanged by this desktop work. |

The `.deb` release configuration declares GTK 3 and WebKitGTK 4.1-or-4.0
runtime dependencies because dynamic loading is invisible to `dpkg-shlibdeps`.
AppImage users need these libraries installed separately. Windows deployment
needs the supported WebView2 Runtime. This source change does not bundle either
engine. The Windows adapter deliberately requires the interfaces used to deny
native Save As and screen capture instead of silently omitting those gates.

## App and host boundary

Apps continue to use the existing `WebReader` widget and its capability checks.
A restricted reader can stay on its admitted document, including fragment changes.
A reader with the existing `web` grant can navigate to public HTTPS pages. Every
navigation is checked again, including redirects and subframes. Local files,
external-app schemes, credentialed URLs and malformed addresses are refused.
An explicitly host-admitted initial HTTP document remains usable; this permits
the isolated loopback acceptance fixture without granting arbitrary local URLs.

Each opened view gets a fresh private browser session. Closing destroys its
native controller and page execution; reopening starts another session. Hiding
or clipping only detaches the visible overlay and preserves the current page.
The native child keeps its full document dimensions inside a clipping parent,
so scrolling the host does not reflow the page to the clipped rectangle.

These views provide no OctoSense tool bridge. Browser permission requests,
downloads and popups are denied. Pages cannot use these browser requests to grant
Splash camera, microphone or filesystem APIs. Linux also cancels file chooser and
print signals. Windows does not intercept HTML file-selection or print UI; their
native interaction remains unverified and must not be described as disabled.
A file selected for a website is separate from an app's host-service grants.
URL validation is not a DNS or network sandbox: a permitted public hostname can
resolve to a private address. Do not treat a reader as an SSRF isolation boundary.

Backend sign-in is a separate host-owned flow. **Linux and Windows still use the
existing external-browser authentication route**; ordinary reader support does
not enable embedded OAuth or expose login cookies to apps. Linux WebKitGTK cannot
reliably prove a navigation is the main frame before requesting an authentication
callback, so the adapter refuses embedded authentication. The Windows adapter
contains callback interception, but production embedded sign-in remains disabled
until its complete host-owned authentication flow is accepted. Google and GitHub
retain their existing provider flows. See the [connected-account guide](../crates/oauth-service/README.md).

## Source walkthrough

- `desktop/src/main.rs` opts the Linux desktop into X11/XWayland selection before
  Makepad chooses a windowing backend; an explicit backend selection wins.
- `tools/runtime-patches/makepad-desktop-webview.patch` contains the reviewed
  runtime extension. `runtime-patches.lock.json` pins both its digest and the
  resulting Makepad tree. `tools/setup.py` applies it over the existing overlays.
- Makepad's `system_browser::BrowserPolicy` parses and checks URLs with `url`.
  `linux_webkit.rs` embeds a GTK plug in an X11 child socket and pumps bounded GTK
  work on the existing UI thread. `windows_webview.rs` embeds a WebView2 controller
  in a clipping child HWND and receives asynchronous callbacks on the UI STA.
- `WebReader` retains its existing overlay lifecycle and converts native loading,
  navigation and failure events into widget state. Policy-blocked navigation is
  nonfatal: a denied iframe cannot hide the allowed parent document. Dynamic
  titles are forwarded; Linux also forwards URI changes and releases its native
  child after a page-requested close callback returns. A missing engine reaches the
  failure path instead of leaving an invisible successful view.

## Acceptance

`crates/browser-smoke` is an unpublished native test host, separate from the
production shell, that mounts the actual `WebReader` widget.
`tools/browser-smoke.py` serves synthetic HTML on loopback and drives that host
through a private control directory. It checks real page JavaScript, DOM editing,
dynamic titles, a blocked iframe preserving the interactive parent, navigation
rejection before a forbidden HTTP request, accepted hide/show commands, stopped
execution after close, fresh cookies after reopen, and native network errors.
It never uses personal accounts or downloads engines.
The Windows workflow also requires an engine-owned PNG snapshot, native settings
readback proving messaging and host objects are disabled, and a page-side attempt
to send a message that must throw an error. WebView2 keeps its `chrome.webview`
namespace even when [messaging is disabled](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2settings.iswebmessageenabled);
namespace presence alone does not grant communication.
On its GPU-less runner, `--software-graphics` explicitly selects the built-in
[Windows WARP rasterizer](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp)
for Makepad using `MAKEPAD_D3D11_WARP=1`; the receipt records this mode. WebView2
remains the real native browser with its normal sandbox. This test does not prove
hardware GPU performance, and production graphics selection stays unchanged.

The shared URL-policy tests run with `cargo test --locked -p octosense-browser-smoke`
and cover restricted documents, public HTTPS, malformed/private URLs and exact
main-frame callbacks. They do not prove native navigation interception by themselves.
The native driver uses automated DOM input; it does not certify physical typing,
accessibility, visual quality, full shell UX, or OAuth.

Linux native acceptance passed eleven checks using WebKitGTK 2.52.6 and Xvfb
21.1.22 with the engine sandbox enabled. The XQueryTree check proved that the GTK
plug is a child of the socket inside the Makepad window, with valid geometry; it
did not capture the display. The same driver also proved that page-requested
`window.close()` removes the native child and stops page heartbeats. Hide/show
command acceptance does not prove native overlay visibility; physical input,
HiDPI and closing during asynchronous Windows controller creation remain
unverified. The Windows CI receipt records its native acceptance result; a
cross-compile alone cannot satisfy that gate. Detailed receipts belong to the PR
and CI artifacts. The `.github/workflows/embedded-browser.yml` Windows job fails
when its installed engine is absent or incompatible; it never converts that into
a skipped success. Only synthetic receipts and engine-owned captures are uploaded.

The local CI merge helper also requires the native Windows workflow to pass on
the exact PR head whenever these browser files change. A local macOS/Linux pass
cannot replace that evidence.
