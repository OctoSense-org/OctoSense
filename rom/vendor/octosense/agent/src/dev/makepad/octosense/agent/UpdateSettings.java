package dev.makepad.octosense.agent;

import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.os.Build;
import android.os.Bundle;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import dev.makepad.octosense.updates.UpdatesSettingsContract;
import dev.makepad.octosense.updates.UpdatesSettingsContract.Review;
import dev.makepad.octosense.updates.UpdatesSettingsSnapshot;
import java.util.Arrays;
import java.util.UUID;
import java.util.concurrent.Executor;
import org.json.JSONArray;
import org.json.JSONObject;

/** Settings view of the existing updater. Status reads never fetch a release. */
final class UpdateSettings {
    private final Context context;
    private final Updater updater;
    private final Executor work;
    private String checkState="never",checkError="",pendingPart="",operationError="";
    private long checkedAt=-1,lastObservation=-1,rebootObservation=-1;
    private Updater.CheckedRelease release;
    private Review review;
    private String rebootKey;
    private long rebootGeneration=-1;

    UpdateSettings(Context context,Updater updater,Executor work) {this.context=context;this.updater=updater;this.work=work;}
    private boolean allowed() {
        UserManager users=context.getSystemService(UserManager.class);
        KeyguardManager keyguard=context.getSystemService(KeyguardManager.class);
        return ActivityManager.getCurrentUser()==UserHandle.myUserId()&&users.isAdminUser()
            &&users.isUserUnlocked()&&!keyguard.isKeyguardLocked()&&!users.hasUserRestriction("no_control_apps");
    }
    private boolean recent(long at) {return UpdatesSettingsContract.recent(at,SystemClock.elapsedRealtime(),UpdatesSettingsContract.OBSERVATION_TTL_MS);}
    private boolean available(String part) {
        return review!=null&&review.available(part,updater.source(),Build.VERSION.INCREMENTAL,updater.homeVersion(),SystemClock.elapsedRealtime());
    }
    private boolean idle() {return pendingPart.isEmpty()&&!"checking".equals(checkState)&&!updater.busy();}
    private void refreshReboot(Bundle status) {
        if("updated_need_reboot".equals(status.getString("rom_phase"))) {
            long generation=status.getLong("phase_generation");
            if(rebootKey==null||generation!=rebootGeneration) {
                rebootKey=UpdatesSettingsContract.fingerprint("update-reboot-v1",UUID.randomUUID().toString(),Build.VERSION.INCREMENTAL);
                rebootGeneration=generation;rebootObservation=-1;
            }
        } else {rebootKey=null;rebootGeneration=-1;rebootObservation=-1;}
    }
    synchronized JSONObject snapshot(long id) throws Exception {
        if(!allowed()) return UpdatesSettingsSnapshot.unavailable(context,id,"restricted");
        Bundle status=updater.status();refreshReboot(status);
        JSONObject current=UpdatesSettingsSnapshot.current(context);
        String slot=status.getString("slot","");if(!slot.isEmpty()) current.put("slot",slot);
        JSONObject check=new JSONObject().put("state",checkState);
        if(checkedAt>=0) check.put("checked_at_ms",checkedAt);
        if(!checkError.isEmpty()) check.put("error",checkError);
        JSONObject offer=null;
        if(review!=null&&release!=null&&(available("rom")||available("home"))) {
            Bundle versions=release.versions;
            offer=new JSONObject().put("key",review.key).put("release",versions.getString("release",""))
                .put("rom_newer",available("rom")).put("home_newer",available("home"));
            if(versions.containsKey("rom_offered")) offer.put("rom_version",versions.getString("rom_offered"));
            if(versions.containsKey("home_offered")) offer.put("home_version",versions.getLong("home_offered"));
        } else if(review!=null&&release!=null&&review.current(updater.source(),Build.VERSION.INCREMENTAL,updater.homeVersion(),SystemClock.elapsedRealtime())) {
            // A checked, up-to-date release is still useful to show. Retire
            // consumed or changed reviews from actionable capabilities below.
            Bundle v=release.versions;
            offer=new JSONObject().put("key",review.key).put("release",v.getString("release",""))
                .put("rom_newer",false).put("home_newer",false);
            if(v.containsKey("rom_offered")) offer.put("rom_version",v.getString("rom_offered"));
            if(v.containsKey("home_offered")) offer.put("home_version",v.getLong("home_offered"));
        }
        String romPhase=romPhase(status.getString("rom_phase","unknown"));
        if(!updater.engineObserved()) romPhase="unavailable";
        if("rom".equals(pendingPart)) romPhase="starting";
        JSONObject rom=new JSONObject().put("phase",romPhase);
        float progress=status.getFloat("rom_progress",Float.NaN);
        if(Float.isFinite(progress)&&progress>=0&&progress<=1) rom.put("progress",progress);
        if(status.containsKey("rom_error")) rom.put("error","The system update could not be completed. Check again to retry.");
        String homePhase=homePhase(status.getString("home_state",""));
        if("home".equals(pendingPart)&&"idle".equals(homePhase)) homePhase="downloading";
        JSONObject home=new JSONObject().put("phase",homePhase);
        if(status.containsKey("home_error")||"failed".equals(homePhase)) home.put("error","The Home update could not be completed. Check again to retry.");
        if(!operationError.isEmpty()) {
            // A queued policy/target failure is observable even before the
            // engine or installer receives an operation.
            check.put("error",operationError);
        }
        JSONArray caps=new JSONArray();
        if(idle()) {
            caps.put("check");
            if(updater.engineObserved()&&available("rom")) caps.put("install_rom");
            if(available("home")) caps.put("install_home");
        }
        if(rebootKey!=null&&pendingPart.isEmpty()&&!"checking".equals(checkState)) {caps.put("reboot");rebootObservation=SystemClock.elapsedRealtime();}
        lastObservation=SystemClock.elapsedRealtime();
        JSONObject out=new JSONObject().put("schema",1).put("request_id",id).put("availability","available")
            .put("current",current).put("check",check).put("offer",offer==null?JSONObject.NULL:offer)
            .put("rom",rom).put("home",home).put("capabilities",caps);
        if(rebootKey!=null) out.put("reboot_key",rebootKey);
        return out;
    }
    private long reviewedAt=-1;
    synchronized String check() {
        if(!allowed()||!recent(lastObservation)||!idle()) return "update_unavailable";
        checkState="checking";checkError="";operationError="";release=null;review=null;reviewedAt=-1;
        try {work.execute(() -> {
            try {
                if(!allowed()) throw new SecurityException();
                Updater.CheckedRelease checked=updater.checkRelease();
                synchronized(this) {
                    if(!allowed()) throw new SecurityException();
                    release=checked;reviewedAt=SystemClock.elapsedRealtime();
                    review=new Review(checked.manifest.toString(),checked.source,Build.VERSION.INCREMENTAL,checked.versions.getLong("home_current"),reviewedAt,
                        checked.versions.getBoolean("rom_newer"),checked.versions.getBoolean("home_newer"));
                    checkedAt=System.currentTimeMillis();checkState="checked";
                }
            } catch(Exception e) {synchronized(this) {
                release=null;review=null;checkState="failed";checkError="Could not check for updates. Check the connection and try again.";
            }}
        });} catch(RuntimeException e) {checkState="failed";checkError="Update service is busy. Try again.";return "update_unavailable";}
        return "update_check_requested";
    }
    synchronized String install(String key,String part) {
        UpdatesSettingsContract.key(key);UpdatesSettingsContract.part(part);
        if(!allowed()||!recent(lastObservation)||!idle()) return "update_unavailable";
        if(review==null||release==null||("rom".equals(part)&&!updater.engineObserved())
                ||!review.claim(key,part,updater.source(),Build.VERSION.INCREMENTAL,updater.homeVersion(),SystemClock.elapsedRealtime()))
            return "update_target_changed";
        Updater.CheckedRelease checked=release;
        pendingPart=part;operationError="";
        try {work.execute(() -> {
            try {
                if(!allowed()) throw new SecurityException();
                Bundle result=updater.applyReviewed(part,checked);
                if(!result.getBoolean("ok")) throw new IllegalStateException();
            } catch(Exception e) {synchronized(this) {
                operationError="The update could not start. Check for updates and review the release again.";
            }} finally {synchronized(this) {pendingPart="";}}
        });} catch(RuntimeException e) {pendingPart="";operationError="Update service is busy. Check again.";return "update_unavailable";}
        return "update_install_requested";
    }
    synchronized String reboot(String key) {
        UpdatesSettingsContract.key(key);refreshReboot(updater.status());
        if(!allowed()||!recent(rebootObservation)||!pendingPart.isEmpty()||"checking".equals(checkState)) return "update_unavailable";
        if(rebootKey==null||!rebootKey.equals(key)) return "update_target_changed";
        rebootObservation=-1;
        return updater.reboot().getBoolean("ok")?"update_reboot_requested":"update_unavailable";
    }
    private static String romPhase(String phase) {
        if(Arrays.asList(Updater.STATUS).contains(phase)||Arrays.asList("starting","cancelled","failed").contains(phase)) return phase;
        return "unknown";
    }
    private static String homePhase(String phase) {
        if(phase.isEmpty()) return "idle";
        if(phase.startsWith("failed")||phase.equals("checksum mismatch")) return "failed";
        return Arrays.asList("idle","downloading","verifying","installing","installed","failed").contains(phase)?phase:"unknown";
    }
}
