package dev.makepad.octosense.systemlanguagesfixture;

import android.content.res.Configuration;
import org.json.JSONObject;

/** Shell app_process observation only. No configuration or SettingsProvider setter. */
public final class ConfigurationObservation {
    public static void main(String[] args) throws Exception {
        Object service = Class.forName("android.app.ActivityManager").getMethod("getService").invoke(null);
        Configuration configuration = (Configuration) Class.forName("android.app.IActivityManager")
                .getMethod("getConfiguration").invoke(service);
        System.out.println(new JSONObject().put("locales", configuration.getLocales().toLanguageTags())
                .put("user_set_locale", Configuration.class.getField("userSetLocale").getBoolean(configuration))
                .put("layout_direction", configuration.getLayoutDirection()));
    }
}
