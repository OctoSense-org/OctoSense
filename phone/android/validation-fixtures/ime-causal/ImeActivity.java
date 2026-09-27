package dev.makepad.android;
import android.app.Activity;
import android.os.Bundle;
import android.text.Selection;
import android.util.Log;
import android.view.KeyEvent;
import android.view.inputmethod.ExtractedTextRequest;
import java.util.Arrays;
public class ImeActivity extends Activity {
    void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    void kinds(Integer... expected){check(MakepadNative.operations.equals(Arrays.asList(expected)),"operation ordering");MakepadNative.operations.clear();}
    @Override public void onCreate(Bundle state){super.onCreate(state);try{
        // Android can request multiple connections before the first editor.
        // The IME is permitted to retain the first returned handle.
        MakepadSurface cold=new MakepadSurface(this);cold.mEditorSession=0;
        MakepadInputConnection first=new MakepadInputConnection(cold,true);cold.mInputConnection=first;
        MakepadInputConnection second=new MakepadInputConnection(cold,true);cold.mInputConnection=second;
        MakepadInputConnection unused=new MakepadInputConnection(cold,true);
        MakepadInputConnection closed=new MakepadInputConnection(cold,true);closed.closeConnection();
        cold.updateImeTextState(0,1,true,"",0,0,-1,-1);
        check(first.commitText("first",1),"earlier cold connection owns first active editor");kinds(1);
        check(cold.mEditable.toString().equals("first"),"first cold editor accepts complete text");
        check(second.commitText("!",1),"same editor duplicate connection");kinds(1);
        check(!closed.commitText("bad",1)&&closed.getEditable().length()==0,"closed bootstrap remains retired");kinds();
        cold.updateImeTextState(MakepadNative.sequence,2,false,"",0,0,-1,-1);
        cold.updateImeTextState(MakepadNative.sequence,3,true,"next",4,4,-1,-1);
        check(!first.commitText("bad",1)&&!second.setComposingText("bad",1),"old connections cannot edit next editor");
        check(first.getEditable().length()==0&&second.getTextBeforeCursor(100,0).length()==0,"old connections cannot read next editor");
        check(!unused.commitText("bad",1)&&unused.getEditable().length()==0,"unused bootstrap cannot adopt a later editor");
        check(!first.performEditorAction(android.view.inputmethod.EditorInfo.IME_ACTION_DONE),"old connection cannot submit next editor");kinds();
        cold.mInputConnection=new MakepadInputConnection(cold,true);
        check(cold.mInputConnection.commitText("!",1)&&cold.mEditable.toString().equals("next!"),"new connection owns next editor");kinds(1);
        // Even the last-created bootstrap handle cannot skip a retired first
        // editor and adopt a later session on the first delivered reply.
        MakepadSurface skipped=new MakepadSurface(this);skipped.mEditorSession=0;
        MakepadInputConnection skippedFirst=new MakepadInputConnection(skipped,true);skipped.mInputConnection=skippedFirst;
        skipped.updateImeTextState(MakepadNative.sequence,3,true,"later",5,5,-1,-1);
        check(!skippedFirst.commitText("bad",1)&&skippedFirst.getEditable().length()==0,"bootstrap reply cannot skip first editor");kinds();
        MakepadSurface inactive=new MakepadSurface(this);inactive.mEditorSession=0;
        MakepadInputConnection pending=new MakepadInputConnection(inactive,true);inactive.mInputConnection=pending;
        inactive.updateImeTextState(MakepadNative.sequence,0,false,"",0,0,-1,-1);
        inactive.updateImeTextState(MakepadNative.sequence,1,true,"",0,0,-1,-1);
        check(pending.commitText("first",1),"initial inactive reply preserves bootstrap");kinds(1);
        MakepadInputConnection duplicate=new MakepadInputConnection(inactive,true);inactive.mInputConnection=duplicate;
        inactive.updateImeTextState(MakepadNative.sequence,1,false,"",0,0,-1,-1);
        check(!pending.commitText("bad",1)&&pending.getEditable().length()==0,"same-session inactive retires earlier handle");
        check(!duplicate.performEditorAction(android.view.inputmethod.EditorInfo.IME_ACTION_DONE),"same-session inactive retires latest handle");kinds();
        inactive.updateImeTextState(MakepadNative.sequence,1,true,"resumed",7,7,-1,-1);
        check(!pending.commitText("bad",1),"inactive handle cannot revive on same-session reply");kinds();
        MakepadInputConnection resumed=new MakepadInputConnection(inactive,true);inactive.mInputConnection=resumed;
        check(resumed.commitText("!",1),"fresh resumed connection works");kinds(1);
        MakepadInputConnection pauseDuplicate=new MakepadInputConnection(inactive,true);inactive.mInputConnection=pauseDuplicate;
        inactive.retireInputConnection(); // Same direct call as Activity.onPause.
        check(!resumed.commitText("bad",1)&&resumed.getEditable().length()==0&& !pauseDuplicate.performEditorAction(android.view.inputmethod.EditorInfo.IME_ACTION_DONE),"pause retires all handles before a native reply");kinds();
        MakepadSurface s=new MakepadSurface(this);setContentView(s);
        MakepadInputConnection ic=new MakepadInputConnection(s,true);s.mInputConnection=ic;
        check(ic.setComposingText("n",1),"compose");check(ic.setComposingText("ni",1),"compose replace");check(ic.commitText("你",1),"commit");
        check(s.mEditable.toString().equals("你"),"optimistic composition");kinds(2,2,1);
        check(ic.replaceText(0,1,"😀ab",1,null),"explicit replacement");kinds(8);
        check(s.mEditable.toString().equals("😀ab"),"replacement mirror");
        ic.setSelection(0,4);kinds(6);
        check(ic.sendKeyEvent(new KeyEvent(KeyEvent.ACTION_DOWN,KeyEvent.KEYCODE_DEL)),"selected delete");kinds(11);
        check(s.mEditable.length()==0,"selected delete mirror");
        ic.commitText("😀",1);kinds(1);ic.deleteSurroundingTextInCodePoints(1,0);kinds(5);
        check(s.mEditable.length()==0,"surrogate delete");
        ic.beginBatchEdit();ic.setComposingText("中",1);ic.setComposingText("中文",1);ic.finishComposingText();ic.endBatchEdit();kinds(2,2,3);
        check(s.mEditable.toString().equals("中文"),"batch composition");
        // Explicit full composing payloads have Android semantics. An editor
        // must not guess whether a caller intended an old prefix or a new word.
        android.widget.EditText nativeEditor=new android.widget.EditText(this);
        nativeEditor.setText("blue");
        android.view.inputmethod.InputConnection nativeIc=nativeEditor.onCreateInputConnection(new android.view.inputmethod.EditorInfo());
        nativeIc.setComposingRegion(0,4);nativeEditor.setText("");nativeIc.setComposingText("blueh",1);
        check(nativeEditor.getText().toString().equals("blueh"),"native composing payload after clear");
        s.updateImeTextState(MakepadNative.sequence,1,true,"",0,0,-1,-1);
        ic.setComposingText("blueh",1);kinds(2);
        check(s.mEditable.toString().equals(nativeEditor.getText().toString()),"Makepad preserves native composing payload semantics");
        check(ic.performEditorAction(android.view.inputmethod.EditorInfo.IME_ACTION_DONE),"current editor action");kinds(13);
        // An authoritative new inactive session retires the old IC and its
        // text even when old-session optimistic edits have not been echoed.
        s.updateImeTextState(MakepadNative.sequence,1,true,"old",3,3,-1,-1);
        ic.commitText("x",1);kinds(1);
        s.updateImeTextState(MakepadNative.sequence-1,2,false,"",0,0,-1,-1);
        check(s.mEditable.length()==0,"inactive new session must clear old editor");
        check(ic.getEditable().length()==0,"inactive session retires old query");
        s.updateImeTextState(MakepadNative.sequence,3,true,"newx",4,4,-1,-1);
        check(s.mEditable.toString().equals("newx"),"new editor acknowledgement");
        s.updateImeTextState(MakepadNative.sequence-1,2,false,"",0,0,-1,-1);
        check(s.mEditable.toString().equals("newx"),"stale lower session cannot clear current editor");
        // A new editor owns the same Surface; the obsolete connection cannot
        // read its synthetic secret through any Android query family.
        s.mEditorSession=4;s.mInputConnection=new MakepadInputConnection(s,true);s.mEditable.replace(0,s.mEditable.length(),"other-field-secret");Selection.setSelection(s.mEditable,s.mEditable.length());
        check(ic.getEditable().length()==0,"retired Editable");
        check(ic.getTextBeforeCursor(100,0).length()==0,"retired before");
        check(ic.getTextAfterCursor(100,0).length()==0,"retired after");
        check(ic.getExtractedText(new ExtractedTextRequest(),0).text.length()==0,"retired extract");
        check(ic.getSurroundingText(100,100,0).getText().length()==0,"retired surrounding");
        check(ic.takeSnapshot().getSurroundingText().getText().length()==0,"retired snapshot");
        check(!ic.performEditorAction(android.view.inputmethod.EditorInfo.IME_ACTION_DONE)&&!ic.commitText("bad",1)&&!ic.setComposingText("bad",1)&&!ic.setSelection(0,1)&&!ic.sendKeyEvent(new KeyEvent(KeyEvent.ACTION_DOWN,KeyEvent.KEYCODE_DEL)),"retired edits");kinds();
        check(s.mEditable.toString().equals("other-field-secret"),"new editor preserved");
        s.mInputConnection.closeConnection();check(!s.mInputConnection.commitText("bad",1)&&s.mInputConnection.getEditable().length()==0,"closed edits and reads");kinds();
        Log.i("OctoSenseImeConnection","PASS composition/atomic replacement/deletion/batching/retired-query-isolation/closed-connection");
    }catch(Throwable e){Log.e("OctoSenseImeConnection","FAIL "+e.getClass().getSimpleName()+" "+e.getMessage());}}
}
