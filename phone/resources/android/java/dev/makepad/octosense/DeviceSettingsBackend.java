package dev.makepad.octosense;

import android.Manifest;
import android.app.ActivityManager;
import android.app.admin.DevicePolicyManager;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.media.AudioManager;
import android.os.BatteryManager;
import android.os.Build;
import android.os.Environment;
import android.os.StatFs;
import android.os.UserManager;
import android.provider.Settings;
import android.text.format.DateFormat;
import dev.makepad.octosense.agent.AgentPlatformClient;
import org.json.JSONArray;
import org.json.JSONObject;

/** Worker-only adapter. No raw setting names, shell commands, or intents cross this boundary. */
final class DeviceSettingsBackend {
    private final Context context;
    private final AgentPlatformClient agent;
    DeviceSettingsBackend(Context context, AgentPlatformClient agent) {
        this.context=context;this.agent=agent;
    }
    private boolean restricted(String restriction) {
        UserManager users=context.getSystemService(UserManager.class);
        return users==null || users.hasUserRestriction(restriction);
    }
    private boolean systemWrite() { return Settings.System.canWrite(context) || (agent!=null&&agent.has("settings")); }
    private boolean secureWrite() {
        return (agent!=null&&agent.has("settings")) || context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED;
    }
    private boolean allowed(DeviceSetting setting) {
        if(setting.audio()) {
            AudioManager audio=context.getSystemService(AudioManager.class);
            return audio!=null&&!audio.isVolumeFixed()&&!restricted(UserManager.DISALLOW_ADJUST_VOLUME)
                    &&context.checkSelfPermission(Manifest.permission.MODIFY_AUDIO_SETTINGS)==PackageManager.PERMISSION_GRANTED;
        }
        if(setting==DeviceSetting.SCREEN_TIMEOUT&&restricted("no_config_screen_timeout")) return false;
        if((setting==DeviceSetting.AUTO_TIME||setting==DeviceSetting.AUTO_TIME_ZONE||setting==DeviceSetting.HOUR_FORMAT)
                &&restricted("no_config_date_time")) return false;
        return setting.table.equals("global") ? secureWrite() : systemWrite();
    }
    private long maximumTimeout() {
        DevicePolicyManager policy=context.getSystemService(DevicePolicyManager.class);
        return policy==null ? 0 : policy.getMaximumTimeToLock(null);
    }
    private int stream(DeviceSetting setting) {
        switch(setting) {
            case VOLUME_MEDIA:return AudioManager.STREAM_MUSIC;
            case VOLUME_ALARM:return AudioManager.STREAM_ALARM;
            case VOLUME_RING:return AudioManager.STREAM_RING;
            case VOLUME_NOTIFICATION:return AudioManager.STREAM_NOTIFICATION;
            default:throw new IllegalArgumentException("Audio setting required");
        }
    }
    private Object read(DeviceSetting setting) {
        try {
            if(setting.audio()) {
                AudioManager audio=context.getSystemService(AudioManager.class);
                int stream=stream(setting),max=audio.getStreamMaxVolume(stream);
                return max>0 ? audio.getStreamVolume(stream)/(double)max : JSONObject.NULL;
            }
            if(setting==DeviceSetting.FONT_SCALE) {
                return (double)Settings.System.getFloat(context.getContentResolver(),Settings.System.FONT_SCALE,
                        context.getResources().getConfiguration().fontScale);
            }
            if(setting==DeviceSetting.HOUR_FORMAT) return DateFormat.is24HourFormat(context);
            String raw=setting.table.equals("system") ? Settings.System.getString(context.getContentResolver(),setting.key)
                    : Settings.Global.getString(context.getContentResolver(),setting.key);
            if(raw==null) return JSONObject.NULL;
            switch(setting) {
                case FONT_SCALE:return Double.valueOf(raw);
                case SCREEN_TIMEOUT:return Long.valueOf(raw);
                default:return Integer.parseInt(raw)!=0;
            }
        } catch(RuntimeException unavailable) { return JSONObject.NULL; }
    }
    JSONObject snapshot(long requestId) throws Exception {
        JSONObject values=new JSONObject();JSONArray capabilities=new JSONArray();
        for(DeviceSetting setting:DeviceSetting.values()) {
            values.put(setting.wire,read(setting));
            if(allowed(setting)) capabilities.put(setting.wire);
        }
        JSONObject result=new JSONObject().put("schema",1).put("request_id",requestId)
                .put("values",values).put("capabilities",capabilities)
                .put("can_request_write_settings",!systemWrite());
        JSONObject about=new JSONObject().put("manufacturer",Build.MANUFACTURER).put("model",Build.MODEL)
                .put("android_version",Build.VERSION.RELEASE).put("api_level",Build.VERSION.SDK_INT)
                .put("security_patch",Build.VERSION.SECURITY_PATCH).put("build",Build.DISPLAY);
        try {about.put("kernel",android.system.Os.uname().release);} catch(Exception unavailable) { }
        ActivityManager manager=context.getSystemService(ActivityManager.class);
        if(manager!=null) {
            ActivityManager.MemoryInfo memory=new ActivityManager.MemoryInfo();manager.getMemoryInfo(memory);
            about.put("memory_total",memory.totalMem).put("memory_available",memory.availMem);
        }
        result.put("about",about);
        JSONObject storage=new JSONObject();
        try {
            StatFs stat=new StatFs(Environment.getDataDirectory().getPath());
            storage.put("total_bytes",stat.getTotalBytes()).put("available_bytes",stat.getAvailableBytes());
        } catch(RuntimeException unavailable) { }
        result.put("storage",storage);
        JSONObject battery=new JSONObject();
        Intent state=context.registerReceiver(null,new IntentFilter(Intent.ACTION_BATTERY_CHANGED));
        if(state!=null) {
            int level=state.getIntExtra(BatteryManager.EXTRA_LEVEL,-1),scale=state.getIntExtra(BatteryManager.EXTRA_SCALE,-1);
            if(level>=0&&scale>0&&level<=scale) battery.put("level",100.0*level/scale);
            String status;
            switch(state.getIntExtra(BatteryManager.EXTRA_STATUS,-1)) {
                case BatteryManager.BATTERY_STATUS_CHARGING:status="charging";break;
                case BatteryManager.BATTERY_STATUS_DISCHARGING:status="discharging";break;
                case BatteryManager.BATTERY_STATUS_FULL:status="full";break;
                case BatteryManager.BATTERY_STATUS_NOT_CHARGING:status="not_charging";break;
                default:status="unknown";
            }
            String plugged;
            switch(state.getIntExtra(BatteryManager.EXTRA_PLUGGED,-1)) {
                case 0:plugged="battery";break;
                case BatteryManager.BATTERY_PLUGGED_AC:plugged="ac";break;
                case BatteryManager.BATTERY_PLUGGED_USB:plugged="usb";break;
                case BatteryManager.BATTERY_PLUGGED_WIRELESS:plugged="wireless";break;
                case 8:plugged="dock";break;
                default:plugged="unknown";
            }
            battery.put("status",status).put("plugged",plugged);
            if(state.hasExtra(BatteryManager.EXTRA_TEMPERATURE)) battery.put("temperature_celsius",state.getIntExtra(BatteryManager.EXTRA_TEMPERATURE,0)/10.0);
        }
        result.put("battery",battery);
        java.util.Date now=new java.util.Date();
        String localTime=DateFormat.getDateFormat(context).format(now)+" "+DateFormat.getTimeFormat(context).format(now);
        result.put("date_time",new JSONObject().put("local_time",localTime)
                .put("time_zone",java.util.TimeZone.getDefault().getID()).put("locale",java.util.Locale.getDefault().toLanguageTag()));
        // An unavailable privileged time detector must not discard ordinary
        // battery, storage and clock observations from this snapshot.
        JSONObject timeControls=null;
        try {if(agent!=null) timeControls=agent.dateTimeSnapshot(requestId);} catch(Exception unavailable) { }
        result.put("time_controls",timeControls==null?JSONObject.NULL:timeControls);
        java.util.TreeSet<String> zones=new java.util.TreeSet<>(java.time.ZoneId.getAvailableZoneIds());
        if(zones.size()<=1024) result.put("time_zones",new JSONArray(zones));
        result.put("max_screen_timeout_ms",maximumTimeout());
        return result;
    }
    /** False means no observed confirmation, even if a write was accepted. Never replay automatically. */
    boolean apply(DeviceSetting setting,Object value) throws Exception {
        setting.validate(value);
        if(!allowed(setting)) throw new SecurityException("Device setting unavailable");
        if(setting==DeviceSetting.SCREEN_TIMEOUT) {
            long maximum=maximumTimeout();
            if(maximum>0&&((Number)value).longValue()>maximum) throw new SecurityException("Screen timeout exceeds policy");
        }
        if(setting.audio()) {
            AudioManager audio=context.getSystemService(AudioManager.class);int stream=stream(setting);
            int target=(int)Math.round(((Number)value).doubleValue()*audio.getStreamMaxVolume(stream));
            if(Build.VERSION.SDK_INT>=28) target=Math.max(audio.getStreamMinVolume(stream),target);
            audio.setStreamVolume(stream,target,0);
            // AudioService may apply a stream change asynchronously. Confirm on
            // this worker, with a bounded wait, without blocking the renderer.
            for(int attempt=0;attempt<20;attempt++) {
                if(audio.getStreamVolume(stream)==target) return true;
                Thread.sleep(25);
            }
            return false;
        }
        String stored=setting.storedValue(value);
        boolean saved;
        if(agent!=null&&agent.has("settings")) saved=agent.applyDeviceSetting(setting,value);
        else if(setting.table.equals("system")) saved=Settings.System.putString(context.getContentResolver(),setting.key,stored);
        else saved=Settings.Global.putString(context.getContentResolver(),setting.key,stored);
        if(!saved) return false;
        Object observed=read(setting);
        if(observed instanceof Number&&value instanceof Number) return Math.abs(((Number)observed).doubleValue()-((Number)value).doubleValue())<.0001;
        return observed.equals(value);
    }
}
