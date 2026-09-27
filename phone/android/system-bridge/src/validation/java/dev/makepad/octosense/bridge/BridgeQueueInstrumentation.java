package dev.makepad.octosense.bridge;

import android.app.Activity;
import android.app.Instrumentation;
import android.app.Notification;
import android.os.Bundle;
import android.os.Process;
import android.service.notification.NotificationListenerService;
import android.service.notification.StatusBarNotification;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;
import dev.makepad.octosense.contracts.Protocol;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/** Validation only: fills a private bridge's real queue with synthetic notices.
 * No notification access, posted notification, root request or device setter. */
public final class BridgeQueueInstrumentation extends Instrumentation {
    private BridgeState state;
    private ThreadPoolExecutor worker;
    private Method offer,snapshot;
    private int checks;
    private CountDownLatch heldWorker;
    private int callbackProcessPid=-1;
    private boolean callbackProcessExited;
    private static final class Subscriber extends ISystemBridgeCallback.Stub {
        final CountDownLatch ready=new CountDownLatch(1);
        final AtomicInteger snapshots=new AtomicInteger();
        @Override public void onSnapshot(String epoch,long revision,Bundle value) {snapshots.incrementAndGet();ready.countDown();}
        @Override public void onDelta(String epoch,long revision,Bundle value) {}
        @Override public void onResyncRequired(String epoch) {}
        @Override public void onCommandResult(String session,long id,int status,String reason) {}
    }

