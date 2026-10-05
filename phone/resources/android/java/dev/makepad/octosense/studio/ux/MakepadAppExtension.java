package dev.makepad.octosense.studio.ux;

import dev.makepad.android.MakepadActivity;

/** Keep the isolated UX package on Home's platform path, including system Back. */
public final class MakepadAppExtension
        extends dev.makepad.octosense.studio.MakepadAppExtension {
    public MakepadAppExtension(MakepadActivity activity) {
        super(activity);
    }
}
