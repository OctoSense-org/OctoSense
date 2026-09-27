package dev.makepad.octosense.permissionfixture;

import android.app.Activity;
import android.os.Bundle;
import android.widget.TextView;

/** No permission requests, sensor access, file access or network use. */
public final class PermissionFixtureActivity extends Activity {
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);TextView text=new TextView(this);
        text.setText("OctoSense disposable permission test app");setContentView(text);
    }
}