    private static final class FixtureListener extends NotificationListenerService {
        volatile StatusBarNotification[] active=new StatusBarNotification[0];
        volatile boolean denied;
        Runnable duringRead;
        @Override public StatusBarNotification[] getActiveNotifications() {
            if(denied) throw new SecurityException("fixture_access_revoked");
            StatusBarNotification[] result=active.clone();
            Runnable callback=duringRead;duringRead=null;
            if(callback!=null) callback.run();
            return result;
        }
    }
    @Override public void onCreate(Bundle args) { super.onCreate(args);start(); }
    private void check(boolean value,String reason) {
        if(!value) throw new AssertionError(reason);
        checks++;
    }
    private boolean enqueue(Runnable task) throws Exception { return (Boolean)offer.invoke(state,task); }
    private void idle() throws Exception {
        worker.submit(() -> {}).get(10,TimeUnit.SECONDS);
    }
    private Bundle snapshot() throws Exception {
        return worker.submit(() -> (Bundle)snapshot.invoke(state)).get(10,TimeUnit.SECONDS);
    }
    private void hold(boolean fill) throws Exception {
        idle();
        CountDownLatch entered=new CountDownLatch(1),release=new CountDownLatch(1);
        heldWorker=release;
        check(enqueue(() -> {
            entered.countDown();
            try {
                if(!release.await(10,TimeUnit.SECONDS)) throw new AssertionError("worker_hold_timeout");
            } catch(InterruptedException e) { Thread.currentThread().interrupt(); }
        }),"worker_hold_rejected");
        check(entered.await(10,TimeUnit.SECONDS),"worker_hold_not_entered");
        if(fill) {
            for(int i=0;i<64;i++) check(enqueue(() -> {}),"queue_filled_early");
            check(!enqueue(() -> {}),"queue_not_bounded");
        }
    }
    private void release() throws Exception {
        heldWorker.countDown();heldWorker=null;
        // A full queue cannot accept a barrier yet. Wait on its real tasks.
        long deadline=System.nanoTime()+TimeUnit.SECONDS.toNanos(10);
        while(!worker.getQueue().isEmpty() && System.nanoTime()<deadline) Thread.sleep(10);
        idle();
        // Include the single main-thread retry when the first refresh was rejected.
        runOnMainSync(() -> {});
        Thread.sleep(100);
        idle();
    }
    private StatusBarNotification notice(int id,String title) {
        Notification value=new Notification.Builder(getTargetContext(),"queue-fixture")
                .setSmallIcon(android.R.drawable.ic_menu_info_details).setContentTitle(title).build();
        return new StatusBarNotification(getTargetContext().getPackageName(),getTargetContext().getPackageName(),
                id,"queue-fixture",Process.myUid(),Process.myPid(),0,value,Process.myUserHandle(),1L);
    }
    private void expect(boolean connected,String... titles) throws Exception {
        Bundle value=snapshot();
        check(value.getBundle("capabilities").getBundle("notifications").getBoolean("accessible")==connected,
                "listener_capability_stale");
        ArrayList<Bundle> notices=value.getParcelableArrayList("notifications");
        check(notices!=null && notices.size()==titles.length,"notification_count_stale");
        for(int i=0;i<titles.length;i++) check(titles[i].equals(notices.get(i).getString("title")),
                "notification_content_stale: expected="+titles[i]+", actual="+notices.get(i).getString("title"));
    }
    private void runChecks() throws Exception {
        state=new BridgeState(getTargetContext(),false);
        Field field=BridgeState.class.getDeclaredField("worker");field.setAccessible(true);
        worker=(ThreadPoolExecutor)field.get(state);
        offer=BridgeState.class.getDeclaredMethod("offer",Runnable.class);offer.setAccessible(true);
        snapshot=BridgeState.class.getDeclaredMethod("snapshot");snapshot.setAccessible(true);
        idle();expect(false);
        FixtureListener first=new FixtureListener(),second=new FixtureListener();
        first.active=new StatusBarNotification[]{notice(1,"first")};
        second.active=new StatusBarNotification[]{notice(1,"replacement")};

        hold(true);state.listenerConnected(first);release();expect(true,"first");
        hold(true);state.listenerDisconnected(first);release();expect(false);

        // Intermediate connections coalesce; delayed old-service callbacks cannot
        // disconnect or delete the replacement service's same-key notification.
        hold(true);
        for(int i=0;i<1000;i++) {state.listenerConnected(first);state.listenerDisconnected(first);}
        state.listenerConnected(second);
        state.listenerDisconnected(first);
        state.notificationPosted(first,notice(2,"stale post"));
        state.notificationRemoved(first,first.active[0]);
        check(worker.getQueue().size()==64,"lifecycle_burst_grew_worker_queue");
        release();expect(true,"replacement");

        // Same Java service instance, new Android connection: queued old events
        // belong to the retired connection even when their notification key matches.
        hold(false);
        state.notificationRemoved(second,second.active[0]);
        state.notificationPosted(second,notice(2,"retired connection"));
        state.listenerDisconnected(second);
        second.active=new StatusBarNotification[]{notice(1,"reconnected")};
        state.listenerConnected(second);
        release();expect(true,"reconnected");

        // Ordinary events still apply in-order within the current connection.
        state.notificationPosted(second,notice(2,"current post"));idle();expect(true,"reconnected","current post");
        state.notificationRemoved(second,notice(2,"current post"));idle();expect(true,"reconnected");

        // Visual identity survives an update, but old authority must expire.
        Bundle before=snapshot().<Bundle>getParcelableArrayList("notifications").get(0);
        state.notificationPosted(second,notice(1,"updated")); idle();
        Bundle updated=snapshot().<Bundle>getParcelableArrayList("notifications").get(0);
        check(before.getString("identity").equals(updated.getString("identity")),"update_replaced_visual_identity");
        check(!before.getString("handle").equals(updated.getString("handle")),"update_kept_old_authority");
        try {state.dismiss(before.getString("handle"));throw new AssertionError("old_dismiss_handle_accepted");}
        catch(BridgeState.Failure expired) {check(expired.status==Protocol.EXPIRED_HANDLE,"old_dismiss_handle_not_expired");}
        state.notificationRemoved(second,notice(1,"updated")); idle();
        state.notificationPosted(second,notice(1,"reposted")); idle();
        Bundle reposted=snapshot().<Bundle>getParcelableArrayList("notifications").get(0);
        check(!updated.getString("identity").equals(reposted.getString("identity")),"repost_reused_visual_identity");

        second.denied=true;
        hold(true);state.listenerConnected(second);release();expect(false);
        second.denied=false;
        state.listenerConnected(second);idle();expect(true,"reconnected");

        // Recovery's authoritative snapshot supersedes events already queued
        // from this same connection, not only events from a retired connection.
        hold(false);
        for(int i=0;i<64;i++) state.notificationPosted(second,notice(1,"queued old title"));
        check(worker.getQueue().remainingCapacity()==0,"notification_backlog_not_full");
        second.active=new StatusBarNotification[]{notice(1,"latest title")};
        state.notificationPosted(second,second.active[0]);
        release();expect(true,"latest title");

        hold(false);
        for(int i=0;i<64;i++) state.notificationRemoved(second,notice(1,"old removal"));
        second.active=new StatusBarNotification[]{notice(1,"reposted title")};
        state.notificationPosted(second,second.active[0]);
        release();expect(true,"reposted title");

        hold(false);
        for(int i=0;i<64;i++) state.notificationPosted(second,notice(1,"removed title"));
        second.active=new StatusBarNotification[0];
        state.notificationRemoved(second,notice(1,"removed title"));
        release();expect(true);

        // A callback received after a query starts must still apply when the
        // query returns its earlier snapshot. Capture the boundary before reading.
        second.active=new StatusBarNotification[]{notice(1,"before query")};
        second.duringRead=() -> {
            second.active=new StatusBarNotification[]{notice(1,"during query")};
            state.notificationPosted(second,second.active[0]);
        };
        state.listenerConnected(second);idle();idle();expect(true,"during query");

        // Command rejection remains explicit and must never execute its action.
        ArrayBlockingQueue<Integer> results=new ArrayBlockingQueue<>(8);
        CountDownLatch subscribed=new CountDownLatch(1);
        String session="queue-validation-session";
        state.subscribe(Process.myUid(),session,new ISystemBridgeCallback.Stub() {
            @Override public void onSnapshot(String epoch,long revision,Bundle value) {subscribed.countDown();}
            @Override public void onDelta(String epoch,long revision,Bundle value) {}
            @Override public void onResyncRequired(String epoch) {}
            @Override public void onCommandResult(String session,long id,int status,String reason) {results.add(status);}
        });
        check(subscribed.await(10,TimeUnit.SECONDS),"subscription_missing");
        AtomicInteger applied=new AtomicInteger();
        hold(true);state.command(Process.myUid(),session,1,"fixture",applied::incrementAndGet);
        check(results.poll(10,TimeUnit.SECONDS)==Protocol.QUEUE_FULL,"queue_full_result_missing");
        release();check(applied.get()==0,"rejected_command_executed");
        state.command(Process.myUid(),session,2,"fixture",applied::incrementAndGet);
        check(results.poll(10,TimeUnit.SECONDS)==Protocol.ACCEPTED,"accepted_result_missing");
        check(results.poll(10,TimeUnit.SECONDS)==Protocol.COMPLETED,"completion_missing");
        idle();check(applied.get()==1,"command_not_applied_once");
        state.unsubscribe(Process.myUid(),session);idle();
        state.listenerDisconnected(second);idle();expect(false);
        subscriptionChecks();
    }
    private void requireUnsubscribed(String session) {
        try {
            state.command(Process.myUid(),session,99,"fixture",() -> {});
            throw new AssertionError("retired_subscription_accepted_command");
        } catch(SecurityException expected) {checks++;}
    }
    private android.os.IBinder.DeathRecipient death() throws Exception {
        Field field=BridgeState.class.getDeclaredField("subscribers");field.setAccessible(true);
        Object subscription=((java.util.Map<?,?>)field.get(state)).get(Process.myUid());
        Field recipient=subscription.getClass().getDeclaredField("death");recipient.setAccessible(true);
        return (android.os.IBinder.DeathRecipient)recipient.get(subscription);
    }
    private void subscriptionChecks() throws Exception {
        int uid=Process.myUid();
        String firstSession="subscription-first",secondSession="subscription-second";
        Subscriber first=new Subscriber(),second=new Subscriber();
        hold(true);state.subscribe(uid,firstSession,first);release();
        check(first.ready.await(2,TimeUnit.SECONDS),"saturated_subscription_lost_initial_snapshot");
        hold(true);state.unsubscribe(uid,firstSession);release();requireUnsubscribed(firstSession);
        int retiredSnapshots=first.snapshots.get();state.permissionsChanged();idle();
        check(first.snapshots.get()==retiredSnapshots,"unsubscribed_callback_still_receives_snapshots");

        first=new Subscriber();
        hold(true);
        state.subscribe(uid,firstSession,first);state.subscribe(uid,secondSession,second);
        state.unsubscribe(uid,firstSession);
        release();
        check(second.ready.await(2,TimeUnit.SECONDS),"replacement_subscription_lost");
        check(first.snapshots.get()==0,"superseded_pending_subscription_was_published");
        requireUnsubscribed(firstSession);

        android.os.IBinder.DeathRecipient oldDeath=death();
        Subscriber replacement=new Subscriber();
        hold(true);state.subscribe(uid,firstSession,replacement);oldDeath.binderDied();release();
        check(replacement.ready.await(2,TimeUnit.SECONDS),"old_binder_death_removed_replacement");
        hold(true);death().binderDied();release();requireUnsubscribed(firstSession);
        remoteDeathCheck();
    }
    private void remoteDeathCheck() throws Exception {
        CountDownLatch connected=new CountDownLatch(1),died=new CountDownLatch(1);
        java.util.concurrent.atomic.AtomicReference<android.os.IBinder> target=new java.util.concurrent.atomic.AtomicReference<>();
        android.content.ServiceConnection connection=new android.content.ServiceConnection() {
            @Override public void onServiceConnected(android.content.ComponentName name,android.os.IBinder binder) {
                target.set(binder);connected.countDown();
            }
            @Override public void onServiceDisconnected(android.content.ComponentName name) {}
        };
        boolean bound=false;
        try {
            bound=getTargetContext().bindService(new android.content.Intent(getTargetContext(),QueueCallbackService.class),
                    connection,android.content.Context.BIND_AUTO_CREATE);
            check(bound&&connected.await(10,TimeUnit.SECONDS),"remote_callback_bind_failed");
            android.os.IBinder binder=target.get();
            android.os.Parcel request=android.os.Parcel.obtain(),reply=android.os.Parcel.obtain();
            try {
                request.writeInterfaceToken(QueueCallbackService.CONTROL);
                check(binder.transact(QueueCallbackService.IDENTITY,request,reply,0),"remote_identity_rejected");
                reply.readException();callbackProcessPid=reply.readInt();
                check(callbackProcessPid>0&&callbackProcessPid!=Process.myPid(),"callback_not_in_separate_process");
            } finally {request.recycle();reply.recycle();}
            binder.linkToDeath(died::countDown,0);
            String session="remote-callback-session";
            state.subscribe(Process.myUid(),session,ISystemBridgeCallback.Stub.asInterface(binder));idle();
            AtomicInteger applied=new AtomicInteger();
            state.command(Process.myUid(),session,1,"fixture",applied::incrementAndGet);idle();
            check(applied.get()==1,"remote_subscriber_not_registered");
            hold(true);
            request=android.os.Parcel.obtain();
            try {
                request.writeInterfaceToken(QueueCallbackService.CONTROL);
                check(binder.transact(QueueCallbackService.EXIT,request,null,android.os.IBinder.FLAG_ONEWAY),"remote_exit_rejected");
            } finally {request.recycle();}
            check(died.await(10,TimeUnit.SECONDS),"remote_binder_death_missing");
            getTargetContext().unbindService(connection);bound=false;
            release();requireUnsubscribed(session);
            callbackProcessExited=!binder.isBinderAlive();
            check(callbackProcessExited,"remote_callback_binder_still_alive");
        } finally {
            if(bound) getTargetContext().unbindService(connection);
        }
    }
    @Override public void onStart() {
        Bundle report=new Bundle();
        try {runChecks();report.putString("result","pass");}
        catch(Throwable e) {report.putString("result","fail");report.putString("failure",e.toString());}
        finally {
            if(heldWorker!=null) heldWorker.countDown();
            if(worker!=null) {
                worker.shutdown();
                try {
                    if(!worker.awaitTermination(10,TimeUnit.SECONDS)) {
                        worker.shutdownNow();report.putString("result","fail");report.putString("cleanup","worker_did_not_exit");
                    }
                } catch(InterruptedException e) {Thread.currentThread().interrupt();report.putString("result","fail");}
            }
            report.putInt("assertions",checks);
            report.putInt("pid",Process.myPid());
            if(callbackProcessPid>0) {
                report.putInt("callback_pid",callbackProcessPid);
                report.putBoolean("callback_process_exited",callbackProcessExited);
            }
            report.putBoolean("synthetic_notifications",true);
            report.putBoolean("notification_access_granted",false);
            report.putBoolean("device_controls_changed",false);
            finish("pass".equals(report.getString("result"))?Activity.RESULT_OK:Activity.RESULT_CANCELED,report);
        }
    }
}
