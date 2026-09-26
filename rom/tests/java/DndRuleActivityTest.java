package dev.makepad.octosense.settingsbroker;

public final class DndRuleActivityTest {
    public static class Pinned {
        boolean active;
        public boolean isActive() { return active; }
        public boolean isAutomaticActive() { throw new AssertionError("Legacy method used on new platform"); }
    }
    public static class OriginalApi35 {
        boolean active;
        public boolean isAutomaticActive() { return active; }
    }
    public static class Unknown {}
    public static class WrongType { public int isActive() { return 1; } }
    public static class MissingDependency {
        public boolean isActive() { throw new NoSuchMethodError("missing internal dependency"); }
        public boolean isAutomaticActive() { throw new AssertionError("Cannot substitute older semantics after native failure"); }
    }
    public static class Fatal {
        public boolean isActive() { throw new OutOfMemoryError("fixture"); }
    }
    private static void check(boolean value) { if (!value) throw new AssertionError(); }
    public static void main(String[] args) {
        Pinned pinned = new Pinned(); OriginalApi35 older = new OriginalApi35();
        check(Boolean.FALSE.equals(DndRuleActivity.read(pinned)));
        check(Boolean.FALSE.equals(DndRuleActivity.read(older)));
        pinned.active = true; older.active = true;
        check(Boolean.TRUE.equals(DndRuleActivity.read(pinned)));
        check(Boolean.TRUE.equals(DndRuleActivity.read(older)));
        check(DndRuleActivity.read(null) == null);
        check(DndRuleActivity.read(new Unknown()) == null);
        check(DndRuleActivity.read(new WrongType()) == null);
        check(DndRuleActivity.read(new MissingDependency()) == null);
        try { DndRuleActivity.read(new Fatal()); throw new AssertionError("VM failure swallowed"); }
        catch (OutOfMemoryError expected) { }
        System.out.println("DND native activity compatibility PASS");
    }
}
