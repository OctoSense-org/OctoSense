package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;

/** Finite owner notification metadata and observed user settings; no channel parcels. */
interface IAppNotifications {
    Bundle snapshot(long requestId,String packageName,int offset,String generation);
    String set(String packageName,String observedKey,String targetKey,String action,String value);
}
