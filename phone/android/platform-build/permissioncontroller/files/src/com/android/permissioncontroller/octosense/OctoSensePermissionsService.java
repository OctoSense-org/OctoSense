package com.android.permissioncontroller.octosense;

import android.app.Service;
import android.app.PendingIntent;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.UserHandle;
import dev.makepad.octosense.permissions.PermissionsSettingsContract;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;
import java.util.Arrays;

/** Current owner only. No transaction performs a permission grant or revoke. */
public final class OctoSensePermissionsService extends Service {
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(!UserHandle.getUserHandleForUid(uid).equals(Process.myUserHandle())||packages==null
                ||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")
                ||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller is not the trusted Settings helper");}
    private final IPermissionSettings.Stub binder=new IPermissionSettings.Stub(){
        @Override public Bundle snapshot(long id,String pkg,String group,int offset,String generation){
            caller();Group selected=group==null?null:Group.parse(group);PermissionsSettingsContract.page(id,pkg,selected,offset,generation);
            long identity=Binder.clearCallingIdentity();
            try{Bundle result=new Bundle();result.putBoolean("ok",true);result.putString("json",PermissionSettingsBackend.get(OctoSensePermissionsService.this).snapshot(id,pkg,selected,offset,generation).toString());return result;}
            catch(Exception unavailable){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}
            finally{Binder.restoreCallingIdentity(identity);}
        }
        @Override public Bundle operation(String pkg,String group,String key,String target){
            caller();Group selected=Group.parse(group);PermissionsSettingsContract.packageName(pkg);PermissionsSettingsContract.key(key);PermissionsSettingsContract.key(target);
            long identity=Binder.clearCallingIdentity();
            try{PendingIntent flow=PermissionSettingsBackend.get(OctoSensePermissionsService.this).operation(pkg,selected,key,target);Bundle result=new Bundle();result.putBoolean("ok",flow!=null);if(flow!=null)result.putParcelable("flow",flow);return result;}
            catch(Exception unavailable){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}
            finally{Binder.restoreCallingIdentity(identity);}
        }
    };
    @Override public IBinder onBind(Intent intent){return binder;}
}
