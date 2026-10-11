import android.os.Process;
import android.os.SystemClock;
import android.view.InputDevice;
import android.view.InputEvent;
import android.view.MotionEvent;
import java.lang.reflect.Method;

/**
 * One synthetic pointer gesture, for an adb-shell app_process invocation.
 * Arguments are x,y,milliseconds, beginning at zero and ending with pointer-up.
 * Compile against android.jar, dex with d8, and run only against a test app.
 * Output is monotonic_ns,action,x,y. No parsing or log formatting runs while
 * the pointer is down. This measures software input, not digitizer latency.
 */
public final class TouchLatency {
    public static void main(String[] args) throws Exception {
        if (args.length < 3 || args.length > 10000) {
            throw new IllegalArgumentException("Expected 3..10000 x,y,milliseconds points");
        }
        int count = args.length;
        float[] x = new float[count], y = new float[count];
        long[] at = new long[count], recorded = new long[count];
        float[] actualX = new float[count], actualY = new float[count];
        int[] actions = new int[count];
        for (int i = 0; i < count; i++) {
            String[] parts = args[i].split(",");
            if (parts.length != 3) throw new IllegalArgumentException("Expected x,y,milliseconds");
            x[i] = Float.parseFloat(parts[0]);
            y[i] = Float.parseFloat(parts[1]);
            long milliseconds = Long.parseLong(parts[2]);
            if (!Float.isFinite(x[i]) || !Float.isFinite(y[i])
                    || milliseconds < 0 || milliseconds > 60000
                    || (i == 0 && milliseconds != 0)) {
                throw new IllegalArgumentException("Invalid point or time");
            }
            at[i] = milliseconds * 1000000L;
            if (i > 0 && at[i] <= at[i - 1]) {
                throw new IllegalArgumentException("Point times must increase");
            }
        }
        Class<?> type;
        try {
            type = Class.forName("android.hardware.input.InputManagerGlobal");
        } catch (ClassNotFoundException olderAndroid) {
            type = Class.forName("android.hardware.input.InputManager");
        }
        Object manager = type.getDeclaredMethod("getInstance").invoke(null);
        Method inject = type.getMethod("injectInputEvent", InputEvent.class, int.class);
        // Only this short-lived adb-shell thread changes priority. The app,
        // system input dispatcher, scheduler policy and device settings do not.
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_DISPLAY);
        long start = System.nanoTime(), down = SystemClock.uptimeMillis();
        int written = 0;
        try {
            for (int i = 0; i < count; i++) {
                long remaining;
                while ((remaining = start + at[i] - System.nanoTime()) > 0) {
                    Thread.sleep(remaining / 1000000L, (int) (remaining % 1000000L));
                }
                long now = System.nanoTime(), elapsed = now - start;
                // A late wake must not inject a burst of stale positions. Skip
                // overdue moves, retaining Down/Up, and sample the requested
                // path at the actual time. The analyzer still reports gaps.
                if (i > 0) {
                    while (i + 1 < count - 1 && at[i + 1] <= elapsed) i++;
                }
                int action = i == 0 ? MotionEvent.ACTION_DOWN
                        : i == count - 1 ? MotionEvent.ACTION_UP : MotionEvent.ACTION_MOVE;
                float px = x[i], py = y[i];
                if (i > 0 && i < count - 1) {
                    double fraction = Math.min(1.0, (double) (elapsed - at[i]) / (at[i + 1] - at[i]));
                    px += (x[i + 1] - x[i]) * fraction;
                    py += (y[i + 1] - y[i]) * fraction;
                }
                MotionEvent event = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, px, py, 0);
                event.setSource(InputDevice.SOURCE_TOUCHSCREEN);
                try {
                    if (!((Boolean) inject.invoke(manager, event, 0))) {
                        throw new IllegalStateException("Touch injection rejected");
                    }
                } finally {
                    event.recycle();
                }
                recorded[written] = now;
                actualX[written] = px;
                actualY[written] = py;
                actions[written++] = action;
            }
        } finally {
            if (written > 0 && actions[written - 1] != MotionEvent.ACTION_UP) {
                MotionEvent cancel = MotionEvent.obtain(down, SystemClock.uptimeMillis(),
                        MotionEvent.ACTION_CANCEL, actualX[written - 1], actualY[written - 1], 0);
                cancel.setSource(InputDevice.SOURCE_TOUCHSCREEN);
                try { inject.invoke(manager, cancel, 0); } finally { cancel.recycle(); }
            }
        }
        StringBuilder output = new StringBuilder(written * 60);
        for (int i = 0; i < written; i++) {
            output.append(recorded[i]).append(',').append(actions[i]).append(',')
                    .append(actualX[i]).append(',').append(actualY[i]).append('\n');
        }
        System.out.print(output);
    }
}
