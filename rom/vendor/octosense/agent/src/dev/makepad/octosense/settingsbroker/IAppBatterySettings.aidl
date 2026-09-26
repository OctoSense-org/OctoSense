package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;

/** Current-owner package policy, never arbitrary AppOps or allowlist entries. */
interface IAppBatterySettings {
    Bundle snapshot(long requestId,String packageName);
    String set(String packageName,String observedKey,String mode);
}
