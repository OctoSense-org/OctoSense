package dev.makepad.octosense.validation;

/** Instrumentation-owned lifetime survives Activity.recreate within the same
 * Home process. A remote close ends the fixture; destroying an old activity
 * during recreation does not complete the test. */
public final class LauncherUiFixture {
    private static volatile boolean active,finished;
    private static volatile String cleanupOwner;
    private static final java.util.concurrent.atomic.AtomicReference<String> cleanupRequested=new java.util.concurrent.atomic.AtomicReference<>();
    public static void start() {active=true;finished=false;}
    public static void allowShortcutCleanup(String owner) {
        if(owner!=null&&!owner.matches("[a-f0-9-]{36}")) throw new IllegalArgumentException("Invalid shortcut owner");
        cleanupOwner=owner;cleanupRequested.set(null);
    }
    public static boolean requestShortcutCleanup(String owner) {
        return active&&owner!=null&&owner.equals(cleanupOwner)&&cleanupRequested.compareAndSet(null,owner);
    }
    public static String takeShortcutCleanup() {return cleanupRequested.getAndSet(null);}
    public static boolean active() {return active;}
    public static void finish() {if(active) finished=true;}
    public static boolean finished() {return finished;}
    public static void detach() {active=false;cleanupOwner=null;cleanupRequested.set(null);}
}
