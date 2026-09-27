package dev.makepad.octosense.bridge;

import android.Manifest;
import android.app.Notification;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.RemoteInput;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothManager;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.database.ContentObserver;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraManager;
import android.media.AudioManager;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.Parcel;
import android.os.PowerManager;
import android.os.RemoteException;
import android.provider.Settings;
import android.service.notification.NotificationListenerService;
import android.service.notification.StatusBarNotification;
import com.topjohnwu.superuser.ipc.RootService;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;
import dev.makepad.octosense.contracts.Protocol;
import dev.makepad.octosense.contracts.NotificationActionSender;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

/** One worker owns revisioned state and native action handles. No periodic poll. */
final class BridgeState {
    interface Operation { void run() throws Exception; }
    static final class Failure extends Exception {
        final int status;
        Failure(int status, String reason) { super(reason); this.status = status; }
    }
    private static final class Subscription {
        final int uid; final String session; final ISystemBridgeCallback callback;
        final LinkedHashMap<Long, Bundle> results = new LinkedHashMap<>();
        long lastCommand;
        IBinder.DeathRecipient death;
        Subscription(int uid, String session, ISystemBridgeCallback callback) {
            this.uid=uid; this.session=session; this.callback=callback;
        }
    }
    private static final class Notice {
        final StatusBarNotification original;
        final String handle = UUID.randomUUID().toString();
        final String identity;
        final ArrayList<String> actions = new ArrayList<>();
        Notice(StatusBarNotification original, String identity) { this.original=original; this.identity=identity; }
    }
    private static final class Action {
        final String noticeHandle; final PendingIntent intent; final RemoteInput[] inputs;
        Action(String noticeHandle, PendingIntent intent, RemoteInput[] inputs) {
            this.noticeHandle=noticeHandle; this.intent=intent; this.inputs=inputs;
        }
    }
    private static final class ListenerSession {
        final NotificationListenerService service;
        ListenerSession(NotificationListenerService service) { this.service=service; }
    }
    private final Context context;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final ThreadPoolExecutor worker = new ThreadPoolExecutor(1,1,0,TimeUnit.MILLISECONDS,
            new ArrayBlockingQueue<>(64), r -> new Thread(r,"OctoSenseBridge"), new ThreadPoolExecutor.AbortPolicy());
    private final ConcurrentHashMap<Integer,Subscription> subscribers = new ConcurrentHashMap<>();
    private final ConcurrentHashMap<Integer,Subscription> requestedSubscribers = new ConcurrentHashMap<>();
    private final LinkedHashMap<String,Notice> notices = new LinkedHashMap<>();
    private final Map<String,Action> actions = new LinkedHashMap<>();
    private final AtomicBoolean refreshPending = new AtomicBoolean();
    private final AtomicBoolean dirty = new AtomicBoolean();
    private final AtomicLong bindingGeneration = new AtomicLong();
    private final AtomicReference<ListenerSession> connectedListener = new AtomicReference<>();
    private final AtomicLong notificationSequence = new AtomicLong();
    private final String epoch = UUID.randomUUID().toString();
    private volatile Bundle capabilities = new Bundle();
    private final java.util.concurrent.CopyOnWriteArraySet<Runnable> settingsObservers = new java.util.concurrent.CopyOnWriteArraySet<>();
    void addSettingsObserver(Runnable observer) { settingsObservers.add(observer); }
    void removeSettingsObserver(Runnable observer) { settingsObservers.remove(observer); }
    private long revision;
    private ListenerSession listenerSession;
    private NotificationListenerService listener;
    private long reconciledNotificationSequence;
    private IRootControl root;
    private boolean rootBinding;
    private ServiceConnection rootConnection;
    private final AtomicLong rootAttempt = new AtomicLong();
    private String rootState = "not_requested";
    private String torchId;
    private boolean torchEnabled;
    private byte[] publishedState;

