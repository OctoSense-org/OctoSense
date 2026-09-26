package dev.makepad.octosense.controls;

import java.util.function.Supplier;

/** Finite native color preferences; no caller-supplied provider names or automatic normalization. */
public final class ColorAccessibility {
    private ColorAccessibility() { throw new AssertionError(); }
    public enum Setting {
        INVERSION("accessibility_display_inversion_enabled",0),
        CORRECTION("accessibility_display_daltonizer_enabled",0),
        MODE("accessibility_display_daltonizer",12);
        public final String key;
        private final int missing;
        Setting(String key,int missing) {this.key=key;this.missing=missing;}
        public String[] choices() {
            return this==MODE?new String[]{"deuteranomaly","protanomaly","tritanomaly","grayscale"}:new String[]{"off","on"};
        }
        public int stored(String value) {
            if(this!=MODE) {
                if("off".equals(value))return 0;
                if("on".equals(value))return 1;
            } else {
                if("deuteranomaly".equals(value))return 12;
                if("protanomaly".equals(value))return 11;
                if("tritanomaly".equals(value))return 13;
                if("grayscale".equals(value))return 0;
            }
            throw new IllegalArgumentException("Unknown color preference choice");
        }
        public String observed(String raw) {
            int value=missing;
            if(raw!=null) {
                if(!raw.matches("-?[0-9]+"))return null;
                try {value=Integer.parseInt(raw);} catch(NumberFormatException unknown){return null;}
            }
            if(this!=MODE)return value==0?"off":value==1?"on":null;
            switch(value) {case 12:return "deuteranomaly";case 11:return "protanomaly";case 13:return "tritanomaly";case 0:return "grayscale";default:return null;}
        }
    }
    public enum Access { WRITABLE, READ_ONLY, RESTRICTED }
    public interface Store {
        String read(Setting setting);
        boolean write(Setting setting,int value);
    }
    /** Shared adapter logic is independently tested against provider failure and authority changes. */
    public static final class Backend {
        private final Store store;
        private final Supplier<Access> access;
        public Backend(Store store,Supplier<Access> access) {this.store=store;this.access=access;}
        public String read(Setting setting) {return setting.observed(store.read(setting));}
        public boolean writable() {return access.get()==Access.WRITABLE;}
        private String denial() {
            Access current=access.get();
            return current==Access.RESTRICTED?"policy_restricted":current==Access.READ_ONLY?"control_unavailable":null;
        }
        public synchronized String apply(Setting setting,String choice) {
            int stored=setting.stored(choice);
            try {
                String denied=denial();if(denied!=null)return denied;
                if(read(setting)==null)return "control_unavailable";
                // Recheck after reading: user/lock/permission may have changed since the snapshot.
                denied=denial();if(denied!=null)return denied;
                if(!store.write(setting,stored))return "control_unavailable";
            } catch(SecurityException|IllegalStateException unavailable) {return "control_unavailable";}
            // Accepted provider write is distinct from matching readback and from compositor proof.
            try {return choice.equals(read(setting))?"control_applied":"control_requested";}
            catch(SecurityException|IllegalStateException unavailable) {return "control_requested";}
        }
    }
}
