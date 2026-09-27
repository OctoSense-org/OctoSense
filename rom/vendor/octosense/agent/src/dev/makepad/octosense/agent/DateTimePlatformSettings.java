package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.app.time.Capabilities;
import android.app.time.TimeCapabilitiesAndConfig;
import android.app.time.TimeConfiguration;
import android.app.time.TimeManager;
import android.app.time.TimeZoneCapabilitiesAndConfig;
import android.app.time.TimeZoneConfiguration;
import android.app.timedetector.TimeDetector;
import android.app.timedetector.TimeDetectorHelper;
import android.app.timezonedetector.TimeZoneDetector;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import dev.makepad.octosense.datetime.DateTimeContract;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.time.Instant;
import java.time.ZoneId;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.UUID;
import org.json.JSONObject;

/** Finite time-detector API. The system server retains final policy authority. */
final class DateTimePlatformSettings {
    private final Context context;
    private final String session=UUID.randomUUID().toString();
    private final LinkedHashMap<String,Long> observations=new LinkedHashMap<>();
    private long clockBase=Long.MIN_VALUE,clockGeneration;
    DateTimePlatformSettings(Context context) {this.context=context;}
    private boolean editable() {
        UserManager users=context.getSystemService(UserManager.class);
        KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0&&users!=null&&users.isAdminUser()
                &&users.isUserUnlocked()&&!users.hasUserRestriction(UserManager.DISALLOW_CONFIG_DATE_TIME)
                &&lock!=null&&!lock.isKeyguardLocked();
    }
    private boolean has(String permission) {return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED;}
    private JSONObject state(long id) throws Exception {
        long elapsed=SystemClock.elapsedRealtime(),now=System.currentTimeMillis(),base=now-elapsed;
        if(clockBase==Long.MIN_VALUE||Math.abs(base-clockBase)>2000) {clockBase=base;clockGeneration++;}
        TimeManager manager=context.getSystemService(TimeManager.class);
        if(manager==null) throw new IllegalStateException("Time detector unavailable");
        TimeCapabilitiesAndConfig time=manager.getTimeCapabilitiesAndConfig();
        TimeZoneCapabilitiesAndConfig zone=manager.getTimeZoneCapabilitiesAndConfig();
        boolean allowed=editable();
        String zoneId=manager.getTimeZoneState().getId();
        JSONObject result=new JSONObject().put("schema",1).put("request_id",id).put("zone",zoneId)
                .put("civil",DateTimeContract.CIVIL.format(Instant.ofEpochMilli(now).atZone(ZoneId.of(zoneId))))
                .put("auto_time",time.getConfiguration().isAutoDetectionEnabled())
                .put("auto_zone",zone.getConfiguration().isAutoDetectionEnabled())
                .put("can_auto_time",allowed&&time.getCapabilities().getConfigureAutoDetectionEnabledCapability()==Capabilities.CAPABILITY_POSSESSED)
                .put("can_auto_zone",allowed&&zone.getCapabilities().getConfigureAutoDetectionEnabledCapability()==Capabilities.CAPABILITY_POSSESSED)
                .put("can_clock",allowed&&has(Manifest.permission.SUGGEST_MANUAL_TIME_AND_ZONE)
                        &&time.getCapabilities().getSetManualTimeCapability()==Capabilities.CAPABILITY_POSSESSED)
                .put("can_zone",allowed&&has(Manifest.permission.SUGGEST_MANUAL_TIME_AND_ZONE)
                        &&zone.getCapabilities().getSetManualTimeZoneCapability()==Capabilities.CAPABILITY_POSSESSED)
                .put("minimum_year",TimeDetectorHelper.INSTANCE.getManualDateSelectionYearMin())
                .put("maximum_year",TimeDetectorHelper.INSTANCE.getManualDateSelectionYearMax());
        // Clock ticks are not configuration changes. A real clock jump, user,
        // policy, automatic-source or zone change retires the displayed draft.
        String exact=session+"|"+clockGeneration+"|"+zoneId+"|"+time+"|"+zone+"|"+allowed;
        byte[] digest=MessageDigest.getInstance("SHA-256").digest(exact.getBytes(StandardCharsets.UTF_8));
        StringBuilder key=new StringBuilder();for(byte b:digest)key.append(String.format(Locale.ROOT,"%02x",b&255));
        return result.put("key",key.toString());
    }
    synchronized JSONObject snapshot(long id) throws Exception {
        JSONObject result=state(id);observations.put(result.getString("key"),SystemClock.elapsedRealtime());
        while(observations.size()>8) observations.remove(observations.keySet().iterator().next());
        return result;
    }
    synchronized String apply(String key,String action,String value,String occurrence) throws Exception {
        DateTimeContract.key(key);DateTimeContract.Action parsed=DateTimeContract.Action.parse(action);DateTimeContract.validate(parsed,value,occurrence);
        Long seen=observations.get(key);
        JSONObject current=state(1);
        if(seen==null||SystemClock.elapsedRealtime()-seen>20000||!key.equals(current.getString("key"))) return "time_target_changed";
        String capability=parsed==DateTimeContract.Action.CLOCK?"can_clock":parsed==DateTimeContract.Action.ZONE?"can_zone":parsed==DateTimeContract.Action.AUTO_TIME?"can_auto_time":"can_auto_zone";
        if(!editable()||!current.getBoolean(capability)) return "policy_restricted";
        observations.clear();
        TimeManager manager=context.getSystemService(TimeManager.class);
        boolean accepted;
        switch(parsed) {
            case CLOCK: {
                long millis;
                try {millis=DateTimeContract.millis(value,current.getString("zone"),occurrence,current.getInt("minimum_year"),current.getInt("maximum_year"));}
                catch(IllegalArgumentException invalid) {return "time_invalid_local";}
                accepted=context.getSystemService(TimeDetector.class).suggestManualTime(TimeDetector.createManualTimeSuggestion(millis,"OctoSense Settings"));
                if(accepted) {clockBase=Long.MIN_VALUE;return Math.abs(System.currentTimeMillis()-millis)<2000?"time_applied":"time_requested";}
                break;
            }
            case ZONE:
                accepted=context.getSystemService(TimeZoneDetector.class).suggestManualTimeZone(TimeZoneDetector.createManualTimeZoneSuggestion(value,"OctoSense Settings"));
                if(accepted) return value.equals(manager.getTimeZoneState().getId())?"time_applied":"time_requested";
                break;
            case AUTO_TIME:
                accepted=manager.updateTimeConfiguration(new TimeConfiguration.Builder().setAutoDetectionEnabled(DateTimeContract.enabled(value)).build());break;
            case AUTO_ZONE:
                accepted=manager.updateTimeZoneConfiguration(new TimeZoneConfiguration.Builder().setAutoDetectionEnabled(DateTimeContract.enabled(value)).build());break;
            default:throw new IllegalArgumentException("Unknown time operation");
        }
        if(!accepted) return "policy_restricted";
        JSONObject observed=state(1);
        return observed.getBoolean(parsed==DateTimeContract.Action.AUTO_TIME?"auto_time":"auto_zone")==DateTimeContract.enabled(value)?"time_applied":"time_requested";
    }
}
