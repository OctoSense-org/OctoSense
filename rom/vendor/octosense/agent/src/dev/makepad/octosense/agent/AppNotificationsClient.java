package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IAppNotifications;

/** Optional system-owned notification settings adapter. Failed binds are retried by visible-page reads. */
final class AppNotificationsClient implements AutoCloseable {
    private final SettingsServiceConnection<IAppNotifications> connection;
    AppNotificationsClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseAppNotificationsService"), IAppNotifications.Stub::asInterface);
    }
    Bundle snapshot(long id,String pkg,int offset,String generation) throws RemoteException {
        dev.makepad.octosense.notifications.AppNotificationsContract.packageName(pkg);
        dev.makepad.octosense.notifications.AppNotificationsContract.page(id,offset,generation);
        IAppNotifications current=connection.current();if(current==null){return null;}
        return current.snapshot(id,pkg,offset,generation);
    }
    String set(String pkg,String key,String target,String action,String value) throws RemoteException {
        dev.makepad.octosense.notifications.AppNotificationsContract.packageName(pkg);
        dev.makepad.octosense.notifications.AppNotificationsContract.key(key);
        dev.makepad.octosense.notifications.AppNotificationsContract.key(target);
        dev.makepad.octosense.notifications.AppNotificationsContract.value(dev.makepad.octosense.notifications.AppNotificationsContract.Action.parse(action),value);
        IAppNotifications current=connection.current();return current==null?"notifications_unavailable":current.set(pkg,key,target,action,value);
    }
    @Override public void close() {connection.close();}
}
