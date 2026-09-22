package dev.makepad.octosense.contracts;

import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Binder;

/** Checks actual transaction identity, not a package name supplied in arguments. */
public final class CallerGuard {
    private CallerGuard() {}

    public static int require(Context context, String... allowedPackages) {
        int uid = Binder.getCallingUid();
        PackageManager pm = context.getPackageManager();
        final int owner;
        try {
            owner = pm.getApplicationInfo(context.getPackageName(), 0).uid;
        } catch (PackageManager.NameNotFoundException e) {
            throw new SecurityException("Bridge package identity unavailable", e);
        }
        if (uid / 100000 != owner / 100000)
            throw new SecurityException("Cross-user bridge access denied");
        if (pm.checkSignatures(owner, uid) != PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller certificate does not match");
        String[] actual = pm.getPackagesForUid(uid);
        if (actual != null) {
            for (String installed : actual) {
                for (String allowed : allowedPackages) {
                    if (allowed.equals(installed)) return uid;
                }
            }
        }
        throw new SecurityException("Caller package is not allowed");
    }
}
