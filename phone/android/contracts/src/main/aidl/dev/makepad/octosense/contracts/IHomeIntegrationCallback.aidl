package dev.makepad.octosense.contracts;
import android.os.Bundle;

oneway interface IHomeIntegrationCallback {
    void onTransition(long transitionId, long layoutRevision, in Bundle event);
    void onExtensionState(in Bundle state);
}
