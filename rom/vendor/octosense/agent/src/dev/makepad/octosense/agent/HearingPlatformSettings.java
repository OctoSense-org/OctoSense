package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.AudioSystem;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import android.view.accessibility.CaptioningManager;
import dev.makepad.octosense.controls.HearingSettings;
import dev.makepad.octosense.controls.HearingSettings.Setting;

/** Native hearing service readback using existing helper authority; no Home write privilege. */
final class HearingPlatformSettings {
    private final Context context;private final HearingSettings.Backend backend;
    HearingPlatformSettings(Context context){this.context=context;backend=new HearingSettings.Backend(new HearingSettings.Store(){
        @Override public String raw(Setting setting){return setting.secure?Settings.Secure.getString(context.getContentResolver(),setting.key):Settings.System.getString(context.getContentResolver(),setting.key);}
        @Override public String service(Setting setting){
            switch(setting){
                case MONO:return AudioSystem.getMasterMono()?"on":"off";
                case BALANCE:return setting.observed(Float.toString(AudioSystem.getMasterBalance()));
                default:{CaptioningManager manager=context.getSystemService(CaptioningManager.class);if(manager==null)return null;
                    switch(setting){case CAPTIONS_ENABLED:return manager.isEnabled()?"on":"off";case CAPTIONS_FONT_SCALE:return setting.observed(Float.toString(manager.getFontScale()));
                        case CAPTIONS_PRESET:{String current=setting.observed(Integer.toString(manager.getRawUserStyle()));return current!=null&&manager.getUserStyle()!=null?current:null;}
                        default:return null;}
                }
            }
        }
        @Override public boolean write(Setting setting,String value){return setting.secure?Settings.Secure.putString(context.getContentResolver(),setting.key,value):Settings.System.putString(context.getContentResolver(),setting.key,value);}
    },this::access);}
    private HearingSettings.Access access(){
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        if(UserHandle.myUserId()!=0||ActivityManager.getCurrentUser()!=0||users==null||!users.isUserUnlocked()||lock==null||lock.isKeyguardLocked())return HearingSettings.Access.RESTRICTED;
        return context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED?HearingSettings.Access.WRITABLE:HearingSettings.Access.READ_ONLY;
    }
    String read(Setting setting){try{if(access()==HearingSettings.Access.RESTRICTED)return null;String value=backend.read(setting);return access()==HearingSettings.Access.RESTRICTED?null:value;}catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){return null;}}
    boolean writable(Setting setting){return backend.writable(setting);}
    String[] choices(Setting setting){return setting.choices();}
    String apply(Setting setting,String value){return backend.apply(setting,value);}
}
