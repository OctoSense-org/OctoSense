package dev.makepad.octosense.agent;
import android.os.Bundle;
import org.json.JSONObject;
/** Fake download/engine boundary. These tests cannot install an APK or reboot. */
final class Updater {
    static final String[] STATUS={"idle","checking","update_available","downloading","verifying","finalizing","updated_need_reboot"};
    String source="https://updates.example/update.json",phase="idle",manifest="offer-a";
    long home=4,generation;int checks,installs,reboots;
    boolean failCheck,failInstall;CheckedRelease installed;
    static final class CheckedRelease {
        final String source;final JSONObject manifest;final Bundle versions;
        CheckedRelease(String s,JSONObject m,Bundle v){source=s;manifest=m;versions=v;}
    }
    String source(){return source;}long homeVersion(){return home;}
    boolean engineObserved(){return true;}boolean busy(){return !phase.equals("idle");}
    Bundle status(){Bundle b=new Bundle();b.putString("rom_phase",phase);b.putLong("phase_generation",generation);return b;}
    CheckedRelease checkRelease(){checks++;if(failCheck)throw new IllegalStateException();
        Bundle v=new Bundle();v.putString("release",manifest);v.putString("rom_offered","rom2");v.putLong("home_offered",5);v.putLong("home_current",home);
        v.putBoolean("rom_newer",true);v.putBoolean("home_newer",true);
        return new CheckedRelease(source,new JSONObject().put("release",manifest),v);
    }
    Bundle applyReviewed(String part,CheckedRelease release){if(failInstall)throw new IllegalStateException();installed=release;installs++;Bundle b=new Bundle();b.putBoolean("ok",true);return b;}
    Bundle reboot(){reboots++;Bundle b=new Bundle();b.putBoolean("ok",true);return b;}
}
