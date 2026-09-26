package dev.makepad.octosense.settingsbroker;

import android.app.INotificationManager;
import android.app.NotificationChannel;
import android.app.NotificationChannelGroup;
import android.app.NotificationManager;
import android.content.Context;
import dev.makepad.octosense.notifications.AppNotificationsContract.Action;
import org.json.JSONArray;
import org.json.JSONObject;

/** Real backend reads and writes: Android's null-ID group is a collection marker. */
public final class AppNotificationsBackendTest {
    private static final String PKG="dev.fixture.legacy";
    private static void check(boolean ok,String reason){if(!ok)throw new AssertionError(reason);}
    private static JSONObject named(JSONObject state,String name){
        JSONArray rows=state.getJSONArray("rows");
        for(int i=0;i<rows.length();i++)if(name.equals(rows.getJSONObject(i).getString("name")))return rows.getJSONObject(i);
        throw new AssertionError("Missing row: "+name);
    }
    public static void main(String[] args)throws Exception{
        Context context=new Context();INotificationManager service=new INotificationManager();NotificationManager.service=service;
        // This is the exact shape returned by PreferencesHelper for an old target-SDK app.
        service.groups.add(new NotificationChannelGroup(null,null));
        service.channels.add(new NotificationChannel(NotificationChannel.DEFAULT_CHANNEL_ID,"Uncategorized",-1000));
        AppNotificationsBackend backend=new AppNotificationsBackend(context);
        JSONObject state=backend.snapshot(1,PKG,0,null),row=named(state,"Uncategorized");
        check(state.getInt("total")==1,"Synthetic group leaked into catalog");
        check(state.getJSONObject("app").getBoolean("can_set"),"Legacy app permission unavailable");
        check(row.getBoolean("linked_app")&&row.getBoolean("can_set"),"Default channel must expose coupled authority");
        check(row.opt("group_name")==JSONObject.NULL&&!row.getBoolean("group_blocked"),"Ungrouped is not a real parent");
        check(backend.apply(PKG,state.getString("key"),row.getString("key"),Action.CHANNEL_ENABLED,"off").equals("notifications_applied"),"Linked channel off");
        check(!service.enabled&&service.channels.get(0).getImportance()==0,"Both legacy preferences must be off");
        state=backend.snapshot(2,PKG,0,null);
        check(backend.apply(PKG,state.getString("key"),state.getJSONObject("app").getString("key"),Action.APP_ENABLED,"on").equals("notifications_applied"),"Linked app on");
        check(service.enabled&&service.channels.get(0).getImportance()==-1000,"Both legacy preferences must be restored");
        check(service.appWrites==2&&service.channelWrites==2&&service.groupWrites==0,"No synthetic group write");

        // Modern inventories can contain real groups and the same null-ID marker together.
        service.onlyDefault=false;context.packages.info.applicationInfo.targetSdkVersion=35;
        service.channels.clear();service.channels.add(new NotificationChannel("loose","Ungrouped",2));
        NotificationChannel grouped=new NotificationChannel("child","Grouped",2);grouped.setGroup("real");service.channels.add(grouped);
        NotificationChannelGroup group=new NotificationChannelGroup("real","Real group");group.setBlocked(true);service.groups.add(group);
        state=backend.snapshot(3,PKG,0,null);
        check(state.getInt("total")==3,"Only two channels and one real group");
        check(!named(state,"Ungrouped").getBoolean("group_blocked"),"Synthetic parent never blocks ungrouped channel");
        check(named(state,"Grouped").getBoolean("group_blocked"),"Real group block must apply");
        check(named(state,"Grouped").getString("group_name").equals("Real group"),"Real group name preserved");
        row=named(state,"Real group");
        check(backend.apply(PKG,state.getString("key"),row.getString("key"),Action.GROUP_ENABLED,"on").equals("notifications_applied"),"Real group still editable");
        check(service.groupWrites==1,"Exactly one real group write");

        state=backend.snapshot(4,PKG,0,null);context.users.restricted=true;
        check(backend.apply(PKG,state.getString("key"),named(state,"Ungrouped").getString("key"),Action.CHANNEL_ENABLED,"off").equals("notifications_restricted"),"Fresh policy must reject old capability");
        context.users.restricted=false;context.lock.locked=true;
        state=backend.snapshot(5,PKG,0,null);
        check(state.getString("availability").equals("restricted")&&state.getJSONArray("rows").length()==0,"Locked inventory must be cleared");
    }
}
