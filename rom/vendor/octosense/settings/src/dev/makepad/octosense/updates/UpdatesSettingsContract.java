package dev.makepad.octosense.updates;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;

/** Finite Settings actions and review identity. Contains no download or install API. */
public final class UpdatesSettingsContract {
    private UpdatesSettingsContract() {}
    public static final long REVIEW_TTL_MS=10*60*1000;
    public static final long OBSERVATION_TTL_MS=20000;
    public static String key(String key) {
        if(key==null||!key.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid update key");
        return key;
    }
    public static String part(String part) {
        if(!"rom".equals(part)&&!"home".equals(part)) throw new IllegalArgumentException("Invalid update part");
        return part;
    }
    public static boolean recent(long observed,long now,long ttl) {
        return observed>=0&&now>=observed&&now-observed<=ttl;
    }
    public static String fingerprint(String... fields) {
        try {
            MessageDigest digest=MessageDigest.getInstance("SHA-256");
            for(String field:fields) {
                byte[] bytes=field.getBytes(StandardCharsets.UTF_8);
                digest.update(ByteBuffer.allocate(4).putInt(bytes.length).array());digest.update(bytes);
            }
            StringBuilder out=new StringBuilder();
            for(byte b:digest.digest()) out.append(String.format(java.util.Locale.ROOT,"%02x",b&255));
            return out.toString();
        } catch(java.security.NoSuchAlgorithmException e) {throw new IllegalStateException(e);}
    }
    /** Consumed before queueing, so duplicate clicks cannot enqueue another install. */
    public static final class Review {
        public final String key;
        private final String source,rom;
        private final long home,observed;
        private final boolean newerRom,newerHome;
        private boolean consumed;
        public Review(String manifest,String source,String rom,long home,long observed,boolean newerRom,boolean newerHome) {
            this.source=source;this.rom=rom;this.home=home;this.observed=observed;
            this.newerRom=newerRom;this.newerHome=newerHome;
            key=fingerprint("update-review-v1",manifest,source,rom,Long.toString(home));
        }
        public synchronized boolean available(String part,String currentSource,String currentRom,long currentHome,long now) {
            part(part);
            return current(currentSource,currentRom,currentHome,now)&&("rom".equals(part)?newerRom:newerHome);
        }
        public synchronized boolean current(String currentSource,String currentRom,long currentHome,long now) {
            return !consumed&&recent(observed,now,REVIEW_TTL_MS)&&source.equals(currentSource)&&rom.equals(currentRom)&&home==currentHome;
        }
        public synchronized boolean claim(String key,String part,String source,String rom,long home,long now) {
            key(key);
            if(!this.key.equals(key)||!available(part,source,rom,home,now)) return false;
            consumed=true;return true;
        }
    }
}
