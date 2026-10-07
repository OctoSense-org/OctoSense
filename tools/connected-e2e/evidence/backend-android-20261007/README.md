# Android backend authentication result

English | [简体中文](README.zh-CN.md)

[Receipt](receipt.json) · [Reproduction](../../ANDROID-BACKEND.md)

Actual OnePlus 6 / Android 15 testing completed the developer backend’s real HTML signup/sign-in, PKCE callback, token exchange, protected identity, native-vault cold restore and logout. No account or token was injected. The final APK also completed native Cancel and a real login callback from an expanded Glance card.

The Glance attempt exposed and fixed a lifecycle defect: Android handoff was retained by event processing but retired during drawing. The final candidate preserves both paths. On successful login, the newly active connection invalidates the old account-bound Glance card; the captured result is the ordinary Glance page, not a success message in that retired card.

[Cancel](glance-cancel.png), [return after login](glance-after-login.png), and [logged out](logged-out.png) are original ADB images from the final APK. The test-only ordinary fixture has clipped heading/status layout on this phone; these captures do not constitute a complete UX pass.

The protected native auth window uses `FLAG_SECURE`, so its visual evidence is limited to native UI hierarchy and actual form interaction. Earlier ordinary-app returns produced two black ADB captures while paired Makepad GPU captures were rendered. That distinguishes app rendering from OS capture, but does not prove what was visible on the physical screen. The final Glance return produced a normal ADB image. The earlier capture limitation remains unresolved.

The receipt separates final-APK checks, earlier same-lab checks, failures and unrun cases. Android negative navigation/network cases, real Google/GitHub and a production HTTPS backend were not tested here. Mac results and protocol tests remain separate evidence. Only the owned package/server were stopped and its reverse mapping removed; two historical fictional connections remain in the isolated test package, with no active account.
