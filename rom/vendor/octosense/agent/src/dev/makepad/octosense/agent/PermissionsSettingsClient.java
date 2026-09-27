package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import com.android.permissioncontroller.octosense.IPermissionSettings;
import dev.makepad.octosense.permissions.PermissionsSettingsContract;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;

/** Native runtime choices and operation consent remain in PermissionController. */
final class PermissionsSettingsClient implements AutoCloseable {
    private final SettingsServiceConnection<IPermissionSettings> connection;
    PermissionsSettingsClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "com.android.permissioncontroller", "com.android.permissioncontroller.octosense.OctoSensePermissionsService"), IPermissionSettings.Stub::asInterface);
    }
    Bundle snapshot(long id,String pkg,String group,int offset,String generation)throws RemoteException{
        Group selected=group==null?null:Group.parse(group);PermissionsSettingsContract.page(id,pkg,selected,offset,generation);
        IPermissionSettings current=connection.current();if(current==null){return null;}
        return current.snapshot(id,pkg,group,offset,generation);
    }
    Bundle operation(String pkg,String group,String key,String target)throws RemoteException{
        PermissionsSettingsContract.packageName(pkg);Group.parse(group);PermissionsSettingsContract.key(key);PermissionsSettingsContract.key(target);
        IPermissionSettings current=connection.current();return current==null?null:current.operation(pkg,group,key,target);
    }
    @Override public void close() {connection.close();}
}
