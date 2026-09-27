package dev.makepad.octosense.sounds;

import android.app.KeyguardManager;
import android.content.Context;
import android.database.Cursor;
import android.media.AudioAttributes;
import android.media.AudioManager;
import android.media.Ringtone;
import android.media.RingtoneManager;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.os.UserManager;
import android.provider.Settings;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;
import static dev.makepad.octosense.sounds.SoundSettingsContract.*;

/** Worker-only bounded sound catalog and observed-target operations. Preview never changes volume/DND. */
public final class SoundSettingsBackend implements AutoCloseable {
    private final Context context;
    private final java.util.function.BooleanSupplier activeOwner;
    private final Handler handler=new Handler(Looper.getMainLooper());
    private Cache cache;
    private Ringtone preview;
    private long previewGeneration;
    private static final class Row {
        final String key,title;final Uri uri;
        Row(Type type,String title,Uri uri) {
            this.title=title;this.uri=uri;this.key=fingerprint(type.wire,uri==null?"silent":uri.toString(),title);
        }
    }
    private static final class Inventory {
        final List<Row> rows;final boolean truncated;
        Inventory(List<Row> rows,boolean truncated) {this.rows=rows;this.truncated=truncated;}
    }
    private static final class Cache {
        final Type type;final String key,current,title;final Uri currentUri;final Inventory inventory;final Observed observed;
        Cache(Type type,Uri uri,String title,Inventory inventory,int user,long now) {
            this.type=type;this.currentUri=uri;this.current=stored(uri);this.title=title;this.inventory=inventory;
            this.key=fingerprint(UUID.randomUUID().toString(),type.wire,current);
            this.observed=new Observed(key,type,user,current,now);
        }
    }
    public SoundSettingsBackend(Context context,java.util.function.BooleanSupplier activeOwner) {this.context=context;this.activeOwner=java.util.Objects.requireNonNull(activeOwner);}
    private int user() {return context.getApplicationInfo().uid/100000;}
    private boolean unlocked() {
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager keyguard=context.getSystemService(KeyguardManager.class);
        return activeOwner.getAsBoolean()&&user()==0&&users!=null&&users.isUserUnlocked()&&keyguard!=null&&!keyguard.isKeyguardLocked();
    }
    private boolean restricted() {
        UserManager users=context.getSystemService(UserManager.class);
        return users==null||users.hasUserRestriction(UserManager.DISALLOW_ADJUST_VOLUME);
    }
    private boolean writable() {return unlocked()&&!restricted()&&Settings.System.canWrite(context);}
    private boolean playable() {return unlocked()&&!restricted()&&context.getSystemService(AudioManager.class)!=null;}
    private static String stored(Uri uri) {return uri==null?"":uri.toString();}
    private static Object nullable(Object value) {return value==null?JSONObject.NULL:value;}
    private static String display(String value) {
        if(value==null)return null;
        StringBuilder out=new StringBuilder();value.codePoints().filter(c->!Character.isISOControl(c)).limit(128).forEach(out::appendCodePoint);
        return out.length()==0?null:out.toString();
    }
    private Uri actual(Type type) {return RingtoneManager.getActualDefaultRingtoneUri(context,type.androidType);}
    private Uri canonical(Uri uri) {
        if(uri==null)return null;
        try {Uri value=context.getContentResolver().canonicalize(uri);return value==null?uri:value;}catch(RuntimeException ignored){return uri;}
    }
    private static boolean media(Uri uri) {
        if(uri==null||!"content".equals(uri.getScheme())||!"media".equals(uri.getAuthority()))return false;
        List<String> segments=uri.getPathSegments();
        return segments.size()==4&&(segments.get(0).equals("internal")||segments.get(0).equals("external")||segments.get(0).equals("external_primary"))
                &&segments.get(1).equals("audio")&&segments.get(2).equals("media")&&segments.get(3).matches("[0-9]+");
    }
    private Inventory inventory(Type type) {
        RingtoneManager manager=new RingtoneManager(context);manager.setType(type.androidType);
        Cursor cursor=null;Map<String,Row> unique=new LinkedHashMap<>();boolean truncated=false;
        try {
            cursor=manager.getCursor();if(cursor==null)throw new IllegalStateException("Sound catalog unavailable");
            int count=cursor.getCount();truncated=count>MAX_SCAN;
            for(int i=0;i<Math.min(count,MAX_SCAN);i++) {
                if(!cursor.moveToPosition(i))throw new IllegalStateException("Incomplete sound catalog");
                String title=display(cursor.getString(RingtoneManager.TITLE_COLUMN_INDEX));Uri uri=manager.getRingtoneUri(i);
                if(title!=null&&media(uri))unique.putIfAbsent(uri.toString(),new Row(type,title,uri));
            }
        } finally {if(cursor!=null)cursor.close();manager.stopPreviousRingtone();}
        List<Row> rows=new ArrayList<>(unique.values());rows.sort((a,b)->{int byTitle=a.title.compareToIgnoreCase(b.title);return byTitle!=0?byTitle:a.key.compareTo(b.key);});
        truncated|=rows.size()>MAX_ROWS-1;
        if(rows.size()>MAX_ROWS-1)rows=new ArrayList<>(rows.subList(0,MAX_ROWS-1));
        rows.add(0,new Row(type,"Silent",null));return new Inventory(rows,truncated);
    }
    private String title(Uri current,Inventory inventory) {
        if(current==null)return "Silent";
        Uri canonical=canonical(current);
        for(Row row:inventory.rows)if(canonical.equals(row.uri))return row.title;
        Ringtone ringtone=null;
        try {ringtone=RingtoneManager.getRingtone(context,current);return ringtone==null?null:display(ringtone.getTitle(context));}
        catch(RuntimeException unavailable){return null;}
        finally {if(ringtone!=null)ringtone.stop();}
    }
    private JSONObject denied(long id,Type type,String status) throws Exception {
        stop();cache=null;
        return new JSONObject().put("schema",1).put("request_id",id).put("type",type.wire).put("status",status).put("key",JSONObject.NULL)
                .put("current",new JSONObject().put("state","unavailable").put("title",JSONObject.NULL))
                .put("offset",0).put("total",0).put("truncated",false).put("rows",new JSONArray())
                .put("can_save",false).put("can_preview",false).put("can_request_access",false);
    }
    public synchronized JSONObject snapshot(long id,String wire,String key,int offset) throws Exception {
        if(id<=0)throw new IllegalArgumentException("Positive request required");Type type=Type.parse(wire);offset(offset,key);
        if(!unlocked()||restricted())return denied(id,type,"restricted");
        try {
            Uri current=actual(type);
            if(key==null) {
                stop();Inventory inventory=inventory(type);
                cache=new Cache(type,current,title(current,inventory),inventory,user(),SystemClock.elapsedRealtime());
            } else if(cache==null||!cache.observed.matches(key,type,user(),stored(current),SystemClock.elapsedRealtime()))return denied(id,type,"expired");
            if(offset>=cache.inventory.rows.size())return denied(id,type,"expired");
            if(!unlocked()||restricted())return denied(id,type,"restricted");
            if(!stored(actual(type)).equals(cache.current))return denied(id,type,"expired");
            JSONArray rows=new JSONArray();Uri canonical=canonical(current);
            for(int i=offset;i<Math.min(offset+PAGE_SIZE,cache.inventory.rows.size());i++) {
                Row row=cache.inventory.rows.get(i);cache.observed.expose(row.key);
                rows.put(new JSONObject().put("key",row.key).put("title",row.title).put("silent",row.uri==null)
                        .put("selected",row.uri==null?current==null:row.uri.equals(canonical)));
            }
            return new JSONObject().put("schema",1).put("request_id",id).put("type",type.wire).put("status","ready").put("key",cache.key)
                    .put("current",new JSONObject().put("state",current==null?"silent":"sound").put("title",nullable(cache.title)))
                    .put("offset",offset).put("total",cache.inventory.rows.size()).put("truncated",cache.inventory.truncated).put("rows",rows)
                    .put("can_save",writable()).put("can_preview",playable()).put("can_request_access",!Settings.System.canWrite(context));
        } catch(RuntimeException unavailable) {return denied(id,type,"unavailable");}
    }
    /** Re-resolves membership/type/title from a new Android cursor before any operation. */
    private Row target(Type type,String key,String target) {
        key(key);key(target);
        if(cache==null||!cache.observed.permits(key,type,user(),stored(actual(type)),target,SystemClock.elapsedRealtime()))return null;
        for(Row row:inventory(type).rows)if(row.key.equals(target))return row;
        return null;
    }
    public synchronized String save(String wire,String key,String target) {
        Type type=Type.parse(wire);key(key);key(target);stop();
        if(!unlocked()||restricted()) {cache=null;return "sound_restricted";}
        if(!writable())return "sound_unavailable";
        try {
            Row row=target(type,key,target);if(row==null)return "sound_target_changed";
            if(row.uri!=null) {String mime=context.getContentResolver().getType(row.uri);if(mime==null||!(mime.startsWith("audio/")||mime.equals("application/ogg")||mime.equals("application/x-flac")))return "sound_unavailable";}
            if(!writable())return "sound_restricted";
            if(!cache.observed.permits(key,type,user(),stored(actual(type)),target,SystemClock.elapsedRealtime()))return "sound_target_changed";
            RingtoneManager.setActualDefaultRingtoneUri(context,type.androidType,row.uri);
            Uri value=canonical(actual(type));boolean confirmed=row.uri==null?value==null:row.uri.equals(value);
            cache=null;return confirmed?"sound_applied":"sound_unconfirmed";
        } catch(RuntimeException unavailable) {cache=null;return "sound_unavailable";}
    }
    public synchronized String preview(String wire,String key,String target) {
        Type type=Type.parse(wire);key(key);key(target);stop();
        if(!playable())return "sound_restricted";
        try {
            Row row=target(type,key,target);if(row==null)return "sound_target_changed";
            if(row.uri==null)return "sound_silent";
            if(!playable())return "sound_restricted";
            preview=RingtoneManager.getRingtone(context,row.uri);if(preview==null)return "sound_unavailable";
            int usage=type==Type.ALARM?AudioAttributes.USAGE_ALARM:type==Type.NOTIFICATION?AudioAttributes.USAGE_NOTIFICATION:AudioAttributes.USAGE_NOTIFICATION_RINGTONE;
            preview.setAudioAttributes(new AudioAttributes.Builder().setUsage(usage).setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build());
            preview.setLooping(false);preview.play();long generation=previewGeneration;
            // Maximum five seconds; a periodic guard also stops on lock/revoked policy.
            handler.postDelayed(new Runnable(){int ticks=0;@Override public void run(){synchronized(SoundSettingsBackend.this){
                if(generation!=previewGeneration)return;
                if(++ticks>=20||!playable())stop();else handler.postDelayed(this,250);
            }}},250);
            return preview.isPlaying()?"sound_preview_started":"sound_preview_requested";
        } catch(RuntimeException unavailable) {stop();return "sound_unavailable";}
    }
    /** Unconditional: leaving the page must be able to stop an expired preview. */
    public synchronized void stop() {previewGeneration++;if(preview!=null){try{preview.stop();}catch(RuntimeException ignored){}preview=null;}}
    public synchronized void invalidate() {stop();cache=null;}
    @Override public void close() {invalidate();}
}
