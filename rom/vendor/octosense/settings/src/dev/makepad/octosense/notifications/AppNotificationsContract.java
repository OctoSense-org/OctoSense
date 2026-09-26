package dev.makepad.octosense.notifications;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Locale;
import org.json.JSONArray;
import org.json.JSONObject;

/** Finite Settings operations; no raw channel IDs, parcels, UIDs or permission payloads. */
public final class AppNotificationsContract {
    private AppNotificationsContract() {}
    public static final int PAGE_SIZE=20,MAX_ROWS=6000;
    public enum Action {
        APP_ENABLED("app_enabled"),GROUP_ENABLED("group_enabled"),CHANNEL_ENABLED("channel_enabled"),CHANNEL_IMPORTANCE("channel_importance");
        public final String wire;Action(String wire){this.wire=wire;}
        public static Action parse(String wire){for(Action action:values())if(action.wire.equals(wire))return action;throw new IllegalArgumentException("Unknown notification action");}
    }
    public static String packageName(String value){
        if(value==null||value.length()>255||(!value.equals("android")&&!value.contains("."))||!value.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*"))throw new IllegalArgumentException("Invalid package");return value;
    }
    public static String key(String value){if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed key");return value;}
    public static void page(long id,int offset,String generation){if(id<=0||offset<0||offset>=MAX_ROWS||offset%PAGE_SIZE!=0||offset>0&&generation==null)throw new IllegalArgumentException("Invalid page");if(generation!=null)key(generation);}
    public static String value(Action action,String value){
        if(value==null)throw new IllegalArgumentException("Missing notification choice");
        if(action==Action.CHANNEL_IMPORTANCE){if(!value.equals("min")&&!value.equals("low")&&!value.equals("default")&&!value.equals("high"))throw new IllegalArgumentException("Invalid importance");}
        else if(!"on".equals(value)&&!"off".equals(value))throw new IllegalArgumentException("Invalid notification choice");return value;
    }
    public static int importance(String value){switch(value){case "min":return 1;case "low":return 2;case "default":return 3;case "high":return 4;default:throw new IllegalArgumentException("Invalid importance");}}
    public static String importance(int value){switch(value){case -1000:return "unspecified";case 0:return "none";case 1:return "min";case 2:return "low";case 3:return "default";case 4:return "high";default:return null;}}
    public static String hash(String value){try{byte[] bytes=MessageDigest.getInstance("SHA-256").digest(value.getBytes(StandardCharsets.UTF_8));StringBuilder out=new StringBuilder();for(byte b:bytes)out.append(String.format(Locale.ROOT,"%02x",b&255));return out.toString();}catch(Exception impossible){throw new IllegalStateException(impossible);}}
    public static String text(CharSequence value,int limit){if(value==null)return "";StringBuilder out=new StringBuilder();value.toString().codePoints().filter(c->!Character.isISOControl(c)&&Character.getType(c)!=Character.FORMAT).limit(limit).forEach(out::appendCodePoint);return out.toString();}
    public static JSONObject unavailable(long id,String pkg,String availability)throws Exception{return new JSONObject().put("schema",1).put("request_id",id).put("package",packageName(pkg)).put("exists",JSONObject.NULL).put("availability",availability).put("key",JSONObject.NULL).put("app",JSONObject.NULL).put("generation",JSONObject.NULL).put("offset",0).put("total",0).put("truncated",false).put("stale",false).put("rows",new JSONArray());}
    /** Only system/policy fixed are administrator locks. USER_FIXED remains an explicit user choice. */
    public static boolean appWritable(boolean known,boolean requested,boolean permissionFixed,boolean importanceLocked,boolean suspended){return known&&requested&&!permissionFixed&&!importanceLocked&&!suspended;}
    public static boolean channelWritable(boolean appLocked,boolean blockable,boolean suspended){return !suspended&&(!appLocked||blockable);}
    public static boolean channelToggleWritable(boolean appLocked,boolean blockable,boolean enabled,boolean suspended){return !suspended&&(!appLocked||blockable||!enabled);}
    /** Default-channel and app permission changes are two service calls, never an atomic transaction. */
    public interface LinkedWrite {
        boolean allowed() throws Exception;
        void channel() throws Exception;
        void app() throws Exception;
        boolean confirmed() throws Exception;
    }
    public static String linkedWrite(LinkedWrite write){
        boolean channelDone=false;
        try{if(!write.allowed())return "notifications_restricted";write.channel();channelDone=true;
            if(!write.allowed())return "notifications_partial";write.app();
            return write.confirmed()?"notifications_applied":"notifications_unconfirmed";
        }catch(Exception failure){return channelDone?"notifications_partial":"notifications_unconfirmed";}
    }
    /** A fresh read renews authority without churning an unchanged open review. Claims never replay. */
    public static final class Lease {
        private final String salt=java.util.UUID.randomUUID().toString();
        private String fingerprint,key;private long at=-1,nonce;private boolean claimed;
        public synchronized String observe(String fingerprint,long now){if(key==null||claimed||!fingerprint.equals(this.fingerprint)){this.fingerprint=fingerprint;key=hash(salt+":"+fingerprint+":"+(++nonce)+":"+now);claimed=false;}at=now;return key;}
        public synchronized boolean claim(String supplied,String fresh,long now){if(key==null||claimed||!key.equals(supplied)||!fingerprint.equals(fresh)||at<0||now<at||now-at>20_000)return false;claimed=true;return true;}
        public synchronized void retire(){key=null;at=-1;claimed=true;}
    }
}
