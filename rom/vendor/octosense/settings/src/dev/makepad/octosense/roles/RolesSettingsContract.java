package dev.makepad.octosense.roles;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Finite default-app navigation and platform confirmation; never a direct role grant. */
public final class RolesSettingsContract {
    public static final int PAGE_SIZE=20,MAX_ROWS=2000;
    public static final long OBSERVATION_MS=20_000,TICKET_MS=120_000;
    private RolesSettingsContract(){}
    public enum RoleId {
        BROWSER("browser","BROWSER","Browser"),HOME("home","HOME","Home app"),
        ASSISTANT("assistant","ASSISTANT","Digital assistant"),DIALER("dialer","DIALER","Phone app"),
        SMS("sms","SMS","SMS app"),CALL_SCREENING("call_screening","CALL_SCREENING","Call screening"),
        CALL_REDIRECTION("call_redirection","CALL_REDIRECTION","Call redirection"),WALLET("wallet","WALLET","Wallet");
        public final String wire,nativeName,label;
        RoleId(String wire,String name,String label){this.wire=wire;nativeName="android.app.role."+name;this.label=label;}
        public static RoleId parse(String wire){for(RoleId role:values())if(role.wire.equals(wire))return role;throw new IllegalArgumentException("Unknown default-app role");}
    }
    public static void key(String value){if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid role target");}
    public static void page(long id,RoleId role,int offset,String generation){
        if(id<=0||offset<0||offset>=MAX_ROWS||offset%PAGE_SIZE!=0||(offset>0&&generation==null)
                ||(role==null&&(offset!=0||generation!=null)))throw new IllegalArgumentException("Invalid role page");
        if(generation!=null)key(generation);
    }
    public static String text(CharSequence input,int limit){if(input==null)return "";StringBuilder out=new StringBuilder();input.toString().codePoints().filter(c->!Character.isISOControl(c)&&Character.getType(c)!=Character.FORMAT).limit(limit).forEach(out::appendCodePoint);return out.toString();}
    public static String hash(String input){try{byte[] bytes=MessageDigest.getInstance("SHA-256").digest(input.getBytes(StandardCharsets.UTF_8));StringBuilder out=new StringBuilder();for(byte b:bytes)out.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return out.toString();}catch(Exception impossible){throw new AssertionError(impossible);}}
    public static JSONObject unavailable(long id,RoleId role,String availability)throws Exception{
        if(id<=0)throw new IllegalArgumentException("Invalid role request");
        return new JSONObject().put("schema",1).put("request_id",id).put("availability",availability).put("roles",new JSONArray())
                .put("role",role==null?JSONObject.NULL:role.wire).put("key",JSONObject.NULL).put("generation",JSONObject.NULL)
                .put("offset",0).put("total",0).put("stale",false).put("truncated",false).put("candidates",new JSONArray()).put("none_target",JSONObject.NULL);
    }
    public static final class Lease {
        private final String salt=UUID.randomUUID().toString();private long serial,observed;private String fingerprint,key;private boolean claimed;
        public String observe(String state,long now){if(!state.equals(fingerprint)||claimed||now<observed||now-observed>OBSERVATION_MS){fingerprint=state;key=hash(salt+":"+(++serial)+":"+state);claimed=false;}observed=now;return key;}
        public boolean claim(String expected,String state,long now){if(claimed||key==null||!key.equals(expected)||!state.equals(fingerprint)||now<observed||now-observed>OBSERVATION_MS)return false;claimed=true;return true;}
        public void retire(){fingerprint=null;key=null;claimed=true;}
    }
}
