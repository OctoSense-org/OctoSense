package dev.makepad.octosense.settingsbroker;
import android.app.PendingIntent;
/** Current-user account metadata/sync; removal always opens trusted confirmation. */
interface IAccountSettings {
    String snapshot(long id);
    String details(long id, String key);
    String masterSync(boolean enabled);
    String sync(String key, String authorityKey, String action, boolean enabled);
    PendingIntent addition(String providerKey);
    PendingIntent removal(String key);
}
