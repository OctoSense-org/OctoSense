package dev.makepad.octosense;

import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.battery.AppBatteryContract;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Package selection is navigation; only the observed capability can authorize a write. */
final class AppBatterySettingsClient {
    private final AgentPlatformClient agent;private final BooleanSupplier foreground;
    private String pkg,key;private long observed=-1;private final Set<String> choices=new HashSet<>();
    AppBatterySettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){pkg=null;key=null;observed=-1;choices.clear();}
    synchronized JSONObject snapshot(long id,String packageName)throws Exception {
        AppBatteryContract.packageName(packageName);invalidate();
        if(!foreground.getAsBoolean())return AppBatteryContract.unavailable(id,packageName);
        JSONObject state=agent.appBatterySnapshot(id,packageName);
        if(state==null||!foreground.getAsBoolean())return AppBatteryContract.unavailable(id,packageName);
        if(state.getLong("request_id")!=id||!packageName.equals(state.getString("package")))throw new IllegalStateException("Wrong app battery observation");
        if(!"available".equals(state.getString("availability"))||!state.getBoolean("can_set"))return state;
        JSONArray offered=state.getJSONArray("choices");if(offered.length()!=3)throw new IllegalStateException("Invalid battery choices");
        for(int i=0;i<offered.length();i++)if(!choices.add(AppBatteryContract.Mode.parse(offered.getString(i)).wire))throw new IllegalStateException("Repeated battery choice");
        pkg=packageName;key=AppBatteryContract.key(state.getString("key"));observed=SystemClock.elapsedRealtime();return state;
    }
    synchronized String set(String packageName,String observedKey,String mode)throws Exception {
        AppBatteryContract.packageName(packageName);AppBatteryContract.key(observedKey);AppBatteryContract.Mode.parse(mode);
        long now=SystemClock.elapsedRealtime();
        if(!foreground.getAsBoolean()||!packageName.equals(pkg)||!observedKey.equals(key)||observed<0||now<observed||now-observed>20000||!choices.contains(mode))return "app_battery_target_changed";
        invalidate();return agent.appBatterySet(packageName,observedKey,mode);
    }
}
