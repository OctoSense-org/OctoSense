package dev.makepad.octosense.bluetooth;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Locale;

public final class BluetoothSettingsContract {
    private BluetoothSettingsContract() {}
    public static final int MAX_DEVICES=256;
    public enum Action {
        PAIR("pair"), CANCEL_PAIR("cancel_pair"), CONNECT("connect"), DISCONNECT("disconnect"), FORGET("forget");
        public final String wire;Action(String wire) {this.wire=wire;}
        public static Action parse(String value) {for(Action action:values()) if(action.wire.equals(value)) return action;throw new IllegalArgumentException("Unknown Bluetooth action");}
    }
    public static String key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid device target");return value;
    }
    public static String name(String value) {
        if(value==null||value.codePoints().allMatch(c -> Character.isWhitespace(c)||Character.isSpaceChar(c))
                ||value.getBytes(StandardCharsets.UTF_8).length>248||value.codePoints().anyMatch(Character::isISOControl))
            throw new IllegalArgumentException("Invalid Bluetooth name");return value;
    }
    public static boolean enabled(Object value) {
        if(!(value instanceof Boolean)) throw new IllegalArgumentException("Boolean required");return (Boolean)value;
    }
    public static String sharingKind(String value) {
        if(!"phonebook".equals(value)&&!"messages".equals(value)) throw new IllegalArgumentException("Invalid sharing kind");return value;
    }
    public static String sharingValue(String value) {
        if(!"ask".equals(value)&&!"allow".equals(value)&&!"deny".equals(value)) throw new IllegalArgumentException("Invalid sharing value");return value;
    }
    public static String fingerprint(String address) {
        if(address==null||!address.matches("[0-9A-F]{2}(:[0-9A-F]{2}){5}")) throw new IllegalArgumentException("Invalid Bluetooth address");
        try {
            byte[] digest=MessageDigest.getInstance("SHA-256").digest(("bluetooth:"+address).getBytes(StandardCharsets.UTF_8));
            StringBuilder out=new StringBuilder();for(byte value:digest) out.append(String.format(Locale.ROOT,"%02x",value&255));return out.toString();
        } catch(java.security.NoSuchAlgorithmException impossible) {throw new AssertionError(impossible);}
    }
}