    BridgeState(Context context) {
        this(context,true);
    }
    // The validation runner uses a private instance with synthetic listener
    // callbacks, without registering a second set of system observers.
    BridgeState(Context context,boolean observeSystem) {
        this.context=context;
        if(observeSystem) registerObservers();
        refresh();
    }
    Bundle capabilities() { return new Bundle(capabilities); }
    void bound() { bindingGeneration.incrementAndGet(); }
    void unbound() {
        long generation=bindingGeneration.get();
        rootCallback(() -> {
            // A later binding/subscription must survive delayed cleanup.
            if(bindingGeneration.get()!=generation) return;
            requestedSubscribers.clear();
            for(Subscription subscription:subscribers.values()) unlink(subscription);
            subscribers.clear();
        });
    }
    private boolean offer(Runnable job) {
        try {
            worker.execute(() -> {
                synchronizeListener();
                synchronizeSubscribers();
                job.run();
                if (dirty.getAndSet(false)) {
                    reconcileNotices();
                    for (Subscription subscription : subscribers.values()) resync(subscription);
                    publishSnapshot();
                }
            });
            return true;
        } catch (RejectedExecutionException e) {
            dirty.set(true);
            // A worker may drain between rejection and marking dirty. Retain a
            // refresh as well, so recovery never depends on the next event.
            refresh();
            return false;
        }
    }
    private void refresh() {
        if (!refreshPending.compareAndSet(false,true)) return;
        enqueueRefresh();
    }
    private void enqueueRefresh() {
        // Keep exactly one retry when full. A lifecycle callback must not need
        // another Android event to become visible after the worker drains.
        if (!offer(() -> { refreshPending.set(false); publishSnapshot(); }))
            main.postDelayed(this::enqueueRefresh,50);
    }
    void permissionsChanged() {refresh();}
    void subscribe(int uid, String session, ISystemBridgeCallback callback) {
        requestedSubscribers.put(uid,new Subscription(uid,session,callback));
        refresh();
    }
    private void synchronizeSubscribers() {
        for(Subscription subscription:subscribers.values()) {
            if(requestedSubscribers.get(subscription.uid)!=subscription) retire(subscription);
        }
        for(Subscription subscription:requestedSubscribers.values()) {
            if(subscribers.get(subscription.uid)==subscription) continue;
            subscription.death=() -> {
                if(requestedSubscribers.remove(subscription.uid,subscription)) refresh();
            };
            try {subscription.callback.asBinder().linkToDeath(subscription.death,0);}
            catch(RemoteException e) {requestedSubscribers.remove(subscription.uid,subscription);continue;}
            if(requestedSubscribers.get(subscription.uid)!=subscription) {unlink(subscription);continue;}
            Subscription previous=subscribers.put(subscription.uid,subscription);
            if(previous!=subscription) unlink(previous);
            // Registration and snapshot share the worker's update boundary.
            sendSnapshot(subscription, snapshot());
        }
    }
    void unsubscribe(int uid, String session) {
        Subscription s=requestedSubscribers.get(uid);
        if(s!=null && s.session.equals(session) && requestedSubscribers.remove(uid,s)) refresh();
    }
    void requestSnapshot(int uid, String session) {
        if(!offer(() -> { Subscription s=session(uid,session); if(s != null) sendSnapshot(s,snapshot()); })) {
            Subscription s=session(uid,session); if(s != null) resync(s);
        }
    }
    private Subscription session(int uid,String session) {
        Subscription s=subscribers.get(uid);
        return s!=null && requestedSubscribers.get(uid)==s && s.session.equals(session) ? s : null;
    }
    private void unlink(Subscription s) { if(s != null) s.callback.asBinder().unlinkToDeath(s.death,0); }
    private void retire(Subscription s) {
        requestedSubscribers.remove(s.uid,s);
        if(subscribers.remove(s.uid,s)) unlink(s);
    }
    private void resync(Subscription s) {
        if(requestedSubscribers.get(s.uid)!=s) return;
        try { s.callback.onResyncRequired(epoch); } catch(RemoteException e) {retire(s);}
    }
    void command(int uid,String session,long id,String operation,Operation action) {
        Subscription s=session(uid,session);
        if(s == null) throw new SecurityException("Subscribe before issuing commands");
        if(!offer(() -> {
            if(this.session(uid,session) != s) { result(s,id,Protocol.DISCONNECTED,"session_replaced"); return; }
            Bundle previous=s.results.get(id);
            if(previous != null) { result(s,id,previous.getInt("status"),previous.getString("reason")); return; }
            if(id <= s.lastCommand) { result(s,id,Protocol.EXPIRED_HANDLE,"command_result_expired"); return; }
            s.lastCommand=id;
            result(s,id,Protocol.ACCEPTED,"accepted");
            int status=Protocol.COMPLETED; String reason="request_dispatched_reconcile_state";
            try { action.run(); }
            catch(Failure e) { status=e.status; reason=e.getMessage(); }
            catch(IllegalArgumentException e) { status=Protocol.INVALID_ARGUMENT; reason="invalid_argument"; }
            catch(SecurityException e) { status=Protocol.ACCESS_DENIED; reason="permission_denied"; }
            catch(Exception e) { status=Protocol.UNCERTAIN; reason="operation_failed_reconcile_state"; }
            Bundle completion=ScopedRootService.result(status,reason);
            s.results.put(id,completion);
            while(s.results.size()>128) s.results.remove(s.results.keySet().iterator().next());
            result(s,id,status,reason);
            publishSnapshot();
        })) result(s,id,Protocol.QUEUE_FULL,"command_not_dispatched");
    }
    private void result(Subscription s,long id,int status,String reason) {
        if(requestedSubscribers.get(s.uid)!=s) return;
        try { s.callback.onCommandResult(s.session,id,status,reason); }
        catch(RemoteException e) {retire(s);}
    }
    private void publishSnapshot() {
        Bundle state=snapshot();
        Parcel parcel=Parcel.obtain();
        byte[] encoded;
        try {parcel.writeBundle(state);encoded=parcel.marshall();} finally {parcel.recycle();}
        if(java.util.Arrays.equals(publishedState,encoded)) return;
        publishedState=encoded;
        revision++;
        main.post(() -> { for(Runnable observer:settingsObservers) observer.run(); });
        for(Subscription s:subscribers.values()) sendSnapshot(s,state);
    }
    private void sendSnapshot(Subscription s,Bundle state) {
        if(requestedSubscribers.get(s.uid)!=s) return;
        try { s.callback.onSnapshot(epoch,revision,state); }
        catch(RemoteException e) {retire(s);}
    }
    private boolean granted(String permission) { return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED; }
    private static Bundle capability(boolean accessible,String route,String prerequisite) {
        Bundle value=new Bundle(); value.putBoolean("discovered",true); value.putBoolean("accessible",accessible);
        value.putBoolean("validated",false); value.putString("route",route); value.putString("prerequisite",accessible ? "" : prerequisite); return value;
    }
    private Bundle snapshot() {
        synchronizeListener();
        Bundle state=new Bundle(); Bundle caps=new Bundle();
        caps.putBundle("brightness",capability(Settings.System.canWrite(context),"public","write_settings"));
        caps.putBundle("rotation",capability(Settings.System.canWrite(context),"public","write_settings"));
        caps.putBundle("volume",capability(true,"public",""));
        NotificationManager nm=context.getSystemService(NotificationManager.class);
        caps.putBundle("dnd",capability(Build.VERSION.SDK_INT < 35 && nm.isNotificationPolicyAccessGranted(),
                Build.VERSION.SDK_INT >= 35 ? "settings" : "public", Build.VERSION.SDK_INT >= 35 ? "android_modes_settings" : "notification_policy_access"));
        caps.putBundle("notifications",capability(listener != null,"public","notification_listener_access"));
        caps.putBundle("torch",capability(torchId != null && granted(Manifest.permission.CAMERA),"public","camera_permission_or_hardware"));
        for(String operation:new String[]{"wifi","bluetooth","battery_saver"}) {
            boolean observable=!operation.equals("bluetooth") || Build.VERSION.SDK_INT<31 || granted(Manifest.permission.BLUETOOTH_CONNECT);
            caps.putBundle(operation,capability(root != null && observable,"root_lineage_22.2",
                    root == null ? "root_grant_and_rom_adapter" : "bluetooth_connect"));
        }
        caps.putString("root_state",rootState); caps.putInt("protocol_major",Protocol.MAJOR);
        capabilities=caps; state.putBundle("capabilities",new Bundle(caps));
        state.putFloat("brightness",Settings.System.getInt(context.getContentResolver(),Settings.System.SCREEN_BRIGHTNESS,128)/255f);
        state.putBoolean("brightness_automatic",Settings.System.getInt(context.getContentResolver(),Settings.System.SCREEN_BRIGHTNESS_MODE,0)==1);
        state.putBoolean("rotation_locked",Settings.System.getInt(context.getContentResolver(),Settings.System.ACCELEROMETER_ROTATION,1)==0);
        AudioManager audio=context.getSystemService(AudioManager.class);
        state.putFloat("volume",audio.getStreamVolume(AudioManager.STREAM_MUSIC)/(float)Math.max(1,audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)));
        state.putBoolean("torch",torchEnabled);
        state.putInt("interruption_filter",nm.getCurrentInterruptionFilter());
        state.putBoolean("battery_saver",context.getSystemService(PowerManager.class).isPowerSaveMode());
        WifiManager wifi=context.getSystemService(WifiManager.class);
        state.putBoolean("wifi",wifi != null && wifi.isWifiEnabled());
        if(Build.VERSION.SDK_INT<31 || granted(Manifest.permission.BLUETOOTH_CONNECT)) {
            BluetoothManager manager=context.getSystemService(BluetoothManager.class);
            BluetoothAdapter bluetooth=manager == null ? null : manager.getAdapter();
            state.putBoolean("bluetooth",bluetooth != null && bluetooth.isEnabled());
        }
        state.putAll(dev.makepad.octosense.contracts.NetworkStatus.read(context));
        ArrayList<Bundle> notificationModels=new ArrayList<>();
        for(Notice notice:notices.values()) notificationModels.add(noticeModel(notice));
        state.putParcelableArrayList("notifications",notificationModels);
        // Measure actual Parcel size, rather than estimating Binder payloads.
        while(parcelBytes(state)>Protocol.MAX_PACKET_BYTES && !notificationModels.isEmpty()) {
            notificationModels.remove(notificationModels.size()-1);
            state.putParcelableArrayList("notifications",notificationModels);
            state.putBoolean("notifications_truncated",true);
        }
        return state;
    }
    private static int parcelBytes(Bundle bundle) {
        Parcel parcel=Parcel.obtain(); try { parcel.writeBundle(bundle); return parcel.dataSize(); } finally { parcel.recycle(); }
    }
    private static String text(CharSequence value,int limit) {
        if(value==null) return ""; String result=value.toString(); return result.substring(0,Math.min(result.length(),limit));
    }
    private Bundle noticeModel(Notice notice) {
        Notification notification=notice.original.getNotification(); Bundle model=new Bundle();
        model.putString("identity",notice.identity);
        model.putString("handle",notice.handle); model.putString("package",notice.original.getPackageName());
        model.putString("title",text(notification.extras.getCharSequence(Notification.EXTRA_TITLE),160));
        model.putString("text",text(notification.extras.getCharSequence(Notification.EXTRA_TEXT),512));
        model.putLong("posted",notice.original.getPostTime()); model.putBoolean("dismissible",notice.original.isClearable());
        ArrayList<Bundle> models=new ArrayList<>();
        for(int i=0;i<notice.actions.size();i++) {
            String handle=notice.actions.get(i); Action action=actions.get(handle); if(action==null) continue;
            Bundle a=new Bundle(); a.putString("handle",handle);
            a.putString("label",i==0 ? "Open" : text(notification.actions[i-1].title,80));
            a.putBoolean("open",i==0);
            a.putBoolean("reply",NotificationActionSender.acceptsReply(action.inputs)); models.add(a);
        }
        model.putParcelableArrayList("actions",models); return model;
    }
    void listenerConnected(NotificationListenerService service) {
        connectedListener.set(new ListenerSession(service));
        refresh();
    }
    private void synchronizeListener() {
        ListenerSession current=connectedListener.get();
        if(listenerSession==current) return;
        reconcileNotices();
    }
    private void reconcileNotices() {
        listenerSession=connectedListener.get();
        listener=listenerSession==null ? null : listenerSession.service;
        // The queried snapshot supersedes already-received events, including
        // those still waiting in the queue. Later callbacks must remain eligible.
        reconciledNotificationSequence=notificationSequence.get();
        if(listener==null) {clearNotices();return;}
        try {
            StatusBarNotification[] active=listener.getActiveNotifications();
            clearNotices();
            if(active!=null) for(StatusBarNotification notice:active) addNotice(notice);
        } catch(SecurityException e) {
            connectedListener.compareAndSet(listenerSession,null);
            listenerSession=null;listener=null;clearNotices();
        }
    }
    void listenerDisconnected(NotificationListenerService service) {
        ListenerSession current=connectedListener.get();
        if(current!=null && current.service==service && connectedListener.compareAndSet(current,null)) refresh();
    }
    void notificationPosted(NotificationListenerService service,StatusBarNotification notification) {
        ListenerSession current=connectedListener.get();
        if(current==null || current.service!=service) return;
        long sequence=notificationSequence.incrementAndGet();
        offer(() -> {
            if(current==connectedListener.get() && current==listenerSession && listener!=null
                    && sequence>reconciledNotificationSequence) {
                addNotice(notification);publishSnapshot();
            }
        });
    }
    void notificationRemoved(NotificationListenerService service,StatusBarNotification notification) {
        ListenerSession current=connectedListener.get();
        if(current==null || current.service!=service) return;
        long sequence=notificationSequence.incrementAndGet();
        offer(() -> {
            if(current==connectedListener.get() && current==listenerSession && listener!=null
                    && sequence>reconciledNotificationSequence) {
                removeNotice(notification.getKey());publishSnapshot();
            }
        });
    }
    private void clearNotices() { notices.clear(); actions.clear(); }
    private void removeNotice(String key) {
        Notice previous=notices.remove(key); if(previous != null) for(String handle:previous.actions) actions.remove(handle);
    }
    private void addNotice(StatusBarNotification notification) {
        // Work-profile content never crosses the bridge's user boundary.
        if(!notification.getUser().equals(android.os.Process.myUserHandle())) return;
        Notice previous=notices.get(notification.getKey());
        String identity=previous==null ? UUID.randomUUID().toString() : previous.identity;
        removeNotice(notification.getKey());
        Notice notice=new Notice(notification,identity); Notification original=notification.getNotification();
        // Reserve index zero for Open even if no content intent is present.
        notice.actions.add("");
        if(original.contentIntent!=null) addAction(notice,0,original.contentIntent,null);
        if(original.actions!=null) for(int i=0;i<Math.min(original.actions.length,Protocol.MAX_ACTIONS-1);i++) {
            Notification.Action action=original.actions[i]; notice.actions.add("");
            if(action.actionIntent != null) addAction(notice,i+1,action.actionIntent,action.getRemoteInputs());
        }
        notices.put(notification.getKey(),notice);
        while(notices.size()>Protocol.MAX_NOTIFICATIONS) removeNotice(notices.keySet().iterator().next());
    }
    private void addAction(Notice notice,int index,PendingIntent intent,RemoteInput[] inputs) {
        String handle=UUID.randomUUID().toString(); notice.actions.set(index,handle); actions.put(handle,new Action(notice.handle,intent,inputs));
    }
    void dismiss(String handle) throws Failure {
        synchronizeListener();
        if(listener==null) throw new Failure(Protocol.PREREQUISITE_MISSING,"notification_listener_unavailable");
        for(Notice notice:notices.values()) if(notice.handle.equals(handle)) {
            if(!notice.original.isClearable()) throw new Failure(Protocol.ACCESS_DENIED,"notification_not_dismissible");
            listener.cancelNotification(notice.original.getKey()); return;
        }
        throw new Failure(Protocol.EXPIRED_HANDLE,"notification_expired");
    }
    void notificationAction(String handle,String reply) throws Failure {
        synchronizeListener();
        Action action=actions.get(handle);
        if(listener==null || action==null) throw new Failure(Protocol.EXPIRED_HANDLE,"notification_action_expired");
        try { NotificationActionSender.send(context,action.intent,action.inputs,reply); }
        catch(PendingIntent.CanceledException e) { throw new Failure(Protocol.EXPIRED_HANDLE,"notification_action_cancelled"); }
    }
    void dismissAll() throws Failure {
        synchronizeListener();
        if(listener==null) throw new Failure(Protocol.PREREQUISITE_MISSING,"notification_listener_unavailable");
        for(Notice notice:notices.values()) if(notice.original.isClearable()) listener.cancelNotification(notice.original.getKey());
    }
    void brightness(float value,boolean automatic) throws Failure {
        if(!Settings.System.canWrite(context)) throw new Failure(Protocol.PREREQUISITE_MISSING,"write_settings_required");
        if(!Settings.System.putInt(context.getContentResolver(),Settings.System.SCREEN_BRIGHTNESS_MODE,automatic?1:0)
                || (!automatic && !Settings.System.putInt(context.getContentResolver(),Settings.System.SCREEN_BRIGHTNESS,Math.max(1,Math.round(value*255)))))
            throw new Failure(Protocol.ACCESS_DENIED,"brightness_write_rejected");
    }
    void rotation(boolean locked) throws Failure {
        if(!Settings.System.canWrite(context)) throw new Failure(Protocol.PREREQUISITE_MISSING,"write_settings_required");
        if(!Settings.System.putInt(context.getContentResolver(),Settings.System.ACCELEROMETER_ROTATION,locked?0:1))
            throw new Failure(Protocol.ACCESS_DENIED,"rotation_write_rejected");
    }
    void volume(float value) {
        AudioManager audio=context.getSystemService(AudioManager.class);
        audio.setStreamVolume(AudioManager.STREAM_MUSIC,Math.round(value*audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)),0);
    }
    void torch(boolean enabled) throws Exception {
        if(torchId==null) throw new Failure(Protocol.UNSUPPORTED,"flashlight_hardware_unavailable");
        if(!granted(Manifest.permission.CAMERA)) throw new Failure(Protocol.PREREQUISITE_MISSING,"camera_permission_required");
        context.getSystemService(CameraManager.class).setTorchMode(torchId,enabled);
    }
    void interruption(int filter) throws Failure {
        if(Build.VERSION.SDK_INT >= 35) throw new Failure(Protocol.PREREQUISITE_MISSING,"use_android_modes_settings");
        NotificationManager manager=context.getSystemService(NotificationManager.class);
        if(!manager.isNotificationPolicyAccessGranted()) throw new Failure(Protocol.PREREQUISITE_MISSING,"notification_policy_access_required");
        manager.setInterruptionFilter(filter);
    }
    private void rootResult(Bundle result) throws Failure {
        if(result==null) throw new Failure(Protocol.UNCERTAIN,"root_result_missing");
        if(result.getInt("status")!=Protocol.COMPLETED) throw new Failure(result.getInt("status"),result.getString("reason"));
    }
    private IRootControl requireRoot() throws Failure {
        if(root==null) throw new Failure(Protocol.PREREQUISITE_MISSING,"root_unavailable"); return root;
    }
    private interface RootOperation { Bundle run(IRootControl control) throws RemoteException; }
    private void rootOperation(RootOperation operation) throws Exception {
        try { rootResult(operation.run(requireRoot())); }
        catch(RemoteException e) {
            root=null; rootState="disconnected";
            releaseRoot(rootAttempt.get());
            throw new Failure(Protocol.UNCERTAIN,"root_disconnected_reconcile_state");
        }
    }
    void rootWifi(boolean value) throws Exception { rootOperation(control -> control.setWifiEnabled(value)); }
    void rootBluetooth(boolean value) throws Exception {
        requireRoot();
        if(Build.VERSION.SDK_INT>=31 && !granted(Manifest.permission.BLUETOOTH_CONNECT))
            throw new Failure(Protocol.PREREQUISITE_MISSING,"bluetooth_connect_required_for_observed_state");
        rootOperation(control -> control.setBluetoothEnabled(value));
    }
    void rootBatterySaver(boolean value) throws Exception { rootOperation(control -> control.setBatterySaver(value)); }
    private void releaseRoot(long attempt) {
        if(!rootAttempt.compareAndSet(attempt,attempt+1)) return;
        main.post(() -> {
            if(rootConnection!=null) { RootService.unbind(rootConnection); rootConnection=null; }
            rootBinding=false;
        });
    }
    private void rootCallback(Runnable callback) {
        // Retain lifecycle callbacks when the bounded worker queue is full.
        if(!offer(callback)) main.postDelayed(() -> rootCallback(callback),50);
    }
    void connectRoot() {
        // User-initiated only. libsu launches its root task asynchronously.
        main.post(() -> {
            if(rootBinding) return; rootBinding=true;
            long attempt=rootAttempt.incrementAndGet();
            offer(() -> { rootState="connecting"; publishSnapshot(); });
            rootConnection=new ServiceConnection() {
                @Override public void onServiceConnected(ComponentName name,IBinder binder) {
                    if(rootAttempt.get()!=attempt) return;
                    try {
                        IRootControl candidate=IRootControl.Stub.asInterface(binder);
                        Bundle identity=candidate.getIdentity();
                        if(rootAttempt.get()!=attempt) return;
                        if(identity.getInt("uid")!=0 || identity.getInt("major")!=Protocol.MAJOR || !identity.getBoolean("adapter_supported")) {
                            rootState="incompatible"; root=null; releaseRoot(attempt);
                        } else { root=candidate; rootState="connected_unvalidated"; }
                    } catch(RemoteException|SecurityException e) {
                        root=null; rootState="disconnected"; releaseRoot(attempt);
                    }
                    publishSnapshot();
                }
                @Override public void onServiceDisconnected(ComponentName name) {
                    if(rootAttempt.get()!=attempt) return;
                    root=null; rootState="disconnected"; releaseRoot(attempt); publishSnapshot();
                }
            };
            RootService.bind(new Intent(context,ScopedRootService.class),this::rootCallback,rootConnection);
            main.postDelayed(() -> rootCallback(() -> {
                if(rootAttempt.get()==attempt && root==null) {
                    rootState="denied_or_unavailable"; releaseRoot(attempt); publishSnapshot();
                }
            }),10_000);
        });
    }
    private void registerObservers() {
        BroadcastReceiver receiver=new BroadcastReceiver() {
            @Override public void onReceive(Context c,Intent intent) { refresh(); }
        };
        IntentFilter filter=new IntentFilter();
        for(String action:new String[]{WifiManager.WIFI_STATE_CHANGED_ACTION,BluetoothAdapter.ACTION_STATE_CHANGED,
                PowerManager.ACTION_POWER_SAVE_MODE_CHANGED,NotificationManager.ACTION_INTERRUPTION_FILTER_CHANGED,
                NotificationManager.ACTION_NOTIFICATION_POLICY_ACCESS_GRANTED_CHANGED,"android.media.VOLUME_CHANGED_ACTION"}) filter.addAction(action);
        if(Build.VERSION.SDK_INT>=33) context.registerReceiver(receiver,filter,Context.RECEIVER_EXPORTED);
        else context.registerReceiver(receiver,filter);
        ContentObserver observer=new ContentObserver(main) {
            @Override public void onChange(boolean selfChange) { refresh(); }
        };
        for(String setting:new String[]{Settings.System.SCREEN_BRIGHTNESS,Settings.System.SCREEN_BRIGHTNESS_MODE,Settings.System.ACCELEROMETER_ROTATION})
            context.getContentResolver().registerContentObserver(Settings.System.getUriFor(setting),false,observer);
        context.getSystemService(ConnectivityManager.class).registerDefaultNetworkCallback(new ConnectivityManager.NetworkCallback() {
            @Override public void onCapabilitiesChanged(Network network,NetworkCapabilities caps) { refresh(); }
            @Override public void onLost(Network network) { refresh(); }
        });
        offer(() -> {
            try {
                CameraManager camera=context.getSystemService(CameraManager.class);
                for(String id:camera.getCameraIdList()) if(Boolean.TRUE.equals(camera.getCameraCharacteristics(id).get(CameraCharacteristics.FLASH_INFO_AVAILABLE))) { torchId=id; break; }
                camera.registerTorchCallback(new CameraManager.TorchCallback() {
                    @Override public void onTorchModeChanged(String id,boolean enabled) {
                        offer(() -> { if(id.equals(torchId)) { torchEnabled=enabled; publishSnapshot(); } });
                    }
                },main);
            } catch(Exception e) { torchId=null; }
        });
    }
}
