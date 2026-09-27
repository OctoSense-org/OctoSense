# Disposable runtime-permission observations

These independent public-SDK apps let the Settings UI probe observe the actual
result of native permission choices. The modern target-35 app requests common
runtime groups; the target-22 companion exercises the platform's legacy revoke
warning. One normal permission and an intentionally undefined requested permission
check that non-runtime inventory does not break the common-group editor.
The legacy manifest reuses the modern Java sources. Both use fresh
independent signing identities when built by the probe runner.

`PermissionFixture` reports only its own `checkSelfPermission`, PackageManager
grant bits and raw AppOp modes. It never grants/revokes, requests permission,
opens a sensor, reads contacts/calendar/call logs/messages or performs network I/O. Raw AppOp modes
preserve foreground-only state rather than collapsing it to the observer's
current process importance. Permission flags are separately observed by the UI
probe using the emulator shell; public requested-permission grant bits do not
establish Ask-every-time or fixed-policy semantics.

Mutation acceptance is restricted to disposable `OctoSense_ROM_Roles_API35`
(`emulator-5560`). Refuse existing fixture packages, record baseline roles and
controller identity, install without automatic runtime grants, and remove both
apps and temporary reinstall files in cleanup. Only the legacy fixture needs
Android's explicit `--bypass-low-target-sdk-block` install option. That option
belongs to this disposable validation setup, never to product installation.
Call-log and SMS requests use the installer's actual restriction policy; the
runner does not change restriction flags, exemptions or default-app roles to
make them grantable. The UI acceptance records those synthetic permission flags.

The accessibility fixture runs in a separate package: revoking or force-stopping
a test app must not terminate the UI driver. A successful callback alone is not
acceptance; inspect grants, relevant flags and AppOps after returning to Settings.
The fixture does not claim to validate physical camera/microphone/location use.
