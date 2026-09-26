package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.location.LocationManager;
import android.net.wifi.WifiManager;
import android.os.PowerManager;
import android.os.Bundle;
import android.os.UserHandle;
import android.os.UserManager;
import dev.makepad.octosense.controls.SettingsControlsBackend;
import dev.makepad.octosense.controls.SettingsControlsContract.Control;

/** Framework-only APIs. Not linked into Home or registered with an app script. */
final class SettingsPlatformControls implements SettingsControlsBackend.Platform {
    private final Context context;
    private final SensorSettingsClient sensors;
    private final ZenSettingsClient zen;
    private final SoundFeedbackSettings sound;
    private final HearingPlatformSettings hearing;
    private final AccessibilityTextMotorPlatformSettings textInteraction;
    SettingsPlatformControls(Context context,SensorSettingsClient sensors,ZenSettingsClient zen) {
        this.context=context;this.sensors=sensors;this.zen=zen;this.sound=new SoundFeedbackSettings(context);this.hearing=new HearingPlatformSettings(context);this.textInteraction=new AccessibilityTextMotorPlatformSettings(context);
    }
    private String sensorKey(Control control) {return control==Control.CAMERA_ACCESS?"camera":"microphone";}
    private boolean has(String permission) {return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED;}
    @Override public boolean supported(Control control) {
        if(control.textInteraction()!=null)return textInteraction.supported(control.textInteraction());
        if(control.page==dev.makepad.octosense.controls.SettingsControlsContract.Page.SOUND_FEEDBACK)return sound.supported(control);
        switch(control) {
            case CAMERA_ACCESS:case MICROPHONE_ACCESS: {
                Bundle state=sensors.snapshot();return state==null||state.getBoolean(sensorKey(control)+"_supported");
            }
            case NOTIFICATION_BUBBLES: {
                ActivityManager manager=context.getSystemService(ActivityManager.class);
                return manager!=null&&!manager.isLowRamDevice()&&context.getResources().getBoolean(com.android.internal.R.bool.config_supportsBubble);
            }
            case ADAPTIVE_BATTERY:return context.getResources().getBoolean(com.android.internal.R.bool.config_smart_battery_available);
            case BATTERY_DISABLE_AT_90:return !context.getResources().getBoolean(com.android.internal.R.bool.config_batterySaverStickyBehaviourDisabled);
            default:return true;
        }
    }
    @Override public String read(Control control) {
        if(control.textInteraction()!=null)return textInteraction.read(control.textInteraction());
        if(control.hearingSetting()!=null)return hearing.read(control.hearingSetting());
        if(control.page==dev.makepad.octosense.controls.SettingsControlsContract.Page.SOUND_FEEDBACK)return sound.read(control);
        switch(control) {
            case DND_MODE: {
                Bundle state=zen.snapshot();return state==null?null:state.getString("mode");
            }
            case LOCATION_ENABLED: {
                LocationManager manager=context.getSystemService(LocationManager.class);
                return manager==null?null:manager.isLocationEnabled()?"on":"off";
            }
            case WIFI_SCANNING: {
                WifiManager manager=context.getSystemService(WifiManager.class);
                return manager==null?null:manager.isScanAlwaysAvailable()?"on":"off";
            }
            case BATTERY_SAVER: {
                PowerManager manager=context.getSystemService(PowerManager.class);
                return manager==null?null:manager.isPowerSaveMode()?"on":"off";
            }
            case CAMERA_ACCESS:case MICROPHONE_ACCESS: {
                Bundle state=sensors.snapshot();String key=sensorKey(control)+"_allowed";
                return state==null||!state.containsKey(key)?null:state.getBoolean(key)?"on":"off";
            }
            default:return null;
        }
    }
    @Override public boolean writable(Control control) {
        if(control.textInteraction()!=null)return textInteraction.writable(control.textInteraction());
        if(control.hearingSetting()!=null)return hearing.writable(control.hearingSetting());
        if(control.colorSetting()!=null) {
            UserManager users=context.getSystemService(UserManager.class);
            android.app.KeyguardManager keyguard=context.getSystemService(android.app.KeyguardManager.class);
            return UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0&&users!=null&&users.isUserUnlocked()
                    &&keyguard!=null&&!keyguard.isKeyguardLocked()&&has(Manifest.permission.WRITE_SECURE_SETTINGS);
        }
        if(control.page==dev.makepad.octosense.controls.SettingsControlsContract.Page.SOUND_FEEDBACK)return sound.writable(control);
        switch(control) {
            case DND_MODE: {
                Bundle state=zen.snapshot();return state!=null&&state.getBoolean("writable");
            }
            case LOCATION_ENABLED:return has(Manifest.permission.WRITE_SECURE_SETTINGS);
            case WIFI_SCANNING:return has(Manifest.permission.NETWORK_SETTINGS);
            case BATTERY_SAVER:return has(Manifest.permission.DEVICE_POWER);
            case CAMERA_ACCESS:case MICROPHONE_ACCESS: {
                Bundle state=sensors.snapshot();return state!=null&&state.getBoolean(sensorKey(control)+"_writable");
            }
            default:return false;
        }
    }
    @Override public String[] choices(Control control){if(control.textInteraction()!=null)return textInteraction.choices(control.textInteraction());if(control.hearingSetting()!=null)return hearing.choices(control.hearingSetting());return control.page==dev.makepad.octosense.controls.SettingsControlsContract.Page.SOUND_FEEDBACK?sound.choices(control):control.choices();}
    @Override public String applyResult(Control control,String value){if(control.textInteraction()!=null)return textInteraction.apply(control.textInteraction(),value);if(control.hearingSetting()!=null)return hearing.apply(control.hearingSetting(),value);return control.page==dev.makepad.octosense.controls.SettingsControlsContract.Page.SOUND_FEEDBACK?sound.apply(control,value):SettingsControlsBackend.Platform.super.applyResult(control,value);}
    @Override public boolean apply(Control control,String value) {
        control.validate(value);
        if(!writable(control)) return false;
        boolean on=value.equals("on");
        switch(control) {
            case DND_MODE:return zen.apply(value);
            case LOCATION_ENABLED:context.getSystemService(LocationManager.class).setLocationEnabledForUser(on,UserHandle.of(UserHandle.myUserId()));return true;
            case WIFI_SCANNING:context.getSystemService(WifiManager.class).setScanAlwaysAvailable(on);return true;
            case BATTERY_SAVER:return context.getSystemService(PowerManager.class).setPowerSaveModeEnabled(on);
            case CAMERA_ACCESS:case MICROPHONE_ACCESS:
                return sensors.apply(control==Control.CAMERA_ACCESS?2:1,on);
            default:return false;
        }
    }
}
