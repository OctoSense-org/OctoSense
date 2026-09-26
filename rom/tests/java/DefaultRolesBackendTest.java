package com.android.permissioncontroller.octosense;

import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.os.SystemClock;
import com.android.role.controller.model.Role;
import com.android.role.controller.model.Roles;
import dev.makepad.octosense.roles.RolesSettingsContract;
import dev.makepad.octosense.roles.RolesSettingsContract.RoleId;
import org.json.JSONObject;

public final class DefaultRolesBackendTest {
    static void check(boolean condition,String message){if(!condition)throw new AssertionError(message);}
    static void bad(Runnable call){try{call.run();throw new AssertionError("Accepted malformed role request");}catch(IllegalArgumentException expected){}}
    static PackageInfo app(Context context,String name,int uid){PackageInfo app=new PackageInfo();app.packageName=name;app.applicationInfo.packageName=name;app.applicationInfo.uid=uid;context.packages.apps.put(name,app);return app;}
    static JSONObject row(JSONObject page,int index){return page.getJSONArray("candidates").getJSONObject(index);}
    static PendingIntent confirm(RoleSettingsBackend backend,JSONObject page,int index)throws Exception{return backend.confirmation(RoleId.BROWSER,page.getString("key"),row(page,index).getString("target"));}
    public static void main(String[] args)throws Exception{
        bad(()->RoleId.parse("android.app.role.SYSTEM_GALLERY"));bad(()->RolesSettingsContract.page(1,RoleId.BROWSER,20,null));
        bad(()->RolesSettingsContract.page(0,null,0,null));bad(()->RolesSettingsContract.key("arbitrary.package"));
        Context context=new Context();Role role=new Role();Roles.INSTANCE.roles.put(RoleId.BROWSER.nativeName,role);
        for(int i=0;i<25;i++){String pkg=String.format("fixture.browser%02d",i);app(context,pkg,10200+i);role.candidates.add(pkg);}
        context.roles.holders.put(RoleId.BROWSER.nativeName,java.util.List.of("fixture.browser00"));
        RoleSettingsBackend backend=RoleSettingsBackend.get(context);
        JSONObject first=backend.snapshot(1,RoleId.BROWSER,0,null);
        check(first.getJSONArray("candidates").length()==20&&first.getInt("total")==25,"Native candidates are bounded and paged");
        check(!row(first,0).getBoolean("can_select")&&row(first,0).getBoolean("selected"),"Holder observation is not another grant");
        check(backend.confirmation(RoleId.BROWSER,first.getString("key"),"0".repeat(64))==null,"Forged target rejected");
        PendingIntent flow=confirm(backend,first,1);check(flow!=null,"Observed choice offers native confirmation");
        check(flow.flags==(PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_ONE_SHOT),"Immutable single-use flow");
        check(flow.intent.flags==(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS),"Fresh confirmation task returns to existing picker");
        check(confirm(backend,first,1)==null,"Observation can issue only one flow");
        check(context.roles.getRoleHolders(RoleId.BROWSER.nativeName).get(0).equals("fixture.browser00"),"Opening a review never mutates the role");
        check(backend.review(flow.intent.ticket,false)!=null&&backend.review(flow.intent.ticket,false)!=null,"Rotation can review same ticket without replay");
        backend.cancel(flow.intent.ticket);check(backend.review(flow.intent.ticket,true)==null,"Cancel retires the native ticket");
        first=backend.snapshot(2,RoleId.BROWSER,0,null);flow=confirm(backend,first,1);
        context.packages.apps.get("fixture.browser01").firstInstallTime++;
        check(backend.review(flow.intent.ticket,true)==null,"Same package reinstallation cannot retarget native confirmation");
        first=backend.snapshot(3,RoleId.BROWSER,0,null);flow=confirm(backend,first,1);role.denied.add("fixture.browser01");
        check(backend.review(flow.intent.ticket,true)==null,"New enhanced-confirmation restriction retires old approval");
        first=backend.snapshot(4,RoleId.BROWSER,0,null);check(!row(first,1).getBoolean("can_select"),"Native restricted choice stays disabled");role.denied.clear();
        first=backend.snapshot(5,RoleId.BROWSER,0,null);flow=confirm(backend,first,1);role.restricted=true;
        check(backend.review(flow.intent.ticket,true)==null,"Admin role restriction rechecked at confirmation");role.restricted=false;
        first=backend.snapshot(6,RoleId.BROWSER,0,null);flow=confirm(backend,first,1);
        context.roles.holders.put(RoleId.BROWSER.nativeName,java.util.List.of("fixture.browser02"));
        check(backend.review(flow.intent.ticket,true)==null,"External holder change invalidates review");
        first=backend.snapshot(7,RoleId.BROWSER,0,null);SystemClock.now+=RolesSettingsContract.OBSERVATION_MS+1;
        check(confirm(backend,first,1)==null,"Expired observed target cannot open confirmation");
        first=backend.snapshot(8,RoleId.BROWSER,0,null);flow=confirm(backend,first,1);SystemClock.now+=RolesSettingsContract.TICKET_MS+1;
        check(backend.review(flow.intent.ticket,true)==null,"Expired platform review cannot apply");
        first=backend.snapshot(9,RoleId.BROWSER,0,null);flow=confirm(backend,first,1);
        RoleSettingsBackend.Reviewed approved=backend.review(flow.intent.ticket,true);
        check(approved.packageName.equals("fixture.browser01")&&backend.review(flow.intent.ticket,true)==null,"One native positive button claims exact reviewed target once");
        check(!backend.isApplied(approved),"Native callback alone does not establish current holder");
        context.roles.holders.put(RoleId.BROWSER.nativeName,java.util.List.of("fixture.browser01"));
        check(backend.isApplied(approved),"Current actual holder confirms native completion");
        context.users.foreground=false;check(!backend.isApplied(approved),"No post-selection behavior for background owner");context.users.foreground=true;
        context.packages.apps.get("fixture.browser01").firstInstallTime++;
        check(!backend.isApplied(approved),"Post-selection behavior cannot retarget a new app incarnation");
        context.roles.holders.put(RoleId.BROWSER.nativeName,java.util.List.of("fixture.browser02"));
        first=backend.snapshot(10,RoleId.BROWSER,0,null);JSONObject page2=backend.snapshot(11,RoleId.BROWSER,20,first.getString("generation"));
        check(page2.getJSONArray("candidates").length()==5,"Final native candidate page");
        check(confirm(backend,first,1)==null,"Only the currently observed page grants review authority");
        role.candidates.remove("fixture.browser24");JSONObject reset=backend.snapshot(12,RoleId.BROWSER,20,first.getString("generation"));
        check(reset.getBoolean("stale")&&reset.getInt("offset")==0,"Catalog change resets paging honestly");
        first=backend.snapshot(13,RoleId.BROWSER,0,null);flow=confirm(backend,first,1);context.lock.locked=true;
        check(backend.review(flow.intent.ticket,true)==null,"Locked owner cannot confirm");
        check(backend.snapshot(14,RoleId.BROWSER,0,null).getString("availability").equals("restricted"),"Lock clears observations");context.lock.locked=false;
        check(backend.review(flow.intent.ticket,true)==null,"Unlock cannot revive retired ticket");
        context.users.foreground=false;check(backend.snapshot(15,RoleId.BROWSER,0,null).getJSONArray("roles").length()==0,"Background owner cannot disclose choices");context.users.foreground=true;
        android.os.Process.user=10;check(backend.snapshot(16,RoleId.BROWSER,0,null).getString("availability").equals("restricted"),"Secondary-user process fails closed");android.os.Process.user=0;
        role.visible=false;check(backend.snapshot(17,RoleId.BROWSER,0,null).getString("availability").equals("unsupported"),"Native invisible roles have no capability");role.visible=true;
        first=backend.snapshot(18,RoleId.BROWSER,0,null);String none=first.getString("none_target");
        flow=backend.confirmation(RoleId.BROWSER,first.getString("key"),none);check(flow!=null&&backend.review(flow.intent.ticket,true).packageName==null,"None requires native-supported explicit confirmation");
        role.none=false;check(backend.snapshot(19,RoleId.BROWSER,0,null).opt("none_target")==JSONObject.NULL,"Role without native None cannot clear holder");
        check(RolesSettingsContract.text("a\n\u200fb",256).equals("ab"),"Unsafe label control characters omitted");
    }
}
