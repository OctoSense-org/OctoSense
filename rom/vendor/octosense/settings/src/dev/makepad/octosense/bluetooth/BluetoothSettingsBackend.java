package dev.makepad.octosense.bluetooth;

import android.Manifest;
import android.app.KeyguardManager;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothClass;
import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothManager;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.SystemClock;
import android.os.UserManager;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import org.json.JSONArray;
import org.json.JSONObject;

/** Bounded Bluetooth observations. Reads never initiate discovery or pairing. */
public final class BluetoothSettingsBackend implements AutoCloseable {
    public interface Platform {
        String connection(BluetoothDevice device);
        String sharing(BluetoothDevice device,String kind);
        boolean action(BluetoothDevice device,BluetoothSettingsContract.Action action);
        boolean sharing(BluetoothDevice device,String kind,String value);
    }
    private final Context context;
    private final BluetoothAdapter adapter;
    private final boolean helper;
    private final Platform platform;
    private final LinkedHashMap<String,Seen> seen=new LinkedHashMap<>();
    private final Map<String,Target> observed=new HashMap<>();
    private long observedAt,discoveryRequestedAt;
    private boolean ownsDiscovery,registered;
    private static final class Seen {
        final BluetoothDevice device;final long at;
        Seen(BluetoothDevice device,long at) {this.device=device;this.at=at;}
    }
    private static final class Target {
        final BluetoothDevice device;final int bond;final String key;
        final boolean nearby;final Long age;
        Target(BluetoothDevice device,int bond,boolean nearby,Long age) {
            this.device=device;this.bond=bond;this.nearby=nearby;this.age=age;
            this.key=BluetoothSettingsContract.fingerprint(device.getAddress());
        }
    }
    public BluetoothSettingsBackend(Context context,boolean helper,Platform platform) {
        this.context=context;this.helper=helper;this.platform=platform;
        BluetoothManager manager=context.getSystemService(BluetoothManager.class);
        adapter=manager==null?null:manager.getAdapter();
        IntentFilter filter=new IntentFilter();
        filter.addAction(BluetoothDevice.ACTION_FOUND);
        filter.addAction(BluetoothAdapter.ACTION_DISCOVERY_FINISHED);
        filter.addAction(BluetoothAdapter.ACTION_STATE_CHANGED);
        if(Build.VERSION.SDK_INT>=33) context.registerReceiver(receiver,filter,Context.RECEIVER_EXPORTED);
        else context.registerReceiver(receiver,filter);
        registered=true;
    }
    private final BroadcastReceiver receiver=new BroadcastReceiver() {
        @Override public void onReceive(Context ignored,Intent intent) {
            synchronized(BluetoothSettingsBackend.this) {
                String action=intent.getAction();
                if(BluetoothAdapter.ACTION_DISCOVERY_FINISHED.equals(action)) ownsDiscovery=false;
                if(BluetoothAdapter.ACTION_STATE_CHANGED.equals(action)&&intent.getIntExtra(BluetoothAdapter.EXTRA_STATE,-1)!=BluetoothAdapter.STATE_ON) {
                    ownsDiscovery=false;seen.clear();observed.clear();
                }
                if(!BluetoothDevice.ACTION_FOUND.equals(action)||!ownsDiscovery||!canRead()) return;
                BluetoothDevice device=intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE);
                if(device==null) return;
                try {
                    String address=device.getAddress();BluetoothSettingsContract.fingerprint(address);
                    seen.remove(address);seen.put(address,new Seen(device,SystemClock.elapsedRealtime()));
                    while(seen.size()>BluetoothSettingsContract.MAX_DEVICES) seen.remove(seen.keySet().iterator().next());
                } catch(SecurityException|IllegalArgumentException unavailable) { /* No observed target. */ }
            }
        }
    };
    private boolean has(String permission) {return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED;}
    private boolean unlocked() {
        UserManager users=context.getSystemService(UserManager.class);
        KeyguardManager keyguard=context.getSystemService(KeyguardManager.class);
        return users!=null&&users.isUserUnlocked()&&keyguard!=null&&!keyguard.isKeyguardLocked();
    }
    private boolean restricted(String key) {
        UserManager users=context.getSystemService(UserManager.class);return users==null||users.hasUserRestriction(key);
    }
    private boolean canRead() {return adapter!=null&&unlocked()&&has(Build.VERSION.SDK_INT>=31?Manifest.permission.BLUETOOTH_CONNECT:Manifest.permission.BLUETOOTH);}
    private boolean canScan() {
        return canRead()&&adapter.isEnabled()&&!restricted(UserManager.DISALLOW_CONFIG_BLUETOOTH)
                &&has(Build.VERSION.SDK_INT>=31?Manifest.permission.BLUETOOTH_SCAN:Manifest.permission.BLUETOOTH_ADMIN)
                &&(Build.VERSION.SDK_INT>=31||has(Manifest.permission.ACCESS_FINE_LOCATION));
    }
    private boolean manage() {
        return canRead()&&helper&&platform!=null&&has("android.permission.BLUETOOTH_PRIVILEGED")
                &&!restricted(UserManager.DISALLOW_CONFIG_BLUETOOTH)&&!restricted(UserManager.DISALLOW_BLUETOOTH);
    }
    private static Object nullable(Object value) {return value==null?JSONObject.NULL:value;}
    private static String display(String value) {
        if(value==null) return null;
        StringBuilder out=new StringBuilder();value.codePoints().filter(c -> !Character.isISOControl(c)).limit(248).forEach(out::appendCodePoint);
        return out.length()==0?null:out.toString();
    }
    private static String bond(int bond) {
        switch(bond) {case BluetoothDevice.BOND_NONE:return "none";case BluetoothDevice.BOND_BONDING:return "bonding";
            case BluetoothDevice.BOND_BONDED:return "bonded";default:return "unknown";}
    }
    private static String kind(BluetoothClass type) {
        if(type==null) return "unknown";
        switch(type.getMajorDeviceClass()) {
            case BluetoothClass.Device.Major.AUDIO_VIDEO:return "audio";
            case BluetoothClass.Device.Major.COMPUTER:return "computer";
            case BluetoothClass.Device.Major.PHONE:return "phone";
            case BluetoothClass.Device.Major.PERIPHERAL:return "input";
            case BluetoothClass.Device.Major.WEARABLE:return "wearable";
            case BluetoothClass.Device.Major.IMAGING:return "imaging";
            case BluetoothClass.Device.Major.UNCATEGORIZED:return "unknown";
            default:return "other";
        }
    }
    private static String transport(int type) {
        switch(type) {case BluetoothDevice.DEVICE_TYPE_CLASSIC:return "classic";case BluetoothDevice.DEVICE_TYPE_LE:return "le";
            case BluetoothDevice.DEVICE_TYPE_DUAL:return "dual";default:return "unknown";}
    }
    private Map<String,Target> targets() {
        Map<String,Target> out=new HashMap<>();
        if(!canRead()||!adapter.isEnabled()) return out;
        long now=SystemClock.elapsedRealtime();
        seen.entrySet().removeIf(entry -> now-entry.getValue().at>60000);
        for(Seen item:seen.values()) {
            Target target=new Target(item.device,item.device.getBondState(),true,Math.max(0,now-item.at));out.put(target.key,target);
        }
        Set<BluetoothDevice> paired=adapter.getBondedDevices();
        if(paired!=null) for(BluetoothDevice device:paired) {
            String key=BluetoothSettingsContract.fingerprint(device.getAddress());Target previous=out.get(key);
            out.put(key,new Target(device,device.getBondState(),previous!=null,previous==null?null:previous.age));
        }
        return out;
    }
    private String connection(Target target) {
        if(platform==null) return null;
        try {return platform.connection(target.device);} catch(SecurityException unavailable) {return null;}
    }
    private JSONArray actions(Target target,String connection) {
        JSONArray out=new JSONArray();if(!manage()||!adapter.isEnabled()) return out;
        if(target.bond==BluetoothDevice.BOND_NONE&&target.nearby) out.put("pair");
        if(target.bond==BluetoothDevice.BOND_BONDING) out.put("cancel_pair");
        if(target.bond==BluetoothDevice.BOND_BONDED) {
            if("disconnected".equals(connection)) out.put("connect");
            if("connected".equals(connection)) out.put("disconnect");
            out.put("forget");
        }
        return out;
    }
    private JSONObject sharing(Target target,String kind) throws Exception {
        String value=null;
        if(platform!=null&&target.bond==BluetoothDevice.BOND_BONDED) {
            try {value=platform.sharing(target.device,kind);} catch(SecurityException unavailable) { /* Unknown. */ }
        }
        JSONArray options=new JSONArray();
        if(value!=null&&manage()&&target.bond==BluetoothDevice.BOND_BONDED&&!restricted("no_bluetooth_sharing"))
            options.put("ask").put("allow").put("deny");
        return new JSONObject().put("value",nullable(value)).put("options",options);
    }
    public synchronized JSONObject snapshot(long id) throws Exception {
        if(id<=0) throw new IllegalArgumentException("Positive request ID required");
        boolean present=adapter!=null&&context.getPackageManager().hasSystemFeature(PackageManager.FEATURE_BLUETOOTH);
        boolean read=present&&canRead();int state=read?adapter.getState():-1;
        String wire=!present?"unsupported":state==BluetoothAdapter.STATE_ON?"on":state==BluetoothAdapter.STATE_OFF?"off":
                state==BluetoothAdapter.STATE_TURNING_ON?"turning_on":state==BluetoothAdapter.STATE_TURNING_OFF?"turning_off":"unavailable";
        boolean on=state==BluetoothAdapter.STATE_ON;
        Boolean discovering=read?adapter.isDiscovering():null;
        if(!Boolean.TRUE.equals(discovering)&&SystemClock.elapsedRealtime()-discoveryRequestedAt>20000) ownsDiscovery=false;
        String name=read?adapter.getName():null;
        JSONArray caps=new JSONArray();
        if(manage()&&(on||state==BluetoothAdapter.STATE_OFF)) caps.put("toggle");
        if(canScan()&&!Boolean.TRUE.equals(discovering)) caps.put("scan");
        if(canScan()&&ownsDiscovery&&Boolean.TRUE.equals(discovering)) caps.put("stop_scan");
        if(manage()&&on&&name!=null) caps.put("rename");
        if(present&&!helper&&(!has(Manifest.permission.BLUETOOTH_CONNECT)||!has(Manifest.permission.BLUETOOTH_SCAN))) caps.put("request_access");
        boolean pairedAvailable=read&&on;
        Map<String,Target> targets;
        try {targets=targets();} catch(SecurityException unavailable) {targets=new HashMap<>();pairedAvailable=false;}
        ArrayList<Target> rows=new ArrayList<>(targets.values());
        rows.sort(Comparator.comparing((Target target) -> target.bond!=BluetoothDevice.BOND_BONDED).thenComparing(target -> target.key));
        observed.clear();JSONArray devices=new JSONArray();
        for(int i=0;i<Math.min(rows.size(),BluetoothSettingsContract.MAX_DEVICES);i++) {
            Target target=rows.get(i);String connection=connection(target);
            JSONObject row=new JSONObject().put("key",target.key).put("name",nullable(display(target.device.getName())))
                    .put("address",target.device.getAddress()).put("kind",kind(target.device.getBluetoothClass()))
                    .put("transport",transport(target.device.getType())).put("bond",bond(target.bond)).put("connection",nullable(connection))
                    .put("nearby",target.nearby).put("last_seen_age_ms",nullable(target.age)).put("actions",actions(target,connection))
                    .put("sharing",new JSONObject().put("phonebook",sharing(target,"phonebook")).put("messages",sharing(target,"messages")));
            devices.put(row);observed.put(target.key,target);
        }
        observedAt=SystemClock.elapsedRealtime();
        return new JSONObject().put("schema",1).put("request_id",id)
                .put("adapter",new JSONObject().put("state",wire).put("name",nullable(display(name))).put("discovering",nullable(discovering)))
                .put("capabilities",caps).put("paired_available",pairedAvailable).put("scan_available",canScan())
                .put("devices",devices).put("devices_total",rows.size()).put("truncated",rows.size()>BluetoothSettingsContract.MAX_DEVICES);
    }
    public synchronized String enabled(boolean value) {
        if(!manage()) return "bluetooth_unavailable";
        return (value?adapter.enable():adapter.disable())?"bluetooth_requested":"bluetooth_unavailable";
    }
    public synchronized String scan(boolean value) {
        if(!canScan()) return "bluetooth_unavailable";
        if(!value) {
            if(!ownsDiscovery||!adapter.isDiscovering()) return "bluetooth_unavailable";
            boolean accepted=adapter.cancelDiscovery();if(accepted) ownsDiscovery=false;
            return accepted?"bluetooth_requested":"bluetooth_unavailable";
        }
        if(adapter.isDiscovering()) return "bluetooth_unavailable";
        ownsDiscovery=adapter.startDiscovery();if(ownsDiscovery) discoveryRequestedAt=SystemClock.elapsedRealtime();
        return ownsDiscovery?"bluetooth_requested":"bluetooth_unavailable";
    }
    public synchronized String name(String value) {
        BluetoothSettingsContract.name(value);
        if(!manage()||!adapter.isEnabled()||!adapter.setName(value)) return "bluetooth_unavailable";
        return value.equals(adapter.getName())?"bluetooth_name_applied":"bluetooth_requested";
    }
    private Target target(String key) {
        BluetoothSettingsContract.key(key);Target previous=observed.get(key);
        if(previous==null||SystemClock.elapsedRealtime()-observedAt>20000) return null;
        Target current=targets().get(key);
        return current!=null&&current.bond==previous.bond?current:null;
    }
    public synchronized String device(String key,BluetoothSettingsContract.Action action) {
        if(!manage()||!adapter.isEnabled()) return "bluetooth_unavailable";
        Target target=target(key);if(target==null) return "bluetooth_target_changed";
        JSONArray permitted=actions(target,connection(target));boolean allowed=false;
        for(int i=0;i<permitted.length();i++) if(action.wire.equals(permitted.optString(i))) allowed=true;
        if(!allowed) return "bluetooth_target_changed";
        boolean accepted=action==BluetoothSettingsContract.Action.PAIR?target.device.createBond():platform.action(target.device,action);
        observed.clear();observedAt=0;
        return accepted?"bluetooth_requested":"bluetooth_unavailable";
    }
    public synchronized String sharing(String key,String kind,String value) {
        BluetoothSettingsContract.sharingKind(kind);BluetoothSettingsContract.sharingValue(value);
        if(!manage()||!adapter.isEnabled()||restricted("no_bluetooth_sharing")) return "bluetooth_unavailable";
        Target target=target(key);if(target==null||target.bond!=BluetoothDevice.BOND_BONDED) return "bluetooth_target_changed";
        if(platform.sharing(target.device,kind)==null||!platform.sharing(target.device,kind,value)) return "bluetooth_unavailable";
        return value.equals(platform.sharing(target.device,kind))?"bluetooth_sharing_applied":"bluetooth_requested";
    }
    @Override public synchronized void close() {
        if(ownsDiscovery&&canScan()) adapter.cancelDiscovery();
        if(registered) context.unregisterReceiver(receiver);
        registered=false;ownsDiscovery=false;seen.clear();observed.clear();
    }
}
