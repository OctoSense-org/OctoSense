# Night Light capability fixture

Disposable **static** resource overlay for a dedicated AOSP Android emulator.
It changes only `android:bool/config_nightDisplayAvailable` so the real
ColorDisplay service can be exercised where the SDK image ships the feature
disabled. It contains no code or Settings implementation and is not included
in Home or the ROM. The dynamic data-installed variant was disabled again at
boot on the validation image; an enabled resource alone did not establish a
working service.

Build with `aapt2 compile/link`, align, and sign with the public AOSP platform test
certificate matching the owned emulator. Do not use a production signing key or
install it on a phone. Check `ro.kernel.qemu=1` and the expected AVD name before
staging. Save the exact presence/values of the secure `night_display_*` rows.
Confirm both the fixture package and its destination are absent before adding:

```
/product/overlay/OctoSenseNightLightValidation/OctoSenseNightLightValidation.apk
```

Use only the dedicated AVD's writable overlay, with directory mode 755, APK mode
644 and `restorecon`. Restart that emulator, then verify
`dumpsys color_display` exposes Night display before testing through OctoSense.
The tested API35 AOSP image reports Off and 2850 K after initialization.

Test the original unsupported state separately. Service policy and setting
readback acceptance are distinct from physical-panel color accuracy and
device-specific LiveDisplay support.

Cleanup must remove only this fixture APK/directory, restore the saved rows
(delete rows originally absent), restart the emulator, and verify the fixture
package is gone, Night display is unavailable again, and preferences match the
baseline. Never leave the capability forced on an unsupported product image.
