package dev.makepad.octosense.contracts;
import android.os.Bundle;

oneway interface ISystemBridgeCallback {
    void onSnapshot(String epoch, long revision, in Bundle state);
    void onDelta(String epoch, long revision, in Bundle patch);
    void onCommandResult(String session, long commandId, int status, String reason);
    void onResyncRequired(String epoch);
}
