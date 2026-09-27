package com.android.systemui.octosense;
import android.os.Bundle;
/** Role-owned camera/microphone controls; no arbitrary service operations. */
interface ISensorSettings {
    Bundle snapshot();
    Bundle setAccess(int sensor, boolean allowed);
}
