package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;
interface IAppStorageSettings {
    Bundle snapshot(long requestId, String packageName);
    Bundle action(String packageName, String observedKey, String action);
}
