package dev.makepad.octosense;

import android.app.PendingIntent;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.permissions.PermissionsSettingsContract;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Choice;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Foreground observed choices only; the native controller owns every grant and warning. */
final class PermissionsSettingsClient {
    private final AgentPlatformClient agent;private final BooleanSupplier foreground;
    private String pkg,key;private Group group;private long observed;private final Set<String> targets=new HashSet<>();
    PermissionsSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){pkg=null;group=null;key=null;observed=0;targets.clear();}
    synchronized JSONObject snapshot(long id,String selectedPackage,Group selected,int offset,String generation)throws Exception{
        PermissionsSettingsContract.page(id,selectedPackage,selected,offset,generation);invalidate();
        if(!foreground.getAsBoolean())return PermissionsSettingsContract.unavailable(id,selectedPackage,selected,"restricted");
        JSONObject state=agent.permissionsSnapshot(id,selectedPackage,selected,offset,generation);
        if(state==null)return PermissionsSettingsContract.unavailable(id,selectedPackage,selected,"unavailable");
        if(!foreground.getAsBoolean())return PermissionsSettingsContract.unavailable(id,selectedPackage,selected,"restricted");
        if(state.getLong("request_id")!=id||!selectedPackage.equals(state.getString("package"))
                ||!java.util.Objects.equals(selected==null?null:selected.wire,state.isNull("group")?null:state.getString("group")))throw new IllegalStateException("Mismatched permission observation");
        if(selected==null||!"available".equals(state.getString("availability"))||!Boolean.TRUE.equals(state.opt("exists"))||state.isNull("key"))return state;
        pkg=selectedPackage;group=selected;key=state.getString("key");PermissionsSettingsContract.key(key);
        JSONArray choices=state.getJSONArray("choices");if(choices.length()>Choice.values().length)throw new IllegalStateException("Oversized permission choices");
        for(int i=0;i<choices.length();i++){JSONObject row=choices.getJSONObject(i);Choice choice=Choice.parse(row.getString("choice"));
            if(row.getBoolean("enabled")){
                if(!choice.actionable()||row.getBoolean("selected"))throw new IllegalStateException("Invalid permission action");
                String target=row.getString("target");PermissionsSettingsContract.key(target);targets.add(target);
            }}
        observed=SystemClock.elapsedRealtime();return state;
    }
    synchronized PendingIntent operation(String selectedPackage,Group selected,String key,String target)throws Exception{
        PermissionsSettingsContract.packageName(selectedPackage);PermissionsSettingsContract.key(key);PermissionsSettingsContract.key(target);
        if(!foreground.getAsBoolean()){invalidate();return null;}
        long now=SystemClock.elapsedRealtime();
        if(!selectedPackage.equals(pkg)||selected!=group||!key.equals(this.key)||!targets.contains(target)||now<observed||now-observed>PermissionsSettingsContract.OBSERVATION_MS)return null;
        invalidate();return agent.permissionChoice(selectedPackage,selected,key,target);
    }
}
