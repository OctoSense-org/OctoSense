package dev.makepad.octosense.settingsa11yfixture;

import android.app.Activity;
import android.os.Bundle;
import dev.makepad.octosense.contracts.SystemSettings;

/** A real foreground, independently signed caller for the shared shade router. */
public final class SettingsRouteProbeActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        SystemSettings.openPreferred(this,getIntent().getStringExtra("destination"));
        finish();
    }
}
