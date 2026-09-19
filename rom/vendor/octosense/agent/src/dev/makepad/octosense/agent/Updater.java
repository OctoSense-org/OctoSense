package dev.makepad.octosense.agent;

import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.PowerManager;
import android.os.UpdateEngine;
import android.os.UpdateEngineCallback;
import android.provider.Settings;
import android.util.Log;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.security.MessageDigest;

/**
 * Over-the-air updates for the ROM and the OctoSense Home app, served from
 * GitHub Releases (docs/updates.md).
 *
 * The release carries update.json. The ROM part is an A/B payload that
 * update_engine streams from its URL straight into the inactive slot while
 * Android keeps running; update_engine checks the payload against the ROM's
 * release key, so only our builds apply. The Home part is a platform-signed APK
 * installed silently with INSTALL_PACKAGES; PackageManager refuses any other
 * signer. Nothing here is trusted for integrity beyond those two checks and
 * the APK's sha256.
 */
final class Updater {
    static final String TAG = "OctoSenseUpdater";
    /** Where update.json lives. GitHub's "latest/download" redirect needs no API or token for a public repo. */
    static final String DEFAULT_SOURCE =
            "https://github.com/OctoSense-org/octosense-rom/releases/latest/download/update.json";
    static final String SETTING_SOURCE = "octosense_update_source";
    static final String HOME_PACKAGE = "dev.makepad.octosense";

    // update_engine's status codes (UpdateEngine.UpdateStatusConstants).
    static final String[] STATUS = {"idle", "checking", "update_available", "downloading",
            "verifying", "finalizing", "updated_need_reboot", "reporting_error_event",
            "attempting_rollback", "disabled", "need_permission_to_update", "cleanup_previous_update"};

    interface Listener { void changed(Bundle status); }

    private final Context context;
    private final Handler handler;
    private final UpdateEngine engine = new UpdateEngine();
    private Listener listener = s -> { };

    private volatile JSONObject manifest;
    private volatile String phase = "idle";
    private volatile float progress;
    private volatile String error = "";
    private volatile String homeState = "";

    Updater(Context context) {
        this.context = context;
        HandlerThread thread = new HandlerThread("octosense-updater");
        thread.start();
        handler = new Handler(thread.getLooper());
        engine.bind(new UpdateEngineCallback() {
            @Override public void onStatusUpdate(int status, float percent) {
                phase = status >= 0 && status < STATUS.length ? STATUS[status] : "status_" + status;
                progress = percent;
                listener.changed(status());
            }
            @Override public void onPayloadApplicationComplete(int errorCode) {
                if (errorCode == 0) {
                    phase = "updated_need_reboot";
                    error = "";
                } else {
                    phase = "failed";
                    error = "update_engine error " + errorCode;
                }
                listener.changed(status());
            }
        }, handler);
    }

    void setListener(Listener l) { listener = l == null ? s -> { } : l; }

    String source() {
        String s = Settings.Secure.getString(context.getContentResolver(), SETTING_SOURCE);
        return s == null || s.isEmpty() ? DEFAULT_SOURCE : s;
    }

    // ---- check ------------------------------------------------------------

    /** Fetches update.json and says what is newer than what runs now. Blocking; call off the main thread. */
    Bundle check() throws Exception {
        JSONObject m = new JSONObject(new String(fetch(source(), 1 << 20), "UTF-8"));
        manifest = m;
        Bundle b = new Bundle();
        b.putBoolean("ok", true);
        b.putString("source", source());
        b.putString("release", m.optString("name"));
        JSONObject rom = m.optJSONObject("rom");
        if (rom != null) {
            long current = Build.TIME / 1000;
            long offered = rom.optLong("timestamp");
            b.putString("rom_current", Build.VERSION.INCREMENTAL);
            b.putString("rom_offered", rom.optString("incremental"));
            b.putBoolean("rom_newer", offered > current
                    && !rom.optString("incremental").equals(Build.VERSION.INCREMENTAL));
        }
        JSONObject home = m.optJSONObject("home");
        if (home != null) {
            long installed = homeVersion();
            b.putLong("home_current", installed);
            b.putLong("home_offered", home.optLong("version_code"));
            b.putBoolean("home_newer", home.optLong("version_code") > installed);
        }
        return b;
    }

    private long homeVersion() {
        try {
            PackageInfo info = context.getPackageManager().getPackageInfo(HOME_PACKAGE, 0);
            return info.getLongVersionCode();
        } catch (PackageManager.NameNotFoundException e) {
            return 0;
        }
    }

    // ---- ROM: A/B payload through update_engine ---------------------------

    /** Starts streaming the ROM into the inactive slot. Returns at once; progress arrives in status(). */
    Bundle applyRom() throws Exception {
        if (manifest == null) check();
        JSONObject rom = manifest.optJSONObject("rom");
        if (rom == null) return fail("no ROM in the release");
        JSONArray props = rom.getJSONArray("payload_properties");
        String[] headers = new String[props.length()];
        for (int i = 0; i < headers.length; i++) headers[i] = props.getString(i);
        // update_engine downloads the URL itself; hand it the storage URL the
        // release link redirects to, since that is what serves byte ranges.
        String url = resolve(rom.getString("url"));
        error = "";
        phase = "starting";
        progress = 0;
        engine.applyPayload(url, rom.getLong("payload_offset"), rom.getLong("payload_size"), headers);
        Log.i(TAG, "applying " + rom.optString("incremental"));
        return ok();
    }

