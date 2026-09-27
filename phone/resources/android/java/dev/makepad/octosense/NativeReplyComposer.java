package dev.makepad.octosense;

import android.graphics.Insets;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.text.Editable;
import android.text.InputFilter;
import android.text.InputType;
import android.text.TextWatcher;
import android.view.Gravity;
import android.view.View;
import android.view.WindowInsets;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputMethodManager;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import dev.makepad.android.MakepadActivity;
import dev.makepad.octosense.contracts.Protocol;
import java.util.Set;
import java.util.UUID;
import java.util.function.Consumer;
import org.json.JSONObject;

/** Native text/IME ownership. Drafts stay in memory and are never replayed.
 * The immutable submission is handed to Home's bounded worker; only Android's
 * main thread owns the views. Completion means PendingIntent dispatch, not
 * delivery to the conversation's recipient.
 */
public final class NativeReplyComposer {
    public interface Sender {boolean submit(String token,String handle,String text);}
    private static final int LIMIT=2000;
    private final MakepadActivity activity;
    private final Sender sender;
    private final Consumer<Boolean> visibility;
    private final Handler main=new Handler(Looper.getMainLooper());
    private LinearLayout panel;
    private EditText input;
    private TextView status;
    private Button send;
    private String token="",handle="";
    private boolean terminal;
    private volatile Submission pending;
    private static final class Submission {
        final String token,handle,text;
        Submission(String token,String handle,String text) {this.token=token;this.handle=handle;this.text=text;}
    }
    public NativeReplyComposer(MakepadActivity activity,Sender sender,Consumer<Boolean> visibility) {
        this.activity=activity;this.sender=sender;this.visibility=visibility;
    }
    public void open(String handle,String title,String label) {
        close();
        if(activity.isFinishing()||activity.isDestroyed()) return;
        this.handle=handle;token=UUID.randomUUID().toString();terminal=false;
        panel=new LinearLayout(activity);panel.setOrientation(LinearLayout.VERTICAL);
        panel.setBackgroundColor(0xff19171f);panel.setClickable(true);panel.setFocusable(true);
        int padding=dp(20);panel.setPadding(padding,padding,padding,padding);
        panel.setOnApplyWindowInsetsListener((view,insets) -> {
            if(Build.VERSION.SDK_INT>=30) {
                Insets bars=insets.getInsets(WindowInsets.Type.systemBars()|WindowInsets.Type.displayCutout()|WindowInsets.Type.ime());
                view.setPadding(padding+bars.left,padding+bars.top,padding+bars.right,padding+bars.bottom);
            }
            return insets;
        });
        ScrollView scroll=new ScrollView(activity);
        LinearLayout contents=new LinearLayout(activity);contents.setOrientation(LinearLayout.VERTICAL);
        TextView heading=text(title.isEmpty()?"Notification reply":title,21);contents.addView(heading);
        input=new EditText(activity);input.setTextColor(0xfff4f1fa);input.setHintTextColor(0xffbdb6c9);
        input.setHint(label.isEmpty()?"Reply":label);input.setContentDescription("Notification reply");
        input.setInputType(InputType.TYPE_CLASS_TEXT|InputType.TYPE_TEXT_FLAG_CAP_SENTENCES|InputType.TYPE_TEXT_FLAG_MULTI_LINE);
        input.setImeOptions(EditorInfo.IME_ACTION_SEND|EditorInfo.IME_FLAG_NO_EXTRACT_UI|EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING);
        input.setFilters(new InputFilter[]{new InputFilter.LengthFilter(LIMIT)});
        input.setMinLines(3);input.setMaxLines(6);input.setSaveEnabled(false);
        input.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        contents.addView(input,new LinearLayout.LayoutParams(-1,-2));
        status=text("",14);status.setAccessibilityLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);contents.addView(status);
        LinearLayout buttons=new LinearLayout(activity);buttons.setGravity(Gravity.END);
        Button cancel=new Button(activity);cancel.setText("Close");cancel.setOnClickListener(v -> close());buttons.addView(cancel);
        send=new Button(activity);send.setText("Send");send.setEnabled(false);send.setOnClickListener(v -> submit());buttons.addView(send);
        contents.addView(buttons);scroll.addView(contents);panel.addView(scroll,new LinearLayout.LayoutParams(-1,-1));
        input.addTextChangedListener(new TextWatcher() {
            public void beforeTextChanged(CharSequence s,int start,int count,int after) {}
            public void onTextChanged(CharSequence s,int start,int before,int count) {updateSend();}
            public void afterTextChanged(Editable value) {}
        });
        input.setOnEditorActionListener((view,action,event) -> {if(action==EditorInfo.IME_ACTION_SEND) {submit();return true;}return false;});
        activity.getApplicationOverlay().addView(panel,new FrameLayout.LayoutParams(-1,-1));
        visibility.accept(true);panel.requestApplyInsets();input.requestFocus();
        EditText editor=input;
        main.post(() -> {if(input==editor) activity.getSystemService(InputMethodManager.class).showSoftInput(editor,InputMethodManager.SHOW_IMPLICIT);});
    }
    private TextView text(String value,int size) {
        TextView view=new TextView(activity);view.setText(value);view.setTextSize(size);view.setTextColor(0xfff4f1fa);return view;
    }
    private int dp(int value) {return Math.round(value*activity.getResources().getDisplayMetrics().density);}
    private void updateSend() {if(send!=null) send.setEnabled(!terminal&&pending==null&&!input.getText().toString().trim().isEmpty());}
    private void submit() {
        if(panel==null||terminal||pending!=null) return;
        String text=input.getText().toString();if(text.trim().isEmpty()||text.length()>LIMIT) return;
        Submission request=new Submission(token,handle,text);pending=request;input.setEnabled(false);updateSend();status.setText("Sending…");
        activity.getSystemService(InputMethodManager.class).hideSoftInputFromWindow(input.getWindowToken(),0);
        if(!sender.submit(request.token,request.handle,request.text)) {
            pending=null;input.setEnabled(true);status.setText("Home is busy. Try again.");updateSend();return;
        }
        main.postDelayed(() -> {if(pending==request) complete(request.token,Protocol.UNCERTAIN);},15_000);
    }
    /** Worker check: a delayed JNI event cannot send a closed or replaced draft. */
    public boolean matchesSubmission(String token,String handle,String text) {
        Submission current=pending;
        return current!=null&&current.token.equals(token)&&current.handle.equals(handle)&&current.text.equals(text);
    }
    public void complete(String token,int result) {
        if(panel==null||!this.token.equals(token)) return;
        if(result==Protocol.ACCEPTED) return;
        pending=null;terminal=true;input.setEnabled(false);updateSend();
        if(result==Protocol.COMPLETED) {input.setText("");status.setText("Reply sent to the app.");}
        else if(result==Protocol.UNCERTAIN||result==Protocol.TIMEOUT||result==Protocol.DISCONNECTED)
            status.setText("Send status unavailable. Check the conversation before sending again.");
        else if(result==Protocol.EXPIRED_HANDLE) {input.setText("");status.setText("This notification changed. Close and reopen Reply.");}
        else status.setText("Reply was not sent. Close and reopen Reply to try again.");
    }
    public void updateTargets(Set<String> handles) {
        if(panel!=null&&!terminal&&!handles.contains(handle)&&pending==null) complete(token,Protocol.EXPIRED_HANDLE);
    }
    public void disconnected() {
        if(panel!=null&&!terminal) complete(token,pending==null?Protocol.EXPIRED_HANDLE:Protocol.UNCERTAIN);
    }
    public boolean close() {
        main.removeCallbacksAndMessages(null);pending=null;token="";handle="";
        if(panel==null) return false;
        activity.getSystemService(InputMethodManager.class).hideSoftInputFromWindow(input.getWindowToken(),0);
        input.setText("");activity.getApplicationOverlay().removeView(panel);
        panel=null;input=null;send=null;status=null;visibility.accept(false);return true;
    }
    public JSONObject validationState() throws org.json.JSONException {
        JSONObject value=new JSONObject().put("visible",panel!=null).put("pending",pending!=null).put("terminal",terminal);
        if(panel!=null) {
            int[] position=new int[2];send.getLocationInWindow(position);
            value.put("length",input.length()).put("send_enabled",send.isEnabled()).put("status",status.getText())
                    .put("send_x",position[0]+send.getWidth()/2).put("send_y",position[1]+send.getHeight()/2)
                    .put("editor_focused",input.hasFocus());
            if(Build.VERSION.SDK_INT>=30) {
                WindowInsets insets=panel.getRootWindowInsets();
                value.put("ime_visible",insets!=null&&insets.isVisible(WindowInsets.Type.ime()));
            }
        }
        return value;
    }
}
