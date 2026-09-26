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
import dev.makepad.octosense.roles.RolesSettingsContract.RoleId;
import java.util.Arrays;

/** No grant API: the only write-related result is a platform-owned confirmation. */
public final class OctoSenseRolesService extends Service {
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(!UserHandle.getUserHandleForUid(uid).equals(Process.myUserHandle())||packages==null
                ||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")
                ||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller is not the trusted Settings helper");}
    private final IRoleSettings.Stub binder=new IRoleSettings.Stub(){
        @Override public Bundle snapshot(long id,String role,int offset,String generation){
            caller();RoleId parsed=role==null?null:RoleId.parse(role);long identity=Binder.clearCallingIdentity();
            try{Bundle result=new Bundle();result.putString("json",RoleSettingsBackend.get(OctoSenseRolesService.this).snapshot(id,parsed,offset,generation).toString());result.putBoolean("ok",true);return result;}
            catch(Exception unavailable){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}
            finally{Binder.restoreCallingIdentity(identity);}
        }
        @Override public Bundle confirmation(String role,String key,String target){
            caller();RoleId parsed=RoleId.parse(role);long identity=Binder.clearCallingIdentity();
            try{PendingIntent flow=RoleSettingsBackend.get(OctoSenseRolesService.this).confirmation(parsed,key,target);Bundle result=new Bundle();result.putBoolean("ok",flow!=null);if(flow!=null)result.putParcelable("flow",flow);return result;}
            catch(Exception unavailable){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}
            finally{Binder.restoreCallingIdentity(identity);}
        }
    };
    @Override public IBinder onBind(Intent intent){return binder;}
}
