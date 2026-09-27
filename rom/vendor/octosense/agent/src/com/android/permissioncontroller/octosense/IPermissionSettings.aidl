package com.android.permissioncontroller.octosense;
import android.os.Bundle;
/** Current-owner common runtime choices; only native operation tokens, never a raw grant. */
interface IPermissionSettings {
    Bundle snapshot(long requestId, String packageName, String group, int offset, String generation);
    Bundle operation(String packageName, String group, String key, String target);
}
