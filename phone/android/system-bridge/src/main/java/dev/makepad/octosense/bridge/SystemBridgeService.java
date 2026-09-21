package dev.makepad.octosense.bridge;

import android.app.Service;
import android.content.Intent;
import android.os.Bundle;
import android.os.IBinder;
import dev.makepad.octosense.contracts.CallerGuard;
import dev.makepad.octosense.contracts.ISystemBridge;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;
import dev.makepad.octosense.contracts.Protocol;

public final class SystemBridgeService extends Service {
    private BridgeState state;
    private int caller() { return CallerGuard.require(this, Protocol.HOME_PACKAGE, Protocol.QUICKSTEP_PACKAGE); }
    @Override public void onCreate() { state = ((BridgeApplication)getApplication()).state(); }
    @Override public IBinder onBind(Intent intent) { state.bound(); return binder; }
    @Override public boolean onUnbind(Intent intent) { state.unbound(); return false; }
    private void command(String session, long id, String operation, BridgeState.Operation action) {
        int uid = caller();
        Protocol.requireSession(session);
        Protocol.requireCommand(id);
        state.command(uid, session, id, operation, action);
    }
    private final ISystemBridge.Stub binder = new ISystemBridge.Stub() {
        @Override public Bundle getProtocolInfo() { caller(); return Protocol.info("octosense-bridge/1"); }
        @Override public Bundle getCapabilities() { caller(); return state.capabilities(); }
        @Override public void subscribe(String s, ISystemBridgeCallback c) {
            int uid = caller(); Protocol.requireSession(s);
            if (c == null) throw new IllegalArgumentException("Missing callback");
            state.subscribe(uid, s, c);
        }
        @Override public void unsubscribe(String s) { int uid = caller(); Protocol.requireSession(s); state.unsubscribe(uid, s); }
        @Override public void requestSnapshot(String s) { int uid = caller(); Protocol.requireSession(s); state.requestSnapshot(uid, s); }
        @Override public void setWifiEnabled(String s, long id, boolean v) { command(s,id,"wifi", () -> state.rootWifi(v)); }
        @Override public void setBluetoothEnabled(String s, long id, boolean v) { command(s,id,"bluetooth", () -> state.rootBluetooth(v)); }
        @Override public void setBrightness(String s, long id, float v, boolean automatic) {
            command(s,id,"brightness", () -> { Protocol.requireUnitValue(v); state.brightness(v,automatic); });
        }
        @Override public void setVolume(String s, long id, float v) {
            command(s,id,"volume", () -> { Protocol.requireUnitValue(v); state.volume(v); });
        }
        @Override public void setTorchEnabled(String s, long id, boolean v) { command(s,id,"torch", () -> state.torch(v)); }
        @Override public void setRotationLocked(String s, long id, boolean v) { command(s,id,"rotation", () -> state.rotation(v)); }
        @Override public void setInterruptionFilter(String s, long id, int v) {
            command(s,id,"dnd", () -> {
                if (v < 1 || v > 4) throw new IllegalArgumentException("Unknown interruption filter");
                state.interruption(v);
            });
        }
        @Override public void setBatterySaver(String s, long id, boolean v) { command(s,id,"battery_saver", () -> state.rootBatterySaver(v)); }
        @Override public void dismissNotification(String s, long id, String handle) {
            command(s,id,"notification.dismiss", () -> { requireHandle(handle); state.dismiss(handle); });
        }
        @Override public void dismissAllNotifications(String s, long id) { command(s,id,"notification.dismiss_all", () -> state.dismissAll()); }
        @Override public void invokeNotificationAction(String s, long id, String handle, String reply) {
            command(s,id,"notification.action", () -> {
                requireHandle(handle);
                if (reply != null && reply.length() > 2000) throw new IllegalArgumentException("Reply too long");
                state.notificationAction(handle,reply);
            });
        }
    };
    private static void requireHandle(String handle) {
        if (handle == null || handle.length() > 128 || handle.isEmpty()) throw new IllegalArgumentException("Invalid handle");
    }
}
