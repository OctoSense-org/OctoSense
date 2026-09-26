package dev.makepad.octosense.rolebrowserfixture;

import android.app.Instrumentation;
import android.app.role.RoleManager;
import android.os.Bundle;

/** Public, read-only APIs report only this disposable app's browser/assistant role. */
public final class RoleBrowserProbe extends Instrumentation {
    private String selectedRole=RoleManager.ROLE_BROWSER;
    @Override public void onCreate(Bundle args) {
        super.onCreate(args);
        if(args!=null&&"assistant".equals(args.getString("role")))selectedRole=RoleManager.ROLE_ASSISTANT;
        start();
    }
    @Override public void onStart() {
        Bundle result=new Bundle();boolean passed=false;
        try {
            RoleManager roles=getTargetContext().getSystemService(RoleManager.class);
            if(roles==null)throw new IllegalStateException("Role service unavailable");
            result.putString("package",getTargetContext().getPackageName());
            result.putString("role",selectedRole);
            result.putBoolean("available",roles.isRoleAvailable(selectedRole));
            result.putBoolean("held",roles.isRoleHeld(selectedRole));
            passed=true;
        }catch(RuntimeException failure) {
            result.putString("failure",failure.getClass().getSimpleName());
        }
        result.putBoolean("passed",passed);finish(passed?0:1,result);
    }
}
