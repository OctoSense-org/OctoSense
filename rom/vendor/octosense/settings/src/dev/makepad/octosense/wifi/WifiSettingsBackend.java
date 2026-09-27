package dev.makepad.octosense.wifi;

import android.Manifest;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.location.LocationManager;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkInfo;
import android.net.wifi.ScanResult;
import android.net.wifi.WifiConfiguration;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.SystemClock;
import android.os.UserManager;
import android.provider.Settings;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONObject;

/** Shared observations; direct mutations require the ROM helper's NETWORK_SETTINGS.
 * All public entry points serialize on this instance. No WifiConfiguration, credential,
 * EAP identity, certificate alias or raw intent is sent to the renderer. */
public final class WifiSettingsBackend implements AutoCloseable {
    private final Context context;
    private final WifiManager wifi;
    private final boolean helper;
    private String scanState="idle";
    private long scanStarted;
    private final Map<String,Row> observed=new HashMap<>();
    private long observedAt;
    private boolean registered;
    public WifiSettingsBackend(Context context,boolean helper) {
        this.context=context;this.helper=helper;wifi=context.getSystemService(WifiManager.class);
        IntentFilter filter=new IntentFilter(WifiManager.SCAN_RESULTS_AVAILABLE_ACTION);
        if(Build.VERSION.SDK_INT>=33) context.registerReceiver(scanReceiver,filter,Context.RECEIVER_EXPORTED);
        else context.registerReceiver(scanReceiver,filter);
        registered=true;
    }
    private final BroadcastReceiver scanReceiver=new BroadcastReceiver() {
        @Override public void onReceive(Context c,Intent intent) {
            // SCAN_RESULTS_AVAILABLE_ACTION is a protected system broadcast.
            synchronized(WifiSettingsBackend.this) {
                scanState=intent.getBooleanExtra(WifiManager.EXTRA_RESULTS_UPDATED,false)?"idle":"unavailable";
                scanStarted=0;
            }
        }
    };
    @Override public synchronized void close() {
        if(registered) {context.unregisterReceiver(scanReceiver);registered=false;}
        observed.clear();observedAt=0;
    }
    private boolean has(String permission) {return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED;}
    private boolean restricted(String name) {
        UserManager users=context.getSystemService(UserManager.class);
        return users==null||!users.isUserUnlocked()||users.hasUserRestriction(name);
    }
    private boolean unlocked() {
        UserManager users=context.getSystemService(UserManager.class);
        android.app.KeyguardManager lock=context.getSystemService(android.app.KeyguardManager.class);
        return users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();
    }
    private boolean privileged() {return helper&&has("android.permission.NETWORK_SETTINGS");}
    private boolean canManage() {return wifi!=null&&privileged()&&unlocked()&&!restricted(UserManager.DISALLOW_CONFIG_WIFI);}
    private boolean canToggle() {return canManage()&&!restricted("no_change_wifi_state")&&has(Manifest.permission.CHANGE_WIFI_STATE);}
    private boolean canReadScan() {
        if(wifi==null||!unlocked()||!has(Manifest.permission.ACCESS_WIFI_STATE)) return false;
        if(privileged()) return true;
        LocationManager location=context.getSystemService(LocationManager.class);
        return has(Manifest.permission.ACCESS_FINE_LOCATION)&&location!=null&&(Build.VERSION.SDK_INT>=28
                ?location.isLocationEnabled():location.isProviderEnabled(LocationManager.GPS_PROVIDER)||location.isProviderEnabled(LocationManager.NETWORK_PROVIDER));
    }
    private static Object nullable(Object value) {return value==null?JSONObject.NULL:value;}
    private static String unquote(String value) {
        if(value==null||value.equals(WifiManager.UNKNOWN_SSID)) return null;
        return value.length()>1&&value.startsWith("\"")&&value.endsWith("\"")?value.substring(1,value.length()-1):value;
    }
    private static String security(ScanResult scan) {
        String caps=scan.capabilities==null?"":scan.capabilities;
        if(caps.contains("EAP")||caps.contains("SUITE_B")) return "eap";
        if(caps.contains("SAE")) return caps.contains("PSK")?"wpa2_wpa3":"wpa3";
        if(caps.contains("PSK")) return "wpa2";
        if(caps.contains("OWE_TRANSITION")) return "open_owe";
        if(caps.contains("OWE")) return "owe";
        if(caps.contains("WEP")) return "wep";
        return caps.contains("RSN")||caps.contains("WPA")?"unknown":"open";
    }
    private static String security(WifiConfiguration config) {
        if(config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.WPA_EAP)
                ||config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.IEEE8021X)
                ||config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.SUITE_B_192)) return "eap";
        if(config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.SAE))
            return config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.WPA_PSK)?"wpa2_wpa3":"wpa3";
        if(config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.WPA_PSK)) return "wpa2";
        if(config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.OWE)) return "owe";
        if(config.wepKeys!=null) for(String key:config.wepKeys) if(key!=null) return "wep";
        return config.allowedKeyManagement.get(WifiConfiguration.KeyMgmt.NONE)?"open":"unknown";
    }
    private static final class Row {
        final String key,rawSsid,security; final int networkId;
        boolean available,connected;Integer signal;
        Row(int networkId,String rawSsid,String security) {
            this.networkId=networkId;this.rawSsid=rawSsid;this.security=security;
            key=WifiSettingsContract.fingerprint(networkId,rawSsid,security);
        }
        JSONObject json(boolean manage,boolean enabled,boolean configure) throws Exception {
            JSONArray actions=new JSONArray();
            if(manage&&networkId>=0) {
                if(enabled&&!connected) actions.put("connect");
                actions.put("forget");
            }
            if(configure) actions.put("configure");
            return new JSONObject().put("key",key).put("ssid",nullable(WifiSettingsContract.displaySsid(rawSsid)))
                    .put("security",security).put("signal_level",nullable(signal)).put("available",available)
                    .put("saved",networkId>=0).put("connected",connected).put("actions",actions);
        }
    }
    private static final class Observation {
        final ArrayList<Row> rows=new ArrayList<>();
        boolean scanAvailable,savedAvailable;Long scanAge;
    }
    private Observation networks(int connectedId) {
        Observation out=new Observation();
        if(wifi==null) return out;
        if(privileged()&&unlocked()) {
            try {
                List<WifiConfiguration> configs=wifi.getConfiguredNetworks();
                out.savedAvailable=configs!=null;
                Map<String,Row> saved=new HashMap<>();
                if(configs!=null) for(WifiConfiguration config:configs) {
                    String name=unquote(config.SSID);
                    if(config.networkId<0||name==null) continue;
                    String identity=WifiSettingsContract.fingerprint(config.networkId,name,"");
                    Row previous=saved.get(identity);
                    String mode=security(config);
                    if(previous!=null) mode=WifiSettingsContract.mergedSecurity(previous.security,mode);
                    Row row=new Row(config.networkId,name,mode);
                    row.connected=config.networkId==connectedId;
                    saved.put(identity,row);
                }
                out.rows.addAll(saved.values());
            } catch(SecurityException unavailable) {out.savedAvailable=false;}
        }
        if(canReadScan()) {
            try {
                List<ScanResult> scans=wifi.getScanResults();
                out.scanAvailable=scans!=null;
                Map<String,Row> unsaved=new HashMap<>();
                long now=SystemClock.elapsedRealtime();
                if(scans!=null) for(ScanResult scan:scans) {
                    String name=scan.SSID;
                    if(name==null||name.isEmpty()) continue;
                    String security=security(scan);
                    long age=Math.max(0,now-scan.timestamp/1000);
                    if(out.scanAge==null||age<out.scanAge) out.scanAge=age;
                    boolean found=false;
                    for(Row row:out.rows) if(row.networkId>=0&&row.rawSsid.equals(name)&&WifiSettingsContract.compatible(row.security,security)) {
                        updateScan(row,scan,age);found=true;
                    }
                    if(!found) {
                        String key=WifiSettingsContract.fingerprint(-1,name,security);
                        Row row=unsaved.get(key);
                        if(row==null) {row=new Row(-1,name,security);unsaved.put(key,row);}
                        updateScan(row,scan,age);
                    }
                }
                out.rows.addAll(unsaved.values());
            } catch(SecurityException unavailable) {out.scanAvailable=false;}
        }
        out.rows.sort(Comparator.comparing((Row row) -> !row.connected)
                .thenComparing(row -> !row.available).thenComparing(row -> row.networkId<0)
                .thenComparing((Row row) -> row.signal==null?1:-row.signal)
                .thenComparing(row -> row.rawSsid).thenComparing(row -> row.key));
        return out;
    }
    private void updateScan(Row row,ScanResult scan,long age) {
        boolean recent=scan.timestamp>0&&age<=60000&&wifi.isWifiEnabled();
        row.available|=recent;
        if(recent) {
            int signal=WifiManager.calculateSignalLevel(scan.level,5);
            row.signal=row.signal==null?signal:Math.max(row.signal,signal);
        }
    }
    private JSONObject connection(WifiInfo info,Observation observed) throws Exception {
        if(!unlocked()) return null;
        JSONObject out=new JSONObject().put("internet","unknown");
        ConnectivityManager connectivity=context.getSystemService(ConnectivityManager.class);
        Network wifiNetwork=null;
        if(connectivity!=null) for(Network network:connectivity.getAllNetworks()) {
            NetworkCapabilities caps=connectivity.getNetworkCapabilities(network);
            NetworkInfo state=connectivity.getNetworkInfo(network);
            if(caps==null||!caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)||state==null||!state.isConnected()) continue;
            wifiNetwork=network;
            out.put("internet",caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)?"validated":
                    caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_CAPTIVE_PORTAL)?"captive_portal":"local_only");
            LinkProperties links=connectivity.getLinkProperties(network);
            if(links!=null) for(LinkAddress address:links.getLinkAddresses()) {
                if(!address.getAddress().isLoopbackAddress()&&!address.getAddress().isLinkLocalAddress()) {
                    out.put("ip_address",address.getAddress().getHostAddress());break;
                }
            }
            break;
        }
        if(wifiNetwork==null) return null;
        if(info==null) return out;
        out.put("ssid",nullable(WifiSettingsContract.displaySsid(unquote(info.getSSID()))));
        String bssid=info.getBSSID();
        if(bssid!=null&&!bssid.equals("02:00:00:00:00:00")) out.put("bssid",bssid);
        if(info.getRssi()>-127&&info.getRssi()<0) out.put("rssi_dbm",info.getRssi());
        if(info.getLinkSpeed()>0) out.put("link_speed_mbps",info.getLinkSpeed());
        if(info.getFrequency()>0) out.put("frequency_mhz",info.getFrequency());
        for(Row row:observed.rows) if(row.connected) {out.put("network_key",row.key);break;}
        return out;
    }
    public synchronized JSONObject snapshot(long requestId) throws Exception {
        if(requestId<=0) throw new IllegalArgumentException("Positive request ID required");
        boolean present=wifi!=null&&context.getPackageManager().hasSystemFeature(PackageManager.FEATURE_WIFI);
        int state=present?wifi.getWifiState():WifiManager.WIFI_STATE_UNKNOWN;
        Boolean enabled=state==WifiManager.WIFI_STATE_ENABLED?Boolean.TRUE:state==WifiManager.WIFI_STATE_DISABLED?Boolean.FALSE:null;
        WifiInfo info=present&&unlocked()?wifi.getConnectionInfo():null;
        Observation networks=networks(info==null||info.getSupplicantState()!=android.net.wifi.SupplicantState.COMPLETED?-1:info.getNetworkId());
        if(scanStarted>0&&networks.scanAge!=null&&SystemClock.elapsedRealtime()-networks.scanAge>=scanStarted) {
            scanState="idle";scanStarted=0;
        }
        if(scanStarted>0&&SystemClock.elapsedRealtime()-scanStarted>20000) {scanState="unavailable";scanStarted=0;}
        JSONArray caps=new JSONArray();
        if(canToggle()) caps.put("toggle");
        if(canReadScan()&&Boolean.TRUE.equals(enabled)&&has(Manifest.permission.CHANGE_WIFI_STATE)) caps.put("scan");
        if(present&&!helper&&!has(Manifest.permission.ACCESS_FINE_LOCATION)) caps.put("request_access");
        boolean configure=configureIntent()!=null;
        JSONArray rows=new JSONArray();observed.clear();
        for(int i=0;i<Math.min(networks.rows.size(),WifiSettingsContract.MAX_NETWORKS);i++) {
            Row row=networks.rows.get(i);observed.put(row.key,row);
            rows.put(row.json(canManage(),Boolean.TRUE.equals(enabled),configure));
        }
        observedAt=SystemClock.elapsedRealtime();
        return new JSONObject().put("schema",1).put("request_id",requestId).put("enabled",nullable(enabled))
                .put("connection",nullable(connection(info,networks))).put("capabilities",caps)
                .put("scan_state",networks.scanAvailable?scanState:"unavailable").put("scan_age_ms",nullable(networks.scanAge))
                .put("saved_available",networks.savedAvailable).put("scan_available",networks.scanAvailable)
                .put("networks",rows).put("networks_total",networks.rows.size())
                .put("truncated",networks.rows.size()>WifiSettingsContract.MAX_NETWORKS);
    }
    public synchronized String setEnabled(boolean enabled) {
        if(!canToggle()) return "wifi_unavailable";
        return wifi.setWifiEnabled(enabled)?"wifi_requested":"wifi_unavailable";
    }
    public synchronized String scan() {
        if(!canReadScan()||!wifi.isWifiEnabled()||!has(Manifest.permission.CHANGE_WIFI_STATE)) return "wifi_unavailable";
        if(scanStarted>0&&SystemClock.elapsedRealtime()-scanStarted<20000) return "wifi_scan_requested";
        if(!wifi.startScan()) {scanState="throttled";return "wifi_scan_throttled";}
        scanState="scanning";scanStarted=SystemClock.elapsedRealtime();return "wifi_scan_requested";
    }
    public synchronized String network(String key,WifiSettingsContract.Action action) {
        WifiSettingsContract.key(key);
        if(action==WifiSettingsContract.Action.CONFIGURE) throw new IllegalArgumentException("Configuration is an Activity flow");
        if(!canManage()) return "wifi_unavailable";
        Row previous=observed.get(key);
        if(previous==null||previous.networkId<0||SystemClock.elapsedRealtime()-observedAt>20000) return "wifi_target_changed";
        Observation current=networks(-1);
        Row target=null;
        for(Row row:current.rows) if(row.key.equals(key)&&row.networkId==previous.networkId) {target=row;break;}
        if(target==null) return "wifi_target_changed";
        // WifiService is the final authority for admin-owned network restrictions.
        boolean accepted=action==WifiSettingsContract.Action.FORGET?wifi.removeNetwork(target.networkId):
                wifi.isWifiEnabled()&&wifi.enableNetwork(target.networkId,true);
        observed.clear();observedAt=0;
        return accepted?"wifi_requested":"wifi_unavailable";
    }
    /** Resolve only a system Settings activity. Credentials never return to Home. */
    public Intent configureIntent() {
        Intent intent=new Intent(Settings.ACTION_WIFI_SETTINGS).setPackage("com.android.settings");
        ResolveInfo resolved=context.getPackageManager().resolveActivity(intent,PackageManager.MATCH_DEFAULT_ONLY|PackageManager.MATCH_SYSTEM_ONLY);
        if(resolved==null||resolved.activityInfo==null||(resolved.activityInfo.applicationInfo.flags
                &(ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_UPDATED_SYSTEM_APP))==0) return null;
        return intent.setComponent(new ComponentName(resolved.activityInfo.packageName,resolved.activityInfo.name));
    }
    public synchronized Intent configureObserved(String key) {
        WifiSettingsContract.key(key);
        Row row=observed.get(key);
        if(row==null||SystemClock.elapsedRealtime()-observedAt>20000||!unlocked()||restricted(UserManager.DISALLOW_CONFIG_WIFI)
                ||restricted("no_add_wifi_config")) return null;
        // AOSP/Lineage's exported credential editor accepts a WifiTracker entry
        // identity. It connects for its caller; no result or WifiConfiguration
        // (which could contain credentials) is requested or received here.
        int[] types;
        switch(row.security) {
            case "open":types=new int[]{0};break;
            case "owe":types=new int[]{6};break;
            case "open_owe":types=new int[]{0,6};break;
            case "wep":types=new int[]{1};break;
            case "wpa2":types=new int[]{2};break;
            case "wpa3":types=new int[]{4};break;
            case "wpa2_wpa3":types=new int[]{2,4};break;
            case "eap":types=new int[]{3,5,9};break;
            default:return configureIntent();
        }
        try {
            JSONArray security=new JSONArray();for(int type:types) security.put(type);
            String scanKey=new JSONObject().put("SSID",row.rawSsid).put("SECURITY_TYPES",security).toString();
            String entry="StandardWifiEntry:"+new JSONObject().put("SCAN_RESULT_KEY",scanKey)
                    .put("IS_TARGETING_NEW_NETWORKS",row.networkId<0);
            Intent intent=new Intent("com.android.settings.WIFI_DIALOG").setComponent(
                    new ComponentName("com.android.settings","com.android.settings.wifi.WifiDialogActivity"))
                    .putExtra("key_chosen_wifientry_key",entry).putExtra("connect_for_caller",true);
            ResolveInfo resolved=context.getPackageManager().resolveActivity(intent,PackageManager.MATCH_DEFAULT_ONLY|PackageManager.MATCH_SYSTEM_ONLY);
            if(resolved!=null&&resolved.activityInfo!=null&&(resolved.activityInfo.applicationInfo.flags
                    &(ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_UPDATED_SYSTEM_APP))!=0) return intent;
        } catch(org.json.JSONException impossible) {throw new IllegalStateException(impossible);}
        return configureIntent();
    }
}
