# Linux desktop package acceptance

The [immutable receipt](evidence/linux-package-37723207058.json) records eight
successful checks of the real `.deb` produced by [desktop package workflow
37723207058](https://github.com/OctoSense-org/OctoSense/actions/runs/37723207058),
artifact `11528131778`. This was an Ubuntu 22.04 package build. Its source was
the workflow's planned pull-request merge commit
`c15a0ab93d6d4991e5774e1b56028247bf9c7ac2`, rather than only the pull-request
head `9f8ce11c7cc890aa77131821c42a0ae61b46fc54`.

The package was extracted with `dpkg-deb -x` into a new private test directory;
it was not installed into the host OS. The extracted executable ran against
its packaged resources in a disposable Xvfb display and D-Bus session. A
read-only bwrap overlay supplied already extracted test dependencies. The run
used a fresh OctoSense profile and no personal provider account. Neither
system packages nor global settings changed.

The eight automated checks cover the packaged executable, isolated display,
continued native execution, native widget snapshot, rendered frame, App Hub
presence, resource loading and normal shutdown. Inspection of that actual
frame also confirmed the Google Calendar, GitHub Notes and Inbox Assistant
listing rows. The agent-consent dialog remained unanswered. All owned test
processes stopped, and the namespace exited successfully.

This evidence does **not** establish App Hub installation, account sign-in,
OS-authenticated write approval, package-manager installation or a final
integrated release. The receipt binds both the package and executable hashes
and the reviewed frame hash. Raw profiles, logs and snapshots remain private.
Future package runs must create new receipts; do not update this record to
claim evidence for a different package or source revision.

For reproduction, verify the downloaded artifact's `.deb` SHA-256 against the
receipt, extract it to a new directory, and launch `usr/bin/octosense` with
fresh `OCTOSENSE_HOME`, `MAKEPAD_HOME` and XDG directories. Use an isolated
X11 display, `MAKEPAD=linux-x11`, `MAKEPAD_REMOTE=on` and
`MAKEPAD_WM_TEST_APP=apphub`. The native instrument's `snap`, `g` and `quit`
endpoints provide the same structural snapshot, frame and normal shutdown
checks. Keep the instrument on loopback, do not approve the agent-consent
dialog, and retain a new private run directory for the raw evidence. The
original environment-specific launch helpers are not shipped as a portable
driver.
