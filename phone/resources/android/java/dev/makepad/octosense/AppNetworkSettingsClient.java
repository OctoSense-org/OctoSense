package dev.makepad.octosense;

import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.appnetwork.AppNetworkBackend;
import dev.makepad.octosense.appnetwork.AppNetworkContract;
import java.util.EnumMap;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

final class AppNetworkSettingsClient {
    private final AgentPlatformClient agent;
    private final BooleanSupplier foreground;
    private final AppNetworkBackend unavailable=new AppNetworkBackend(null,SystemClock::elapsedRealtime);
    private String pkg,key;private long at=-1;
    private final EnumMap<AppNetworkContract.Field,Boolean> offered=new EnumMap<>(AppNetworkContract.Field.class);
    AppNetworkSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){pkg=null;key=null;at=-1;offered.clear();}
    synchronized JSONObject snapshot(long id,String pkg)throws Exception{
        AppNetworkContract.packageName(pkg);invalidate();
        if(!foreground.getAsBoolean())return unavailable.snapshot(id,pkg);
        JSONObject state=agent.appNetworkSnapshot(id,pkg);
        if(state==null||!foreground.getAsBoolean())return unavailable.snapshot(id,pkg);
        if(state.getLong("request_id")!=id||!pkg.equals(state.getString("package")))throw new IllegalStateException("Mismatched app network observation");
        if(!"available".equals(state.getString("availability")))return state;
        this.pkg=pkg;key=AppNetworkContract.key(state.getString("key"));JSONArray rows=state.getJSONArray("controls");
        if(rows.length()!=AppNetworkContract.Field.values().length)throw new IllegalStateException("Incomplete network controls");
        for(int i=0;i<rows.length();i++){JSONObject row=rows.getJSONObject(i);AppNetworkContract.Field f=AppNetworkContract.Field.parse(row.getString("field"));
            if(row.getBoolean("can_set"))offered.put(f,!row.getBoolean("value"));}
        at=SystemClock.elapsedRealtime();return state;
    }
    synchronized String set(String pkg,String key,AppNetworkContract.Field field,boolean enabled)throws Exception{
        AppNetworkContract.packageName(pkg);AppNetworkContract.key(key);long now=SystemClock.elapsedRealtime();
        boolean fresh=foreground.getAsBoolean()&&pkg.equals(this.pkg)&&key.equals(this.key)&&at>=0&&now>=at&&now-at<=20000
            &&Boolean.valueOf(enabled).equals(offered.get(field));
        invalidate();return fresh?agent.appNetworkSet(pkg,key,field,enabled):"app_network_target_changed";
    }
}
