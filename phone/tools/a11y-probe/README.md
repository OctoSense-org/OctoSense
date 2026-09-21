# Accessibility probe

A minimal instrumentation that drives the launcher the way a screen reader
does: it finds a shell node by its spoken label, gives it accessibility
focus and performs its click action through `UiAutomation`. `adb shell
input` taps bypass TalkBack's touch exploration, so this is the scriptable
check that the shell's virtual nodes (`ShellAccessibility.java`) work.

Build by hand with the pinned JDK 17 and build-tools 35 (see `android/README.md`):

```sh
aapt2 link -o out/base.apk --manifest AndroidManifest.xml -I "$ANDROID_JAR"
javac --release 17 -cp "$ANDROID_JAR" -d out/classes src/dev/makepad/octosense/a11yprobe/Probe.java
d8 --release --lib "$ANDROID_JAR" --output out out/classes/dev/makepad/octosense/a11yprobe/*.class
cp out/base.apk out/probe.unsigned.apk && (cd out && zip -q probe.unsigned.apk classes.dex)
zipalign -f 4 out/probe.unsigned.apk out/probe.aligned.apk
apksigner sign --ks "$DEBUG_KEYSTORE" --ks-pass pass:android --out out/probe.apk out/probe.aligned.apk
adb install -r out/probe.apk
adb shell am instrument -w -e label Notifications dev.makepad.octosense.a11yprobe/.Probe
```

The result bundle reports `found`, `is_focused` and `click_action`; pass
`-e click false` to focus only.
