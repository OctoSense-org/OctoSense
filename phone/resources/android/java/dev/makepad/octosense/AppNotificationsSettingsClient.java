package dev.makepad.octosense;

import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.notifications.AppNotificationsContract;
import dev.makepad.octosense.notifications.AppNotificationsContract.Action;
import java.util.HashMap;
import java.util.Map;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Only the freshly observed app/page grants mutation targets. No public-API fallback or replay. */
final class AppNotificationsSettingsClient {
    private final AgentPlatformClient agent;
    private final BooleanSupplier foreground;
    private String pkg,key,app;
    private long observedAt;
    private boolean appWritable;
    private final Map<String,JSONObject> rows=new HashMap<>();
    AppNotificationsSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){pkg=key=app=null;rows.clear();appWritable=false;observedAt=0;}
    synchronized JSONObject snapshot(long id,String pkg,int offset,String generation)throws Exception{
        AppNotificationsContract.packageName(pkg);AppNotificationsContract.page(id,offset,generation);
        invalidate();
        if(!foreground.getAsBoolean())return AppNotificationsContract.unavailable(id,pkg,"restricted");
        JSONObject state=agent.appNotificationsSnapshot(id,pkg,offset,generation);
        if(state==null)return AppNotificationsContract.unavailable(id,pkg,"unavailable");
        if(!foreground.getAsBoolean())return AppNotificationsContract.unavailable(id,pkg,"restricted");
        if(state.getLong("request_id")!=id||!pkg.equals(state.getString("package")))throw new IllegalStateException("Mismatched notification state");
        if(!"available".equals(state.getString("availability"))||!state.optBoolean("exists",false))return state;
        this.pkg=pkg;key=AppNotificationsContract.key(state.getString("key"));
        JSONObject record=state.getJSONObject("app");app=AppNotificationsContract.key(record.getString("key"));appWritable=record.getBoolean("can_set");
        JSONArray entries=state.getJSONArray("rows");if(entries.length()>AppNotificationsContract.PAGE_SIZE)throw new IllegalStateException("Oversized notification page");
        for(int i=0;i<entries.length();i++){JSONObject row=entries.getJSONObject(i);rows.put(AppNotificationsContract.key(row.getString("key")),row);}
        observedAt=SystemClock.elapsedRealtime();return state;
    }
    synchronized String set(String pkg,String key,String target,Action action,String value)throws Exception{
        AppNotificationsContract.packageName(pkg);AppNotificationsContract.key(key);AppNotificationsContract.key(target);AppNotificationsContract.value(action,value);
        if(!foreground.getAsBoolean()){invalidate();return "notifications_restricted";}
        long now=SystemClock.elapsedRealtime();
        if(!pkg.equals(this.pkg)||!key.equals(this.key)||now<observedAt||now-observedAt>20_000)return "notifications_target_changed";
        boolean allowed=false;
        if(action==Action.APP_ENABLED)allowed=target.equals(app)&&appWritable;
        else{JSONObject row=rows.get(target);if(row!=null&&row.getBoolean("can_set")){
            boolean group="group".equals(row.getString("kind"));
            if(action==Action.GROUP_ENABLED)allowed=group;
            else if(!group&&action==Action.CHANNEL_ENABLED)allowed=true;
            else if(!group&&action==Action.CHANNEL_IMPORTANCE){JSONArray options=row.getJSONArray("importance_options");for(int i=0;i<options.length();i++)if(value.equals(options.getString(i)))allowed=true;}
        }}
        if(!allowed)return "notifications_restricted";
        invalidate();return agent.appNotificationsSet(pkg,key,target,action,value);
    }
}
