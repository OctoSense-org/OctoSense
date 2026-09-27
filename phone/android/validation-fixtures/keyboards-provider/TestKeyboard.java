package dev.makepad.octosense.keyboardprovider;

import android.inputmethodservice.InputMethodService;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

/** A synthetic IME: never reads surrounding text, and writes only to its test editor. */
public final class TestKeyboard extends InputMethodService {
    public static final String EDITOR_PACKAGE = "dev.makepad.octosense.keyboardsfixture";
    public static final String SAMPLE = "OctoSense keyboard check 中文";
    private boolean testEditor;

    @Override public void onStartInput(EditorInfo info, boolean restarting) {
        super.onStartInput(info, restarting);
        testEditor = info != null && EDITOR_PACKAGE.equals(info.packageName);
        android.content.SharedPreferences prefs = getSharedPreferences("observation", MODE_PRIVATE);
        android.content.SharedPreferences.Editor edit = prefs.edit().putBoolean("test_editor_active", testEditor);
        if (testEditor) edit.putInt("test_input_starts", prefs.getInt("test_input_starts", 0) + 1);
        edit.commit();
    }

    @Override public void onFinishInput() {
        testEditor = false;
        getSharedPreferences("observation", MODE_PRIVATE).edit().putBoolean("test_editor_active", false).commit();
        super.onFinishInput();
    }

    @Override public View onCreateInputView() {
        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setPadding(16, 16, 16, 24);
        TextView title = new TextView(this);
        title.setText(getApplicationInfo().loadLabel(getPackageManager()));
        title.setTextSize(20);
        panel.addView(title);
        Button send = new Button(this);
        send.setText("Send test text");
        send.setOnClickListener(view -> {
            EditorInfo info = getCurrentInputEditorInfo();
            if (!testEditor || info == null || !EDITOR_PACKAGE.equals(info.packageName)) return;
            InputConnection connection = getCurrentInputConnection();
            if (connection == null) return;
            boolean accepted = connection.commitText(SAMPLE, 1);
            android.content.SharedPreferences prefs = getSharedPreferences("observation", MODE_PRIVATE);
            prefs.edit().putBoolean("last_commit_accepted", accepted)
                    .putInt("commits", prefs.getInt("commits", 0) + 1).commit();
        });
        panel.addView(send);
        return panel;
    }
}
