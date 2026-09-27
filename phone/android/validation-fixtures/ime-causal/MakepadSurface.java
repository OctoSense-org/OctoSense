package dev.makepad.android;
import android.content.Context;
import android.text.Editable;
import android.text.SpannableStringBuilder;
import android.view.View;
import android.text.Selection;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.InputMethodManager;
class MakepadSurface extends View {
    static final int INPUT_MODE_TEXT=0,INPUT_MODE_ASCII=1,INPUT_MODE_URL=2,INPUT_MODE_NUMERIC=3,INPUT_MODE_TEL=4,INPUT_MODE_EMAIL=5,INPUT_MODE_DECIMAL=6;
    final Editable mEditable=new SpannableStringBuilder();
    long mEditorSession=1,mOptimisticEditSequence,mInputConnectionEpoch;
    boolean mImeTextActive;
    boolean applyingEditorState;
    MakepadInputConnection mInputConnection;
    MakepadSurface(Context c) {super(c);}
    long editorSession(){return mEditorSession;}
    long connectionEpoch(){return mInputConnectionEpoch;}
    void acceptedEditorOperation(long sequence){mOptimisticEditSequence=sequence;}
    Editable getEditable(){return mEditable;}
    int getInputMode(){return INPUT_MODE_TEXT;}
    boolean isMultiline(){return true;}
    /* REAL_SURFACE_RETIRE */
    /* REAL_SURFACE_UPDATE */
}
