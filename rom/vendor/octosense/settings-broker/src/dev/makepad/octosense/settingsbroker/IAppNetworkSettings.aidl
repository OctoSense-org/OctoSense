package dev.makepad.octosense.settingsbroker;
import android.os.Bundle;
interface IAppNetworkSettings {
    Bundle snapshot(long id,String packageName);
    Bundle set(String packageName,String key,String field,boolean enabled);
}
