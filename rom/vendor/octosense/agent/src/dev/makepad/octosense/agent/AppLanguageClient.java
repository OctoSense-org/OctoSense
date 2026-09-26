package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IAppLanguageSettings;

/** Optional system-owned per-app language adapter. Failed binds are retried by visible-page reads. */
final class AppLanguageClient implements AutoCloseable {
    private final SettingsServiceConnection<IAppLanguageSettings> connection;
    AppLanguageClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseAppLanguageService"), IAppLanguageSettings.Stub::asInterface);
    }
    Bundle snapshot(long id,String pkg,String key,String parent,String query,int offset)throws RemoteException {
        dev.makepad.octosense.applanguage.AppLanguageContract.read(id,pkg,key,parent,query,offset);
        IAppLanguageSettings current=connection.current();if(current==null){return null;}
        return current.snapshot(id,pkg,key,parent,query,offset);
    }
    String select(String pkg,String key,String choice)throws RemoteException {
        dev.makepad.octosense.applanguage.AppLanguageContract.packageName(pkg);
        dev.makepad.octosense.applanguage.AppLanguageContract.key(key);
        dev.makepad.octosense.applanguage.AppLanguageContract.key(choice);
        IAppLanguageSettings current=connection.current();return current==null?"app_language_unavailable":current.select(pkg,key,choice);
    }
    @Override public void close() {connection.close();}
}
