package dev.makepad.octosense.quickstep;

import android.app.Service;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.os.Bundle;
import android.os.IBinder;
import android.os.RemoteException;
import android.os.UserHandle;
import dev.makepad.octosense.contracts.CallerGuard;
import dev.makepad.octosense.contracts.HomeLayout;
import dev.makepad.octosense.contracts.IHomeIntegration;
import dev.makepad.octosense.contracts.IHomeIntegrationCallback;
import dev.makepad.octosense.contracts.Protocol;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

/** Authenticated Home layout transport for the separate Quickstep package.
 * The SDK prototype exposes only layout transport. The platform APK explicitly
 * enables the local controller endpoint through its own manifest metadata.
 */
public final class HomeIntegrationService extends Service {
    private static volatile HomeIntegrationService active;
    private final String epoch=UUID.randomUUID().toString();
    private final ThreadPoolExecutor worker=new ThreadPoolExecutor(1,1,0,TimeUnit.MILLISECONDS,
            new ArrayBlockingQueue<>(32),r -> new Thread(r,"OctoSenseHomeLayout"),new ThreadPoolExecutor.AbortPolicy());
    private final AtomicBoolean overflow=new AtomicBoolean();
    private final AtomicLong geometryGeneration=new AtomicLong();
    private final HomeTransitions transitions=new HomeTransitions(this,this::offer,this::transition);
    private volatile boolean controllersImplemented;
    // The worker owns session state. A controller may read this immutable
    // snapshot without a lock or synchronous IPC; null requires generic Home.
    private volatile HomeLayout usable;
    private HomeLayout layout;
    private long layoutGeneration=-1;
    private volatile String session="";
    private volatile long subscription;
    private String layoutEpoch="";
    private long revision=-1,eventRevision;
    private IHomeIntegrationCallback callback;
    private IBinder.DeathRecipient death;
    private volatile boolean needsSubscription;
    private volatile boolean closed;

