package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IAppBatterySettings;

/** Optional system-owned battery policy adapter. Failed binds are retried by visible-page reads. */
final class AppBatteryClient implements AutoCloseable {
    private final SettingsServiceConnection<IAppBatterySettings> connection;
    AppBatteryClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseAppBatteryService"), IAppBatterySettings.Stub::asInterface);
    }
    Bundle snapshot(long id,String pkg) throws RemoteException {
        dev.makepad.octosense.battery.AppBatteryContract.packageName(pkg);
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        IAppBatterySettings current=connection.current();if(current==null){return null;}
        return current.snapshot(id,pkg);
    }
    String set(String pkg,String key,String mode) throws RemoteException {
        dev.makepad.octosense.battery.AppBatteryContract.packageName(pkg);
        dev.makepad.octosense.battery.AppBatteryContract.key(key);
        dev.makepad.octosense.battery.AppBatteryContract.Mode.parse(mode);
        IAppBatterySettings current=connection.current();return current==null?"app_battery_unavailable":current.set(pkg,key,mode);
    }
    @Override public void close() {connection.close();}
}
