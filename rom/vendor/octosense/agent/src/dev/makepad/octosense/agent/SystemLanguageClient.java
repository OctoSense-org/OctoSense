package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.ISystemLanguageSettings;

/** Optional system-owned ordered system-language adapter. Failed binds are retried by visible-page reads. */
final class SystemLanguageClient implements AutoCloseable {
    private final SettingsServiceConnection<ISystemLanguageSettings> connection;
    SystemLanguageClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseSystemLanguageService"), ISystemLanguageSettings.Stub::asInterface);
    }
    Bundle snapshot(long id,String key,String parent,String query,int offset)throws RemoteException {
        dev.makepad.octosense.systemlanguage.SystemLanguageContract.read(id,key,parent,query,offset);
        ISystemLanguageSettings current=connection.current();if(current==null){return null;}
        return current.snapshot(id,key,parent,query,offset);
    }
    String apply(String key,String[] targets)throws RemoteException {
        dev.makepad.octosense.systemlanguage.SystemLanguageContract.order(key,targets);
        ISystemLanguageSettings current=connection.current();return current==null?"languages_unavailable":current.apply(key,targets);
    }
    @Override public void close() {connection.close();}
}
