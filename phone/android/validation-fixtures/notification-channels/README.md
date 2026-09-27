# Synthetic notification publisher

This disposable, independently signed APK uses public Android APIs to create
24 channels (23 in two groups and one ungrouped) and post one synthetic notification. Separate
instrumentation reports only its own notification state. It is separate from
the Settings UI driver because revoking a target app's notification permission
may terminate that target's process.

The emulator scenario controls these channels through OctoSense
Settings and checks real delivery, app permission, group blocking, importance,
and preservation of unrelated channel fields. The ungrouped channel verifies
that Android's synthetic null-ID group container is not offered as an editable
group. `setup`, `post`, `cancel`, `state`,
`delete` and `recreate` are finite fixture operations. No service or activity is
exported, and no production app or personal notification is read or modified.
The runner must refuse an existing installation and remove this APK afterward.

The [legacy manifest](../notification-legacy/AndroidManifest.xml) reuses this
Java source with a separate package and target API 25. It creates no custom
channel; Android supplies its sole default channel. The probe checks that its
app permission and channel switch follow the native coupled behavior, including
restoration of the observed `IMPORTANCE_UNSPECIFIED` value.
