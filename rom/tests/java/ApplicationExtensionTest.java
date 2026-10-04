package dev.makepad.android;

/** The production reflection loader runs against only the Android metadata transport doubles. */
public final class ApplicationExtensionTest {
    public static final class BackExtension implements MakepadActivity.ApplicationExtension {
        final MakepadActivity activity;
        public BackExtension(MakepadActivity activity) { this.activity = activity; }
        public boolean onBackPressed() { activity.home = true; return true; }
        public boolean usesSystemBackCallback() { return true; }
    }

    public static void main(String[] args) {
        MakepadActivity custom = new MakepadActivity("dev.makepad.octosense.shapetest");
        custom.manager.info.metaData = new android.os.Bundle();
        custom.manager.info.metaData.put("dev.makepad.android.APPLICATION_EXTENSION", BackExtension.class.getName());
        custom.create();
        require(custom.extension() != null, "custom package lost its manifest-declared Android integration");
        require(custom.extension().usesSystemBackCallback(), "predictive Back owner missing");
        require(custom.extension().onBackPressed() && custom.home, "Back did not reach the shell");

        MakepadActivity legacy = new MakepadActivity("dev.makepad.android");
        legacy.create();
        require(legacy.extension() instanceof MakepadAppExtension, "package-name fallback was lost");
        MakepadActivity empty = new MakepadActivity("dev.makepad.android");
        empty.manager.info.metaData = new android.os.Bundle();
        empty.create();
        require(empty.extension() instanceof MakepadAppExtension, "unrelated metadata disabled the fallback");
        MakepadActivity blank = new MakepadActivity("dev.makepad.android");
        blank.manager.info.metaData = new android.os.Bundle();
        blank.manager.info.metaData.put("dev.makepad.android.APPLICATION_EXTENSION", "");
        blank.create();
        require(blank.extension() instanceof MakepadAppExtension, "empty setting disabled the fallback");
        MakepadActivity override = new MakepadActivity("dev.makepad.android");
        override.manager.info.metaData = custom.manager.info.metaData;
        override.create();
        require(override.extension() instanceof BackExtension, "explicit setting must override the conventional class");
        MakepadActivity ordinary = new MakepadActivity("dev.makepad.example");
        ordinary.create();
        require(ordinary.extension() == null, "ordinary apps must not require an extension");
    }

    static void require(boolean value, String message) { if (!value) throw new AssertionError(message); }
}

class MakepadActivity {
    interface ApplicationExtension {
        default boolean onBackPressed() { return false; }
        default boolean usesSystemBackCallback() { return false; }
    }
    final android.content.pm.PackageManager manager = new android.content.pm.PackageManager();
    private final String packageName;
    private ApplicationExtension mApplicationExtension;
    boolean home;
    MakepadActivity(String packageName) { this.packageName = packageName; }
    String getPackageName() { return packageName; }
    android.content.pm.PackageManager getPackageManager() { return manager; }
    void create() { createApplicationExtension(); }
    ApplicationExtension extension() { return mApplicationExtension; }
    /* LOADER */
}

class MakepadAppExtension implements MakepadActivity.ApplicationExtension {
    public MakepadAppExtension(MakepadActivity activity) {}
}

class Log {
    static void e(String tag, String message, Throwable failure) {
        throw new AssertionError(message, failure);
    }
}
