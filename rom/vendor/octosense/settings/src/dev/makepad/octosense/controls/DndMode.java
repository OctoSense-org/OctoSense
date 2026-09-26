package dev.makepad.octosense.controls;

/** The four manual Android Zen modes. Values are not SettingsProvider writes. */
public enum DndMode {
    OFF("off",0,1), PRIORITY("priority",1,2), ALARMS("alarms",3,4), SILENCE("silence",2,3);
    public final String wire;
    public final int zen,interruptionFilter;
    DndMode(String wire,int zen,int interruptionFilter) {
        this.wire=wire;this.zen=zen;this.interruptionFilter=interruptionFilter;
    }
    public static DndMode parse(String value) {
        for(DndMode mode:values()) if(mode.wire.equals(value)) return mode;
        throw new IllegalArgumentException("Unknown Do Not Disturb mode");
    }
    public static DndMode fromZen(int value) {
        for(DndMode mode:values()) if(mode.zen==value) return mode;
        return null;
    }
    public static DndMode fromInterruptionFilter(int value) {
        for(DndMode mode:values()) if(mode.interruptionFilter==value) return mode;
        return null;
    }
    public static String[] choices() {
        DndMode[] modes=values();String[] result=new String[modes.length];
        for(int index=0;index<modes.length;index++) result[index]=modes[index].wire;
        return result;
    }
}
