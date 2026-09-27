package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.ContentResolver;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.content.res.Resources;
import android.media.AudioManager;
import android.os.UserManager;
import android.os.VibrationAttributes;
import android.os.Vibrator;
import android.provider.Settings;
import android.telephony.TelephonyManager;
import dev.makepad.octosense.controls.SettingsControlsContract.Control;
import dev.makepad.octosense.controls.SoundFeedbackContract;

/** Pinned native Sound/Vibration controllers; no playback or haptic preview side effects. */
final class SoundFeedbackSettings {
    private final Context context;
    SoundFeedbackSettings(Context context){this.context=context;}
    private Resources settingsResources(){
        try{
            PackageManager packages=context.getPackageManager();ApplicationInfo info=packages.getApplicationInfo("com.android.settings",0);
            if((info.flags&ApplicationInfo.FLAG_SYSTEM)==0||packages.checkSignatures("android","com.android.settings")!=PackageManager.SIGNATURE_MATCH)return null;
            return packages.getResourcesForApplication(info);
        }catch(PackageManager.NameNotFoundException|SecurityException unavailable){return null;}
    }
    private Integer resource(String name,String type){
        Resources resources=settingsResources();if(resources==null)return null;
        int id=resources.getIdentifier(name,type,"com.android.settings");if(id==0)return null;
        try{return type.equals("bool")?(resources.getBoolean(id)?1:0):resources.getInteger(id);}catch(Resources.NotFoundException unavailable){return null;}
    }
    private Vibrator vibrator(){return context.getSystemService(Vibrator.class);}
    private Boolean vibration(){Vibrator vibrator=vibrator();return vibrator==null?null:vibrator.hasVibrator();}
    private Boolean capability(Control control){
        switch(control){
            case CHARGING_SOUNDS:{Integer show=resource("config_show_charging_sounds","bool");return show==null?null:show==1;}
            case LOCK_SOUNDS:{Integer show=resource("config_show_screen_locking_sounds","bool");return show==null?null:show==1;}
            case DIALPAD_TONES:{TelephonyManager phone=context.getSystemService(TelephonyManager.class);return phone==null?null:phone.isVoiceCapable();}
            case CHARGING_VIBRATION:{Boolean supported=vibration();if(!Boolean.TRUE.equals(supported))return supported;return capability(Control.CHARGING_SOUNDS);}
            case KEYBOARD_VIBRATION:{Boolean supported=vibration();if(!Boolean.TRUE.equals(supported))return supported;int id=context.getResources().getIdentifier("config_keyboardVibrationSettingsSupported","bool","android");return id!=0&&context.getResources().getBoolean(id);}
            default:{Boolean supported=vibration();if(!Boolean.TRUE.equals(supported))return supported;
                if(control.intensity()){Integer levels=resource("config_vibration_supported_intensity_levels","integer");return levels==null?null:levels>=1&&levels<=3;}return true;}
        }
    }
    boolean supported(Control control){return !Boolean.FALSE.equals(capability(control));}
    private int usage(Control control){switch(control){case RING_VIBRATION:return VibrationAttributes.USAGE_RINGTONE;case NOTIFICATION_VIBRATION:return VibrationAttributes.USAGE_NOTIFICATION;case ALARM_VIBRATION:return VibrationAttributes.USAGE_ALARM;case MEDIA_VIBRATION:return VibrationAttributes.USAGE_MEDIA;case TOUCH_VIBRATION:return VibrationAttributes.USAGE_TOUCH;default:throw new IllegalArgumentException("Vibration control required");}}
    private int defaultIntensity(Control control){Vibrator vibrator=vibrator();return vibrator==null?-1:vibrator.getDefaultVibrationIntensity(usage(control));}
    private String key(Control control){switch(control){
        case CHARGING_SOUNDS:return Settings.Secure.CHARGING_SOUNDS_ENABLED;case CHARGING_VIBRATION:return Settings.Secure.CHARGING_VIBRATION_ENABLED;
        case LOCK_SOUNDS:return Settings.System.LOCKSCREEN_SOUNDS_ENABLED;case DIALPAD_TONES:return Settings.System.DTMF_TONE_WHEN_DIALING;
        case VIBRATION_ENABLED:return Settings.System.VIBRATE_ON;case KEYBOARD_VIBRATION:return Settings.System.KEYBOARD_VIBRATION_ENABLED;
        case RING_VIBRATION:return Settings.System.RING_VIBRATION_INTENSITY;case NOTIFICATION_VIBRATION:return Settings.System.NOTIFICATION_VIBRATION_INTENSITY;
        case ALARM_VIBRATION:return Settings.System.ALARM_VIBRATION_INTENSITY;case MEDIA_VIBRATION:return Settings.System.MEDIA_VIBRATION_INTENSITY;
        case TOUCH_VIBRATION:return Settings.System.HAPTIC_FEEDBACK_INTENSITY;default:throw new IllegalArgumentException("Sound feedback control required");}}
    private boolean secure(Control control){return control==Control.CHARGING_SOUNDS||control==Control.CHARGING_VIBRATION;}
    private String raw(Control control){return secure(control)?Settings.Secure.getString(context.getContentResolver(),key(control)):Settings.System.getString(context.getContentResolver(),key(control));}
    String read(Control control){
        if(!Boolean.TRUE.equals(capability(control)))return null;
        if(control==Control.TOUCH_VIBRATION){String legacy=SoundFeedbackContract.toggle(Settings.System.getString(context.getContentResolver(),Settings.System.HAPTIC_FEEDBACK_ENABLED));if(legacy==null)return null;if(legacy.equals("off"))return "off";}
        return control.intensity()?SoundFeedbackContract.intensity(raw(control),defaultIntensity(control),resource("config_vibration_supported_intensity_levels","integer")):SoundFeedbackContract.toggle(raw(control));
    }
    private boolean owner(){
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return ActivityManager.getCurrentUser()==0&&users!=null&&users.isAdminUser()&&users.isUserUnlocked()&&!users.hasUserRestriction(UserManager.DISALLOW_ADJUST_VOLUME)&&lock!=null&&!lock.isKeyguardLocked();
    }
    boolean writable(Control control){
        if(!owner()||!Boolean.TRUE.equals(capability(control))||context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)!=PackageManager.PERMISSION_GRANTED)return false;
        if(control.intensity()||control==Control.KEYBOARD_VIBRATION){if(!"on".equals(SoundFeedbackContract.toggle(Settings.System.getString(context.getContentResolver(),Settings.System.VIBRATE_ON))))return false;}
        if(control==Control.RING_VIBRATION||control==Control.NOTIFICATION_VIBRATION){AudioManager audio=context.getSystemService(AudioManager.class);if(audio==null||audio.getRingerModeInternal()==AudioManager.RINGER_MODE_SILENT)return false;}
        return read(control)!=null;
    }
    String[] choices(Control control){
        if(!writable(control))return new String[0];
        if(!control.intensity())return control.choices();
        Integer levels=resource("config_vibration_supported_intensity_levels","integer");return levels==null?new String[0]:SoundFeedbackContract.choices(levels,defaultIntensity(control));
    }
    private boolean putSystem(String key,int value){return owner()&&Settings.System.putInt(context.getContentResolver(),key,value);}
    String apply(Control control,String value){
        control.validate(value);if(!writable(control))return "control_unavailable";
        boolean offered=false;for(String choice:choices(control))if(choice.equals(value))offered=true;if(!offered)return "control_unavailable";
        int stored=control.intensity()?SoundFeedbackContract.stored(value,resource("config_vibration_supported_intensity_levels","integer"),defaultIntensity(control)):value.equals("on")?1:0;
        SoundFeedbackContract.Coupling coupling=control==Control.RING_VIBRATION?SoundFeedbackContract.Coupling.RING
            :control==Control.TOUCH_VIBRATION?SoundFeedbackContract.Coupling.TOUCH:SoundFeedbackContract.Coupling.NONE;
        String result=SoundFeedbackContract.apply(coupling,stored,control.intensity()?defaultIntensity(control):0,new SoundFeedbackContract.Store(){
            public boolean allowed(){return owner();}
            private String companion(SoundFeedbackContract.Slot slot){switch(slot){case RING_LEGACY:return Settings.System.VIBRATE_WHEN_RINGING;
                case TOUCH_LEGACY:return Settings.System.HAPTIC_FEEDBACK_ENABLED;case HARDWARE_TOUCH:return Settings.System.HARDWARE_HAPTIC_FEEDBACK_INTENSITY;default:throw new IllegalArgumentException("Companion required");}}
            public boolean write(SoundFeedbackContract.Slot slot,int value){
                try{
                    if(slot!=SoundFeedbackContract.Slot.PRIMARY)return putSystem(companion(slot),value);
                    return secure(control)?owner()&&Settings.Secure.putInt(context.getContentResolver(),key(control),value):putSystem(key(control),value);
                }catch(SecurityException|IllegalStateException denied){return false;}
            }
            public Integer read(SoundFeedbackContract.Slot slot){try{return SoundFeedbackContract.integer(slot==SoundFeedbackContract.Slot.PRIMARY?raw(control):Settings.System.getString(context.getContentResolver(),companion(slot)),null);}catch(SecurityException|IllegalStateException unavailable){return null;}}
        });
        if(control==Control.KEYBOARD_VIBRATION&&result.equals("control_applied"))context.getContentResolver().notifyChange(Settings.System.getUriFor(Settings.System.KEYBOARD_VIBRATION_ENABLED),null,ContentResolver.NOTIFY_NO_DELAY);
        return result;
    }
}
