package dev.makepad.octosense.controls;

import android.Manifest;
import android.app.KeyguardManager;
import android.app.NotificationManager;
import android.app.admin.DevicePolicyManager;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.location.LocationManager;
import android.net.wifi.WifiManager;
import android.os.BatteryManager;
import android.os.Build;
import android.os.PowerManager;
import android.os.UserManager;
import android.provider.Settings;
import dev.makepad.octosense.controls.SettingsControlsContract.Control;
import dev.makepad.octosense.controls.SettingsControlsContract.Page;
import org.json.JSONArray;
import org.json.JSONObject;

/** Shared observed controls. Hidden API hooks exist only in the privileged ROM service. */
public final class SettingsControlsBackend {
    public interface Platform {
        boolean supported(Control control);
        String read(Control control);
        boolean writable(Control control);
        boolean apply(Control control,String value);
        default String[] choices(Control control){return control.choices();}
        default String applyResult(Control control,String value){return !apply(control,value)?"control_unavailable":value.equals(read(control))?"control_applied":"control_requested";}

    }
    private final Context context;
    private final boolean helper;
    private final Platform platform;
    private final ColorAccessibility.Backend colors;
    public SettingsControlsBackend(Context context,boolean helper,Platform platform) {
        this.context=context;this.helper=helper;this.platform=platform;
        this.colors=new ColorAccessibility.Backend(new ColorAccessibility.Store() {
            @Override public String read(ColorAccessibility.Setting setting) {return Settings.Secure.getString(context.getContentResolver(),setting.key);}
            @Override public boolean write(ColorAccessibility.Setting setting,int value) {return Settings.Secure.putInt(context.getContentResolver(),setting.key,value);}
        },() -> {
            if(!unlocked())return ColorAccessibility.Access.RESTRICTED;
            return helper&&context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED
                    &&platform!=null&&platform.writable(Control.COLOR_INVERSION)
                    ?ColorAccessibility.Access.WRITABLE:ColorAccessibility.Access.READ_ONLY;
        });
    }
    private boolean feature(String name) {return context.getPackageManager().hasSystemFeature(name);}
    private boolean supported(Control control) {
        switch(control) {
            case LOCATION_ENABLED: if(!feature(PackageManager.FEATURE_LOCATION)) return false;break;
            case WIFI_SCANNING: if(!feature(PackageManager.FEATURE_WIFI)) return false;break;
            case BLUETOOTH_SCANNING: if(!feature(PackageManager.FEATURE_BLUETOOTH_LE)) return false;break;
            case CAMERA_ACCESS: if(Build.VERSION.SDK_INT<31||!feature(PackageManager.FEATURE_CAMERA_ANY)) return false;break;
            case MICROPHONE_ACCESS: if(Build.VERSION.SDK_INT<31||!feature(PackageManager.FEATURE_MICROPHONE)) return false;break;
            case NOTIFICATION_HISTORY: if(Build.VERSION.SDK_INT<30) return false;break;
            case NOTIFICATION_BUBBLES: if(Build.VERSION.SDK_INT<30) return false;break;
            default:break;
        }
        return platform==null||platform.supported(control);
    }
    private boolean restriction(String name) {
        UserManager users=context.getSystemService(UserManager.class);
        return users==null||users.hasUserRestriction(name);
    }
    private boolean unlocked() {
        UserManager users=context.getSystemService(UserManager.class);
        KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();
    }
    private boolean restricted(Control control) {
        if(!unlocked()) return true;
        if((control==Control.DND_MODE||control.page==Page.SOUND_FEEDBACK)&&restriction(UserManager.DISALLOW_ADJUST_VOLUME)) return true;
        if(control.page==Page.LOCATION&&(restriction("no_config_location")||restriction("no_share_location"))) return true;
        if(control==Control.WIFI_SCANNING&&restriction(UserManager.DISALLOW_CONFIG_WIFI)) return true;
        if(control==Control.BLUETOOTH_SCANNING&&(restriction(UserManager.DISALLOW_CONFIG_BLUETOOTH)||restriction(UserManager.DISALLOW_BLUETOOTH))) return true;
        DevicePolicyManager policies=context.getSystemService(DevicePolicyManager.class);
        if(control==Control.CAMERA_ACCESS&&(restriction("no_camera")||(policies!=null&&policies.getCameraDisabled(null)))) return true;
        if(control==Control.MICROPHONE_ACCESS&&restriction(UserManager.DISALLOW_UNMUTE_MICROPHONE)) return true;
        if(control==Control.LOCKSCREEN_NOTIFICATIONS||control==Control.LOCKSCREEN_SENSITIVE) {
            if(policies==null) return true;
            int disabled=policies.getKeyguardDisabledFeatures(null);
            int flag=control==Control.LOCKSCREEN_NOTIFICATIONS?DevicePolicyManager.KEYGUARD_DISABLE_SECURE_NOTIFICATIONS:DevicePolicyManager.KEYGUARD_DISABLE_UNREDACTED_NOTIFICATIONS;
            if((disabled&flag)!=0) return true;
        }
        return false;
    }
    private String read(Control control) {
        if(control.colorSetting()!=null)return colors.read(control.colorSetting());
        if(control==Control.BATTERY_DISABLE_AT_90) {
            Integer threshold=globalInt("low_power_sticky_auto_disable_level",90),trigger=globalInt("low_power_trigger_level",0);
            if(threshold==null||trigger==null||Math.max(threshold,trigger)!=90) return null;
        }
        if(control.table!=null) {
            String raw=control.table.equals("secure")?Settings.Secure.getString(context.getContentResolver(),control.key)
                    :Settings.Global.getString(context.getContentResolver(),control.key);
            return control.observedRaw(raw);
        }
        if(platform!=null) return platform.read(control);
        switch(control) {
            case DND_MODE: {
                NotificationManager manager=context.getSystemService(NotificationManager.class);
                DndMode mode=manager==null?null:DndMode.fromInterruptionFilter(manager.getCurrentInterruptionFilter());
                return mode==null?null:mode.wire;
            }
            case LOCATION_ENABLED: {
                LocationManager manager=context.getSystemService(LocationManager.class);
                return manager==null||Build.VERSION.SDK_INT<28?null:manager.isLocationEnabled()?"on":"off";
            }
            case WIFI_SCANNING: {
                WifiManager manager=context.getSystemService(WifiManager.class);
                return manager==null?null:manager.isScanAlwaysAvailable()?"on":"off";
            }
            case BATTERY_SAVER: {
                PowerManager manager=context.getSystemService(PowerManager.class);
                return manager==null?null:manager.isPowerSaveMode()?"on":"off";
            }
            default:return null;
        }
    }
    private Integer globalInt(String key,int missing) {
        String raw=Settings.Global.getString(context.getContentResolver(),key);
        if(raw==null) return missing;
        if(!raw.matches("-?[0-9]+")) return null;
        try {return Integer.valueOf(raw);} catch(NumberFormatException invalid) {return null;}
    }
    private boolean writable(Control control) {
        if(!helper||restricted(control)) return false;
        if(control.colorSetting()!=null)return colors.writable();
        if(control==Control.LOCKSCREEN_SENSITIVE&&!"on".equals(read(Control.LOCKSCREEN_NOTIFICATIONS))) return false;
        if(control==Control.BATTERY_SAVER) {
            Intent battery=context.registerReceiver(null,new IntentFilter(Intent.ACTION_BATTERY_CHANGED));
            if(battery==null||battery.getIntExtra(BatteryManager.EXTRA_PLUGGED,-1)!=0) return false;
        }
        if(control==Control.BATTERY_THRESHOLD&&!Integer.valueOf(0).equals(globalInt("automatic_power_save_mode",0))) return false;
        if(control==Control.BATTERY_DISABLE_AT_90&&read(control)==null) return false;
        return control.table!=null?context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED
                :platform!=null&&platform.writable(control);
    }
    private JSONObject observation(Control control) throws Exception {
        String value=null,availability="unavailable";JSONArray options=new JSONArray();
        try {
            if(!supported(control)) availability="unsupported";
            else if(restricted(control)) {availability="restricted";value=read(control);}
            else {
                value=read(control);
                if(value!=null) {
                    availability="available";
                    if(writable(control)) for(String option:platform!=null&&(control.page==Page.SOUND_FEEDBACK||control.hearingSetting()!=null)?platform.choices(control):control.choices()) options.put(option);
                }
            }
        } catch(SecurityException|IllegalStateException unavailable) {availability="unavailable";options=new JSONArray();}
        return new JSONObject().put("id",control.wire).put("value",value==null?JSONObject.NULL:value)
                .put("options",options).put("availability",availability);
    }
    public JSONObject snapshot(long id,Page page) throws Exception {
        if(id<=0) throw new IllegalArgumentException("Positive request ID required");
        JSONArray controls=new JSONArray();
        for(Control control:Control.values()) if(control.page==page) controls.put(observation(control));
        return new JSONObject().put("schema",1).put("request_id",id).put("page",page.wire).put("controls",controls);
    }
    public synchronized String apply(Page page,Control control,String value) {
        if(control.page!=page) throw new IllegalArgumentException("Wrong control page");
        control.validate(value);
        try {
            if(restricted(control)) return "policy_restricted";
            if(!supported(control)||read(control)==null||!writable(control)) return "control_unavailable";
            if(control.colorSetting()!=null)return colors.apply(control.colorSetting(),value);
            boolean accepted;
            if(control.table!=null) {
                int stored=control.stored(value);
                accepted=control.table.equals("secure")?Settings.Secure.putInt(context.getContentResolver(),control.key,stored)
                        :Settings.Global.putInt(context.getContentResolver(),control.key,stored);
            } else return platform.applyResult(control,value);
            if(!accepted) return "control_unavailable";
            return value.equals(read(control))?"control_applied":"control_requested";
        } catch(SecurityException|IllegalStateException unavailable) {return "control_unavailable";}
    }
}
