package dev.makepad.octosense.notificationfixture;

import android.app.Instrumentation;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationChannelGroup;
import android.app.NotificationManager;
import android.content.Context;
import android.graphics.Color;
import android.os.Bundle;
import android.os.SystemClock;
import android.service.notification.StatusBarNotification;
import org.json.JSONArray;
import org.json.JSONObject;
import java.util.ArrayList;
import java.util.List;

/** Separate disposable app: revoking its permission cannot kill the UI probe. */
public final class NotificationFixture extends Instrumentation {
    private static final String CHANNEL="probe_channel_00";
    private static final String GROUP="probe_group_a";
    private String operation="state";
    @Override public void onCreate(Bundle args) {
        super.onCreate(args);if(args!=null)operation=args.getString("operation","state");start();
    }
    private static NotificationChannel channel(int index) {
        String suffix=index<10?"0"+index:Integer.toString(index);
        NotificationChannel channel=new NotificationChannel("probe_channel_"+suffix,
                "Probe channel "+suffix,NotificationManager.IMPORTANCE_LOW);
        channel.setDescription("Disposable synthetic notification channel "+suffix);
        // Keep one channel ungrouped: the platform's privileged inventory adds
        // a synthetic null-ID group container for it, not a mutable group.
        if(index!=23)channel.setGroup(index<12?GROUP:"probe_group_b");
        channel.setSound(null,null);
        channel.setVibrationPattern(new long[]{0,17,23,31});channel.enableVibration(false);
        channel.enableLights(true);channel.setLightColor(Color.BLUE);
        channel.setShowBadge(true);channel.setLockscreenVisibility(Notification.VISIBILITY_PRIVATE);
        channel.setAllowBubbles(false);
        return channel;
    }
    private static NotificationChannel observedChannel(NotificationManager manager,boolean legacy) {
        if(!legacy)return manager.getNotificationChannel(CHANNEL);
        List<NotificationChannel> channels=manager.getNotificationChannels();
        if(channels.size()!=1)throw new IllegalStateException("Expected one platform legacy channel");
        return channels.get(0);
    }
    private static Notification notification(Context context,boolean legacy) {
        Notification.Builder builder=legacy?new Notification.Builder(context):new Notification.Builder(context,CHANNEL);
        return builder.setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle("OctoSense synthetic notification")
                .setContentText("Disposable channel delivery check")
                .setOnlyAlertOnce(true).build();
    }
    private static JSONObject snapshot(NotificationManager manager,boolean legacy)throws Exception {
        JSONObject state=new JSONObject();state.put("enabled",manager.areNotificationsEnabled());
        state.put("channel_count",manager.getNotificationChannels().size());
        state.put("group_count",manager.getNotificationChannelGroups().size());
        NotificationChannelGroup group=manager.getNotificationChannelGroup(GROUP);
        state.put("group_blocked",group==null?JSONObject.NULL:group.isBlocked());
        NotificationChannel channel=observedChannel(manager,legacy);
        if(channel==null)state.put("channel",JSONObject.NULL);
        else {
            JSONObject row=new JSONObject();row.put("importance",channel.getImportance());
            row.put("sound",channel.getSound()==null?JSONObject.NULL:channel.getSound().toString());
            row.put("name",channel.getName());row.put("description",channel.getDescription());
            row.put("group",channel.getGroup());row.put("badge",channel.canShowBadge());
            row.put("vibrate",channel.shouldVibrate());row.put("lights",channel.shouldShowLights());
            row.put("light_color",channel.getLightColor());row.put("visibility",channel.getLockscreenVisibility());
            row.put("bubbles",channel.canBubble());row.put("bypass_dnd",channel.canBypassDnd());
            JSONArray pattern=new JSONArray();if(channel.getVibrationPattern()!=null)for(long value:channel.getVibrationPattern())pattern.put(value);
            row.put("pattern",pattern);state.put("channel",row);
        }
        JSONArray active=new JSONArray();
        for(StatusBarNotification notification:manager.getActiveNotifications())active.put(notification.getId());
        state.put("active",active);return state;
    }
    @Override public void onStart() {
        Bundle result=new Bundle();
        try {
            Context context=getTargetContext();NotificationManager manager=context.getSystemService(NotificationManager.class);
            boolean legacy=context.getApplicationInfo().targetSdkVersion<26;
            if(operation.equals("setup")) {
                if(legacy){manager.notify(7001,notification(context,true));manager.cancelAll();}
                else {
                    manager.createNotificationChannelGroup(new NotificationChannelGroup(GROUP,"Probe group A"));
                    manager.createNotificationChannelGroup(new NotificationChannelGroup("probe_group_b","Probe group B"));
                    List<NotificationChannel> channels=new ArrayList<>();for(int i=0;i<24;i++)channels.add(channel(i));
                    manager.createNotificationChannels(channels);
                }
            }else if(operation.equals("post")) {
                manager.cancelAll();
                manager.notify(7001,notification(context,legacy));
                SystemClock.sleep(300);
            }else if(operation.equals("cancel"))manager.cancelAll();
            else if(operation.equals("delete")&&!legacy)manager.deleteNotificationChannel(CHANNEL);
            else if(operation.equals("recreate")&&!legacy)manager.createNotificationChannel(channel(0));
            else if(!operation.equals("state"))throw new IllegalArgumentException("Unknown fixture operation");
            result.putString("state",snapshot(manager,legacy).toString());result.putBoolean("passed",true);finish(0,result);
        }catch(Exception failure){result.putString("failure",failure.getClass().getSimpleName()+": "+failure.getMessage());finish(1,result);}
    }
}
