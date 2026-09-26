# Disposable default-browser candidates

These public-SDK fixtures register HTTP/HTTPS browser activities, but render only
local text and never load a URL. They request no role-management authority and
contain no role setter. Their instrumentation reports only their own browser
role availability/holder state through public RoleManager APIs.

The assistant manifest reuses the same local-only Activity and public probe,
registering exported `ACTION_ASSIST` with `CATEGORY_DEFAULT`. It requests no
permissions and qualifies through the native Assistant role's Activity path.
This exercises a role that is not publicly requestable and its real None flow.
Capture/restore the three legacy assistant/voice-service Secure rows as well as
all role holders; do not assume clearing the role restores absent raw rows.

The secondary manifest reuses both Java classes with an independent package and
signing identity. Two candidates allow a Settings test to distinguish a reviewed
selection from a different or retired target. The fixtures are only for the
disposable role-validation emulator; refuse existing packages and remove them
afterward. Restore the original role selection before cleanup, and compare the
final role-holder state with the saved baseline. These files do not by themselves
establish native qualification or successful selection; actual adapter/UI
acceptance must verify both.
