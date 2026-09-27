package com.android.permissioncontroller.octosense;

import android.app.PendingIntent;
import android.content.Context;
import android.os.SystemClock;
import dev.makepad.octosense.permissions.PermissionsSettingsContract;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Choice;
import org.json.JSONObject;

public final class RuntimePermissionsBackendTest {
    static final String PKG="fixture.permissions";
    static void check(boolean condition,String message){if(!condition)throw new AssertionError(message);}
    static String hash(String value){return PermissionsSettingsContract.hash(value);}
    static NativePermissionModel.State state(){
        NativePermissionModel.State state=new NativePermissionModel.State();state.pkg=PKG;state.label="Fixture";state.exists=true;state.groupAvailable=true;state.group=Group.CAMERA;
        state.identity=hash("incarnation1");state.fingerprint=hash("denied1");state.generation=hash("groups1");
        state.choices.add(new NativePermissionModel.Option(Choice.ALLOW_FOREGROUND,false,true));state.choices.add(new NativePermissionModel.Option(Choice.ASK,false,true));
        state.choices.add(new NativePermissionModel.Option(Choice.ONE_TIME,false,true));state.choices.add(new NativePermissionModel.Option(Choice.DENY,true,true));return state;
    }
    static JSONObject snapshot(PermissionSettingsBackend backend)throws Exception{return backend.snapshot(1,PKG,Group.CAMERA,0,null);}
    static PendingIntent operation(PermissionSettingsBackend backend,JSONObject observed)throws Exception{return backend.operation(PKG,Group.CAMERA,observed.getString("key"),observed.getJSONArray("choices").getJSONObject(0).getString("target"));}
    static void invalid(Runnable action){try{action.run();throw new AssertionError("Malformed request accepted");}catch(IllegalArgumentException expected){}}
    public static void main(String[] args)throws Exception{
        for(String pkg:new String[]{"", "single", "中文.app", "com.bad/path", "com..bad", "com.bad\n", "com.1bad"})invalid(()->PermissionsSettingsContract.packageName(pkg));
        PermissionsSettingsContract.packageName("android");PermissionsSettingsContract.packageName(PKG);
        invalid(()->PermissionsSettingsContract.page(0,PKG,null,0,null));invalid(()->PermissionsSettingsContract.page(1,PKG,null,20,null));
        invalid(()->PermissionsSettingsContract.page(1,PKG,Group.CAMERA,20,hash("page")));invalid(()->Group.parse("android.permission.CAMERA"));invalid(()->Choice.parse("grant_uid"));
        Context context=new Context();PermissionSettingsBackend backend=PermissionSettingsBackend.get(context);
        NativePermissionModel.next=state();JSONObject first=snapshot(backend);
        check(first.getJSONArray("choices").length()==4,"Observed finite choices exported");
        check(!first.getJSONArray("choices").getJSONObject(2).getBoolean("enabled"),"ASK_ONCE cannot become a command");
        check(!first.getJSONArray("choices").getJSONObject(3).getBoolean("enabled"),"Selected choice is not re-dispatched");
        check(first.getString("key").equals(snapshot(backend).getString("key")),"Identical foreground polls retain observed authority");
        check(backend.operation(PKG,Group.CAMERA,first.getString("key"),hash("forged"))==null,"Forged choice rejected");
        check(backend.operation("fixture.other",Group.CAMERA,first.getString("key"),first.getJSONArray("choices").getJSONObject(0).getString("target"))==null,"Observed package identity required");
        NativePermissionModel.next.fingerprint=hash("external-change");check(operation(backend,first)==null,"External state retires an old action");
        first=snapshot(backend);PendingIntent flow=operation(backend,first);check(flow!=null,"Observed action produces native operation token");
        check((flow.flags&(PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_ONE_SHOT))==(PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_ONE_SHOT),"Token immutable and one-shot");
        check(operation(backend,first)==null,"Lease cannot issue duplicate tokens");
        check(flow.intent.destination==OctoSensePermissionOperationActivity.class,"Only unexported native operation Activity");
        PermissionSettingsBackend.Ticket reviewed=backend.redeem(flow.intent.ticket,NativePermissionModel.next);
        check(reviewed!=null&&backend.redeem(flow.intent.ticket,NativePermissionModel.next)==null,"Only one Activity redeems operation");
        check(backend.valid(reviewed,NativePermissionModel.next),"Current warning can proceed");
        context.lock.locked=true;check(!backend.valid(reviewed,NativePermissionModel.next),"Lock blocks native positive button");context.lock.locked=false;
        NativePermissionModel.next.fingerprint=hash("policy-fixed");check(!backend.valid(reviewed,NativePermissionModel.next),"Warning approval rereads state and policy");
        NativePermissionModel.next=state();first=snapshot(backend);flow=operation(backend,first);
        NativePermissionModel.next.identity=hash("reinstalled");check(backend.redeem(flow.intent.ticket,NativePermissionModel.next)==null,"Reinstall cannot retarget an open operation");
        NativePermissionModel.next=state();first=snapshot(backend);SystemClock.now+=PermissionsSettingsContract.OBSERVATION_MS+1;check(operation(backend,first)==null,"Observation expiry enforced");
        first=snapshot(backend);flow=operation(backend,first);SystemClock.now+=PermissionsSettingsContract.TICKET_MS+1;check(backend.peek(flow.intent.ticket)==null,"Native review expires");
        first=snapshot(backend);flow=operation(backend,first);backend.cancel(flow.intent.ticket);check(backend.peek(flow.intent.ticket)==null,"Cancel is inert and cannot replay");
        first=snapshot(backend);flow=operation(backend,first);reviewed=backend.redeem(flow.intent.ticket,NativePermissionModel.next);
        check(!backend.applied(reviewed,NativePermissionModel.next),"Dispatch is not observed success");
        NativePermissionModel.next.choices.clear();NativePermissionModel.next.choices.add(new NativePermissionModel.Option(Choice.ALLOW_FOREGROUND,true,true));
        check(backend.applied(reviewed,NativePermissionModel.next),"Observed native choice establishes completion");
        context.users.foreground=false;check(!backend.applied(reviewed,NativePermissionModel.next),"Background owner has no completion authority");
        check(snapshot(backend).getString("availability").equals("restricted"),"No permission inventory while owner inactive");context.users.foreground=true;
        android.os.Process.user=10;check(snapshot(backend).getString("availability").equals("restricted"),"Secondary-user process denied");android.os.Process.user=0;
        NativePermissionModel.next=state();NativePermissionModel.next.exists=false;check(!snapshot(backend).getBoolean("exists"),"Verified missing target explicit");
        NativePermissionModel.next=state();NativePermissionModel.next.groupAvailable=false;check(snapshot(backend).getString("availability").equals("unsupported"),"Unrequested group cannot gain choices");
        NativePermissionModel.next=state();NativePermissionModel.next.group=null;
        for(int i=0;i<25;i++){NativePermissionModel.Row row=new NativePermissionModel.Row();row.name="group"+i;row.label="Group "+i;row.category="denied";row.target=hash(row.name);NativePermissionModel.next.rows.add(row);}
        JSONObject page=backend.snapshot(2,PKG,null,0,null);check(page.getJSONArray("groups").length()==20,"Bounded group inventory");
        page=backend.snapshot(3,PKG,null,20,page.getString("generation"));check(page.getJSONArray("groups").length()==5,"Native-only groups retained and paged");
        NativePermissionModel.next.generation=hash("changed-inventory");page=backend.snapshot(4,PKG,null,20,page.getString("generation"));check(page.getBoolean("stale")&&page.getInt("offset")==0,"Changed group inventory resets page");
        check(PermissionsSettingsContract.unavailable(1,PKG,null,"unavailable").opt("exists")==JSONObject.NULL,"Unknown authority does not invent app existence");
    }
}
