package dev.makepad.octosense.controls;

/** Device intensity choices, preserving stored custom values outside the offered granularity. */
public final class SoundFeedbackContract {
    private SoundFeedbackContract() {}
    public static Integer integer(String raw,Integer missing) {
        if(raw==null)return missing;
        if(!raw.matches("0|[1-9][0-9]{0,8}"))return null;
        try{return Integer.valueOf(raw);}catch(NumberFormatException invalid){return null;}
    }
    public static String toggle(String raw) {
        Integer value=integer(raw,1);return value==null?null:value==0?"off":value==1?"on":null;
    }
    public static String intensity(String raw,int defaultValue,int levels) {
        if(defaultValue<0||defaultValue>3||levels<1||levels>3)return null;
        Integer value=integer(raw,defaultValue);if(value==null||value>3)return null;
        if(value==0)return "off";if(levels==1&&value==defaultValue)return "default";
        return value==1?"low":value==2?"medium":"high";
    }
    public static String[] choices(int levels,int defaultValue) {
        if(defaultValue<0||defaultValue>3)return new String[0];
        switch(levels){case 1:return defaultValue==0?new String[]{"off"}:new String[]{"off","default"};
            case 2:return new String[]{"off","low","high"};case 3:return new String[]{"off","low","medium","high"};default:return new String[0];}
    }
    public static int stored(String value,int levels,int defaultValue) {
        boolean offered=false;for(String option:choices(levels,defaultValue))if(option.equals(value))offered=true;
        if(!offered)throw new IllegalArgumentException("Unsupported vibration intensity");
        switch(value){case "off":return 0;case "default":return defaultValue;case "low":return 1;case "medium":return 2;case "high":return 3;default:throw new IllegalArgumentException("Unknown intensity");}
    }
    public static boolean intensityChoice(String value){return "off".equals(value)||"default".equals(value)||"low".equals(value)||"medium".equals(value)||"high".equals(value);}
    public enum Coupling { NONE, RING, TOUCH }
    public enum Slot { PRIMARY, RING_LEGACY, TOUCH_LEGACY, HARDWARE_TOUCH }
    public interface Store {boolean allowed();boolean write(Slot slot,int value);Integer read(Slot slot);}
    /** Finite compatibility writes. This is deliberately not an atomic transaction. */
    public static String apply(Coupling coupling,int value,int defaultValue,Store store) {
        if(!store.allowed()||!store.write(Slot.PRIMARY,value))return "control_unavailable";
        boolean complete=true;
        if(coupling==Coupling.RING)complete=companion(store,Slot.RING_LEGACY,value==0?0:1);
        if(coupling==Coupling.TOUCH){
            boolean legacy=companion(store,Slot.TOUCH_LEGACY,value==0?0:1);
            boolean hardware=companion(store,Slot.HARDWARE_TOUCH,value==0?defaultValue:value);
            complete=legacy&&hardware;
        }
        if(!complete||!store.allowed())return "control_partial";
        return Integer.valueOf(value).equals(store.read(Slot.PRIMARY))?"control_applied":"control_requested";
    }
    private static boolean companion(Store store,Slot slot,int value){return store.allowed()&&store.write(slot,value)&&Integer.valueOf(value).equals(store.read(slot));}
}
