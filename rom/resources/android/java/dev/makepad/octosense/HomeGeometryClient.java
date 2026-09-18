package dev.makepad.octosense;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.RemoteException;
import android.view.View;
import dev.makepad.android.MakepadActivity;
import dev.makepad.octosense.contracts.HomeLayout;
import dev.makepad.octosense.contracts.HomeTransitionEvent;
import dev.makepad.octosense.contracts.IHomeIntegration;
import dev.makepad.octosense.contracts.IHomeIntegrationCallback;
import dev.makepad.octosense.contracts.Protocol;
import java.util.ArrayList;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;

/** Converts renderer geometry on the activity thread; all Binder work uses
 * Home's existing bounded worker. A single replaceable slot coalesces layouts.
 */
public final class HomeGeometryClient {
    interface Dispatch {boolean offer(Runnable task);}
    interface Events {void emit(String channel,JSONObject value);}
    private final MakepadActivity activity;
    private final Dispatch worker;
    private final Events events;
    private final Handler main=new Handler(Looper.getMainLooper());
    private final String session=UUID.randomUUID().toString();
    private final AtomicReference<Pending> latest=new AtomicReference<>();
    private final AtomicBoolean queued=new AtomicBoolean(),resubscribe=new AtomicBoolean();
    private volatile long generation;
    private final java.util.concurrent.atomic.AtomicLong catalogToken=new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicLong connectionToken=new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicLong callbackToken=new java.util.concurrent.atomic.AtomicLong();
    private long catalogRevision=-1;
    private long revision;
    // Activity-thread state; callback work is gated again after posting here.
    private long transitionId,lastTransitionId;
    private volatile boolean resumed,closed;
    private boolean bound,binding;
    private boolean covered;
    private android.view.WindowInsets lastInsets;
    private int attempts;
    // Worker-owned service and callback ordering state.
    private IHomeIntegration service;
    private String extensionEpoch="";
    private long eventRevision=-1;
    private volatile JSONObject observed=new JSONObject();
    private static final class Pending {
        final long generation,revision;final Bundle layout;final boolean ready;
        Pending(long g,long r,Bundle b,boolean v) {generation=g;revision=r;layout=b;ready=v;}
    }
    private final View.OnLayoutChangeListener layoutListener=(v,l,t,r,b,ol,ot,or,ob) -> {
        if(l!=ol||t!=ot||r!=or||b!=ob) invalidate();
    };
    private final android.view.ViewTreeObserver.OnWindowFocusChangeListener focusListener=focused -> invalidate();
    public HomeGeometryClient(MakepadActivity activity,Dispatch worker,Events events) {
        this.activity=activity;this.worker=worker;this.events=events;
        activity.getApplicationOverlay().addOnLayoutChangeListener(layoutListener);
        activity.getApplicationOverlay().getViewTreeObserver().addOnWindowFocusChangeListener(focusListener);
        activity.getApplicationOverlay().setOnApplyWindowInsetsListener((view,insets) -> {
            if(!insets.equals(lastInsets)) {lastInsets=insets;invalidate();}return insets;
        });
    }
    public JSONObject validationState() {
        try {
            JSONObject value=new JSONObject(observed.toString());Pending pending=latest.get();
            value.put("generation",generation).put("local_ready",pending!=null&&pending.ready&&resumed)
                    .put("local_revision",pending==null?-1:pending.revision).put("catalog_revision",catalogRevision)
                    .put("transition_id",transitionId);
            if(pending!=null&&pending.layout!=null) {
                JSONArray icons=new JSONArray();ArrayList<Bundle> entries=pending.layout.getParcelableArrayList("icons");
                for(Bundle entry:entries) {JSONArray rect=new JSONArray();for(float n:entry.getFloatArray("bounds")) rect.put(n);
                    icons.put(new JSONObject().put("component",entry.getString("component")).put("user",entry.getLong("user")).put("bounds",rect));}
                value.put("icons",icons).put("display_id",pending.layout.getInt("display_id")).put("rotation",pending.layout.getInt("rotation"));
            }
            return value;
        } catch(Exception e) {return new JSONObject();}
    }
    public void catalogChanged() {
        catalogToken.incrementAndGet();
        Runnable change=() -> {catalogRevision=-1;invalidate();};
        if(Looper.myLooper()==Looper.getMainLooper()) change.run();else main.post(change);
    }
    public long catalogToken() {return catalogToken.get();}
    public void catalogPublished(long revision,long token) {
        main.post(() -> {if(!closed&&catalogToken.get()==token) {catalogRevision=revision;invalidate();}});
    }
    public void onResume() {resumed=true;invalidate();bind();}
    public void onPause() {resumed=false;invalidate();}
    public void setCovered(boolean value) {if(covered!=value) {covered=value;invalidate();}}
    /** Main thread, also used before package/profile changes can expose stale icons. */
    public void invalidate() {
        if(closed) return;
        long g=++generation;latest.set(new Pending(g,++revision,null,false));drain();
        worker.offer(() -> {try {events.emit("home.layout.request",new JSONObject().put("generation",g));} catch(Exception ignored) {}});
    }
    public void layout(String payload) {
        if(closed||payload.length()>65536) return;
        try {
            JSONObject source=new JSONObject(payload);
            if(source.getLong("generation")!=generation||source.optLong("transition_id",0)!=transitionId) return;
            View root=activity.getApplicationOverlay();
            if(root.getWidth()<=0||root.getHeight()<=0||root.getDisplay()==null) return;
            if(Math.abs(source.getDouble("pixel_width")-root.getWidth())>1||Math.abs(source.getDouble("pixel_height")-root.getHeight())>1)
                throw new IllegalArgumentException("Renderer and activity viewport differ");
            int[] origin=new int[2];root.getLocationOnScreen(origin);
            Bundle packet=new Bundle();packet.putInt("version",1);packet.putString("epoch",session);
            packet.putLong("transition_id",transitionId);
            packet.putInt("display_id",root.getDisplay().getDisplayId());packet.putInt("rotation",root.getDisplay().getRotation());
            packet.putIntArray("viewport",new int[]{origin[0],origin[1],origin[0]+root.getWidth(),origin[1]+root.getHeight()});
            JSONArray safe=source.getJSONArray("insets");if(safe.length()!=4) throw new IllegalArgumentException("Insets");
            packet.putIntArray("insets",new int[]{(int)Math.round(unit(safe,0)*root.getWidth()),(int)Math.round(unit(safe,1)*root.getHeight()),
                    (int)Math.round(unit(safe,2)*root.getWidth()),(int)Math.round(unit(safe,3)*root.getHeight())});
            JSONArray icons=source.getJSONArray("icons");if(icons.length()>HomeLayout.MAX_ICONS) throw new IllegalArgumentException("Icon count");
            ArrayList<Bundle> entries=new ArrayList<>();
            for(int i=0;i<icons.length();i++) {
                JSONObject icon=icons.getJSONObject(i);JSONArray bounds=icon.getJSONArray("bounds");if(bounds.length()!=4) throw new IllegalArgumentException("Bounds");
                Bundle entry=new Bundle();entry.putString("component",icon.getString("component"));entry.putLong("user",icon.getLong("user"));
                entry.putFloatArray("bounds",new float[]{(float)(origin[0]+unit(bounds,0)*root.getWidth()),(float)(origin[1]+unit(bounds,1)*root.getHeight()),
                        (float)(origin[0]+unit(bounds,2)*root.getWidth()),(float)(origin[1]+unit(bounds,3)*root.getHeight())});entries.add(entry);
            }
            packet.putParcelableArrayList("icons",entries);
            long r=++revision;packet=HomeLayout.decode(r,packet).bundle();
            latest.set(new Pending(generation,r,packet,resumed&&!covered&&root.hasWindowFocus()&&source.getBoolean("ready")
                    &&catalogRevision>0&&source.getLong("catalog_revision")==catalogRevision));drain();
        } catch(Exception e) {latest.set(new Pending(generation,++revision,null,false));drain();}
    }
    private static double unit(JSONArray array,int index) throws Exception {
        double value=array.getDouble(index);if(!Double.isFinite(value)||value<0||value>1) throw new IllegalArgumentException("Invalid normalized coordinate");return value;
    }
    private void drain() {
        if(closed||!queued.compareAndSet(false,true)) return;
        if(!worker.offer(() -> {
            Pending sent=latest.get();
            try {
                if(service!=null) {
                    if(resubscribe.getAndSet(false)) {eventRevision=-1;service.subscribe(session,callback);}
                    if(sent!=null) {
                        if(sent.layout!=null&&sent.generation==generation) service.publishHomeLayout(session,sent.revision,sent.layout);
                        service.setHomeReady(session,sent.revision,sent.layout!=null&&sent.generation==generation&&resumed&&sent.ready);
                    }
                }
            } catch(RemoteException|SecurityException e) {service=null;status("disconnected","layout_transport_failed");}
            finally {queued.set(false);if(latest.get()!=sent) main.post(this::drain);}
        })) {queued.set(false);main.postDelayed(this::drain,50);}
    }
    private void status(String state,String reason) {
        try {JSONObject next=new JSONObject().put("connection",state).put("reason",reason).put("controllers_implemented",false)
                .put("transitions_validated",false).put("generation",generation);observed=next;events.emit("home.extension",next);} catch(Exception ignored) {}
    }
    private void resetTransition() {
        lastTransitionId=0;if(transitionId!=0) {transitionId=0;invalidate();}
    }
    private void bind() {
        if(dev.makepad.octosense.validation.BridgeInstrumentation.homeTransportTest) return;
        if(closed||!resumed||bound||binding) return;binding=true;
        if(!worker.offer(() -> {
            boolean trusted=activity.getPackageManager().checkSignatures(activity.getPackageName(),Protocol.QUICKSTEP_PACKAGE)==PackageManager.SIGNATURE_MATCH;
            if(!trusted) status("unavailable","quickstep_missing_or_certificate_mismatch");
            main.post(() -> {
                binding=false;if(closed||!resumed||bound||!trusted) return;
                try {bound=activity.bindService(new Intent().setComponent(new ComponentName(Protocol.QUICKSTEP_PACKAGE,
                        Protocol.QUICKSTEP_PACKAGE+".HomeIntegrationService")),connection,Context.BIND_AUTO_CREATE);
                    if(!bound) worker.offer(() -> status("unavailable","quickstep_bind_failed"));
                } catch(SecurityException e) {worker.offer(() -> status("denied","quickstep_bind_denied"));}
            });
        })) {binding=false;main.postDelayed(this::bind,100);}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {
            long token=connectionToken.incrementAndGet();resetTransition();
            if(!worker.offer(() -> {
                try {
                    IHomeIntegration next=IHomeIntegration.Stub.asInterface(binder);Bundle info=next.getProtocolInfo();
                    if(closed||connectionToken.get()!=token) return;
                    if(info.getInt("major",-1)!=Protocol.MAJOR) {status("incompatible","protocol_major_mismatch");return;}
                    service=next;extensionEpoch=info.getString("epoch","");eventRevision=-1;attempts=0;
                    next.subscribe(session,callback);status("connected","");main.post(HomeGeometryClient.this::drain);
                } catch(RemoteException|SecurityException e) {service=null;status("disconnected","quickstep_handshake_failed");}
            })) reconnect();
        }
        @Override public void onServiceDisconnected(ComponentName name) {
            connectionToken.incrementAndGet();resetTransition();
            worker.offer(() -> {service=null;extensionEpoch="";status("disconnected","quickstep_process_died");});
        }
        @Override public void onBindingDied(ComponentName name) {reconnect();}
        @Override public void onNullBinding(ComponentName name) {reconnect();}
    };
    private void reconnect() {
        main.post(() -> {
            if(closed) return;connectionToken.incrementAndGet();resetTransition();
            if(bound) {activity.unbindService(connection);bound=false;}
            worker.offer(() -> {service=null;extensionEpoch="";status("disconnected","quickstep_binding_died");});
            if(resumed&&attempts<5) main.postDelayed(this::bind,250L<<attempts++);
        });
    }
    private final IHomeIntegrationCallback.Stub callback=new IHomeIntegrationCallback.Stub() {
        @Override public void onTransition(long transition,long layout,Bundle event) {
            if(!worker.offer(() -> {
                try {
                    HomeTransitionEvent value=HomeTransitionEvent.decode(transition,layout,event);
                    if(!session.equals(value.session)||!extensionEpoch.equals(value.epoch)||value.eventRevision<=eventRevision) return;
                    eventRevision=value.eventRevision;long token=connectionToken.get(),boundary=callbackToken.get();
                    main.post(() -> applyTransition(value,token,boundary));
                } catch(IllegalArgumentException|ClassCastException e) {
                    resubscribe.set(true);main.post(HomeGeometryClient.this::invalidate);
                }
            })) {resubscribe.set(true);main.post(HomeGeometryClient.this::invalidate);}
        }
        @Override public void onExtensionState(Bundle state) {
            if(!worker.offer(() -> {
                if(!session.equals(state.getString("session"))||!extensionEpoch.equals(state.getString("epoch"))) return;
                long r=state.getLong("event_revision",-1);if(r<=eventRevision) return;eventRevision=r;
                try {JSONObject value=new JSONObject().put("connection","connected").put("reason",state.getString("reason"))
                        .put("layout_revision",state.getLong("layout_revision")).put("home_ready",state.getBoolean("home_ready"))
                        .put("icon_count",state.getInt("icon_count")).put("controllers_implemented",state.getBoolean("controllers_implemented"))
                        .put("transitions_validated",false);
                    observed=value;events.emit("home.extension",value);
                } catch(Exception ignored) {}
                if(state.getInt("status")==Protocol.QUEUE_FULL||"layout_epoch_changed_resubscribe".equals(state.getString("reason"))) {
                    callbackToken.incrementAndGet();resubscribe.set(true);long token=connectionToken.get();
                    main.post(() -> {if(connectionToken.get()==token) {resetTransition();drain();}});
                } else if("subscribed".equals(state.getString("reason"))) {
                    callbackToken.incrementAndGet();
                    long token=connectionToken.get();main.post(() -> {if(connectionToken.get()==token) resetTransition();});
                } else if("layout_invalidated".equals(state.getString("reason"))) {
                    long token=connectionToken.get();
                    main.post(() -> {if(!closed&&connectionToken.get()==token) invalidate();});
                }
            })) {resubscribe.set(true);main.post(thisDrain());}
        }
    };
    private void applyTransition(HomeTransitionEvent event,long token,long boundary) {
        if(closed||connectionToken.get()!=token||callbackToken.get()!=boundary) return;
        if(event.phase==HomeTransitionEvent.STARTED) {
            android.view.Display display=activity.getApplicationOverlay().getDisplay();
            if(event.id<=lastTransitionId||display==null||display.getDisplayId()!=event.displayId||display.getRotation()!=event.rotation) return;
            lastTransitionId=event.id;transitionId=event.id;invalidate();
        } else if(event.phase==HomeTransitionEvent.FINISHED||event.phase==HomeTransitionEvent.CANCELLED) {
            lastTransitionId=Math.max(lastTransitionId,event.id);
            if(transitionId!=event.id) return;transitionId=0;invalidate();
        } else if(transitionId!=event.id) return;
        if(!worker.offer(() -> {
            if(closed||connectionToken.get()!=token||callbackToken.get()!=boundary) return;
            try {events.emit("home.transition",new JSONObject().put("id",event.id).put("phase",event.phase)
                    .put("epoch",event.epoch).put("event_revision",event.eventRevision)
                    .put("layout_revision",event.layoutRevision).put("progress",event.progress));}
            catch(Exception ignored) {}
        })) {resubscribe.set(true);invalidate();}
    }
    private Runnable thisDrain() {return this::drain;}
    public void onDestroy() {
        closed=true;resumed=false;connectionToken.incrementAndGet();latest.set(null);main.removeCallbacksAndMessages(null);
        activity.getApplicationOverlay().removeOnLayoutChangeListener(layoutListener);
        activity.getApplicationOverlay().getViewTreeObserver().removeOnWindowFocusChangeListener(focusListener);
        activity.getApplicationOverlay().setOnApplyWindowInsetsListener(null);
        if(bound) {activity.unbindService(connection);bound=false;}
        // Binder death/final unbind clears the service cache even if this
        // worker is shut down before an explicit unsubscribe could execute.
    }
}
