package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import com.android.permissioncontroller.octosense.IRoleSettings;
import dev.makepad.octosense.roles.RolesSettingsContract;
import dev.makepad.octosense.roles.RolesSettingsContract.RoleId;

/** Native role qualification and confirmation remain in PermissionController. */
final class RolesSettingsClient implements AutoCloseable {
    private final SettingsServiceConnection<IRoleSettings> connection;
    RolesSettingsClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "com.android.permissioncontroller", "com.android.permissioncontroller.octosense.OctoSenseRolesService"), IRoleSettings.Stub::asInterface);
    }
    Bundle snapshot(long id,String role,int offset,String generation)throws RemoteException{
        RoleId selected=role==null?null:RoleId.parse(role);RolesSettingsContract.page(id,selected,offset,generation);
        IRoleSettings current=connection.current();if(current==null){return null;}
        return current.snapshot(id,role,offset,generation);
    }
    Bundle confirmation(String role,String key,String target)throws RemoteException{
        RoleId.parse(role);RolesSettingsContract.key(key);RolesSettingsContract.key(target);
        IRoleSettings current=connection.current();return current==null?null:current.confirmation(role,key,target);
    }
    @Override public void close() {connection.close();}
}
