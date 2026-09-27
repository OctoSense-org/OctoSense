# Settings account broker

`OctoSenseSettingsBroker` is an on-demand, platform-signed system APK using
`android.uid.system`. Android 15 SystemUI uses `android.uid.systemui`; its sensor
privacy role does not give it AccountManager's system-UID visibility bypass.
The general OctoSense agent keeps its own UID.

Home's trusted built-in Settings module → signed Agent Binder → broker is the
only full account route. The broker exports one finite account service under the
existing signature permission and independently verifies the exact Agent package,
matching signer, same user, current user and unlocked state on every call.
Metadata reads and sync commands use the shared Accounts contract, bounded opaque
identities and 30-second observation expiry. No passwords, tokens, provider auth
bundles, arbitrary settings keys or arbitrary intents cross this interface.

Add opens Android's provider-filtered account flow. Remove opens an unexported
native confirmation through an immutable, one-shot PendingIntent. Both the
service and positive confirmation re-resolve the account and policy. Removal is
bound to the observed account incarnation, so removing and recreating an account
with the same name cannot retarget an old confirmation. Non-system overlays are
hidden on the confirmation window. Home accepts account PendingIntents only when
their creator is this broker package.

There is no launcher, boot receiver or persistent process. Installation requires
a platform-signed ROM/system image; an ordinary Home APK continues to report
limited caller-visible accounts and cannot remove accounts through this route.
System-UID code has broad OS authority; this design narrows the exported API,
not the inherent authority of native code linked into the broker.
