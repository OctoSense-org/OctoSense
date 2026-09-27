package dev.makepad.octosense.keyboardprovider;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import org.json.JSONObject;

/** DUMP-gated read-only observations from the synthetic provider itself. */
public final class KeyboardObservation extends BroadcastReceiver {
    @Override public void onReceive(Context context, Intent intent) {
        try {
            if (!"state".equals(intent.getStringExtra("operation"))) throw new IllegalArgumentException("Finite observation required");
            SharedPreferences prefs = context.getSharedPreferences("observation", Context.MODE_PRIVATE);
            setResultCode(0);
            String state = new JSONObject().put("package", context.getPackageName())
                    .put("test_editor_active", prefs.getBoolean("test_editor_active", false))
                    .put("test_input_starts", prefs.getInt("test_input_starts", 0))
                    .put("commits", prefs.getInt("commits", 0))
                    .put("last_commit_accepted", prefs.getBoolean("last_commit_accepted", false))
                    .put("settings_opens", prefs.getInt("settings_opens", 0)).toString();
            setResultData(android.util.Base64.encodeToString(state.getBytes(java.nio.charset.StandardCharsets.UTF_8), android.util.Base64.NO_WRAP));
        } catch (Exception failure) {
            setResultCode(1);
            setResultData("unavailable");
        }
    }
}
