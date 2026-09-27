package dev.makepad.octosense.network;

import java.net.IDN;
import java.util.Locale;

/** Finite radio/policy controls. No interface accepts SettingsProvider keys. */
public final class NetworkSettingsContract {
    private NetworkSettingsContract() {}
    public static String key(String key) {
        if(key==null||!key.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid network observation key");return key;
    }
    public static boolean recent(long at,long now) {return at>=0&&now>=at&&now-at<=20000;}
    public static String fingerprint(String... fields) {
        try {
            java.security.MessageDigest digest=java.security.MessageDigest.getInstance("SHA-256");
            for(String field:fields) {
                byte[] bytes=field.getBytes(java.nio.charset.StandardCharsets.UTF_8);
                digest.update(java.nio.ByteBuffer.allocate(4).putInt(bytes.length).array());digest.update(bytes);
            }
            StringBuilder result=new StringBuilder();for(byte b:digest.digest()) result.append(String.format(Locale.ROOT,"%02x",b&255));return result.toString();
        } catch(java.security.NoSuchAlgorithmException e) {throw new IllegalStateException(e);}
    }
    public static Boolean enabled(Object value) {
        if(!(value instanceof Boolean)) throw new IllegalArgumentException("Boolean required");return (Boolean)value;
    }
    public static String mode(String mode) {
        if(!"off".equals(mode)&&!"automatic".equals(mode)&&!"hostname".equals(mode)) throw new IllegalArgumentException("Invalid DNS mode");
        return mode;
    }
    public static String hostname(String value) {
        if(value==null||value.isEmpty()||value.length()>253) throw new IllegalArgumentException("Enter a DNS provider hostname");
        for(int i=0;i<value.length();i++) if(Character.isWhitespace(value.charAt(i))||Character.isISOControl(value.charAt(i)))
            throw new IllegalArgumentException("Hostname contains whitespace");
        String ascii=IDN.toASCII(value,IDN.USE_STD3_ASCII_RULES).toLowerCase(Locale.ROOT);
        if(ascii.endsWith(".")) ascii=ascii.substring(0,ascii.length()-1);
        if(ascii.isEmpty()||ascii.length()>253) throw new IllegalArgumentException("Invalid hostname length");
        String[] labels=ascii.split("\\.",-1);
        for(String label:labels) if(label.isEmpty()||label.length()>63||!label.matches("[a-z0-9](?:[a-z0-9-]*[a-z0-9])?"))
            throw new IllegalArgumentException("Enter a hostname, without a URL or IP address");
        if(Character.isDigit(labels[labels.length-1].charAt(0))) throw new IllegalArgumentException("IP addresses cannot authenticate DNS providers");
        return ascii;
    }
    public static String dnsHostname(String mode,Object value) {
        mode(mode);
        if("hostname".equals(mode)) {
            if(!(value instanceof String)) throw new IllegalArgumentException("Hostname required");return hostname((String)value);
        }
        if(value!=null) throw new IllegalArgumentException("Hostname only belongs to provider mode");
        return null;
    }
    public static String observedMode(String raw,String defaultMode) {
        if(raw==null||raw.isEmpty()) raw=defaultMode;
        if(raw==null||raw.isEmpty()) return "automatic"; // verified ConnectivitySettingsUtils default
        switch(raw) {case "off":return "off";case "opportunistic":return "automatic";case "hostname":return "hostname";default:return null;}
    }
    public static Boolean observedBoolean(String raw) {if("0".equals(raw))return Boolean.FALSE;if("1".equals(raw))return Boolean.TRUE;return null;}
}
