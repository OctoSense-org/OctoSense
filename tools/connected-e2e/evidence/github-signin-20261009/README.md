# GitHub sign-in sheet and Notes account card — 2026-10-09

English | [简体中文](README.zh-CN.md)

The host's provider sign-in sheet and GitHub Notes' account card were
redesigned together. [notes_signin.py](../../notes_signin.py) drove both on the
Linux build host under headless Weston, with debug builds of
`connected-app-host` and `connected-install` from this branch and the signed-out
synthetic GitHub (`--provider-fixture=github-sign-in`). The sign-in service,
sheet, connection store and installed app were real; only github.com was a
fixture. The bundle was the GitHub Notes 0.2.2 candidate's `main.splash` under
the sample id `org.octosense.samples.githubnotes`. [receipt.json](receipt.json)
holds the binary, source and capture digests and the five checks. The driver
passed three runs in a row.

Before, the sheet named the app only by its id; the code was plain text under a
small link, and Continue stayed visible while waiting. A decline showed the raw
provider error ([consent](00-before-consent.png), [code](00-before-code.png),
[declined](00-before-declined.png)).

After:

- [the card](01-connect-card.png) offers one Connect GitHub action and a
  plain-language access choice;
- [the sheet](02-consent.png) names the app above its id and lists the access;
- [the code step](03-code.png) has Copy and Open GitHub (which copies the code
  too), where to paste it, a moving wait line and the time left;
- after approval [the card](04-connected.png) names the account and its access,
  and the repositories load;
- [disconnecting](05-disconnected.png) asks first and keeps the note;
- a decline reads as plain words in [the sheet](06-sheet-declined.png) and
  [the card](07-app-declined.png), which offers Try again;
- [a cancel](08-cancelled.png) leaves a neutral note;
- on a 1200 px window [the sheet](09-wide-code.png) is a 560 px column. A scratch
  driver captured this frame; `notes_signin.py` has no wide mode.

These frames were inspected one by one. Live GitHub sign-in, Google sign-in in
this sheet, macOS and Android rendering, and physical approval are
**unverified**.
