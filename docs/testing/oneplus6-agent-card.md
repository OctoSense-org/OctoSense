# OnePlus 6: system agent → Mail card

English | [简体中文](oneplus6-agent-card.zh-CN.md)

**Passed on the physical OnePlus 6:** the system agent delegated to Mail's
agent, Mail published a notice, its notification appeared over system chat,
a native finger tap opened the full card, and closing it left the same card
visible in Glance. The accepted card is `os.mail/agent-card-e2e-accepted`.

The tested runtime was the clean commit
`c3eaea2ce8bc06192e805438b590f885c870db29`, based on main `e6bfad93`.
The [build record](evidence/agent-card/build.json) binds the APK and original
1080×2280 screenshots by SHA-256. This change is independent of App Studio.
Packaging used the separate-package build helper from `88550095`, outside
the checkout with its repository root redirected here; the compiled Rust
source was `c3eaea2c`. The package was `dev.makepad.octosense.studio`.

## What ran

Both agent turns used the existing, authorized DeepSeek `deepseek-v4-flash`
configuration. A test operator drove Android touch input over ADB and
reviewed the screenshots; the model did not click or judge the screen.
Mail used its existing `mail_demo` transport and the synthetic identity
`agent-test@example.invalid`, added through the host's sign-in UI. The
recorded turns contain no mail-reading or email-sending tools.

Developer options were compiled in, but developer grants were absent.
The first attempt exercised the real “Let Mail's agent start?” consent sheet;
the operator tapped Allow. The accepted replay reused that consent and peer.
Production Home, Bridge, the default launcher and ROM were not replaced.

| Stage | Observed evidence |
| --- | --- |
| System delegates | `peer_send_input` targets `os-mail-f6e8245bc1167c11` with the accepted card ID. |
| Mail publishes | The separate Mail peer calls `mail_notify`; the host returns success, the same ID and `replaced: false`. |
| Notification displays | The Mail banner is visible above the still-open system chat. |
| Native tap opens full card | Android touch at `(540,175)` opens the keyed card sheet; title and complete body are readable. |
| Close returns to Glance | Touching the measured × at `(973,807)` closes the sheet in the same test Activity. Glance shows the same title and body. |
| Reply reaches system agent | `peer_gather` observes the completed Mail peer; both sessions have committed terminal replies. Neither claims to have inspected the screen. |

[Notification above chat](evidence/agent-card/01-notification-over-chat.png) ·
[Full card](evidence/agent-card/02-full-card.png) ·
[Glance after closing](evidence/agent-card/03-glance-after-close.png)

The [sanitized transcript](evidence/agent-card/agent-evidence.json) records
tool arguments, result previews and committed assistant replies, with no
reasoning or provider credentials. The [keyed route log](evidence/agent-card/notification-route.log)
and [touch observations](evidence/agent-card/observation.json) connect the
agent call to the displayed card. The notification screenshot includes an
older reply above the current turn; use the accepted card ID in these
records to identify this run.

## Repeating the flow

1. Use an authorized phone with a separate test package and an authorized
   provider profile. Launch with `mail_demo: true` in `makepad.APP_CONFIG`.
2. Add the synthetic Mail account through the host sheet, using the demo
   transport's public password `demo`.
3. Send the [recorded prompt](evidence/agent-card/system-prompt.txt) to system
   chat. The run submitted it through the existing `system-chat-send:` test
   action. Let the model obtain consent if needed, delegate to Mail's actual
   peer slug, and gather its reply.
4. Keep chat open. Tap the notification, inspect the full card, then tap ×
   and inspect Glance. Obtain touch targets from the actual screen/layout;
   the recorded pixel coordinates are specific to this phone's density.
5. Compare the app-owned card ID across the peer tool result and shell log.
   An agent's success reply alone does not establish that a card rendered
   or that a tap worked.

## Bugs found and validation

Before the fix, chat painted over notifications, and a visible toast ignored
Android touch because its handler accepted only mouse presses. The fix puts
notifications/card sheets above chat, handles matching touch releases,
retains cancelled drags, recovers captures interrupted by a modal, and opens
the exact card with Glance beneath it. Sheet × and backdrop accept touch.

On `c3eaea2c`, all **838 shell tests passed**, including seven new touch
regressions. Desktop checks with default features and `mobile-apps`, the
Home check with `mobile-apps`, both shell dependency-graph checks and
`tools/setup.py --check --cargo` passed. The physical Android build and
flow above also passed. These unit checks are not a headless UI E2E test;
the interaction evidence comes from the actual phone.

## Limits and cleanup

- This tests the **OctoSense in-app toast**, not Android's notification tray,
  background delivery or the internal notification shade.
- `mail.notify` supplies content to the shell's existing L0 notice template.
  It does not generate new L0 source or launch Mail's full application.
- Calendar, live IMAP/SMTP, card replacement, expiry and persistence across
  process restarts were not tested in the accepted run.
- Android Back closed the sheet but also exited the renamed test Activity
  in an earlier replay. The pinned Makepad loader looks for
  `<package>.MakepadAppExtension`, while Home's class retains its original
  package. The test package therefore misses Home's native Back override.
  The accepted flow uses ×; normal installed Home's Back behavior remains
  **unverified** here.

The temporary provider profile was removed, the original screen timeout
restored, the test package stopped and ADB verified unprivileged. Synthetic
Mail state and test evidence remain for inspection; see the
[cleanup receipt](evidence/agent-card/cleanup.json).
