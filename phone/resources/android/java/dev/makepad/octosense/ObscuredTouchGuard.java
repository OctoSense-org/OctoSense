package dev.makepad.octosense;

import android.view.MotionEvent;

/** Reject the whole gesture if another window obscures any part of it. */
public final class ObscuredTouchGuard {
    private boolean blocked;

    public boolean accept(MotionEvent event) {
        if (event.getActionMasked() == MotionEvent.ACTION_DOWN) blocked = false;
        // PARTIALLY_OBSCURED is available from API 29; its flag bit is harmless
        // on older devices. Check it too, not only the touch coordinate itself.
        if ((event.getFlags() & (MotionEvent.FLAG_WINDOW_IS_OBSCURED | 0x2)) != 0) blocked = true;
        return !blocked;
    }
}
