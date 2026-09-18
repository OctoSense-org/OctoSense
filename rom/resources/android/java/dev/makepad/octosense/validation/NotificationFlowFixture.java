package dev.makepad.octosense.validation;

import android.app.Activity;
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
import android.graphics.drawable.Icon;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.EditText;
import dev.makepad.android.MakepadActivity;
import dev.makepad.octosense.contracts.Protocol;
import java.util.ArrayList;
import java.util.UUID;
import org.json.JSONObject;

/** Real notification source/receiver. Home owns the sole bridge subscription,
 * renderer actions, editor and reply dispatch throughout this fixture. */
public final class NotificationFlowFixture {
    private static volatile NotificationFlowFixture active;
    private final String owner=UUID.randomUUID().toString();
    private final String title="OctoSense phone test · "+owner.substring(0,6);
    private final String channel="octosense-flow-"+owner;
    private final Context context;
    private final NotificationManager manager;
    private PendingIntent target;
    private volatile boolean accessible,observed,finished,delivered,removed,revoked;
    private volatile int received;
    private volatile String reply="";
    private NotificationFlowFixture(Context context) {this.context=context;manager=context.getSystemService(NotificationManager.class);}
    public static boolean active() {return active!=null;}

    /** Strip unrelated content before it enters Home's worker/JNI/render model. */
    public static Bundle filter(Bundle incoming) {
        NotificationFlowFixture fixture=active;if(fixture==null) return incoming;
        Bundle state=new Bundle(incoming);
        ArrayList<Bundle> own=new ArrayList<>(),all=incoming.getParcelableArrayList("notifications");
        if(all!=null) for(Bundle notice:all)
            if(Protocol.HOME_PACKAGE.equals(notice.getString("package"))&&fixture.title.equals(notice.getString("title"))) own.add(new Bundle(notice));
        state.putParcelableArrayList("notifications",own);
        Bundle caps=state.getBundle("capabilities"),notifications=caps==null?null:caps.getBundle("notifications");
        boolean allowed=notifications!=null&&notifications.getBoolean("accessible");
        if(fixture.accessible&&!allowed&&own.isEmpty()) fixture.revoked=true;
        fixture.accessible=allowed;fixture.observed=true;
        if(!own.isEmpty()) fixture.delivered=true;
        if(fixture.delivered&&own.isEmpty()) fixture.removed=true;
        return state;
    }
    private void post(String text) {
        manager.notify(owner,51,new Notification.Builder(context,channel)
                .setSmallIcon(android.R.drawable.ic_dialog_info).setContentTitle(title).setContentText(text)
                .setOnlyAlertOnce(true)
                .addAction(new Notification.Action.Builder(Icon.createWithResource(context,android.R.drawable.ic_menu_send),"Reply",target)
                    .addRemoteInput(new RemoteInput.Builder("message").setLabel("Reply to phone test").build()).build()).build());
    }
    private boolean posted() {
        for(android.service.notification.StatusBarNotification notice:manager.getActiveNotifications())
            if(owner.equals(notice.getTag())&&notice.getId()==51) return true;
        return false;
    }
    public static JSONObject state() throws Exception {
        NotificationFlowFixture fixture=active;
        if(fixture==null) return new JSONObject().put("active",false);
        return new JSONObject().put("active",true).put("title",fixture.title).put("accessible",fixture.accessible)
                .put("observed",fixture.observed).put("received",fixture.received).put("reply",fixture.reply)
                .put("posted",fixture.posted()).put("removed",fixture.removed).put("revoked",fixture.revoked);
    }
    public static void command(android.net.Uri route,MakepadActivity activity) {
        NotificationFlowFixture fixture=active;if(fixture==null) throw new IllegalStateException("Notification flow is not active");
        switch(route.getPath()) {
            case "/flow/post": fixture.post("Live fixture notification. Swipe left, then tap Reply.");break;
            case "/flow/update": fixture.post("Notification updated. An older draft must be invalidated.");break;
            case "/flow/text": {
                if(!(activity.getCurrentFocus() instanceof EditText)) throw new IllegalStateException("Reply editor is not focused");
                String value=route.getQueryParameter("value");
                if(value==null||value.length()>2000) throw new IllegalArgumentException("Invalid fixture text");
                InputConnection input=((EditText)activity.getCurrentFocus()).onCreateInputConnection(new EditorInfo());
                if(input==null||!input.commitText(value,1)) throw new IllegalStateException("Reply input rejected");break;
            }
            case "/flow/back": activity.onBackPressed();break;
            case "/flow/finish": fixture.finished=true;break;
            default:throw new IllegalArgumentException("Unknown notification flow route");
        }
    }
    public static void run(Instrumentation instrumentation,Bundle report) throws Exception {
        Context context=instrumentation.getTargetContext();
        NotificationFlowFixture fixture=new NotificationFlowFixture(context);
        MakepadActivity owned=null;
        String action=Protocol.HOME_PACKAGE+".FLOW_REPLY."+fixture.owner;
        BroadcastReceiver receiver=new BroadcastReceiver() {
            @Override public void onReceive(Context c,Intent intent) {
                Bundle result=RemoteInput.getResultsFromIntent(intent);
                fixture.reply=result==null?"":String.valueOf(result.getCharSequence("message"));
                fixture.received++;fixture.post("Reply received: "+fixture.reply);
            }
        };
        context.registerReceiver(receiver,new IntentFilter(action),Context.RECEIVER_NOT_EXPORTED);
        fixture.target=PendingIntent.getBroadcast(context,0,new Intent(action).setPackage(context.getPackageName()),
                PendingIntent.FLAG_MUTABLE|PendingIntent.FLAG_CANCEL_CURRENT);
        active=fixture;
        try {
            if(!fixture.manager.areNotificationsEnabled()) throw new AssertionError("fixture_post_permission_missing");
            NotificationChannel channel=new NotificationChannel(fixture.channel,"OctoSense phone test",NotificationManager.IMPORTANCE_LOW);
            channel.setSound(null,null);fixture.manager.createNotificationChannel(channel);
            context.startActivity(new Intent(Intent.ACTION_MAIN).setComponent(new ComponentName(context.getPackageName(),context.getPackageName()+".MakepadApp"))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("--remote",true));
            long deadline=SystemClock.elapsedRealtime()+20_000;
            while((ValidationRemote.ownedActivity()==null||!fixture.observed)&&SystemClock.elapsedRealtime()<deadline) SystemClock.sleep(50);
            owned=ValidationRemote.ownedActivity();
            if(owned==null||!fixture.observed||fixture.accessible) throw new AssertionError("fixture_initial_home_state_invalid");
            Bundle phase=new Bundle();phase.putString("fixture_phase","ready_for_grant");instrumentation.sendStatus(1,phase);
            deadline=SystemClock.elapsedRealtime()+300_000;
            while(!fixture.finished&&SystemClock.elapsedRealtime()<deadline) SystemClock.sleep(100);
            if(!fixture.finished||fixture.received!=1||fixture.reply.isEmpty()||!fixture.delivered||!fixture.removed||!fixture.revoked||fixture.accessible)
                throw new AssertionError("fixture_flow_not_completed");
            report.putInt("replies_received",fixture.received);report.putBoolean("real_home_subscription_used",true);
            report.putBoolean("delivery_removal_revocation_observed",true);report.putBoolean("external_message_sent",false);
            report.putInt("owned_pid",android.os.Process.myPid());report.putString("retained_content","fixture_only");
        } finally {
            fixture.manager.cancel(fixture.owner,51);fixture.manager.deleteNotificationChannel(fixture.channel);
            fixture.target.cancel();context.unregisterReceiver(receiver);
            if(owned!=null&&!owned.isFinishing()&&!owned.isDestroyed()) {Activity activity=owned;instrumentation.runOnMainSync(activity::finishAndRemoveTask);}
            // Keep filtering until the owned Home has left the foreground.
            active=null;
        }
    }
}
