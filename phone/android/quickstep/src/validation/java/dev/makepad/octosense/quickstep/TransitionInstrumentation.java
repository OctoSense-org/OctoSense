package dev.makepad.octosense.quickstep;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.ComponentName;
import android.os.Bundle;
import android.os.Process;
import android.os.SystemClock;
import android.os.UserManager;
import dev.makepad.octosense.contracts.HomeLayout;
import java.util.ArrayList;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** Validation variant only. Exercises local coordinator ordering with Android's
 * actual profile/package queries; never registers a gesture or surface controller.
 */
public final class TransitionInstrumentation extends Instrumentation {
    private final String session=UUID.randomUUID().toString();
    private final ComponentName clock=new ComponentName("com.android.deskclock","com.android.deskclock.DeskClock");
    private final ThreadPoolExecutor worker=new ThreadPoolExecutor(1,1,0,TimeUnit.MILLISECONDS,
            new ArrayBlockingQueue<>(32),r -> new Thread(r,"OctoSenseTransitionValidation"),new ThreadPoolExecutor.AbortPolicy());
    private final ArrayBlockingQueue<Bundle> events=new ArrayBlockingQueue<>(32);
    private int checks;
    private long serial,revision;
    private HomeTransitions coordinator;

    @Override public void onCreate(Bundle args) {super.onCreate(args);start();}
    private boolean offer(Runnable task) {
        try {worker.execute(task);return true;} catch(java.util.concurrent.RejectedExecutionException e) {return false;}
    }
    private void check(boolean value,String reason) {if(!value) throw new AssertionError(reason);checks++;}
    private void work(Runnable task) throws Exception {
        java.util.concurrent.Future<?> future=worker.submit(task);future.get(10,TimeUnit.SECONDS);
    }
    private Bundle event(long id,int phase) throws Exception {
        Bundle value=events.poll(10,TimeUnit.SECONDS);
        check(value!=null&&value.getLong("id")==id&&value.getInt("phase")==phase,"missing_or_misordered_lifecycle_"+phase);
        return value;
    }
    private HomeTransitions.Return begin() {
        HomeTransitions.Return value=coordinator.begin(clock,Process.myUserHandle(),0,0,session,1);
        check(value!=null,"begin_rejected");return value;
    }
    private Bundle layout(long id) {
        Bundle value=new Bundle();value.putInt("version",1);value.putString("epoch",session);value.putLong("transition_id",id);
        value.putInt("display_id",0);value.putInt("rotation",0);value.putIntArray("viewport",new int[]{0,0,1080,2280});
        value.putIntArray("insets",new int[]{0,0,0,0});Bundle icon=new Bundle();icon.putString("component",clock.flattenToString());
        icon.putLong("user",serial);icon.putFloatArray("bounds",new float[]{100,200,240,340});
        ArrayList<Bundle> icons=new ArrayList<>();icons.add(icon);value.putParcelableArrayList("icons",icons);return value;
    }
    private void publish(Bundle value) throws Exception {
        HomeLayout parsed=HomeLayout.decode(++revision,value);work(() -> coordinator.publish(parsed));
    }
    private void runChecks() throws Exception {
        serial=getTargetContext().getSystemService(UserManager.class).getSerialNumberForUser(Process.myUserHandle());
        check(serial>=0,"unknown_current_profile");
        coordinator=new HomeTransitions(getTargetContext(),this::offer,(value,phase,progress) -> {
            Bundle event=new Bundle();event.putLong("id",value.id);event.putInt("phase",phase);
            event.putFloat("progress",progress);event.putLong("user",value.userSerial);
            if(!events.offer(event)) throw new AssertionError("test_callback_overflow");return true;
        });
        HomeTransitions.Return value=begin();check(event(value.id,0).getLong("user")==serial,"profile_serial_changed");
        publish(layout(0));check(value.target(0,0)==null,"ordinary_layout_used_for_transition");
        for(String field:new String[]{"component","user","display_id","rotation","epoch"}) {
            Bundle packet=layout(value.id);
            if(field.equals("component")) packet.<Bundle>getParcelableArrayList("icons").get(0).putString(field,"com.android.deskclock/.Other");
            else if(field.equals("user")) packet.<Bundle>getParcelableArrayList("icons").get(0).putLong(field,serial+1);
            else if(field.equals("epoch")) packet.putString(field,UUID.randomUUID().toString());
            else packet.putInt(field,1);
            publish(packet);check(value.target(0,0)==null,"mismatched_"+field+"_used");
        }
        publish(layout(value.id));HomeTransitions.Target target=value.target(0,0);
        check(target!=null&&target.left==100&&target.bottom==340&&target.layoutRevision==revision,"matching_target_missing");
        check(value.target(1,0)==null&&value.target(0,1)==null,"wrong_frame_display_or_rotation_accepted");
        coordinator.invalidate();check(value.target(0,0)==null,"invalidated_target_survived");
        publish(layout(value.id));check(value.target(0,0)!=null,"fresh_target_not_restored");
        value.progress(0.2f);value.progress(0.5f);value.progress(0.7f);
        Bundle progress=event(value.id,1);check(progress.getFloat("progress")>=0.2f&&progress.getFloat("progress")<=0.7f,"invalid_progress");
        work(() -> {});check(events.isEmpty(),"progress_burst_not_coalesced");
        value.finish(true);check(value.target(0,0)==null,"cancel_not_immediate");event(value.id,3);
        value.finish(false);publish(layout(value.id));work(() -> {});
        check(value.target(0,0)==null&&events.isEmpty(),"cancel_resurrected_or_reported_finish");

        CountDownLatch release=new CountDownLatch(1),blocked=new CountDownLatch(1);
        offer(() -> {blocked.countDown();try {release.await(10,TimeUnit.SECONDS);} catch(InterruptedException e) {Thread.currentThread().interrupt();}});
        check(blocked.await(10,TimeUnit.SECONDS),"worker_barrier_missing");
        try {HomeTransitions.Return early=begin();early.finish(true);} finally {release.countDown();}
        work(() -> {});check(events.isEmpty(),"cancel_before_start_emitted_start");

        HomeTransitions.Return first=begin();event(first.id,0);publish(layout(first.id));
        HomeTransitions.Return next=begin();check(first.target(0,0)==null,"superseded_target_survived");
        event(first.id,3);event(next.id,0);publish(layout(first.id));check(next.target(0,0)==null,"prior_transition_layout_reused");
        publish(layout(next.id));check(next.target(0,0)!=null,"successor_target_missing");
        next.finish(false);event(next.id,2);next.finish(true);work(() -> {});check(events.isEmpty(),"duplicate_terminal_event");

        HomeTransitions.Return expires=begin();event(expires.id,0);publish(layout(expires.id));
        SystemClock.sleep(1100);check(expires.target(0,0)==null,"expired_target_used");
        coordinator.reset();event(expires.id,3);publish(layout(expires.id));check(expires.target(0,0)==null,"reset_target_reused");

        CountDownLatch releaseFull=new CountDownLatch(1),fullBlocked=new CountDownLatch(1);
        offer(() -> {fullBlocked.countDown();try {releaseFull.await(10,TimeUnit.SECONDS);} catch(InterruptedException e) {Thread.currentThread().interrupt();}});
        check(fullBlocked.await(10,TimeUnit.SECONDS),"overflow_barrier_missing");
        try {
            for(int i=0;i<32;i++) check(offer(() -> {}),"queue_filled_early");
            check(coordinator.begin(clock,Process.myUserHandle(),0,0,session,1)==null,"overflow_begin_accepted");
        } finally {releaseFull.countDown();}
        // Wait for the actual bounded executor to drain without adding more work.
        worker.shutdown();check(worker.awaitTermination(10,TimeUnit.SECONDS),"worker_did_not_stop");
        check(events.isEmpty(),"rejected_transition_emitted_callback");
    }
    @Override public void onStart() {
        Bundle report=new Bundle();int code=Activity.RESULT_OK;
        try {runChecks();report.putString("result","pass");}
        catch(Throwable e) {code=Activity.RESULT_CANCELED;report.putString("result","fail");report.putString("error",e.toString());}
        finally {if(coordinator!=null) coordinator.close();worker.shutdownNow();report.putInt("checks",checks);report.putBoolean("native_controller_deployed",false);finish(code,report);}
    }
}
