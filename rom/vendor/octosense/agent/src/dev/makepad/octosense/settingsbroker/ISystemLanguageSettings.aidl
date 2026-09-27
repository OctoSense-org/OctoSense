package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;
interface ISystemLanguageSettings {
    Bundle snapshot(long requestId, String key, String parent, String query, int offset);
    String apply(String key, in String[] orderedTargets);
}
