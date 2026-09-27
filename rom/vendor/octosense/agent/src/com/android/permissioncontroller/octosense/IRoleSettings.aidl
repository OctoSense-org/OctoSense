package com.android.permissioncontroller.octosense;
import android.os.Bundle;

/** Only the signed OctoSense Agent may observe or request native confirmation. */
interface IRoleSettings {
    Bundle snapshot(long requestId, String role, int offset, String generation);
    Bundle confirmation(String role, String key, String target);
}
