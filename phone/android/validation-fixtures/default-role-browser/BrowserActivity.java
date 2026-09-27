package dev.makepad.octosense.rolebrowserfixture;

import android.app.Activity;
import android.os.Bundle;
import android.widget.TextView;

/** A qualifying browser target that never loads a URL or sends network traffic. */
public final class BrowserActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        TextView text=new TextView(this);
        text.setText("Local browser validation. No page was loaded.");
        text.setPadding(24,24,24,24);
        setContentView(text);
    }
}