    private boolean offer(Runnable operation) {
        if(closed) return false;
        try {worker.execute(() -> {
            if(closed) return;
            recoverOverflow();
            try {operation.run();} catch(IllegalArgumentException|ClassCastException e) {usable=null;layout=null;transitions.invalidate();state("invalid_layout",Protocol.INVALID_ARGUMENT);}
            finally {recoverOverflow();}
        });return true;} catch(java.util.concurrent.RejectedExecutionException e) {usable=null;transitions.invalidate();overflow.set(true);return false;}
    }
    private void recoverOverflow() {
        if(overflow.getAndSet(false)) {usable=null;layout=null;transitions.reset();needsSubscription=true;state("resync_required",Protocol.QUEUE_FULL);}
    }
    private void clear() {
        ++subscription;transitions.reset();usable=null;layout=null;revision=-1;layoutEpoch="";session="";needsSubscription=false;
        if(callback!=null&&death!=null) try {callback.asBinder().unlinkToDeath(death,0);} catch(java.util.NoSuchElementException ignored) {}
        callback=null;death=null;
    }
    private void state(String reason,int status) {
        IHomeIntegrationCallback target=callback;if(target==null) return;
        Bundle value=new Bundle();value.putString("epoch",epoch);value.putLong("event_revision",++eventRevision);
        value.putString("session",session);value.putString("reason",reason);value.putInt("status",status);
        value.putLong("layout_revision",revision);value.putBoolean("home_ready",usable!=null&&layoutGeneration==geometryGeneration.get());
        value.putInt("icon_count",layout==null?0:layout.iconCount());
        value.putBoolean("controllers_implemented",controllersImplemented);value.putBoolean("transitions_validated",false);
        try {target.onExtensionState(value);} catch(RemoteException e) {clear();}
    }
    private boolean matches(String s) {
        return !needsSubscription&&callback!=null&&session.equals(s);
    }
    private int caller() {return CallerGuard.require(this,Protocol.HOME_PACKAGE);}
    /** Native code calls this in-process. No Binder/profile query occurs here. */
    public static HomeTransitions.Return beginReturn(ComponentName component,UserHandle user,int display,int rotation) {
        HomeIntegrationService service=active;
        if(service==null||service.closed||!service.controllersImplemented||service.needsSubscription) return null;
        String session=service.session;if(session.isEmpty()) return null;
        return service.transitions.begin(component,user,display,rotation,session,service.subscription);
    }
    private boolean transition(HomeTransitions.Return value,int phase,float progress) {
        if(!controllersImplemented||value.subscription!=subscription||!matches(value.session)) return false;
        Bundle event=new Bundle();event.putInt("version",1);event.putString("session",session);event.putString("epoch",epoch);
        event.putLong("event_revision",++eventRevision);event.putInt("phase",phase);event.putFloat("progress",progress);
        event.putString("component",value.component.flattenToString());event.putLong("user",value.userSerial);
        event.putInt("display_id",value.displayId);event.putInt("rotation",value.rotation);
        try {callback.onTransition(value.id,value.layoutRevision,event);return true;} catch(RemoteException e) {clear();return false;}
    }
    private void invalidateGeometry() {
        geometryGeneration.incrementAndGet();usable=null;transitions.invalidate();
        offer(() -> {
            usable=null;layout=null;layoutGeneration=-1;transitions.reset();
            state("layout_invalidated",Protocol.COMPLETED);
        });
    }
    private final BroadcastReceiver invalidateReceiver=new BroadcastReceiver() {
        @Override public void onReceive(Context context,Intent intent) {invalidateGeometry();}
    };
    @Override public void onCreate() {
        super.onCreate();active=this;
        IntentFilter filter=new IntentFilter(Intent.ACTION_SCREEN_OFF);
        filter.addAction(Intent.ACTION_MANAGED_PROFILE_AVAILABLE);filter.addAction(Intent.ACTION_MANAGED_PROFILE_UNAVAILABLE);
        filter.addAction(Intent.ACTION_MANAGED_PROFILE_REMOVED);filter.addAction(Intent.ACTION_USER_UNLOCKED);
        filter.addAction("android.intent.action.USER_STOPPED");filter.addAction("android.intent.action.USER_SWITCHED");
        filter.addAction(Intent.ACTION_PACKAGES_SUSPENDED);filter.addAction(Intent.ACTION_PACKAGES_UNSUSPENDED);
        filter.addAction(Intent.ACTION_EXTERNAL_APPLICATIONS_AVAILABLE);filter.addAction(Intent.ACTION_EXTERNAL_APPLICATIONS_UNAVAILABLE);
        IntentFilter packages=new IntentFilter(Intent.ACTION_PACKAGE_ADDED);
        packages.addAction(Intent.ACTION_PACKAGE_REMOVED);packages.addAction(Intent.ACTION_PACKAGE_REPLACED);
        packages.addAction(Intent.ACTION_PACKAGE_CHANGED);packages.addDataScheme("package");
        if(android.os.Build.VERSION.SDK_INT>=33) registerReceiver(invalidateReceiver,filter,Context.RECEIVER_NOT_EXPORTED);
        else registerReceiver(invalidateReceiver,filter);
        if(android.os.Build.VERSION.SDK_INT>=33) registerReceiver(invalidateReceiver,packages,Context.RECEIVER_NOT_EXPORTED);
        else registerReceiver(invalidateReceiver,packages);
        offer(() -> {
            try {Bundle metadata=getPackageManager().getApplicationInfo(getPackageName(),PackageManager.GET_META_DATA).metaData;
                controllersImplemented=metadata!=null&&metadata.getBoolean("octosense.native.controllers",false);
            } catch(PackageManager.NameNotFoundException e) {controllersImplemented=false;}
        });
    }
    @Override public void onConfigurationChanged(Configuration configuration) {
        invalidateGeometry();super.onConfigurationChanged(configuration);
    }
    @Override public IBinder onBind(Intent intent) {return binder;}
    @Override public boolean onUnbind(Intent intent) {transitions.invalidate();offer(this::clear);return false;}
    @Override public void onDestroy() {
        closed=true;if(active==this) active=null;transitions.close();unregisterReceiver(invalidateReceiver);
        usable=null;worker.getQueue().clear();worker.execute(this::clear);worker.shutdown();super.onDestroy();
    }
    private final IHomeIntegration.Stub binder=new IHomeIntegration.Stub() {
        @Override public Bundle getProtocolInfo() {
            caller();Bundle value=Protocol.info("octosense-quickstep-layout/1");value.putString("epoch",epoch);
            value.putBoolean("controllers_implemented",controllersImplemented);value.putBoolean("transitions_validated",false);return value;
        }
        @Override public void subscribe(String s,IHomeIntegrationCallback target) {
            caller();Protocol.requireSession(s);if(target==null) throw new IllegalArgumentException("Missing callback");
            offer(() -> {
                clear();session=s;callback=target;
                IBinder token=target.asBinder();death=() -> offer(() -> {if(callback!=null&&callback.asBinder().equals(token)) clear();});
                try {token.linkToDeath(death,0);state("subscribed",Protocol.COMPLETED);} catch(RemoteException e) {clear();}
            });
        }
        @Override public void unsubscribe(String s) {caller();Protocol.requireSession(s);offer(() -> {if(matches(s)) clear();});}
        @Override public void publishHomeLayout(String s,long r,Bundle value) {
            caller();Protocol.requireSession(s);
            long generation=geometryGeneration.get();
            transitions.invalidate();
            // Binder dispatch owns this unmarshalled Bundle until worker decode;
            // decode copies only the bounded primitive schema into its snapshot.
            offer(() -> {
                if(!matches(s)||generation!=geometryGeneration.get()) return;
                HomeLayout next=HomeLayout.decode(r,value);
                if(!layoutEpoch.isEmpty()&&!layoutEpoch.equals(next.epoch)) {usable=null;layout=null;needsSubscription=true;state("layout_epoch_changed_resubscribe",Protocol.EXPIRED_HANDLE);return;}
                if(r<=revision) {state("stale_layout_revision",Protocol.EXPIRED_HANDLE);return;}
                layout=next;layoutGeneration=generation;layoutEpoch=next.epoch;revision=r;usable=null;state("layout_cached",Protocol.COMPLETED);
            });
        }
        @Override public void setHomeReady(String s,long r,boolean ready) {
            caller();Protocol.requireSession(s);
            long generation=ready?geometryGeneration.get():geometryGeneration.incrementAndGet();
            if(!ready) {usable=null;transitions.invalidate();}
            offer(() -> {
                if(!matches(s)) {
                    // A retired session can race a new subscription. Its
                    // immediate invalidation must not strand the current Home
                    // with an unusable cache and no request to republish.
                    if(!ready&&layoutGeneration!=geometryGeneration.get()) {
                        usable=null;layout=null;layoutGeneration=-1;
                        state("layout_invalidated",Protocol.EXPIRED_HANDLE);
                    }
                    return;
                }
                if(!ready) {usable=null;layout=null;layoutGeneration=-1;state("home_not_ready",Protocol.COMPLETED);return;}
                if(layout==null||r!=revision||layoutGeneration!=generation||generation!=geometryGeneration.get()) {
                    usable=null;transitions.invalidate();state("readiness_revision_mismatch",Protocol.EXPIRED_HANDLE);return;
                }
                usable=layout;transitions.publish(layout);state("home_ready",Protocol.COMPLETED);
            });
        }
    };
}
