package dev.makepad.octosense.studio;

import android.content.Intent;
import android.view.MotionEvent;
import dev.makepad.android.MakepadActivity;

/**
 * The pinned activity looks up <application package>.MakepadAppExtension.
 * Keep the isolated Studio test APK on the same integration path as Home.
 */
public class MakepadAppExtension implements MakepadActivity.ApplicationExtension {
    private final dev.makepad.octosense.MakepadAppExtension delegate;

    public MakepadAppExtension(MakepadActivity activity) {
        delegate = new dev.makepad.octosense.MakepadAppExtension(activity);
    }

    @Override public void command(String channel, String payload) {
        delegate.command(channel, payload);
    }

    @Override public void onResume() {
        delegate.onResume();
    }

    @Override public void onPause() {
        delegate.onPause();
    }

    @Override public void onIntent(Intent intent) {
        delegate.onIntent(intent);
    }

    @Override public void onDestroy() {
        delegate.onDestroy();
    }

    @Override public boolean onActivityResult(int requestCode, int resultCode, Intent data) {
        return delegate.onActivityResult(requestCode, resultCode, data);
    }

    @Override public boolean onBackPressed() {
        return delegate.onBackPressed();
    }

    @Override public boolean usesSystemBackCallback() {
        return delegate.usesSystemBackCallback();
    }

    @Override public boolean filterTouchEvent(MotionEvent event) {
        return delegate.filterTouchEvent(event);
    }
}
