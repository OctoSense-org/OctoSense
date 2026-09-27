package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IKeyboardSettings;

/** Optional system-owned keyboard adapter. Failed binds are retried by visible-page reads. */
final class KeyboardClient implements AutoCloseable {
    private final SettingsServiceConnection<IKeyboardSettings> connection;
    KeyboardClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseKeyboardService"), IKeyboardSettings.Stub::asInterface);
    }
    Bundle snapshot(long id,String query,int offset)throws RemoteException {
        dev.makepad.octosense.keyboards.KeyboardContract.read(id,query,offset);
        IKeyboardSettings current=connection.current();if(current==null){return null;}
        return current.snapshot(id,query,offset);
    }
    Bundle prepare(long id,String key,String target,String operation)throws RemoteException {
        dev.makepad.octosense.keyboards.KeyboardContract.flow(id,key,target,operation);
        IKeyboardSettings current=connection.current();return current==null?null:current.prepare(id,key,target,operation);
    }
    @Override public void close() {connection.close();}
}
