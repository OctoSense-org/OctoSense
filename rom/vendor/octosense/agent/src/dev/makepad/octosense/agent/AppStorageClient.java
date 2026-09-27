package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IAppStorageSettings;

/** Optional system-owned app storage adapter. Failed binds are retried by visible-page reads. */
final class AppStorageClient implements AutoCloseable {
    private final SettingsServiceConnection<IAppStorageSettings> connection;
    AppStorageClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseAppStorageService"), IAppStorageSettings.Stub::asInterface);
    }
    Bundle snapshot(long id,String pkg) throws RemoteException {
        dev.makepad.octosense.appstorage.AppStorageContract.packageName(pkg);
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        IAppStorageSettings current=connection.current();if(current==null){return null;}
        return current.snapshot(id,pkg);
    }
    Bundle action(String pkg,String key,String action) throws RemoteException {
        dev.makepad.octosense.appstorage.AppStorageContract.packageName(pkg);
        dev.makepad.octosense.appstorage.AppStorageContract.key(key);
        dev.makepad.octosense.appstorage.AppStorageContract.Action.parse(action);
        IAppStorageSettings current=connection.current();return current==null?null:current.action(pkg,key,action);
    }
    @Override public void close() {connection.close();}
}
