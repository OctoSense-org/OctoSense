package dev.makepad.octosense.agent;
import android.content.Context;
import android.os.SystemClock;
import dev.makepad.octosense.updates.UpdatesSettingsContract;
import org.json.JSONObject;
import java.util.ArrayDeque;

public final class UpdatesSettingsStateTest {
    static void check(boolean value){if(!value)throw new AssertionError();}
    static final class Fixture {
        final Context context=new Context();final Updater updater=new Updater();final ArrayDeque<Runnable> jobs=new ArrayDeque<>();
        final UpdateSettings settings=new UpdateSettings(context,updater,jobs::add);
        JSONObject snapshot() throws Exception{return settings.snapshot(1);}
        String offer() throws Exception {
            snapshot();check(settings.check().equals("update_check_requested"));jobs.remove().run();
            return snapshot().getJSONObject("offer").getString("key");
        }
    }
    public static void main(String[] args) throws Exception {
        Fixture f=new Fixture();
        for(int i=0;i<5;i++)f.snapshot();check(f.updater.checks==0&&f.jobs.isEmpty());
        check(f.settings.check().equals("update_check_requested"));
        check(f.snapshot().getJSONObject("check").getString("state").equals("checking"));
        check(f.settings.check().equals("update_unavailable"));check(f.jobs.size()==1);
        f.jobs.remove().run();String key=f.snapshot().getJSONObject("offer").getString("key");
        check(f.settings.install(UpdatesSettingsContract.fingerprint("forged"),"rom").equals("update_target_changed"));
        f.updater.manifest="offer-b"; // Another updater entry point sees a different latest release.
        check(f.settings.install(key,"rom").equals("update_install_requested"));
        check(f.settings.install(key,"rom").equals("update_unavailable"));
        check(!f.snapshot().getJSONArray("capabilities").contains("install_rom"));
        f.jobs.remove().run();check(f.updater.installs==1&&f.updater.installed.manifest.getString("release").equals("offer-a"));

        Fixture stale=new Fixture();String old=stale.offer();stale.updater.home=5;
        check(stale.settings.install(old,"rom").equals("update_target_changed"));check(stale.jobs.isEmpty());
        check(stale.snapshot().get("offer")==JSONObject.NULL);
        Fixture source=new Fixture();String oldSource=source.offer();source.updater.source="https://other.example/update.json";
        check(source.settings.install(oldSource,"home").equals("update_target_changed"));
        Fixture expired=new Fixture();String expiredKey=expired.offer();SystemClock.now+=UpdatesSettingsContract.REVIEW_TTL_MS+1;expired.snapshot();
        check(expired.settings.install(expiredKey,"home").equals("update_target_changed"));

        Fixture switched=new Fixture();String target=switched.offer();check(switched.settings.install(target,"home").equals("update_install_requested"));
        android.app.ActivityManager.user=10;switched.jobs.remove().run();check(switched.updater.installs==0);
        check(switched.snapshot().getString("availability").equals("restricted"));android.app.ActivityManager.user=0;
        check(switched.snapshot().getJSONObject("check").has("error"));
        Fixture failed=new Fixture();failed.updater.failCheck=true;failed.snapshot();failed.settings.check();failed.jobs.remove().run();
        check(failed.snapshot().getJSONObject("check").getString("state").equals("failed"));check(failed.snapshot().getJSONArray("capabilities").contains("check"));
        Fixture installFailure=new Fixture();String failureKey=installFailure.offer();installFailure.updater.failInstall=true;
        installFailure.settings.install(failureKey,"home");installFailure.jobs.remove().run();check(installFailure.snapshot().getJSONObject("check").has("error"));

        Fixture restart=new Fixture();restart.updater.phase="updated_need_reboot";restart.updater.generation=3;
        String restartKey=restart.snapshot().getString("reboot_key");restart.updater.generation=8;
        check(restart.settings.reboot(restartKey).equals("update_unavailable"));check(restart.updater.reboots==0);
        String current=restart.snapshot().getString("reboot_key");check(!current.equals(restartKey));
        check(restart.settings.reboot(current).equals("update_reboot_requested"));check(restart.updater.reboots==1);
        check(restart.settings.reboot(current).equals("update_unavailable"));
    }
}
