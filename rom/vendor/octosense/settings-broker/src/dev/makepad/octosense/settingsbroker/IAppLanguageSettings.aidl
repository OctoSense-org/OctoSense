package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;
interface IAppLanguageSettings {
    Bundle snapshot(long requestId, String packageName, String catalogKey, String parentKey, String query, int offset);
    String select(String packageName, String catalogKey, String choiceKey);
}
