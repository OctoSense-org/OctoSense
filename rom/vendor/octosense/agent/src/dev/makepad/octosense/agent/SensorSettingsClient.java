package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import com.android.systemui.octosense.ISensorSettings;

/** Optional role-owned SystemUI adapter. Failed binds are retried by visible-page reads. */
final class SensorSettingsClient implements AutoCloseable {
    private final SettingsServiceConnection<ISensorSettings> connection;
    SensorSettingsClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "com.android.systemui", "com.android.systemui.octosense.OctoSenseSensorSettingsService"), ISensorSettings.Stub::asInterface);
    }
    Bundle snapshot() {
        ISensorSettings current=connection.current();
        if(current==null) {return null;}
        try {Bundle result=current.snapshot();return result!=null&&result.getBoolean("ok")?result:null;}
        catch(RemoteException|SecurityException unavailable) {return null;}
    }
    boolean apply(int sensor,boolean allowed) {
        if(sensor!=1&&sensor!=2) throw new IllegalArgumentException("Unknown sensor");
        ISensorSettings current=connection.current();if(current==null) return false;
        try {Bundle result=current.setAccess(sensor,allowed);return result!=null&&result.getBoolean("ok");}
        catch(RemoteException|SecurityException unavailable) {return false;}
    }
    @Override public void close() {connection.close();}
}
