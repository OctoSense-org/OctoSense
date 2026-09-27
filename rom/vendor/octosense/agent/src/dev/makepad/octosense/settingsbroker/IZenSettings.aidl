package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;

/** Finite DND operations; no native policy objects, rule IDs or arbitrary URIs. */
interface IZenSettings {
    Bundle snapshot();
    boolean setMode(String mode);
    Bundle settingsSnapshot(long requestId,int offset,String generation);
    Bundle policy(String key,String field,String value);
    Bundle schedule(String key,String target,String name,in int[] days,int startMinute,int endMinute,boolean exitAtAlarm,boolean enabled);
    Bundle enabled(String key,String target,boolean enabled);
    Bundle deleteRule(String key,String target);
}
