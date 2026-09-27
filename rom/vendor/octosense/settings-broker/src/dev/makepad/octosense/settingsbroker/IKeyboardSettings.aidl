package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;
interface IKeyboardSettings {
    Bundle snapshot(long requestId, String query, int offset);
    Bundle prepare(long requestId, String key, String target, String operation);
}
