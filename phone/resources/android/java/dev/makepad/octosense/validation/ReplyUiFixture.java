package dev.makepad.octosense.validation;

import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.EditText;
import dev.makepad.android.MakepadActivity;
import dev.makepad.octosense.NativeReplyComposer;
import java.util.Collections;
import org.json.JSONObject;

/** Only instrumentation creates this disposable editor fixture. It never
 * sends a notification, contacts a recipient or obtains notification access. */
public final class ReplyUiFixture {
    private static ReplyUiFixture active;
    private final NativeReplyComposer composer;
    private final MakepadActivity activity;
    private String lastToken="",lastText="";
    private int submissions;
    private boolean busy;
    private ReplyUiFixture(MakepadActivity activity) {
        this.activity=activity;
        composer=new NativeReplyComposer(activity,(token,handle,text) -> {
            if(busy) return false;
            lastToken=token;lastText=text;submissions++;return true;
        },visible -> {});
    }
    public static void attach(MakepadActivity activity) {detach();active=new ReplyUiFixture(activity);}
    public static void detach() {if(active!=null) {active.composer.close();active=null;}}
    public static JSONObject state() throws Exception {
        if(active==null) return new JSONObject().put("available",false);
        return active.composer.validationState().put("available",true).put("submissions",active.submissions).put("fixture_text",active.lastText);
    }
    public static void command(android.net.Uri route) {
        ReplyUiFixture fixture=active;if(fixture==null) throw new IllegalStateException("Reply fixture is not running");
        switch(route.getPath()) {
            case "/reply/open": fixture.busy=false;fixture.composer.open("fixture-reply-handle","Reply validation","Reply");break;
            case "/reply/text": {
                if(!(fixture.activity.getCurrentFocus() instanceof EditText)) throw new IllegalStateException("Native editor is not focused");
                EditText editor=(EditText)fixture.activity.getCurrentFocus();
                InputConnection input=editor.onCreateInputConnection(new EditorInfo());
                if(input==null||!input.commitText(route.getQueryParameter("value"),1)) throw new IllegalStateException("Input connection rejected text");break;
            }
            case "/reply/clear": {
                if(!(fixture.activity.getCurrentFocus() instanceof EditText)) throw new IllegalStateException("Native editor is not focused");
                EditText editor=(EditText)fixture.activity.getCurrentFocus();editor.selectAll();
                InputConnection input=editor.onCreateInputConnection(new EditorInfo());
                if(input==null||!input.commitText("",1)) throw new IllegalStateException("Input connection rejected deletion");break;
            }
            case "/reply/result": fixture.composer.complete(fixture.lastToken,Integer.parseInt(route.getQueryParameter("status")));break;
            case "/reply/invalidate": fixture.composer.updateTargets(Collections.emptySet());break;
            case "/reply/disconnect": fixture.composer.disconnected();break;
            case "/reply/busy": fixture.busy=true;break;
            case "/reply/close": fixture.composer.close();break;
            default: throw new IllegalArgumentException("Unknown reply fixture route");
        }
    }
}
