package dev.makepad.octosense.accounts;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Locale;
import java.util.LinkedHashMap;
import java.util.function.LongSupplier;

/** Finite actions and opaque observed identities; no caller-provided Account or sync Bundle. */
public final class AccountsSettingsContract {
    private AccountsSettingsContract() {}
    public static final int MAX_ACCOUNTS=128,MAX_PROVIDERS=128,MAX_AUTHORITIES=64;
    public enum SyncAction {
        AUTO("auto"),SYNC_NOW("sync_now"),CANCEL("cancel");
        public final String wire;SyncAction(String wire) {this.wire=wire;}
        public static SyncAction parse(String value) {
            for(SyncAction action:values()) if(action.wire.equals(value)) return action;
            throw new IllegalArgumentException("Unknown sync action");
        }
    }
    public static String key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid observed account target");
        return value;
    }
    public static boolean enabled(Object value) {
        if(!(value instanceof Boolean)) throw new IllegalArgumentException("Boolean required");return (Boolean)value;
    }
    public static Boolean syncValue(SyncAction action,Object value) {
        if(action==SyncAction.AUTO) return enabled(value);
        if(value!=null) throw new IllegalArgumentException("Only auto sync accepts a value");
        return null;
    }
    public static String fingerprint(String kind,int uid,String...parts) {
        try {
            MessageDigest digest=MessageDigest.getInstance("SHA-256");
            for(String value:concat(kind,Integer.toString(uid),parts)) {
                byte[] bytes=value.getBytes(StandardCharsets.UTF_8);
                digest.update(Integer.toString(bytes.length).getBytes(StandardCharsets.US_ASCII));digest.update((byte)':');digest.update(bytes);
            }
            StringBuilder out=new StringBuilder();for(byte value:digest.digest()) out.append(String.format(Locale.ROOT,"%02x",value&255));return out.toString();
        } catch(java.security.NoSuchAlgorithmException impossible) {throw new AssertionError(impossible);}
    }
    /** Bounded, expiring evidence of a returned target, independently testable without Android. */
    public static final class Observations {
        private final int maximum;private final long ttl;private final LongSupplier clock;
        private final LinkedHashMap<String,Long> entries=new LinkedHashMap<>();
        public Observations(int maximum,long ttl,LongSupplier clock) {
            if(maximum<=0||ttl<0) throw new IllegalArgumentException("Invalid observation bounds");
            this.maximum=maximum;this.ttl=ttl;this.clock=clock;
        }
        private void expire(long now) {entries.entrySet().removeIf(entry->now<entry.getValue()||now-entry.getValue()>ttl);}
        public synchronized void add(String key) {
            AccountsSettingsContract.key(key);long now=clock.getAsLong();expire(now);entries.remove(key);
            while(entries.size()>=maximum) entries.remove(entries.keySet().iterator().next());entries.put(key,now);
        }
        public synchronized boolean contains(String key) {expire(clock.getAsLong());return entries.containsKey(key);}
        public synchronized void clear() {entries.clear();}
    }
    private static String[] concat(String kind,String uid,String[] parts) {
        String[] values=new String[parts.length+2];values[0]=kind;values[1]=uid;System.arraycopy(parts,0,values,2,parts.length);return values;
    }
}
