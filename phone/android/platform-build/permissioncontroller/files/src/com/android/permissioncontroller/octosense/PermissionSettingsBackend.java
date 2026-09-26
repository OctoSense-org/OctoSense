package com.android.permissioncontroller.octosense;

import android.app.KeyguardManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.os.Process;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import dev.makepad.octosense.permissions.PermissionsSettingsContract;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Choice;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Short observation authority and native operation tickets; all grants remain in the native model. */
final class PermissionSettingsBackend {
    private static PermissionSettingsBackend instance;
    static synchronized PermissionSettingsBackend get(Context context){if(instance==null)instance=new PermissionSettingsBackend(context.getApplicationContext());return instance;}
    private final Context context;final NativePermissionModel source;
    private final PermissionsSettingsContract.Lease lease=new PermissionsSettingsContract.Lease();
    private String observedPackage;private Group observedGroup;private final Set<String> observedTargets=new HashSet<>();
    private final Map<String,Ticket> tickets=new HashMap<>();
    private PermissionSettingsBackend(Context context){this.context=context;source=new NativePermissionModel(context);}
    boolean owner(){UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return Process.myUserHandle().equals(UserHandle.SYSTEM)&&users!=null&&users.isUserForeground()&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();}
    private static String target(NativePermissionModel.State state,Choice choice){return PermissionsSettingsContract.hash(state.identity+":"+state.group.wire+":"+choice.wire);}
    JSONObject snapshot(long id,String pkg,Group group,int offset,String expected)throws Exception{
        PermissionsSettingsContract.page(id,pkg,group,offset,expected);
        if(!owner()){retire();return PermissionsSettingsContract.unavailable(id,pkg,group,"restricted");}
        // Waiting for main-thread LiveData is allowed only on this Binder worker.
        // Never hold the authority lock while the main thread constructs a model.
        NativePermissionModel.State state=source.read(pkg,group);
        synchronized(this){
            if(!owner()){retire();return PermissionsSettingsContract.unavailable(id,pkg,group,"restricted");}
            if(group==null||!pkg.equals(observedPackage)||group!=observedGroup||!state.exists||!state.groupAvailable)lease.retire();
            observedTargets.clear();observedPackage=pkg;observedGroup=group;
            JSONObject result=PermissionsSettingsContract.unavailable(id,pkg,group,"available").put("exists",state.exists);
            if(!state.exists)return result;
            result.put("app_label",state.label);
            if(group==null){
                int total=Math.min(state.rows.size(),PermissionsSettingsContract.MAX_GROUPS);
                boolean stale=expected!=null&&!expected.equals(state.generation)||offset>0&&offset>=total;if(stale)offset=0;
                JSONArray rows=new JSONArray();
                for(int i=offset;i<Math.min(total,offset+PermissionsSettingsContract.PAGE_SIZE);i++){
                    NativePermissionModel.Row row=state.rows.get(i);
                    rows.put(new JSONObject().put("target",row.target).put("group",row.group==null?JSONObject.NULL:row.group.wire)
                            .put("label",row.label).put("category",row.category).put("subtitle",row.subtitle==null?JSONObject.NULL:row.subtitle));
                }
                return result.put("generation",state.generation).put("offset",offset).put("total",total).put("stale",stale)
                        .put("truncated",state.rows.size()>total).put("groups",rows);
            }
            if(!state.groupAvailable)return result.put("availability","unsupported");
            JSONArray choices=new JSONArray();
            for(NativePermissionModel.Option option:state.choices){String target=target(state,option.choice);if(option.enabled)observedTargets.add(target);
                choices.put(new JSONObject().put("choice",option.choice.wire).put("label",option.label).put("selected",option.selected).put("enabled",option.enabled)
                        .put("target",option.enabled?target:JSONObject.NULL));}
            return result.put("key",lease.observe(state.fingerprint,SystemClock.elapsedRealtime())).put("choices",choices)
                    .put("detail",state.detail==null?JSONObject.NULL:state.detail);
        }
    }
    static final class Ticket {
        final String id,pkg,identity,fingerprint;final Group group;final Choice choice;final long expires;
        Ticket(NativePermissionModel.State state,Choice choice,long now){id=UUID.randomUUID().toString();pkg=state.pkg;group=state.group;identity=state.identity;fingerprint=state.fingerprint;this.choice=choice;expires=now+PermissionsSettingsContract.TICKET_MS;}
    }
    PendingIntent operation(String pkg,Group group,String key,String target)throws Exception{
        PermissionsSettingsContract.packageName(pkg);PermissionsSettingsContract.key(key);PermissionsSettingsContract.key(target);
        synchronized(this){if(!owner()||!pkg.equals(observedPackage)||group!=observedGroup||!observedTargets.contains(target))return null;}
        NativePermissionModel.State state=source.read(pkg,group);
        synchronized(this){
            if(!owner()||!state.exists||!state.groupAvailable||!pkg.equals(observedPackage)||group!=observedGroup||!observedTargets.contains(target))return null;
            Choice selected=null;for(NativePermissionModel.Option option:state.choices)if(option.enabled&&target(state,option.choice).equals(target))selected=option.choice;
            if(selected==null||!selected.actionable()||!lease.claim(key,state.fingerprint,SystemClock.elapsedRealtime()))return null;
            long now=SystemClock.elapsedRealtime();tickets.values().removeIf(ticket->now>=ticket.expires);if(tickets.size()>=8)return null;
            Ticket ticket=new Ticket(state,selected,now);tickets.put(ticket.id,ticket);
            Intent intent=new Intent(context,OctoSensePermissionOperationActivity.class).setIdentifier(ticket.id).putExtra("ticket",ticket.id)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS);
            return PendingIntent.getActivity(context,0,intent,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_ONE_SHOT);
        }
    }
    synchronized Ticket peek(String id){Ticket ticket=tickets.get(id);return ticket!=null&&owner()&&SystemClock.elapsedRealtime()<ticket.expires?ticket:null;}
    synchronized Ticket redeem(String id,NativePermissionModel.State state){Ticket ticket=peek(id);if(!valid(ticket,state))return null;tickets.remove(id);return ticket;}
    boolean valid(Ticket ticket,NativePermissionModel.State state){
        if(ticket==null||state==null||!owner()||SystemClock.elapsedRealtime()>=ticket.expires||!state.exists||!state.groupAvailable
                ||ticket.group!=state.group||!ticket.pkg.equals(state.pkg)||!ticket.identity.equals(state.identity)||!ticket.fingerprint.equals(state.fingerprint))return false;
        NativePermissionModel.Option option=state.option(ticket.choice);return option!=null&&option.enabled;
    }
    boolean applied(Ticket ticket,NativePermissionModel.State state){
        if(ticket==null||state==null||!owner()||!state.exists||!ticket.pkg.equals(state.pkg)||ticket.group!=state.group||!ticket.identity.equals(state.identity))return false;
        NativePermissionModel.Option option=state.option(ticket.choice);return option!=null&&option.selected;
    }
    synchronized void cancel(String id){tickets.remove(id);}
    private synchronized void retire(){lease.retire();observedPackage=null;observedGroup=null;observedTargets.clear();tickets.clear();}
}
