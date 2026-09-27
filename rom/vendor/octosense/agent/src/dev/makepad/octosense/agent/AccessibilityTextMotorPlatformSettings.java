package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.RemoteException;
import android.os.ServiceManager;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import android.view.IWindowManager;
import android.view.ViewConfiguration;
import android.view.WindowManagerGlobal;
import android.view.accessibility.AccessibilityManager;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings.Access;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings.Backend;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings.Effects;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings.Field;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings.Key;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings.State;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.EnumMap;

/** Finite native preference writes and independent framework readback. */
final class AccessibilityTextMotorPlatformSettings {
    private final Context context;
    private final Backend backend;

    AccessibilityTextMotorPlatformSettings(Context context) {
        this.context=context;
        backend=new Backend(new AccessibilityTextMotorSettings.Store() {
            @Override public State read(){return nativeState();}
            @Override public boolean write(Key key,String finiteValue){
                return key.global?Settings.Global.putString(context.getContentResolver(),key.key,finiteValue)
                        :Settings.Secure.putString(context.getContentResolver(),key.key,finiteValue);
            }
        },this::access);
    }

    private Access access() {
        UserManager users=context.getSystemService(UserManager.class);
        KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        if(UserHandle.myUserId()!=0||ActivityManager.getCurrentUser()!=0||users==null
                ||!users.isUserUnlocked()||lock==null||lock.isKeyguardLocked())return Access.RESTRICTED;
        return context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED
                ?Access.WRITABLE:Access.READ_ONLY;
    }

    private State nativeState() {
        EnumMap<Key,String> raw=new EnumMap<>(Key.class);
        for(Key key:Key.values())raw.put(key,key.global?Settings.Global.getString(context.getContentResolver(),key.key)
                :Settings.Secure.getString(context.getContentResolver(),key.key));
        Effects effects=new Effects();
        effects.required.addAll(java.util.EnumSet.of(Field.HIGH_CONTRAST,Field.BOLD_TEXT,
                Field.REMOVE_ANIMATIONS,Field.TOUCH_HOLD,Field.ACTION_TIMEOUT));
        AccessibilityManager manager=context.getSystemService(AccessibilityManager.class);
        // A missing native getter remains unknown for only the affected field.
        if(manager!=null&&ServiceManager.checkService(Context.ACCESSIBILITY_SERVICE)!=null) {
            effects.highContrast=highContrast(manager);
            try {
                effects.interactiveRecommendation=manager.getRecommendedTimeoutMillis(0,AccessibilityManager.FLAG_CONTENT_CONTROLS);
                effects.nonInteractiveRecommendation=manager.getRecommendedTimeoutMillis(0,AccessibilityManager.FLAG_CONTENT_TEXT|AccessibilityManager.FLAG_CONTENT_ICONS);
            }catch(SecurityException|IllegalStateException|LinkageError unavailable){/* Unknown native service observation. */}
        }
        try {effects.fontWeight=context.getResources().getConfiguration().fontWeightAdjustment;}
        catch(LinkageError unavailable){/* Older Configuration without the adjustment field. */}
        try {effects.longPress=ViewConfiguration.getLongPressTimeout();}
        catch(SecurityException|IllegalStateException|LinkageError unavailable){/* Core setting not observed. */}
        try {IWindowManager wm=WindowManagerGlobal.getWindowManagerService();if(wm!=null)effects.animationScales=wm.getAnimationScales();}
        catch(RemoteException|SecurityException|IllegalStateException|LinkageError unavailable){/* WMS read is required for this row. */}
        return new State(raw,vectorCursor(),effects);
    }

    /** Two audited Android framework spellings only; no arbitrary reflected caller target. */
    private static Boolean highContrast(AccessibilityManager manager) {
        for(String name:new String[]{"isHighContrastTextEnabled","isHighTextContrastEnabled"})try {
            Method method=AccessibilityManager.class.getMethod(name);
            if(method.getReturnType()!=boolean.class||Modifier.isStatic(method.getModifiers()))return null;
            return (Boolean)method.invoke(manager);
        }catch(NoSuchMethodException absent){/* Try the older native spelling. */}
        catch(IllegalAccessException|InvocationTargetException|SecurityException|LinkageError unavailable){return null;}
        return null;
    }

    /** The legacy large-pointer setting is not the vector-cursor customization model. */
    private static Boolean vectorCursor() {
        try {
            Method method=Class.forName("android.view.flags.Flags").getMethod("enableVectorCursorA11ySettings");
            if(method.getReturnType()!=boolean.class||!Modifier.isStatic(method.getModifiers()))return null;
            return (Boolean)method.invoke(null);
        }catch(ClassNotFoundException|NoSuchMethodException|IllegalAccessException|InvocationTargetException|SecurityException|LinkageError unavailable){return null;}
    }

    boolean supported(Field field){return field!=Field.LARGE_POINTER||!Boolean.TRUE.equals(vectorCursor());}
    String read(Field field){return backend.read(field).value;}
    boolean writable(Field field){return backend.read(field).choices.length>0;}
    String[] choices(Field field){return backend.read(field).choices;}
    String apply(Field field,String choice){return backend.apply(field,choice);}
}
