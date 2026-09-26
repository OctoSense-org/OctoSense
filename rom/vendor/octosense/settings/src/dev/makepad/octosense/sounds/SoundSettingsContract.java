package dev.makepad.octosense.sounds;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HashSet;
import java.util.Locale;
import java.util.Set;

/** Finite sound types and observed catalog targets. No URI crosses the script boundary. */
public final class SoundSettingsContract {
    public static final int PAGE_SIZE=20, MAX_ROWS=1000, MAX_SCAN=5000;
    public static final long CATALOG_LIFETIME_MS=300000;
    private SoundSettingsContract() {}
    public enum Type {
        RINGTONE("ringtone",1), NOTIFICATION("notification",2), ALARM("alarm",4);
        public final String wire; public final int androidType;
        Type(String wire,int androidType) {this.wire=wire;this.androidType=androidType;}
        public static Type parse(String value) {
            for(Type type:values()) if(type.wire.equals(value)) return type;
            throw new IllegalArgumentException("Unknown sound type");
        }
    }
    public static String key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid sound target");
        return value;
    }
    public static int offset(int value,String key) {
        if(value<0||value>=MAX_ROWS||value%PAGE_SIZE!=0||value!=0&&key==null) throw new IllegalArgumentException("Invalid sound page");
        if(key!=null) key(key);return value;
    }
    public static boolean recent(long at,long now) {return at>=0&&now>=at&&now-at<=CATALOG_LIFETIME_MS;}
    public static String fingerprint(String... fields) {
        try {
            MessageDigest digest=MessageDigest.getInstance("SHA-256");
            for(String field:fields) {byte[] value=field.getBytes(StandardCharsets.UTF_8);digest.update(ByteBuffer.allocate(4).putInt(value.length).array());digest.update(value);}
            StringBuilder result=new StringBuilder();for(byte value:digest.digest())result.append(String.format(Locale.ROOT,"%02x",value&255));return result.toString();
        } catch(java.security.NoSuchAlgorithmException missing) {throw new IllegalStateException(missing);}
    }
    /** Browsing exposes only its returned rows; knowledge of another catalog row is not authority. */
    public static final class Observed {
        private final String key,current;
        private final Type type;
        private final int user;
        private final long at;
        private final Set<String> targets=new HashSet<>();
        public Observed(String key,Type type,int user,String current,long at) {
            this.key=key(key);this.type=type;this.user=user;this.current=current;this.at=at;
        }
        public void expose(String target) {targets.add(key(target));}
        public boolean matches(String key,Type type,int user,String current,long now) {
            return this.key.equals(key)&&this.type==type&&this.user==user&&this.current.equals(current)&&recent(at,now);
        }
        public boolean permits(String key,Type type,int user,String current,String target,long now) {
            return matches(key,type,user,current,now)&&targets.contains(target);
        }
    }
}
