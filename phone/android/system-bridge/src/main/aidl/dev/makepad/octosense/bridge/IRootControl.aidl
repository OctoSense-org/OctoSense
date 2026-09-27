package dev.makepad.octosense.bridge;
import android.os.Bundle;

// Only the ordinary bridge package can call this interface. No shell strings.
interface IRootControl {
    Bundle getIdentity();
    Bundle setWifiEnabled(boolean enabled);
    Bundle setBluetoothEnabled(boolean enabled);
    Bundle setBatterySaver(boolean enabled);
}
