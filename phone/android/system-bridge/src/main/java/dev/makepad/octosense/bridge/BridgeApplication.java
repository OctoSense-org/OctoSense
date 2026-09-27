package dev.makepad.octosense.bridge;

import android.app.Application;

public final class BridgeApplication extends Application {
    private BridgeState state;
    @Override public void onCreate() {
        super.onCreate();
        state = new BridgeState(this);
    }
    BridgeState state() { return state; }
}
