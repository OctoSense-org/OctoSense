package dev.makepad.octosense.keyboardsfixture;

import android.app.Activity;
import android.os.Bundle;
import android.text.Editable;
import android.text.TextWatcher;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;

/** A native consumer that records its own text; no IME/default/permission setters. */
public final class KeyboardEditorActivity extends Activity {
    private EditText editor;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setPadding(24, 48, 24, 24);
        TextView title = new TextView(this);
        title.setText("Synthetic keyboard editor");
        title.setTextSize(24);
        panel.addView(title);
        editor = new EditText(this);
        editor.setHint("Test keyboard text");
        editor.setSingleLine(true);
        editor.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        editor.setImeOptions(android.view.inputmethod.EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING);
        editor.addTextChangedListener(new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence value, int start, int count, int after) {}
            @Override public void onTextChanged(CharSequence value, int start, int before, int count) { record(); }
            @Override public void afterTextChanged(Editable text) {}
        });
        panel.addView(editor);
        setContentView(panel);
        record();
    }
    @Override public void onResume() {
        super.onResume();
        editor.post(() -> {
            editor.requestFocus();
            getSystemService(InputMethodManager.class).showSoftInput(editor, InputMethodManager.SHOW_IMPLICIT);
            record();
        });
    }
    @Override public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (editor != null) editor.post(this::record);
    }
    private void record() {
        getSharedPreferences("observation", MODE_PRIVATE).edit()
                .putString("text", editor.getText().toString())
                .putBoolean("editor_focused", editor.hasFocus())
                .putBoolean("window_focused", hasWindowFocus())
                .putBoolean("visible", editor.isShown()).commit();
    }
}
