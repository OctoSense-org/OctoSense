package dev.makepad.octosense.permissions;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Common runtime groups and native, observed choices. Never accepts permission flags or a UID. */
public final class PermissionsSettingsContract {
    public static final int PAGE_SIZE=20,MAX_GROUPS=64;
    public static final long OBSERVATION_MS=20_000,TICKET_MS=120_000;
    private PermissionsSettingsContract(){}
    public enum Group {
        CAMERA("camera","CAMERA","Camera"),MICROPHONE("microphone","MICROPHONE","Microphone"),
        LOCATION("location","LOCATION","Location"),CONTACTS("contacts","CONTACTS","Contacts"),
        CALENDAR("calendar","CALENDAR","Calendar"),PHONE("phone","PHONE","Phone"),
        CALL_LOG("call_log","CALL_LOG","Call logs"),SMS("sms","SMS","SMS"),
        NEARBY_DEVICES("nearby_devices","NEARBY_DEVICES","Nearby devices"),
        ACTIVITY_RECOGNITION("activity_recognition","ACTIVITY_RECOGNITION","Physical activity"),
        SENSORS("sensors","SENSORS","Body sensors");
        public final String wire,nativeName,label;
        Group(String wire,String name,String label){this.wire=wire;nativeName="android.permission-group."+name;this.label=label;}
        public static Group parse(String value){for(Group group:values())if(group.wire.equals(value))return group;throw new IllegalArgumentException("Unknown permission group");}
        public static Group fromNative(String name){for(Group group:values())if(group.nativeName.equals(name))return group;return null;}
    }
    public enum Choice {
        ALLOW("allow","Allow"),ALLOW_ALWAYS("allow_always","Allow all the time"),
        ALLOW_FOREGROUND("allow_foreground","Allow only while using the app"),
        ASK("ask","Ask every time"),ONE_TIME("one_time","Allowed this time"),
        DENY("deny","Don't allow"),DENY_FOREGROUND("deny_foreground","Don't allow foreground access"),
        PRECISE("precise","Use precise location"),APPROXIMATE("approximate","Use approximate location");
        public final String wire,label;Choice(String wire,String label){this.wire=wire;this.label=label;}
        public static Choice parse(String value){for(Choice choice:values())if(choice.wire.equals(value))return choice;throw new IllegalArgumentException("Unknown permission choice");}
        public boolean actionable(){return this!=ONE_TIME;}
    }
    public static void packageName(String value){
        if(value==null||value.length()>255||!(value.equals("android")||value.contains("."))
                ||!value.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*"))throw new IllegalArgumentException("Invalid permission package");
    }
    public static void key(String value){if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid permission target");}
    public static void page(long id,String pkg,Group group,int offset,String generation){
        packageName(pkg);if(id<=0||offset<0||offset>=MAX_GROUPS||offset%PAGE_SIZE!=0||(offset>0&&generation==null)
                ||(group!=null&&(offset!=0||generation!=null)))throw new IllegalArgumentException("Invalid permission page");
        if(generation!=null)key(generation);
    }
    public static String text(CharSequence value,int limit){if(value==null)return "";StringBuilder out=new StringBuilder();value.toString().codePoints().filter(c->!Character.isISOControl(c)&&Character.getType(c)!=Character.FORMAT).limit(limit).forEach(out::appendCodePoint);return out.toString();}
    public static String hash(String value){try{byte[] bytes=MessageDigest.getInstance("SHA-256").digest(value.getBytes(StandardCharsets.UTF_8));StringBuilder out=new StringBuilder();for(byte b:bytes)out.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return out.toString();}catch(Exception impossible){throw new AssertionError(impossible);}}
    public static JSONObject unavailable(long id,String pkg,Group group,String availability)throws Exception{
        page(id,pkg,group,0,null);
        return new JSONObject().put("schema",1).put("request_id",id).put("package",pkg).put("exists",JSONObject.NULL)
                .put("availability",availability).put("group",group==null?JSONObject.NULL:group.wire)
                .put("key",JSONObject.NULL).put("generation",JSONObject.NULL).put("offset",0).put("total",0)
                .put("stale",false).put("truncated",false).put("groups",new JSONArray()).put("choices",new JSONArray()).put("detail",JSONObject.NULL);
    }
    public static final class Lease {
        private final String salt=UUID.randomUUID().toString();private long serial,observed;private String fingerprint,key;private boolean claimed;
        public String observe(String state,long now){if(!state.equals(fingerprint)||claimed||now<observed||now-observed>OBSERVATION_MS){fingerprint=state;key=hash(salt+":"+(++serial)+":"+state);claimed=false;}observed=now;return key;}
        public boolean claim(String expected,String state,long now){if(claimed||key==null||!key.equals(expected)||!state.equals(fingerprint)||now<observed||now-observed>OBSERVATION_MS)return false;claimed=true;return true;}
        public void retire(){fingerprint=null;key=null;claimed=true;}
    }
}
