package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.app.AppOpsManager;
import android.app.INotificationManager;
import android.app.KeyguardManager;
import android.app.NotificationHistory;
import android.content.Context;
import android.content.pm.PackageManager;
import android.database.ContentObserver;
import android.os.Handler;
import android.os.Looper;
import android.os.Process;
import android.os.ServiceManager;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import android.text.format.DateFormat;
import dev.makepad.octosense.notifications.NotificationHistoryContract;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Date;
import java.util.List;
import java.util.PriorityQueue;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicLong;
import org.json.JSONArray;
import org.json.JSONObject;

/** Owner-user notification history, read only, with an expiring stable paging snapshot. */
final class NotificationHistorySettings {
    private final Context context;
    private final Handler handler=new Handler(Looper.getMainLooper());
    private final Runnable expiry=() -> {synchronized(this) {clear();}};
    private final AtomicLong generation=new AtomicLong();
    private final ContentObserver observer=new ContentObserver(null) {
        @Override public void onChange(boolean selfChange) {
            generation.incrementAndGet();
            handler.post(() -> {synchronized(NotificationHistorySettings.this) {if(cacheGeneration!=generation.get()) clear();}});
        }
    };
    private long cacheGeneration;
    private String key;
    private long expires;
    private boolean truncated;
    private List<Row> rows=new ArrayList<>();
    private static final class Row {
        final String pkg,title,text,channel;
        final long posted;
        Row(NotificationHistory.HistoricalNotification n) {
            pkg=NotificationHistoryContract.text(n.getPackage(),255);
            title=NotificationHistoryContract.text(n.getTitle(),128);
            text=NotificationHistoryContract.text(n.getText(),512);
            channel=NotificationHistoryContract.text(n.getChannelName(),64);
            posted=n.getPostedTimeMs();
        }
    }
    private static final Comparator<Row> ORDER=Comparator.comparingLong((Row row)->row.posted)
            .thenComparing(row->row.pkg).thenComparing(row->row.channel)
            .thenComparing(row->row.title).thenComparing(row->row.text);
    NotificationHistorySettings(Context context) {
        this.context=context;
        context.getContentResolver().registerContentObserver(Settings.Secure.getUriFor("notification_history_enabled"),false,observer);
    }
    synchronized void invalidate() {generation.incrementAndGet();clear();}
    synchronized void close() {context.getContentResolver().unregisterContentObserver(observer);clear();}
    private boolean allowed() {
        UserManager users=context.getSystemService(UserManager.class);
        KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0&&users!=null
                &&users.isAdminUser()&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();
    }
    private void clear() {handler.removeCallbacks(expiry);rows.clear();key=null;expires=0;truncated=false;}
    private Boolean enabled() {
        String raw=Settings.Secure.getString(context.getContentResolver(),"notification_history_enabled");
        return raw==null||raw.equals("0")?Boolean.FALSE:raw.equals("1")?Boolean.TRUE:null;
    }
    private boolean hasAccess() {
        if(context.checkSelfPermission(Manifest.permission.ACCESS_NOTIFICATIONS)!=PackageManager.PERMISSION_GRANTED) return false;
        AppOpsManager ops=context.getSystemService(AppOpsManager.class);
        if(ops==null) return false;
        int mode=ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_ACCESS_NOTIFICATIONS,Process.myUid(),context.getPackageName());
        return mode==AppOpsManager.MODE_ALLOWED||mode==AppOpsManager.MODE_DEFAULT;
    }
    private JSONObject empty(long id,String status,Boolean enabled) throws Exception {
        clear();return new JSONObject().put("schema",1).put("request_id",id).put("status",status)
                .put("enabled",enabled==null?JSONObject.NULL:enabled).put("key",JSONObject.NULL)
                .put("offset",0).put("total",0).put("truncated",false).put("rows",new JSONArray());
    }
    synchronized JSONObject snapshot(long id,String observedKey,int offset) throws Exception {
        NotificationHistoryContract.request(id,observedKey,offset);
        if(!allowed()) return empty(id,"restricted",null);
        Boolean enabled=enabled();
        if(enabled==null) return empty(id,"unavailable",null);
        if(!enabled) return empty(id,"disabled",false);
        if(!hasAccess()) return empty(id,"unavailable",true);
        if(observedKey==null) {
            clear();
            long observedGeneration=generation.get();
            INotificationManager service=INotificationManager.Stub.asInterface(ServiceManager.getService(Context.NOTIFICATION_SERVICE));
            if(service==null) return empty(id,"unavailable",true);
            NotificationHistory history=service.getNotificationHistory(context.getPackageName(),null);
            if(history==null) return empty(id,"unavailable",true);
            PriorityQueue<Row> recent=new PriorityQueue<>(ORDER);
            int scanned=0,matching=0;
            while(history.hasNextNotification()&&scanned<NotificationHistoryContract.MAX_SCAN) {
                NotificationHistory.HistoricalNotification item=history.getNextNotification();scanned++;
                // The framework returns current profiles too; they never enter this owner-only UI.
                if(item==null||item.getUserId()!=0||UserHandle.getUserId(item.getUid())!=0
                        ||item.getPostedTimeMs()<0||item.getPackage()==null) continue;
                matching++;recent.add(new Row(item));
                if(recent.size()>NotificationHistoryContract.MAX_ROWS) recent.poll();
            }
            truncated=matching>NotificationHistoryContract.MAX_ROWS||history.hasNextNotification();
            rows=new ArrayList<>(recent);rows.sort(ORDER.reversed());
            // A lock, user switch or disabling history during the Binder read must retire its data.
            if(!allowed()) return empty(id,"restricted",null);
            Boolean after=enabled();
            if(!Boolean.TRUE.equals(after)) return empty(id,after==null?"unavailable":"disabled",after);
            if(!hasAccess()) return empty(id,"unavailable",true);
            if(generation.get()!=observedGeneration) return empty(id,"expired",true);
            cacheGeneration=observedGeneration;
            key=UUID.randomUUID().toString().replace("-","")+UUID.randomUUID().toString().replace("-","");
            expires=SystemClock.elapsedRealtime()+NotificationHistoryContract.LIFETIME_MS;
            handler.postDelayed(expiry,NotificationHistoryContract.LIFETIME_MS);
        } else if(key==null||!observedKey.equals(key)||generation.get()!=cacheGeneration||SystemClock.elapsedRealtime()>=expires) {
            return empty(id,"expired",true);
        }
        if(offset>0&&offset>=rows.size()) return empty(id,"expired",true);
        JSONArray page=new JSONArray();
        java.text.DateFormat date=DateFormat.getMediumDateFormat(context),time=DateFormat.getTimeFormat(context);
        for(int i=offset;i<Math.min(offset+NotificationHistoryContract.PAGE_SIZE,rows.size());i++) {
            Row row=rows.get(i);String label=row.pkg;
            try {label=context.getPackageManager().getApplicationLabel(context.getPackageManager().getApplicationInfo(row.pkg,0)).toString();}
            catch(PackageManager.NameNotFoundException removed) {}
            Date posted=new Date(row.posted);
            page.put(new JSONObject().put("package",row.pkg).put("label",NotificationHistoryContract.text(label,64))
                    .put("posted",row.posted).put("when",NotificationHistoryContract.text(date.format(posted)+" · "+time.format(posted),96))
                    .put("channel",row.channel).put("title",row.title).put("text",row.text));
        }
        if(!allowed()) return empty(id,"restricted",null);
        Boolean after=enabled();
        if(!Boolean.TRUE.equals(after)) return empty(id,after==null?"unavailable":"disabled",after);
        if(!hasAccess()) return empty(id,"unavailable",true);
        if(generation.get()!=cacheGeneration) return empty(id,"expired",true);
        return new JSONObject().put("schema",1).put("request_id",id).put("status","ready").put("enabled",true)
                .put("key",key).put("offset",offset).put("total",rows.size()).put("truncated",truncated).put("rows",page);
    }
}
