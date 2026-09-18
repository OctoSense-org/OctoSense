package dev.makepad.octosense.validation;

import android.app.Instrumentation;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.RemoteInput;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.ServiceConnection;
import android.os.Bundle;
import android.os.IBinder;
import android.os.SystemClock;
import dev.makepad.octosense.contracts.ISystemBridge;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;
import dev.makepad.octosense.contracts.Protocol;
import java.util.ArrayList;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.function.Predicate;

/** Opt-in real listener test. Grants belong to the host runner; only our
 * uniquely tagged fixture is retained from incoming notification snapshots. */
final class NotificationRoundtrip {
    private final Instrumentation instrumentation;
    private final String session=UUID.randomUUID().toString();
    private final String title="OctoSense fixture "+session;
    private final ArrayBlockingQueue<Bundle> states=new ArrayBlockingQueue<>(64);
    private final ArrayBlockingQueue<Bundle> results=new ArrayBlockingQueue<>(64);
    private final CountDownLatch connected=new CountDownLatch(1);
    private volatile ISystemBridge bridge;
    private volatile boolean overflow;
    private int checks;
    private String epoch;
    private long revision=-1;

    NotificationRoundtrip(Instrumentation instrumentation) {this.instrumentation=instrumentation;}
    private void check(boolean value,String reason) {if(!value) throw new AssertionError(reason);checks++;}
    private void phase(String value) {Bundle status=new Bundle();status.putString("fixture_phase",value);instrumentation.sendStatus(1,status);}
    private final ISystemBridgeCallback.Stub callback=new ISystemBridgeCallback.Stub() {
        @Override public void onSnapshot(String e,long r,Bundle incoming) {
            Bundle state=new Bundle();state.putString("epoch",e);state.putLong("revision",r);
            Bundle caps=incoming.getBundle("capabilities");
            Bundle notifications=caps==null?null:caps.getBundle("notifications");
            state.putBoolean("accessible",notifications!=null&&notifications.getBoolean("accessible"));
            ArrayList<Bundle> all=incoming.getParcelableArrayList("notifications");
            state.putBoolean("empty",all==null||all.isEmpty());
            // No other notification content, identity or handles enter the queue or report.
            if(all!=null) for(Bundle notice:all) {
                if(Protocol.HOME_PACKAGE.equals(notice.getString("package"))&&title.equals(notice.getString("title")))
                    state.putBundle("fixture",new Bundle(notice));
            }
            overflow|=!states.offer(state);
        }
        @Override public void onDelta(String e,long r,Bundle delta) {overflow=true;}
        @Override public void onResyncRequired(String e) {overflow=true;}
        @Override public void onCommandResult(String s,long id,int status,String reason) {
            if(!session.equals(s)) {overflow=true;return;}
            Bundle result=new Bundle();result.putLong("id",id);result.putInt("status",status);
            overflow|=!results.offer(result);
        }
    };
    private Bundle await(Predicate<Bundle> predicate,String failure) throws Exception {
        long deadline=SystemClock.elapsedRealtime()+30_000;
        while(SystemClock.elapsedRealtime()<deadline) {
            Bundle value=states.poll(250,TimeUnit.MILLISECONDS);
            check(!overflow,"fixture_callback_overflow");
            if(value==null) {bridge.requestSnapshot(session);continue;}
            String e=value.getString("epoch");long r=value.getLong("revision");
            check(e!=null&&!e.isEmpty(),"fixture_epoch_missing");
            check(epoch==null||!epoch.equals(e)||r>=revision,"fixture_revision_regressed");
            epoch=e;revision=r;
            if(predicate.test(value)) return value;
        }
        throw new AssertionError(failure);
    }
    private void completion(long id,int status,boolean accepted) throws Exception {
        boolean acceptance=false;long deadline=SystemClock.elapsedRealtime()+10_000;
        while(SystemClock.elapsedRealtime()<deadline) {
            Bundle value=results.poll(250,TimeUnit.MILLISECONDS);if(value==null) continue;
            check(!overflow,"fixture_callback_overflow");
            check(value.getLong("id")==id,"fixture_uncorrelated_result");
            if(value.getInt("status")==Protocol.ACCEPTED) {acceptance=true;continue;}
            check(value.getInt("status")==status,"fixture_unexpected_status_"+value.getInt("status"));
            check(!accepted||acceptance,"fixture_completion_without_acceptance");return;
        }
        throw new AssertionError("fixture_completion_timeout");
    }
    private String replyHandle(Bundle notice) {
        ArrayList<Bundle> actions=notice.getParcelableArrayList("actions");
        if(actions!=null) for(Bundle action:actions) if(action.getBoolean("reply")) return action.getString("handle");
        throw new AssertionError("fixture_reply_action_missing");
    }
    void run(Context context,Bundle report) throws Exception {
        NotificationManager manager=context.getSystemService(NotificationManager.class);
        String channel="octosense-validation-"+session;
        ArrayBlockingQueue<Intent> received=new ArrayBlockingQueue<>(8);
        BroadcastReceiver receiver=new BroadcastReceiver() {
            @Override public void onReceive(Context c,Intent intent) {received.offer(intent);}
        };
        String action=Protocol.HOME_PACKAGE+".FIXTURE_REPLY."+session;
        context.registerReceiver(receiver,new IntentFilter(action),Context.RECEIVER_NOT_EXPORTED);
        PendingIntent target=PendingIntent.getBroadcast(context,0,new Intent(action).setPackage(context.getPackageName()),
                PendingIntent.FLAG_MUTABLE|PendingIntent.FLAG_CANCEL_CURRENT);
        ServiceConnection connection=new ServiceConnection() {
            public void onServiceConnected(ComponentName name,IBinder binder) {bridge=ISystemBridge.Stub.asInterface(binder);connected.countDown();}
            public void onServiceDisconnected(ComponentName name) {bridge=null;}
        };
        boolean bound=false;
        try {
            check(manager.areNotificationsEnabled(),"fixture_post_permission_missing");
            NotificationChannel definition=new NotificationChannel(channel,"OctoSense validation",NotificationManager.IMPORTANCE_LOW);
            definition.setSound(null,null);manager.createNotificationChannel(definition);
            bound=context.bindService(new Intent().setComponent(new ComponentName(Protocol.BRIDGE_PACKAGE,
                    Protocol.BRIDGE_PACKAGE+".SystemBridgeService")),connection,Context.BIND_AUTO_CREATE);
            check(bound&&connected.await(10,TimeUnit.SECONDS)&&bridge!=null,"fixture_bridge_bind_timeout");
            bridge.subscribe(session,callback);
            await(v -> !v.getBoolean("accessible")&&v.getBoolean("empty"),"fixture_initial_access_not_revoked");
            phase("ready_for_grant");
            await(v -> v.getBoolean("accessible"),"fixture_listener_did_not_connect");
            phase("listener_connected");
            RemoteInput input=new RemoteInput.Builder("message").setLabel("Reply").build();
            Notification.Builder builder=new Notification.Builder(context,channel)
                    .setSmallIcon(android.R.drawable.ic_dialog_info).setContentTitle(title).setContentText("fixture first")
                    .setOnlyAlertOnce(true).setContentIntent(target)
                    .addAction(new Notification.Action.Builder(android.R.drawable.ic_menu_send,"Fixture reply",target).addRemoteInput(input).build());
            manager.notify(session,41,builder.build());
            Bundle first=await(v -> v.getBundle("fixture")!=null&&"fixture first".equals(v.getBundle("fixture").getString("text")),
                    "fixture_not_delivered").getBundle("fixture");
            String handle=replyHandle(first),reply="Fixture reply: 你好 👋\nsecond line";
            phase("fixture_delivered");
            bridge.invokeNotificationAction(session,1,handle,reply);completion(1,Protocol.COMPLETED,true);
            Intent delivered=received.poll(5,TimeUnit.SECONDS);check(delivered!=null,"fixture_reply_not_received");
            Bundle data=RemoteInput.getResultsFromIntent(delivered);
            check(data!=null&&reply.contentEquals(data.getCharSequence("message")),"fixture_reply_content_changed");
            check(RemoteInput.getResultsSource(delivered)==RemoteInput.SOURCE_FREE_FORM_INPUT,"fixture_reply_source_missing");
            phase("fixture_reply_received");
            bridge.invokeNotificationAction(session,1,handle,"duplicate must not be sent");completion(1,Protocol.COMPLETED,false);
            check(received.poll(300,TimeUnit.MILLISECONDS)==null,"fixture_duplicate_reply_sent");
            manager.notify(session,41,builder.setContentText("fixture updated").build());
            Bundle updated=await(v -> v.getBundle("fixture")!=null&&"fixture updated".equals(v.getBundle("fixture").getString("text")),
                    "fixture_update_not_delivered").getBundle("fixture");
            check(!handle.equals(replyHandle(updated)),"fixture_updated_action_handle_reused");
            bridge.invokeNotificationAction(session,2,handle,reply);completion(2,Protocol.EXPIRED_HANDLE,true);
            check(received.poll(300,TimeUnit.MILLISECONDS)==null,"fixture_expired_reply_sent");
            bridge.dismissNotification(session,3,updated.getString("handle"));completion(3,Protocol.COMPLETED,true);
            await(v -> v.getBundle("fixture")==null,"fixture_dismiss_not_observed");
            boolean ownActive=false;
            for(android.service.notification.StatusBarNotification notice:manager.getActiveNotifications())
                if(session.equals(notice.getTag())&&notice.getId()==41) ownActive=true;
            check(!ownActive,"fixture_still_active_in_notification_manager");
            phase("fixture_dismissed");
            bridge.invokeNotificationAction(session,4,replyHandle(updated),reply);completion(4,Protocol.EXPIRED_HANDLE,true);
            manager.notify(session,41,builder.setContentText("fixture revoke").build());
            Bundle revoke=await(v -> v.getBundle("fixture")!=null&&"fixture revoke".equals(v.getBundle("fixture").getString("text")),
                    "fixture_repost_not_delivered").getBundle("fixture");
            phase("ready_for_revoke");
            await(v -> !v.getBoolean("accessible")&&v.getBoolean("empty"),"fixture_revocation_not_observed");
            bridge.invokeNotificationAction(session,5,replyHandle(revoke),reply);completion(5,Protocol.EXPIRED_HANDLE,true);
            bridge.dismissNotification(session,6,revoke.getString("handle"));completion(6,Protocol.PREREQUISITE_MISSING,true);
            check(received.poll(300,TimeUnit.MILLISECONDS)==null,"fixture_action_dispatched_after_revocation");
            report.putBoolean("real_listener_delivery_update_dismiss_reply_verified",true);
            report.putBoolean("reply_deduplication_and_handle_expiry_verified",true);
            report.putBoolean("revocation_clears_state_and_rejects_actions",true);
            report.putBoolean("external_message_sent",false);
        } finally {
            manager.cancel(session,41);target.cancel();manager.deleteNotificationChannel(channel);context.unregisterReceiver(receiver);
            try {if(bridge!=null) bridge.unsubscribe(session);}
            finally {if(bound) context.unbindService(connection);}
            states.clear();results.clear();
            report.putInt("checks",checks);report.putString("retained_content","fixture_only");
        }
    }
}
