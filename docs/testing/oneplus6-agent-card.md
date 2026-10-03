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

After this original run, the temporary provider profile was removed, the original screen timeout
restored, the test package stopped and ADB verified unprivileged. Synthetic
Mail state and test evidence remain for inspection; see the
[cleanup receipt](evidence/agent-card/cleanup.json).

## Presentation polish: composite-build replay

**The later portrait replay passed** with card
`os.mail/mail-card-polish-1791014993`. This tests the presentation changes in
`9e65b3bc`: the rounded phone banner, bottom card sheet, larger close target
and revised notice spacing. The installed APK came from clean composite
[`ccf8013f`](https://github.com/OctoSense-org/OctoSense/commit/ccf8013f2bd7adbb6c20d5f52f47bcfbcbb55313), which also contains App Studio.
It is **not an exact-`9e65b3bc` device build**. The
[installed APK fingerprint](evidence/agent-card-polish/installed-build.json)
matches the [composite build receipt](evidence/agent-card-polish/build.json).
Original `c3eaea2c` evidence above remains separate.

The [new transcript](evidence/agent-card-polish/agent-evidence.json) records
system `peer_send_input` to Mail, the separate peer's successful `mail_notify`
with this card ID and `replaced: false`, then `peer_gather` and both terminal
replies. The [route log](evidence/agent-card-polish/notification-route.log)
and [touch record](evidence/agent-card-polish/observation.json) confirm that
touch `(540,270)` opened that card, and × at `(962,1510)` closed it in the
same Activity. Coordinates use the recorded 450-dpi display.

[Banner above chat](evidence/agent-card-polish/01-notification-over-chat.png) ·
[Full card](evidence/agent-card-polish/02-full-card.png) ·
[Glance after close](evidence/agent-card-polish/03-glance-after-close.png)

Independent screenshot review found readable text, complete full-card body,
visible close control and intact margins, with the same card in Glance after
closing. The banner intentionally elides its body. Glance's white status-bar
icons still have low contrast against its light background; this shell issue
remains. Older Calendar-model output behind the banner belongs to a separate
test. See the [visual review](evidence/agent-card-polish/visual-review.json)
and [artifact hashes](evidence/agent-card-polish/sha256.json).

The first polish replay stopped because the operator harness searched an old
log snapshot for a not-yet-recorded touch rectangle. Refreshing logs before
target selection fixed the harness; runtime code did not change. The
[failed-attempt summary](evidence/agent-card-polish/prior-attempt.json)
preserves this distinction. The successful transcript also contains an
`orphaned_by_restart` error for an older Mail turn; the accepted publication
and terminal reply belong to a different turn.

The replay reused existing manual consent and the synthetic demo account.
Developer grants were absent during the flow, then restored to their exact
previous bytes or absence; the test package was stopped. The wrapper left
provider and consent records untouched, as its
[cleanup receipt](evidence/agent-card-polish/cleanup.json) records.
Production Home was untouched. This replay adds no claim about Android Back,
dark mode, landscape, system-tray notifications or live mail.

After all template and notification tests, the operator separately removed
both isolated packages' temporary provider configurations and developer
grants, restoring their original absence. Both packages were stopped, the
60-second screen timeout restored, and ADB verified unprivileged. This later
[final cleanup](evidence/agent-card-polish/final-cleanup.json) supersedes the
wrapper's temporary grant restoration; it did not change production Home,
the default launcher or ROM.

Before the composite replay, `9e65b3bc` passed **840 shell tests**, desktop
default/mobile-apps and Home mobile-apps checks, both dependency-graph checks,
setup validation and an Android build. Those checks and this composite
device replay have different source scopes; neither substitutes for an
exact-polish-build device test.
