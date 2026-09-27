package dev.makepad.octosense.quickstep;

import android.content.ComponentName;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.LauncherApps;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import dev.makepad.octosense.contracts.HomeLayout;
import dev.makepad.octosense.contracts.HomeTransitionEvent;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.ScheduledThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** Local native-controller endpoint. Binder/profile queries run on the layout
 * service's bounded worker; animation frames only read immutable targets.
 */
public final class HomeTransitions {
    interface Dispatch {boolean offer(Runnable task);}
    interface Events {boolean send(Return transition,int phase,float progress);}
    private static final AtomicLong IDS=new AtomicLong();
    private final Context context;
    private final Dispatch worker;
    private final Events events;
    private final AtomicReference<Return> current=new AtomicReference<>();
    private final AtomicLong invalidation=new AtomicLong();
    private final ScheduledThreadPoolExecutor clock=new ScheduledThreadPoolExecutor(1,
            r -> new Thread(r,"OctoSenseHomeProgress"));

    HomeTransitions(Context context,Dispatch worker,Events events) {
        this.context=context;this.worker=worker;this.events=events;
        clock.setRemoveOnCancelPolicy(true);
    }

    /** Immutable display-pixel target. No SurfaceControl or mutable arrays cross threads. */
    public static final class Target {
        public final float left,top,right,bottom;
        public final long layoutRevision;
        private final long validUntil,generation;
        private Target(HomeLayout layout,float[] bounds,long generation) {
            left=bounds[0];top=bounds[1];right=bounds[2];bottom=bounds[3];layoutRevision=layout.revision;
            validUntil=SystemClock.uptimeMillis()+1000;this.generation=generation;
        }
    }

    public final class Return {
        public final long id=IDS.incrementAndGet();
        final ComponentName component;
        final UserHandle user;
        final int displayId,rotation;
        final String session;
        final long subscription;
        final long deadline=SystemClock.uptimeMillis()+5000;
        private final AtomicInteger terminal=new AtomicInteger();
        private final AtomicBoolean progressQueued=new AtomicBoolean();
        private volatile Target target;
        private volatile float progress;
        private volatile float sentProgress;
        private volatile ScheduledFuture<?> ticker;
        // Worker-owned identity and lifecycle delivery state.
        long userSerial=-1,layoutRevision=-1;
        private boolean started,ended;
        private final Runnable progressTask=() -> {
            progressQueued.set(false);
            if(current.get()==this&&terminal.get()==0&&started) {
                float value=progress;
                if(value!=sentProgress&&events.send(this,HomeTransitionEvent.PROGRESS,value)) sentProgress=value;
            }
        };
        private Return(ComponentName component,UserHandle user,int display,int rotation,String session,long subscription) {
            this.component=component;this.user=user;displayId=display;this.rotation=rotation;this.session=session;this.subscription=subscription;
        }
        /** UI/animation thread. This path never performs IPC, locks or target lookup. */
        public Target target(int display,int orientation) {
            Target value=target;long now=SystemClock.uptimeMillis();
            return terminal.get()==0&&current.get()==this&&display==displayId&&orientation==rotation
                    &&now<deadline&&value!=null&&now<value.validUntil&&value.generation==invalidation.get()?value:null;
        }
        /** Frame thread only writes a scalar. Scheduling and IPC stay off it. */
        public void progress(float value) {
            if(terminal.get()!=0||!Float.isFinite(value)) return;
            progress=Math.max(0,Math.min(1,value));
        }
        private void tick() {
            if(terminal.get()!=0||current.get()!=this) {stopTicker();return;}
            if(SystemClock.uptimeMillis()>=deadline) {finish(true);stopTicker();return;}
            if(progress==sentProgress||!progressQueued.compareAndSet(false,true)) return;
            if(!worker.offer(progressTask)) progressQueued.set(false);
        }
        private void stopTicker() {
            ScheduledFuture<?> value=ticker;if(value!=null) value.cancel(false);
        }
        public void finish(boolean cancelled) {
            if(!terminal.compareAndSet(0,cancelled?2:1)) return;
            target=null;current.compareAndSet(this,null);
            worker.offer(() -> complete(this));
        }
    }

    Return begin(ComponentName component,UserHandle user,int display,int rotation,String session,long subscription) {
        if(component==null||user==null||display<0||rotation<0||rotation>3) return null;
        Return next=new Return(component,user,display,rotation,session,subscription);
        Return previous=current.getAndSet(next);if(previous!=null) previous.finish(true);
        if(!worker.offer(() -> {
            if(current.get()!=next||next.terminal.get()!=0) return;
            if(SystemClock.uptimeMillis()>=next.deadline) {next.finish(true);return;}
            next.userSerial=serialIfAvailable(next);
            if(next.userSerial<0) {next.finish(true);return;}
            next.started=events.send(next,HomeTransitionEvent.STARTED,0);
            if(!next.started) next.finish(true);
            else if(next.terminal.get()==0) {
                try {next.ticker=clock.scheduleWithFixedDelay(next::tick,50,50,TimeUnit.MILLISECONDS);}
                catch(java.util.concurrent.RejectedExecutionException e) {next.finish(true);}
            }
        })) {next.finish(true);return null;}
        return next;
    }

    private long serialIfAvailable(Return value) {
        try {
            UserManager users=context.getSystemService(UserManager.class);
            LauncherApps apps=context.getSystemService(LauncherApps.class);
            if(users==null||apps==null||!users.isUserUnlocked(value.user)
                    ||!apps.isActivityEnabled(value.component,value.user)) return -1;
            ApplicationInfo app=apps.getApplicationInfo(value.component.getPackageName(),0,value.user);
            if(!app.enabled||(app.flags&ApplicationInfo.FLAG_SUSPENDED)!=0) return -1;
            return users.getSerialNumberForUser(value.user);
        } catch(RuntimeException|android.content.pm.PackageManager.NameNotFoundException e) {return -1;}
    }

    /** Worker only. A fresh positive transition token is required even if an
     * ordinary layout was previously ready. Profile checks never guess a serial.
     */
    void publish(HomeLayout layout) {
        Return value=current.get();if(value==null) return;
        long generation=invalidation.get();
        value.target=null;
        if(layout==null||!value.started||value.terminal.get()!=0||SystemClock.uptimeMillis()>=value.deadline
                ||layout.transitionId!=value.id||!layout.epoch.equals(value.session)
                ||layout.displayId!=value.displayId||layout.rotation!=value.rotation
                ||serialIfAvailable(value)!=value.userSerial) return;
        float[] bounds=layout.boundsFor(value.component,value.userSerial);
        if(bounds!=null&&current.get()==value&&value.terminal.get()==0) {
            value.layoutRevision=layout.revision;value.target=new Target(layout,bounds,generation);
        }
    }
    /** May be called immediately by lifecycle/configuration invalidation. */
    void invalidate() {invalidation.incrementAndGet();Return value=current.get();if(value!=null) value.target=null;}
    void reset() {Return value=current.getAndSet(null);if(value!=null) value.finish(true);}
    void close() {reset();clock.shutdownNow();}
    private void complete(Return value) {
        value.stopTicker();
        if(value.ended) return;value.ended=true;
        if(value.started) events.send(value,value.terminal.get()==1?HomeTransitionEvent.FINISHED:HomeTransitionEvent.CANCELLED,
                value.terminal.get()==1?1:value.progress);
    }
}
