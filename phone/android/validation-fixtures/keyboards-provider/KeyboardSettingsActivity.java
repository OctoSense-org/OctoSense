package dev.makepad.octosense.keyboardprovider;

import android.app.Activity;
import android.os.Bundle;
import android.widget.TextView;

/** Public provider-owned settings destination, with no setting mutation. */
public final class KeyboardSettingsActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        android.content.SharedPreferences prefs = getSharedPreferences("observation", MODE_PRIVATE);
        prefs.edit().putInt("settings_opens", prefs.getInt("settings_opens", 0) + 1).commit();
        TextView view = new TextView(this);
        view.setText("Provider settings: " + getApplicationInfo().loadLabel(getPackageManager()));
        view.setTextSize(24);
        view.setPadding(24, 48, 24, 24);
        setContentView(view);
    }
}