    /** Stops a running ROM update; the inactive slot is left unbootable until the next apply. */
    Bundle cancel() {
        engine.cancel();
        phase = "cancelled";
        return ok();
    }

    // ---- Home app: silent APK install -------------------------------------

    /** Downloads the Home APK, checks its sha256, installs it. Blocking; call off the main thread. */
    Bundle applyHome() throws Exception {
        if (manifest == null) check();
        JSONObject home = manifest.optJSONObject("home");
        if (home == null) return fail("no Home app in the release");
        homeState = "downloading";
        File apk = new File(context.getCacheDir(), "home-update.apk");
        download(home.getString("url"), apk);
        String sum = sha256(apk);
        if (!sum.equalsIgnoreCase(home.getString("sha256"))) {
            apk.delete();
            homeState = "checksum mismatch";
            return fail("home APK checksum mismatch");
        }
        homeState = "installing";
        PackageInstaller installer = context.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams params =
                new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        params.setAppPackageName(HOME_PACKAGE);
        params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED);
        int id = installer.createSession(params);
        try (PackageInstaller.Session session = installer.openSession(id)) {
            try (InputStream in = new FileInputStream(apk);
                 OutputStream out = session.openWrite("base.apk", 0, apk.length())) {
                copy(in, out);
                session.fsync(out);
            }
            String action = "dev.makepad.octosense.agent.INSTALL_RESULT." + id;
            context.registerReceiver(new BroadcastReceiver() {
                @Override public void onReceive(Context c, Intent intent) {
                    int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, -999);
                    String message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE);
                    homeState = status == PackageInstaller.STATUS_SUCCESS ? "installed" : "failed: " + message;
                    Log.i(TAG, "home install " + homeState);
                    c.unregisterReceiver(this);
                    listener.changed(status());
                }
            }, new IntentFilter(action), Context.RECEIVER_NOT_EXPORTED);
            Intent result = new Intent(action).setPackage(context.getPackageName());
            PendingIntent pi = PendingIntent.getBroadcast(context, id, result,
                    PendingIntent.FLAG_MUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
            session.commit(pi.getIntentSender());
        }
        apk.delete();
        return ok();
    }

    // ---- reboot and status ------------------------------------------------

    Bundle reboot() {
        if (!"updated_need_reboot".equals(phase)) return fail("no update waiting for a restart");
        context.getSystemService(PowerManager.class).reboot(null);
        return ok();
    }

    Bundle status() {
        Bundle b = ok();
        b.putString("rom_phase", phase);
        b.putFloat("rom_progress", progress);
        if (!error.isEmpty()) b.putString("rom_error", error);
        if (!homeState.isEmpty()) b.putString("home_state", homeState);
        b.putString("running", Build.VERSION.INCREMENTAL);
        b.putString("slot", android.os.SystemProperties.get("ro.boot.slot_suffix"));
        return b;
    }

    // ---- HTTP -------------------------------------------------------------

    private static HttpURLConnection open(String url) throws Exception {
        HttpURLConnection c = (HttpURLConnection) new URL(url).openConnection();
        c.setConnectTimeout(20000);
        c.setReadTimeout(60000);
        c.setRequestProperty("User-Agent", "OctoSense-Updater");
        return c;
    }

    /** Follows redirects to the final URL without downloading the body. */
    private static String resolve(String url) throws Exception {
        for (int hops = 0; hops < 8; hops++) {
            HttpURLConnection c = open(url);
            c.setInstanceFollowRedirects(false);
            c.setRequestMethod("HEAD");
            int code = c.getResponseCode();
            String next = c.getHeaderField("Location");
            c.disconnect();
            if (code / 100 != 3 || next == null) return url;
            url = new URL(new URL(url), next).toString();
        }
        return url;
    }

    private static byte[] fetch(String url, int max) throws Exception {
        HttpURLConnection c = open(url);
        try (InputStream in = c.getInputStream()) {
            if (c.getResponseCode() != 200) throw new Exception("HTTP " + c.getResponseCode() + " for " + url);
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buf = new byte[16384];
            for (int n; (n = in.read(buf)) > 0; ) {
                out.write(buf, 0, n);
                if (out.size() > max) throw new Exception("response too large");
            }
            return out.toByteArray();
        } finally {
            c.disconnect();
        }
    }

    private static void download(String url, File to) throws Exception {
        HttpURLConnection c = open(url);
        try (InputStream in = c.getInputStream(); OutputStream out = new FileOutputStream(to)) {
            if (c.getResponseCode() != 200) throw new Exception("HTTP " + c.getResponseCode() + " for " + url);
            copy(in, out);
        } finally {
            c.disconnect();
        }
    }

    private static void copy(InputStream in, OutputStream out) throws Exception {
        byte[] buf = new byte[1 << 16];
        for (int n; (n = in.read(buf)) > 0; ) out.write(buf, 0, n);
    }

    private static String sha256(File f) throws Exception {
        MessageDigest md = MessageDigest.getInstance("SHA-256");
        try (InputStream in = new FileInputStream(f)) {
            byte[] buf = new byte[1 << 16];
            for (int n; (n = in.read(buf)) > 0; ) md.update(buf, 0, n);
        }
        StringBuilder s = new StringBuilder();
        for (byte x : md.digest()) s.append(String.format("%02x", x));
        return s.toString();
    }

    private static Bundle ok() { Bundle b = new Bundle(); b.putBoolean("ok", true); return b; }
    private static Bundle fail(String why) {
        Bundle b = new Bundle(); b.putBoolean("ok", false); b.putString("reason", why); return b;
    }
}
