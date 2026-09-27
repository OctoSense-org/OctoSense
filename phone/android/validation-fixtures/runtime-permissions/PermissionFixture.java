package dev.makepad.octosense.permissionfixture;

import android.app.AppOpsManager;
import android.app.Instrumentation;
import android.content.Context;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.Process;
import org.json.JSONArray;
import org.json.JSONObject;

/** Independent app: public read-only permission/AppOp observations, no access to user data. */
public final class PermissionFixture extends Instrumentation {
    @Override public void onCreate(Bundle args) { super.onCreate(args); start(); }
    @Override public void onStart() {
        Bundle result=new Bundle();
        try {
            Context context=getTargetContext();String pkg=context.getPackageName();
            PackageManager pm=context.getPackageManager();
            PackageInfo info=pm.getPackageInfo(pkg,PackageManager.GET_PERMISSIONS);
            AppOpsManager appOps=context.getSystemService(AppOpsManager.class);
            JSONObject state=new JSONObject();state.put("package",pkg);
            state.put("uid",Process.myUid());state.put("target_sdk",context.getApplicationInfo().targetSdkVersion);
            JSONArray permissions=new JSONArray();
            if(info.requestedPermissions!=null)for(int i=0;i<info.requestedPermissions.length;i++) {
                String permission=info.requestedPermissions[i];JSONObject row=new JSONObject();
                row.put("name",permission);
                row.put("granted",context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED);
                row.put("package_granted",pm.checkPermission(permission,pkg)==PackageManager.PERMISSION_GRANTED);
                row.put("requested_granted",(info.requestedPermissionsFlags[i]&PackageInfo.REQUESTED_PERMISSION_GRANTED)!=0);
                String op=AppOpsManager.permissionToOp(permission);
                row.put("op",op==null?JSONObject.NULL:op);
                row.put("mode",op==null?JSONObject.NULL:appOps.unsafeCheckOpRawNoThrow(op,Process.myUid(),pkg));
                permissions.put(row);
            }
            state.put("permissions",permissions);result.putString("state",state.toString());
            result.putBoolean("passed",true);finish(0,result);
        }catch(Exception failure) {
            result.putString("failure",failure.getClass().getSimpleName()+": "+failure.getMessage());
            result.putBoolean("passed",false);finish(1,result);
        }
    }
}
