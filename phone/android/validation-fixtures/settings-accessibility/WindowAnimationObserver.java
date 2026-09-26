package dev.makepad.octosense.settingsa11yfixture;

/** Read-only shell entry point: SDK35's text dump omits animation scales. */
public final class WindowAnimationObserver {
    public static void main(String[] args) throws Exception {
        if (args.length != 0) throw new IllegalArgumentException("No arguments expected");
        Object service = Class.forName("android.view.WindowManagerGlobal")
                .getMethod("getWindowManagerService").invoke(null);
        float[] values = (float[]) Class.forName("android.view.IWindowManager")
                .getMethod("getAnimationScales").invoke(service);
        if (values == null || values.length != 3) throw new IllegalStateException("Missing scales");
        for (float value : values) if (!Float.isFinite(value)) throw new IllegalStateException("Invalid scale");
        System.out.println("window_animation_scales=" + values[0] + "," + values[1] + "," + values[2]);
    }
}
