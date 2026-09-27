package com.android.systemui.octosense;

import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.app.Service;
import android.app.admin.DevicePolicyManager;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.hardware.SensorPrivacyManager;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import java.util.Arrays;

/** Role-owned switches exposed only to the platform-signed ROM helper. */
public final class OctoSenseSensorSettingsService extends Service {
    private static final String AGENT="dev.makepad.octosense.agent";
    private void caller() {
        int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(UserHandle.getUserId(uid)!=UserHandle.myUserId()||packages==null||!Arrays.asList(packages).contains(AGENT)
                ||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller is not the trusted Settings helper");
    }
    private static void sensor(int sensor) {
        if(sensor!=SensorPrivacyManager.Sensors.CAMERA&&sensor!=SensorPrivacyManager.Sensors.MICROPHONE)
            throw new IllegalArgumentException("Unsupported sensor");
    }
    private boolean unlocked() {
        KeyguardManager lock=getSystemService(KeyguardManager.class);UserManager users=getSystemService(UserManager.class);
        return ActivityManager.getCurrentUser()==UserHandle.myUserId()&&lock!=null&&!lock.isKeyguardLocked()
                &&users!=null&&users.isUserUnlocked();
    }
    private boolean writable(int sensor) {
        UserManager users=getSystemService(UserManager.class);SensorPrivacyManager privacy=getSystemService(SensorPrivacyManager.class);
        boolean camera=sensor==SensorPrivacyManager.Sensors.CAMERA;
        if(!unlocked()||users.hasUserRestriction(camera?UserManager.DISALLOW_CAMERA_TOGGLE:UserManager.DISALLOW_MICROPHONE_TOGGLE)
                ||privacy==null||!privacy.supportsSensorToggle(sensor)||privacy.isSensorPrivacyEnabled(SensorPrivacyManager.TOGGLE_TYPE_HARDWARE,sensor)) return false;
        DevicePolicyManager policies=getSystemService(DevicePolicyManager.class);
        return camera?policies!=null&&!policies.getCameraDisabled(null):!users.hasUserRestriction(UserManager.DISALLOW_UNMUTE_MICROPHONE);
    }
    private Bundle state() {
        Bundle result=new Bundle();result.putBoolean("ok",unlocked());if(!result.getBoolean("ok")) return result;
        SensorPrivacyManager privacy=getSystemService(SensorPrivacyManager.class);
        for(int sensor:new int[]{SensorPrivacyManager.Sensors.CAMERA,SensorPrivacyManager.Sensors.MICROPHONE}) {
            String key=sensor==SensorPrivacyManager.Sensors.CAMERA?"camera":"microphone";
            boolean supported=privacy!=null&&privacy.supportsSensorToggle(sensor);result.putBoolean(key+"_supported",supported);
            if(supported) {result.putBoolean(key+"_allowed",!privacy.areAnySensorPrivacyTogglesEnabled(sensor));result.putBoolean(key+"_writable",writable(sensor));}
        }
        return result;
    }
    private final ISensorSettings.Stub binder=new ISensorSettings.Stub() {
        @Override public Bundle snapshot() {
            caller();long token=Binder.clearCallingIdentity();try {return state();} finally {Binder.restoreCallingIdentity(token);}
        }
        @Override public Bundle setAccess(int sensor,boolean allowed) {
            caller();sensor(sensor);long token=Binder.clearCallingIdentity();
            try {
                if(!writable(sensor)) {Bundle denied=new Bundle();denied.putBoolean("ok",false);return denied;}
                getSystemService(SensorPrivacyManager.class).setSensorPrivacy(SensorPrivacyManager.Sources.SETTINGS,sensor,!allowed,UserHandle.myUserId());
                return state();
            } finally {Binder.restoreCallingIdentity(token);}
        }
    };
    @Override public IBinder onBind(Intent intent) {return binder;}
}
