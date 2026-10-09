package dev.makepad.octosense;

import android.Manifest;
import android.app.Fragment;
import android.content.ContentProviderOperation;
import android.content.ContentProviderResult;
import android.content.ContentUris;
import android.content.ContentValues;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.provider.CalendarContract;
import dev.makepad.android.MakepadActivity;
import java.util.ArrayList;
import java.util.TimeZone;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.function.BiConsumer;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Package-private OS adapter; only Rust's admitted, reviewed host commands reach it.
 * No network, account login, sync-adapter flag, or calendar creation capability. */
final class DeviceCalendarClient {
    private final MakepadActivity activity;
    private final BiConsumer<String,JSONObject> emit;
    private final BooleanSupplier foreground;
    private final Handler main=new Handler(Looper.getMainLooper());
    private final ThreadPoolExecutor worker=new ThreadPoolExecutor(1,1,0,TimeUnit.MILLISECONDS,
        new ArrayBlockingQueue<>(8),r -> new Thread(r,"DeviceCalendar"),new ThreadPoolExecutor.AbortPolicy());
    private volatile boolean closed;
    private final java.util.concurrent.ConcurrentHashMap<String,Long> active=new java.util.concurrent.ConcurrentHashMap<>();
    private void live(String id){
        Long until=active.get(id);
        if(closed||until==null||SystemClock.elapsedRealtime()>=until)throw new IllegalStateException("cancelled: Calendar command expired or its app account changed");
    }
    private static final String[] CAL={"_id","account_name","account_type","calendar_displayName","calendar_access_level"};
    private static final String[] EVENT={"_id","calendar_id","title","dtstart","dtend","eventTimezone","allDay","eventLocation","description","rrule","rdate","original_id","hasAttendeeData"};
    DeviceCalendarClient(MakepadActivity activity,BiConsumer<String,JSONObject> emit,BooleanSupplier foreground){this.activity=activity;this.emit=emit;this.foreground=foreground;}
    void close(){closed=true;active.clear();worker.shutdownNow();}
    boolean granted(){return activity.checkSelfPermission(Manifest.permission.READ_CALENDAR)==PackageManager.PERMISSION_GRANTED
        &&activity.checkSelfPermission(Manifest.permission.WRITE_CALENDAR)==PackageManager.PERMISSION_GRANTED;}
    void command(String payload){
        if(closed||payload.length()>65536)return;
        try{
            JSONObject request=new JSONObject(payload);String id=request.getString("id");
            if(!id.matches("[a-f0-9-]{36}"))return;
            String op=request.getString("operation");
            if("cancel".equals(op)){active.remove(id);return;}
            long remaining=Math.min(45000,Math.max(0,request.optLong("expires_after_ms",0)));
            if(remaining==0||active.size()>=16){reply(id,null,"busy: Calendar command expired or queue is full");return;}
            active.put(id,SystemClock.elapsedRealtime()+remaining);
            if("permission".equals(op)){
                main.post(() -> {
                    try{live(id);}catch(IllegalStateException e){reply(id,null,e.getMessage());return;}
                    if(!foreground.getAsBoolean()){reply(id,null,"foreground_required: Return to the app");return;}
                    if(granted()){reply(id,status(),null);return;}
                    if(activity.getFragmentManager().findFragmentByTag("OctoSenseDeviceCalendarPermission")!=null){reply(id,null,"busy: Calendar permission is already pending");return;}
                    PermissionFragment fragment=new PermissionFragment();fragment.client=this;fragment.requestId=id;
                    activity.getFragmentManager().beginTransaction().add(fragment,"OctoSenseDeviceCalendarPermission").commitAllowingStateLoss();
                });return;
            }
            try{worker.execute(() -> {
                if(closed)return;
                try{
                    live(id);
                    if(!"status".equals(op)&&!granted())throw new IllegalStateException("authorization_required: OS calendar permission is not granted");
                    if(("save".equals(op)||"delete".equals(op))&&!foreground.getAsBoolean())throw new IllegalStateException("foreground_required: Change cancelled before OS dispatch");
                    reply(id,execute(request),null);
                }catch(SecurityException e){reply(id,null,"authorization_required: OS calendar permission changed");}
                 catch(IllegalStateException e){reply(id,null,e.getMessage());}
                 catch(Exception e){reply(id,null,"platform_error: Calendar operation failed; refresh before retrying an approved write");}
            });}catch(java.util.concurrent.RejectedExecutionException e){reply(id,null,"busy: Calendar queue is full");}
        }catch(Exception ignored){}
    }
    private JSONObject status(){try{return new JSONObject().put("os_permission",granted()?"granted":"not_granted");}catch(Exception e){return new JSONObject();}}
    private void reply(String id,JSONObject data,String error){
        active.remove(id);
        if(closed)return;
        try{JSONObject reply=new JSONObject().put("id",id).put("ok",error==null);
            if(error==null)reply.put("data",data);else reply.put("error",error);
            if(reply.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length>262144)
                reply=new JSONObject().put("id",id).put("ok",false).put("error","limit: Calendar reply exceeds the Android 256 KiB bridge limit; reduce the list limit");
            emit.accept("device_calendar.result",reply);
        }catch(Exception ignored){}
    }
    public static final class PermissionFragment extends Fragment {
        DeviceCalendarClient client;String requestId;
        @Override public void onCreate(Bundle state){super.onCreate(state);
            if(client==null||client.closed||!client.foreground.getAsBoolean()){finish();return;}
            try{client.live(requestId);}catch(IllegalStateException e){client.reply(requestId,null,e.getMessage());finish();return;}
            requestPermissions(new String[]{Manifest.permission.READ_CALENDAR,Manifest.permission.WRITE_CALENDAR},9406);
        }
        @Override public void onRequestPermissionsResult(int code,String[] permissions,int[] results){
            if(code!=9406)return;if(client!=null)client.reply(requestId,client.status(),null);finish();
        }
        private void finish(){if(getFragmentManager()!=null)getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();}
    }
    private Cursor query(Uri uri,String[] projection,String selection,String[] args,String sort){
        CancellationSignal cancel=new CancellationSignal();Runnable expire=cancel::cancel;main.postDelayed(expire,30000);
        try{return activity.getContentResolver().query(uri,projection,selection,args,sort,cancel);}
        finally{main.removeCallbacks(expire);}
    }
    private static String text(Cursor cursor,int column){String s=cursor.getString(column);return s==null?"":s;}
    private JSONObject calendarRow(Cursor c)throws Exception{
        String account=text(c,2)+"\n"+text(c,1);
        if(account.length()>1024||text(c,3).length()>2048||text(c,1).length()>2048)throw new IllegalStateException("limit: Calendar metadata exceeds bounds");
        return new JSONObject().put("id",text(c,0)).put("account",account).put("account_name",text(c,1))
            .put("name",text(c,3)).put("writable",c.getInt(4)>=CalendarContract.Calendars.CAL_ACCESS_CONTRIBUTOR);
    }
    private JSONObject calendar(String id)throws Exception{
        if(!id.matches("[0-9]{1,19}"))throw new IllegalStateException("invalid_arguments: Invalid calendar id");
        try(Cursor c=query(CalendarContract.Calendars.CONTENT_URI,CAL,"_id=?",new String[]{id},null)){
            if(c==null||!c.moveToFirst())throw new IllegalStateException("calendar_missing: Calendar no longer exists");return calendarRow(c);
        }
    }
    private JSONObject selected(JSONObject expected,boolean write)throws Exception{
        JSONObject actual=calendar(expected.getString("id"));
        if(!actual.getString("account").equals(expected.getString("account")))throw new IllegalStateException("account_changed: Calendar source changed");
        if(write&&!actual.getBoolean("writable"))throw new IllegalStateException("read_only: Calendar cannot be edited");return actual;
    }
    private JSONObject eventRow(Cursor c)throws Exception{
        JSONObject e=new JSONObject().put("id",text(c,0)).put("title",text(c,2)).put("start_ms",c.getLong(3)).put("end_ms",c.getLong(4))
            .put("timezone",text(c,5).isEmpty()?"UTC":text(c,5)).put("all_day",c.getInt(6)!=0).put("location",text(c,7)).put("notes",text(c,8))
            .put("recurring",!text(c,9).isEmpty()||!text(c,10).isEmpty()||!c.isNull(11)).put("has_attendees",c.getInt(12)!=0);
        if(e.getString("title").length()>512||e.getString("location").length()>2048||e.getString("notes").length()>8192)throw new IllegalStateException("limit: Event content exceeds public API bounds");return e;
    }
    private JSONObject event(JSONObject cal,String id)throws Exception{
        selected(cal,false);if(!id.matches("[0-9]{1,19}"))throw new IllegalStateException("invalid_arguments: Invalid event id");
        try(Cursor c=query(CalendarContract.Events.CONTENT_URI,EVENT,"_id=? AND calendar_id=? AND deleted=0",new String[]{id,cal.getString("id")},null)){
            if(c==null||!c.moveToFirst())throw new IllegalStateException("event_missing: Event is not in the selected calendar");return eventRow(c);
        }
    }
    private static void editable(JSONObject event)throws Exception{
        if(event.getBoolean("recurring")||event.getBoolean("has_attendees"))throw new IllegalStateException("unsupported_event: Recurring events and invitations are read-only");
    }
    private static boolean same(JSONObject a,JSONObject b)throws Exception{
        for(String key:new String[]{"id","title","start_ms","end_ms","timezone","all_day","location","notes","recurring","has_attendees"})if(!a.get(key).equals(b.get(key)))return false;return true;
    }
    private JSONObject execute(JSONObject q)throws Exception{
        String op=q.getString("operation");if("status".equals(op))return status();
        if("calendars".equals(op)){
            JSONArray rows=new JSONArray();try(Cursor c=query(CalendarContract.Calendars.CONTENT_URI,CAL,null,null,"_id ASC")){
                if(c!=null)while(c.moveToNext()){if(rows.length()==64)throw new IllegalStateException("limit: At most 64 calendar choices");rows.put(calendarRow(c));}
            }return new JSONObject().put("calendars",rows);
        }
        if("calendar".equals(op))return calendar(q.getString("calendar_id"));
        JSONObject cal=q.getJSONObject("calendar");selected(cal,"save".equals(op)||"delete".equals(op));
        if("get".equals(op))return event(cal,q.getString("event_id")).put("_calendar",selected(cal,false));
        if("list".equals(op)){
            long start=q.getLong("start_ms"),end=q.getLong("end_ms");int limit=q.getInt("limit");
            if(start<0||end<=start||end-start>93L*86400000||limit<1||limit>200)throw new IllegalStateException("invalid_arguments: Unbounded calendar window");
            Uri.Builder uri=CalendarContract.Instances.CONTENT_URI.buildUpon();ContentUris.appendId(uri,start);ContentUris.appendId(uri,end);
            JSONArray rows=new JSONArray();boolean truncated=false;
            try(Cursor c=query(uri.build(),new String[]{"event_id","begin","end"},"calendar_id=?",new String[]{cal.getString("id")},"begin ASC")){
                if(c!=null)while(c.moveToNext()){
                    if(rows.length()==limit){truncated=true;break;}
                    JSONObject e=event(cal,text(c,0));e.put("start_ms",c.getLong(1));e.put("end_ms",c.getLong(2));rows.put(e);
                }
            }return new JSONObject().put("events",rows).put("truncated",truncated);
        }
        if(!"save".equals(op)&&!"delete".equals(op))throw new IllegalStateException("method_unavailable: Unknown native command");
        JSONObject previous=q.optJSONObject("previous");JSONObject current=null;
        if(previous!=null){current=event(cal,previous.getString("id"));editable(current);if(!same(current,previous))throw new IllegalStateException("conflict: Event changed after review");}
        ArrayList<ContentProviderOperation> batch=new ArrayList<>();
        String[] account=cal.getString("account").split("\n",2);
        batch.add(ContentProviderOperation.newAssertQuery(CalendarContract.Calendars.CONTENT_URI)
            .withSelection("_id=? AND account_type=? AND account_name=? AND calendar_access_level>=?",new String[]{cal.getString("id"),account[0],account[1],"500"}).withExpectedCount(1).build());
        if(previous!=null){
            String where="_id=? AND calendar_id=? AND deleted=0 AND COALESCE(title,'')=? AND dtstart=? AND dtend=? AND COALESCE(eventTimezone,'UTC')=? AND allDay=? AND COALESCE(eventLocation,'')=? AND COALESCE(description,'')=? AND COALESCE(rrule,'')='' AND COALESCE(rdate,'')='' AND original_id IS NULL AND COALESCE(hasAttendeeData,0)=0";
            String[] args={previous.getString("id"),cal.getString("id"),previous.getString("title"),previous.get("start_ms").toString(),previous.get("end_ms").toString(),previous.getString("timezone"),previous.getBoolean("all_day")?"1":"0",previous.getString("location"),previous.getString("notes")};
            batch.add(ContentProviderOperation.newAssertQuery(CalendarContract.Events.CONTENT_URI).withSelection(where,args).withExpectedCount(1).build());
        }
        if("delete".equals(op)){
            if(previous==null)throw new IllegalStateException("invalid_arguments: Delete requires reviewed event");
            batch.add(ContentProviderOperation.newDelete(ContentUris.withAppendedId(CalendarContract.Events.CONTENT_URI,Long.parseLong(previous.getString("id")))).withExpectedCount(1).build());
        }else{
            JSONObject e=q.getJSONObject("event");String zone=e.getString("timezone");
            if(!java.util.Arrays.asList(TimeZone.getAvailableIDs()).contains(zone)&&!"UTC".equals(zone))throw new IllegalStateException("invalid_arguments: Unknown timezone");
            ContentValues v=new ContentValues();v.put("calendar_id",Long.parseLong(cal.getString("id")));v.put("title",e.getString("title"));v.put("dtstart",e.getLong("start_ms"));v.put("dtend",e.getLong("end_ms"));
            v.put("eventTimezone",zone);v.put("allDay",e.getBoolean("all_day")?1:0);v.put("eventLocation",e.getString("location"));v.put("description",e.getString("notes"));
            if(previous==null)batch.add(ContentProviderOperation.newInsert(CalendarContract.Events.CONTENT_URI).withValues(v).build());
            else batch.add(ContentProviderOperation.newUpdate(ContentUris.withAppendedId(CalendarContract.Events.CONTENT_URI,Long.parseLong(previous.getString("id")))).withValues(v).withExpectedCount(1).build());
        }
        live(q.getString("id"));
        if(!granted()||closed||!foreground.getAsBoolean())throw new IllegalStateException("authorization_required: Calendar access changed before commit");
        ContentProviderResult[] result=activity.getContentResolver().applyBatch(CalendarContract.AUTHORITY,batch);
        if("delete".equals(op))return new JSONObject().put("deleted",true).put("event_id",previous.getString("id"));
        String id=previous!=null?previous.getString("id"):Long.toString(ContentUris.parseId(result[result.length-1].uri));
        return new JSONObject().put("saved",true).put("event",event(cal,id));
    }
}
