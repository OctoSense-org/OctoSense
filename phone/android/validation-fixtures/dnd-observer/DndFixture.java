package dev.makepad.octosense.dndfixture;

import android.app.Instrumentation;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.os.Bundle;
import android.os.SystemClock;
import android.service.notification.NotificationListenerService;
import android.service.notification.StatusBarNotification;
import org.json.JSONArray;
import org.json.JSONObject;

/** Read-only native policy/rule observer and independent synthetic ranking probe. */
public final class DndFixture extends Instrumentation {
    private static final String CHANNEL="octosense_dnd_validation";
    private static final String[] CATEGORIES={Notification.CATEGORY_ALARM,
            Notification.CATEGORY_EVENT,Notification.CATEGORY_REMINDER,
            Notification.CATEGORY_MESSAGE,Notification.CATEGORY_CALL,Notification.CATEGORY_STATUS};
    private String operation="state";
    @Override public void onCreate(Bundle args){
        super.onCreate(args);if(args!=null)operation=args.getString("operation","state");start();
    }
    private JSONObject state(NotificationManager manager)throws Exception{
        // Only a disposable instrumentation process adopts this permission for
        // observations. Neither Home nor the synthetic publisher gets DND access.
        getUiAutomation().adoptShellPermissionIdentity("android.permission.MANAGE_NOTIFICATIONS");
        try{
            return DndNativeSnapshot.read(manager);
        }finally{getUiAutomation().dropShellPermissionIdentity();}
    }
    private void setup(NotificationManager manager){
        NotificationChannel channel=new NotificationChannel(CHANNEL,"Synthetic DND validation",NotificationManager.IMPORTANCE_DEFAULT);
        channel.setSound(null,null);channel.enableVibration(false);channel.setShowBadge(false);
        manager.createNotificationChannel(channel);
    }
    private JSONArray post(NotificationManager manager)throws Exception{
        setup(manager);
        if(!manager.areNotificationsEnabled())throw new AssertionError("Synthetic publisher notifications are not enabled");
        NotificationChannel channel=manager.getNotificationChannel(CHANNEL);
        if(channel==null||channel.canBypassDnd())throw new AssertionError("Synthetic channel must not bypass DND");
        manager.cancelAll();
        int base=(int)(SystemClock.elapsedRealtime()%1_000_000)*10;
        for(int i=0;i<CATEGORIES.length;i++)manager.notify(base+i,new Notification.Builder(getTargetContext(),CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_info).setContentTitle("OctoSense synthetic DND notice")
                .setContentText("Disposable ranking validation").setCategory(CATEGORIES[i]).setOnlyAlertOnce(true).build());
        long deadline=SystemClock.uptimeMillis()+15_000;
        do{
            DndRankingListener listener=DndRankingListener.connected;
            if(listener!=null){
                JSONArray rows=new JSONArray();NotificationListenerService.RankingMap map=listener.getCurrentRanking();
                StatusBarNotification[] notices=listener.getActiveNotifications();
                if(map!=null&&notices!=null)for(StatusBarNotification notice:notices){
                    if(!getTargetContext().getPackageName().equals(notice.getPackageName())||notice.getId()<base||notice.getId()>=base+CATEGORIES.length)continue;
                    NotificationListenerService.Ranking ranking=new NotificationListenerService.Ranking();
                    if(map.getRanking(notice.getKey(),ranking))rows.put(new JSONObject().put("category",CATEGORIES[notice.getId()-base])
                            .put("matches_filter",ranking.matchesInterruptionFilter()).put("importance",ranking.getImportance())
                            .put("suspended",ranking.isSuspended()).put("visual_effects",ranking.getSuppressedVisualEffects())
                            .put("bypass_dnd",ranking.getChannel()!=null&&ranking.getChannel().canBypassDnd()));
                }
                if(rows.length()==CATEGORIES.length)return rows;
            }
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Synthetic ranking observations unavailable");
    }
    @Override public void onStart(){
        Bundle result=new Bundle();
        try{
            NotificationManager manager=getTargetContext().getSystemService(NotificationManager.class);
            if(operation.equals("setup"))setup(manager);
            else if(operation.equals("post")||operation.equals("post_ranking"))result.putString("ranking",post(manager).toString());
            else if(operation.equals("cancel"))manager.cancelAll();
            else if(!operation.equals("state"))throw new IllegalArgumentException("Unknown fixture operation");
            // A ranking-only invocation may run under another instrumentation's
            // UiAutomation. It must not take that connection or shell identity.
            if(!operation.equals("post_ranking"))result.putString("state",state(manager).toString());
            result.putBoolean("passed",true);finish(0,result);
        }catch(Throwable failure){result.putString("failure",failure.getClass().getSimpleName()+": "+failure.getMessage());finish(1,result);}
    }
}
