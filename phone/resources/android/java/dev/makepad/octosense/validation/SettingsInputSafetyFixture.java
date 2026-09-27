package dev.makepad.octosense.validation;

import android.app.Instrumentation;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.InputDevice;
import android.view.MotionEvent;
import android.widget.Button;
import android.widget.FrameLayout;
import dev.makepad.android.MakepadActivity;

/** Opt-in emulator check through the real Activity dispatch and native child view. */
final class SettingsInputSafetyFixture {
    private static void check(boolean value, String message) {
        if (!value) throw new AssertionError(message);
    }
    private static void onMain(Instrumentation runner,Runnable action) {
        java.util.concurrent.atomic.AtomicReference<Throwable> failure=new java.util.concurrent.atomic.AtomicReference<>();
        runner.runOnMainSync(() -> {try {action.run();} catch(Throwable error) {failure.set(error);}});
        if(failure.get()!=null) throw new AssertionError("input fixture failed",failure.get());
    }
    static void run(Instrumentation runner, Bundle report) {
        Intent launch = runner.getTargetContext().getPackageManager()
                .getLaunchIntentForPackage(runner.getTargetContext().getPackageName());
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        MakepadActivity activity = (MakepadActivity) runner.startActivitySync(launch);
        Button[] button = new Button[1];
        int[] clicks = {0};
        try {
            onMain(runner,() -> {
                check(activity.checkSelfPermission("android.permission.HIDE_OVERLAY_WINDOWS")
                        == PackageManager.PERMISSION_GRANTED, "overlay permission missing");
                button[0] = new Button(activity);
                button[0].setText("Input security fixture");
                button[0].setOnClickListener(view -> clicks[0]++);
                activity.getApplicationOverlay().addView(button[0], new FrameLayout.LayoutParams(280, 160));
            });
            runner.waitForIdleSync();
            float[] point=new float[2];
            onMain(runner,() -> {
                int[] position = new int[2];button[0].getLocationInWindow(position);
                check(button[0].getWidth() > 0 && button[0].getHeight() > 0, "fixture not laid out");
                point[0]=position[0]+button[0].getWidth()/2f;point[1]=position[1]+button[0].getHeight()/2f;
            });
            float x=point[0],y=point[1];
            onMain(runner,() -> {
                touch(activity,0,0,x,y);touch(activity,1,0,x,y);
            });
            // Android posts performClick; assertions run after that UI work.
            runner.waitForIdleSync();check(clicks[0]==1,"clean touch did not click");
            onMain(runner,() -> {
                touch(activity,0,1,x,y);touch(activity,1,0,x,y);
            });
            runner.waitForIdleSync();check(clicks[0]==1,"obscured down escaped guard");
            onMain(runner,() -> {
                touch(activity,0,0,x,y);touch(activity,2,2,x,y);touch(activity,1,0,x,y);
            });
            runner.waitForIdleSync();check(clicks[0]==1,"overlay during gesture did not cancel");
            onMain(runner,() -> {
                check(!button[0].isPressed(),"cancel left pressed native view");
                touch(activity,0,0,x,y);touch(activity,1,0,x,y);
            });
            runner.waitForIdleSync();check(clicks[0]==2,"clean gesture after cancel did not recover");
            report.putBoolean("obscured_gestures_cancelled_before_dispatch",true);
        } finally {
            onMain(runner,() -> {
                if(button[0]!=null) activity.getApplicationOverlay().removeView(button[0]);
                activity.finish();
            });
        }
    }
    private static void touch(MakepadActivity activity,int action,int flags,float x,float y) {
        long now=SystemClock.uptimeMillis();
        MotionEvent.PointerProperties properties=new MotionEvent.PointerProperties();
        properties.id=0;properties.toolType=MotionEvent.TOOL_TYPE_FINGER;
        MotionEvent.PointerCoords coords=new MotionEvent.PointerCoords();
        coords.x=x;coords.y=y;coords.pressure=1;coords.size=1;
        MotionEvent event=MotionEvent.obtain(now,now,action,1,new MotionEvent.PointerProperties[]{properties},
                new MotionEvent.PointerCoords[]{coords},0,0,1,1,0,0,InputDevice.SOURCE_TOUCHSCREEN,flags);
        try {activity.dispatchTouchEvent(event);} finally {event.recycle();}
    }
}
