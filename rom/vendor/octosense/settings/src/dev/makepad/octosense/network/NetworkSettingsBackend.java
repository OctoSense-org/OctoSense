package dev.makepad.octosense.network;

import android.Manifest;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.net.ConnectivityManager;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.os.SystemClock;
import android.os.UserManager;
import android.provider.Settings;
import org.json.JSONObject;

/** Shared readback; hidden radio/network-policy APIs remain in the ROM helper. */
public final class NetworkSettingsBackend {
    public interface Platform {
        Boolean dataSaver();
        Boolean dnsValidated(LinkProperties properties);
        boolean airplane(boolean enabled);
        boolean dataSaver(boolean enabled);
        boolean privateDns(String mode,String hostname);
    }
    private final Context context;
    private final boolean helper;
    private final Platform platform;
    private String observedKey;
    private long observedAt=-1;
    public NetworkSettingsBackend(Context context,boolean helper,Platform platform) {this.context=context;this.helper=helper;this.platform=platform;}
    private boolean has(String permission) {return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED;}
    private boolean unlocked() {
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();
    }
    private boolean admin() {UserManager users=context.getSystemService(UserManager.class);return users!=null&&users.isAdminUser();}
    private boolean restricted(String name) {UserManager users=context.getSystemService(UserManager.class);return users==null||users.hasUserRestriction(name);}
    private String read(String key) {return Settings.Global.getString(context.getContentResolver(),key);}
    private static Object optional(Object value) {return value==null?JSONObject.NULL:value;}
    private static final class State {
        String dnsMode,hostname,key;
        Boolean airplane,dataSaver;
        boolean canAirplane,canDataSaver,canDns,dnsObserved;
    }
    private State read() {
        State state=new State();String airplane=null,rawMode=null,defaultMode=null,hostname=null;
        try {
            airplane=read("airplane_mode_on");rawMode=read("private_dns_mode");defaultMode=read("private_dns_default_mode");hostname=read("private_dns_specifier");
            state.dnsObserved=true;
            state.airplane=NetworkSettingsContract.observedBoolean(airplane);
            state.dnsMode=NetworkSettingsContract.observedMode(rawMode,defaultMode);
            if(hostname!=null&&!hostname.isEmpty()) {
                try {state.hostname=NetworkSettingsContract.hostname(hostname);}catch(IllegalArgumentException invalid) {}
            }
            if("hostname".equals(state.dnsMode)&&state.hostname==null) state.dnsMode=null;
        } catch(SecurityException unavailable) {}
        try {
            if(platform!=null) state.dataSaver=platform.dataSaver();
            else {
                ConnectivityManager cm=context.getSystemService(ConnectivityManager.class);
                if(cm!=null) switch(cm.getRestrictBackgroundStatus()) {
                    case ConnectivityManager.RESTRICT_BACKGROUND_STATUS_DISABLED:state.dataSaver=false;break;
                    case ConnectivityManager.RESTRICT_BACKGROUND_STATUS_ENABLED:
                    case ConnectivityManager.RESTRICT_BACKGROUND_STATUS_WHITELISTED:state.dataSaver=true;break;
                    default:break;
                }
            }
        } catch(SecurityException|IllegalStateException unavailable) {}
        boolean writable=helper&&platform!=null&&unlocked();
        state.canAirplane=writable&&state.airplane!=null&&admin()&&!restricted("no_airplane_mode")&&has("android.permission.NETWORK_SETTINGS");
        state.canDataSaver=writable&&state.dataSaver!=null&&admin()&&!restricted("no_config_mobile_networks")&&has("android.permission.MANAGE_NETWORK_POLICY");
        state.canDns=writable&&state.dnsObserved&&admin()&&!restricted("disallow_config_private_dns")&&has(Manifest.permission.WRITE_SECURE_SETTINGS);
        // Use raw values in identity so normalization cannot conceal an external edit.
        state.key=NetworkSettingsContract.fingerprint("network-settings-v1",Integer.toString(android.os.Process.myUid()),String.valueOf(airplane),String.valueOf(rawMode),String.valueOf(defaultMode),String.valueOf(hostname),String.valueOf(state.dataSaver));
        return state;
    }
    public synchronized JSONObject snapshot(long id) throws Exception {
        if(id<=0) throw new IllegalArgumentException("Positive request ID required");
        State state=read();observedKey=state.key;observedAt=SystemClock.elapsedRealtime();
        Boolean connected=null,dnsActive=null;String transport=null,internet="unknown",dnsValidation="unknown";
        try {
            ConnectivityManager cm=context.getSystemService(ConnectivityManager.class);
            if(cm!=null) {
                Network network=cm.getActiveNetwork();connected=network!=null;
                if(network==null) dnsValidation="offline";
                else {
                    NetworkCapabilities caps=cm.getNetworkCapabilities(network);LinkProperties properties=cm.getLinkProperties(network);
                    if(caps!=null) {
                        transport=caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)?"vpn":caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)?"wifi"
                            :caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)?"mobile":caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)?"ethernet":"other";
                        internet=caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)?"validated"
                            :caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_CAPTIVE_PORTAL)?"captive_portal":"local_only";
                    }
                    if(properties!=null) {
                        dnsActive=properties.isPrivateDnsActive();
                        Boolean valid=platform==null?null:platform.dnsValidated(properties);
                        if(valid!=null) dnsValidation=valid?"validated":"unvalidated";
                    }
                }
            }
        } catch(SecurityException|IllegalStateException unavailable) {}
        return new JSONObject().put("schema",1).put("request_id",id).put("key",state.key)
            .put("airplane",new JSONObject().put("enabled",optional(state.airplane)).put("can_set",state.canAirplane))
            .put("data_saver",new JSONObject().put("enabled",optional(state.dataSaver)).put("can_set",state.canDataSaver))
            .put("private_dns",new JSONObject().put("mode",optional(state.dnsMode)).put("hostname",optional(state.hostname))
                .put("active",optional(dnsActive)).put("validation",dnsValidation).put("can_set",state.canDns))
            .put("connection",new JSONObject().put("connected",optional(connected)).put("transport",optional(transport)).put("internet",internet));
    }
    private State target(String key) {
        NetworkSettingsContract.key(key);
        if(!key.equals(observedKey)||!NetworkSettingsContract.recent(observedAt,SystemClock.elapsedRealtime())) return null;
        State state=read();return key.equals(state.key)?state:null;
    }
    public synchronized String airplane(String key,boolean enabled) {
        State state=target(key);if(state==null) return "network_target_changed";
        if(!state.canAirplane) return "network_unavailable";observedAt=-1;
        try {if(!platform.airplane(enabled)) return "network_unavailable";return Boolean.valueOf(enabled).equals(read().airplane)?"network_applied":"network_requested";}
        catch(SecurityException|IllegalStateException unavailable) {return "network_unavailable";}
    }
    public synchronized String dataSaver(String key,boolean enabled) {
        State state=target(key);if(state==null) return "network_target_changed";
        if(!state.canDataSaver) return "network_unavailable";observedAt=-1;
        try {if(!platform.dataSaver(enabled)) return "network_unavailable";return Boolean.valueOf(enabled).equals(read().dataSaver)?"network_applied":"network_requested";}
        catch(SecurityException|IllegalStateException unavailable) {return "network_unavailable";}
    }
    public synchronized String privateDns(String key,String mode,String hostname) {
        String normalized=NetworkSettingsContract.dnsHostname(mode,hostname);
        State state=target(key);if(state==null) return "network_target_changed";
        if(!state.canDns) return "network_unavailable";observedAt=-1;
        try {
            if(!platform.privateDns(mode,normalized)) return "network_unavailable";
            State observed=read();return mode.equals(observed.dnsMode)&&(!mode.equals("hostname")||normalized.equals(observed.hostname))?"network_applied":"network_requested";
        } catch(SecurityException|IllegalStateException unavailable) {return "network_unavailable";}
    }
}
