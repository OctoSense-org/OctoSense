package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IAppNetworkSettings;

/** Optional system-owned app network policy. Reads retry failed service bindings. */
final class AppNetworkClient implements AutoCloseable {
    private final SettingsServiceConnection<IAppNetworkSettings> connection;
    AppNetworkClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseAppNetworkService"), IAppNetworkSettings.Stub::asInterface);
    }
    private interface Operation {Bundle run(IAppNetworkSettings service) throws RemoteException;}
    private Bundle request(Operation operation) {
        IAppNetworkSettings current=connection.current();
        if(current==null){return null;}
        try{return operation.run(current);}catch(RemoteException|SecurityException unavailable){return null;}
    }
    Bundle snapshot(long id,String pkg) {return request(service->service.snapshot(id,pkg));}
    Bundle set(String pkg,String key,String field,boolean enabled) {return request(service->service.set(pkg,key,field,enabled));}
    @Override public void close() {connection.close();}
}
