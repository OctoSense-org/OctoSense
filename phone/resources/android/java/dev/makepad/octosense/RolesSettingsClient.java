package dev.makepad.octosense;

import android.app.PendingIntent;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.roles.RolesSettingsContract;
import dev.makepad.octosense.roles.RolesSettingsContract.RoleId;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Only observed candidates can open a native consent flow; never an automatic grant. */
final class RolesSettingsClient {
    private final AgentPlatformClient agent;private final BooleanSupplier foreground;
    private RoleId role;private String key;private long observed;private final Set<String> targets=new HashSet<>();
    RolesSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){role=null;key=null;observed=0;targets.clear();}
    synchronized JSONObject snapshot(long id,RoleId selected,int offset,String generation)throws Exception{
        RolesSettingsContract.page(id,selected,offset,generation);invalidate();
        if(!foreground.getAsBoolean())return RolesSettingsContract.unavailable(id,selected,"restricted");
        JSONObject state=agent.rolesSnapshot(id,selected,offset,generation);
        if(state==null)return RolesSettingsContract.unavailable(id,selected,"unavailable");
        if(!foreground.getAsBoolean())return RolesSettingsContract.unavailable(id,selected,"restricted");
        if(state.getLong("request_id")!=id||!java.util.Objects.equals(selected==null?null:selected.wire,state.isNull("role")?null:state.getString("role")))throw new IllegalStateException("Mismatched role observation");
        if(selected==null||!"available".equals(state.getString("availability")))return state;
        role=selected;key=state.getString("key");RolesSettingsContract.key(key);
        JSONArray rows=state.getJSONArray("candidates");if(rows.length()>RolesSettingsContract.PAGE_SIZE)throw new IllegalStateException("Oversized role page");
        for(int i=0;i<rows.length();i++){JSONObject row=rows.getJSONObject(i);String target=row.getString("target");RolesSettingsContract.key(target);if(row.getBoolean("can_select"))targets.add(target);}
        if(!state.isNull("none_target")){String target=state.getString("none_target");RolesSettingsContract.key(target);targets.add(target);}
        observed=SystemClock.elapsedRealtime();return state;
    }
    synchronized PendingIntent confirmation(RoleId selected,String key,String target)throws Exception{
        RolesSettingsContract.key(key);RolesSettingsContract.key(target);
        if(!foreground.getAsBoolean()){invalidate();return null;}
        long now=SystemClock.elapsedRealtime();
        if(selected!=role||!key.equals(this.key)||!targets.contains(target)||now<observed||now-observed>RolesSettingsContract.OBSERVATION_MS)return null;
        invalidate();return agent.roleConfirmation(selected,key,target);
    }
}
