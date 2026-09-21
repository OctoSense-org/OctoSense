package dev.makepad.octosense.contracts;
import android.os.Bundle;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;

interface ISystemBridge {
    Bundle getProtocolInfo();
    Bundle getCapabilities();
    oneway void subscribe(String session, ISystemBridgeCallback callback);
    oneway void unsubscribe(String session);
    oneway void requestSnapshot(String session);
    oneway void setWifiEnabled(String session, long commandId, boolean enabled);
    oneway void setBluetoothEnabled(String session, long commandId, boolean enabled);
    oneway void setBrightness(String session, long commandId, float value, boolean automatic);
    oneway void setVolume(String session, long commandId, float value);
    oneway void setTorchEnabled(String session, long commandId, boolean enabled);
    oneway void setRotationLocked(String session, long commandId, boolean locked);
    oneway void setInterruptionFilter(String session, long commandId, int filter);
    oneway void setBatterySaver(String session, long commandId, boolean enabled);
    oneway void dismissNotification(String session, long commandId, String handle);
    oneway void dismissAllNotifications(String session, long commandId);
    oneway void invokeNotificationAction(String session, long commandId, String handle, String reply);
}
