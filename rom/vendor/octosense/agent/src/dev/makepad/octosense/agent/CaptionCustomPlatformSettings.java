package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import android.view.accessibility.CaptioningManager;
import android.view.accessibility.CaptioningManager.CaptionStyle;
import dev.makepad.octosense.controls.CaptionCustomSettings;
import dev.makepad.octosense.controls.CaptionCustomSettings.Field;
import dev.makepad.octosense.controls.CaptionCustomSettings.Key;
import java.util.EnumMap;

/** Finite custom-caption adapter. Scope identity is supplied by authenticated host transport. */
final class CaptionCustomPlatformSettings {
    private final Context context;
    private final CaptionCustomSettings.Backend backend;
    CaptionCustomPlatformSettings(Context context){
        this.context=context;
        backend=new CaptionCustomSettings.Backend(new CaptionCustomSettings.Store(){
            @Override public CaptionCustomSettings.State read(){
                EnumMap<Key,String> raw=raw();CaptioningManager manager=context.getSystemService(CaptioningManager.class);
                if(manager==null)return null;
                CaptionStyle style=CaptionStyle.getCustomStyle(context.getContentResolver());if(style==null)return null;
                CaptionCustomSettings.NativeStyle nativeStyle=new CaptionCustomSettings.NativeStyle(
                    style.foregroundColor,style.backgroundColor,style.windowColor,style.edgeType,style.edgeColor,
                    style.hasForegroundColor(),style.hasBackgroundColor(),style.hasWindowColor(),style.hasEdgeType(),style.hasEdgeColor(),style.mRawTypeface);
                boolean enabled=manager.isEnabled();int preset=manager.getRawUserStyle();
                // A provider update during the read is an unavailable observation, never an invented style.
                if(!raw.equals(raw()))return null;
                return new CaptionCustomSettings.State(raw,enabled,preset,nativeStyle);
            }
            @Override public boolean write(Key key,String value){return Settings.Secure.putString(context.getContentResolver(),key.key,value);}
        },this::access,SystemClock::elapsedRealtime);
    }
    private EnumMap<Key,String> raw(){EnumMap<Key,String> values=new EnumMap<>(Key.class);for(Key key:Key.values())values.put(key,Settings.Secure.getString(context.getContentResolver(),key.key));return values;}
    private CaptionCustomSettings.Access access(){
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        if(UserHandle.myUserId()!=0||ActivityManager.getCurrentUser()!=0||users==null||!users.isUserUnlocked()||lock==null||lock.isKeyguardLocked())return CaptionCustomSettings.Access.RESTRICTED;
        return context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED?CaptionCustomSettings.Access.WRITABLE:CaptionCustomSettings.Access.READ_ONLY;
    }
    String read(Field field){return backend.read(field);}
    Boolean customSelected(){return backend.customSelected();}
    boolean scopeActive(String session,long visit){return backend.scopeActive(session,visit);}
    String[] choices(String session,long visit,Field field){return backend.choices(session,visit,field);}
    String apply(String session,long visit,Field field,String value){return backend.apply(session,visit,field,value);}
    void replaceSession(String session){backend.replaceSession(session);}
    boolean enterScope(String session,long visit){return backend.enterScope(session,visit);}
    void leaveScope(String session,long visit){backend.leaveScope(session,visit);}
    void invalidateSession(String session){backend.invalidateSession(session);}
    void invalidate(){backend.invalidate();}
}
