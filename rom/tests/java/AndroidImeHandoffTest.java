// Method markers are filled from the pinned, patched runtime by the Python test.
public class AndroidImeHandoffTest {
    static final StringBuilder text = new StringBuilder();
    static int down, up, lastMeta, repeats, volume, deletes;
    static boolean deferred;
    static final java.util.ArrayList<Runnable> queued=new java.util.ArrayList<>();
    static Surface current;
    static void flush() { for (Runnable event : queued) event.run(); queued.clear(); }

    static class KeyEvent {
        static final int ACTION_DOWN=0, ACTION_UP=1, ACTION_MULTIPLE=2;
        static final int KEYCODE_VOLUME_UP=24, KEYCODE_VOLUME_DOWN=25,
            KEYCODE_DEL=67, KEYCODE_FORWARD_DEL=112, KEYCODE_ENTER=66,
            KEYCODE_DPAD_LEFT=21, KEYCODE_DPAD_RIGHT=22, KEYCODE_DPAD_UP=19,
            KEYCODE_DPAD_DOWN=20, KEYCODE_MOVE_HOME=122, KEYCODE_MOVE_END=123,
            KEYCODE_PAGE_UP=92, KEYCODE_PAGE_DOWN=93;
        final int action, code, unicode, meta, repeat;
        final String characters;
        KeyEvent(int action, int code, int unicode, int meta, int repeat, String characters) {
            this.action=action; this.code=code; this.unicode=unicode;
            this.meta=meta; this.repeat=repeat; this.characters=characters;
        }
        int getAction() { return action; }
        int getKeyCode() { return code; }
        int getUnicodeChar() { return unicode; }
        int getMetaState() { return meta; }
        int getRepeatCount() { return repeat; }
        String getCharacters() { return characters; }
        boolean isCtrlPressed() { return (meta & 0x1000)!=0; }
        boolean isAltPressed() { return (meta & 2)!=0; }
    }
    static class MakepadNative {
        static long sequence;
        static long beginInput() {return ++sequence;}
        static void surfaceOnKeyDown(int code, int meta, boolean repeat) {
            down++; lastMeta=meta; if (repeat) repeats++;
        }
        static void surfaceOnKeyUp(int code, int meta) { up++; lastMeta=meta; }
        static void surfaceOnCharacter(int code) { text.appendCodePoint(code); }
    }
    static class View {
        boolean onKeyUp(int key, KeyEvent event) { volume++; return false; }
    }
    static class Editable { final StringBuilder value = new StringBuilder(); }
    static class Selection {
        static int getSelectionStart(Editable ignored) { return ignored.value.length(); }
        static int getSelectionEnd(Editable ignored) { return ignored.value.length(); }
    }
    static class Surface extends View {
        final Editable editable = new Editable();
        Connection mInputConnection;
        boolean mImeTextActive;
        Editable getEditable() { return editable; }
        boolean isMultiline() { return true; }
        /* SURFACE_METHOD */
    }
    static class BaseConnection {
        final Surface mSurface;
        BaseConnection(Surface surface) { mSurface=surface; }
        boolean sendKeyEvent(KeyEvent event) {
            return mSurface.onKey(mSurface, event.getKeyCode(), event);
        }
        boolean commitText(CharSequence value, int cursor) {
            mSurface.editable.value.append(value);
            String snapshot=mSurface.editable.value.toString();
            queued.add(() -> { text.setLength(0); text.append(snapshot); });
            if (!deferred) flush();
            return true;
        }
        boolean deleteSurroundingTextInCodePoints(int before, int after) { deletes++; return true; }
    }
    static class Connection extends BaseConnection {
        boolean mHardwareEdit,closed; int mSuppressOperations;
        boolean ownsEditor() {return !closed;}
        boolean sendEdit(int kind,String text,int start,int end,int cursor) {return true;}
        Editable getEditable() {return mSurface.getEditable();}
        Connection(Surface surface) { super(surface); }
        /* CONNECTION_METHODS */
    }
    static class MakepadInputConnection extends Connection {
        MakepadInputConnection(Surface surface,boolean full) {super(surface);}
    }
    static KeyEvent key(int action, int codepoint) {
        return new KeyEvent(action, 30, codepoint, 0, 0, null);
    }
    static void check(boolean condition, String message) {
        if (!condition) throw new AssertionError(message + "; text=" + text);
    }
    static void reset() { text.setLength(0); down=up=lastMeta=repeats=volume=deletes=0;
        queued.clear(); deferred=false; if(current!=null)current.editable.value.setLength(0); }
    public static void main(String[] arguments) {
        Surface surface=new Surface(); current=surface;
        Connection connection=new Connection(surface);
        for (boolean pressViaIme : new boolean[]{false,true}) {
            for (boolean releaseViaIme : new boolean[]{false,true}) {
                reset();
                if (pressViaIme) connection.sendKeyEvent(key(0,'b'));
                else surface.onKey(surface,30,key(0,'b'));
                check(text.toString().equals("b"), "press inserts once before release");
                if (releaseViaIme) connection.sendKeyEvent(key(1,'b'));
                else surface.onKey(surface,30,key(1,'b'));
                check(text.toString().equals("b"), "handoff must neither duplicate nor lose a key");
            }
        }
        reset();
        // Two identical presses remain two characters; no time/text deduplication.
        for (int n=0;n<2;n++) {
            connection.sendKeyEvent(key(0,'o'));
            surface.onKey(surface,30,key(1,'o'));
        }
        check(text.toString().equals("oo"), "legitimate repeated characters");
        reset();
        surface.onKey(surface,30,key(0,'b'));
        surface.onKey(surface,30,new KeyEvent(0,30,'b',0,1,null));
        connection.sendKeyEvent(key(1,'b'));
        check(text.toString().equals("bb") && repeats==1, "key auto-repeat");
        reset();
        surface.onKey(surface,30,new KeyEvent(0,30,'B',1,0,null));
        surface.onKey(surface,30,new KeyEvent(1,30,'b',0,0,null));
        check(text.toString().equals("B") && down==1 && up==1, "press-time modifier character");
        reset();
        connection.sendKeyEvent(new KeyEvent(0,KeyEvent.KEYCODE_DPAD_LEFT,0,0x1000,0,null));
        surface.onKey(surface,KeyEvent.KEYCODE_DPAD_LEFT,new KeyEvent(1,KeyEvent.KEYCODE_DPAD_LEFT,0,0x1000,0,null));
        check(text.length()==0 && down==1 && up==1 && lastMeta==0x1000, "navigation and modifier delivery");
        reset();
        surface.onKey(surface,0,new KeyEvent(2,0,0,0,0,"中"));
        check(text.toString().equals("中"), "legacy ACTION_MULTIPLE fallback");
        reset();
        connection.sendKeyEvent(new KeyEvent(0,KeyEvent.KEYCODE_DEL,0,0,0,null));
        connection.sendKeyEvent(new KeyEvent(1,KeyEvent.KEYCODE_DEL,0,0,0,null));
        check(deletes==1 && text.length()==0, "IME delete remains single");
        reset();
        surface.onKey(surface,KeyEvent.KEYCODE_VOLUME_UP,new KeyEvent(1,KeyEvent.KEYCODE_VOLUME_UP,0,0,0,null));
        check(volume==1 && text.length()==0, "volume remains delegated");
        // Surface deltas and asynchronous IME full states used to corrupt each
        // other: Surface(w,f), InputConnection(i,i) must still produce wifi.
        surface.mInputConnection=connection; surface.mImeTextActive=true;
        for(boolean defer : new boolean[]{false,true}) {
            reset(); deferred=defer;
            surface.onKey(surface,30,key(0,'w'));
            connection.sendKeyEvent(key(0,'i'));
            surface.onKey(surface,30,key(0,'f'));
            connection.sendKeyEvent(key(0,'i'));
            flush();
            check(text.toString().equals("wifi") && surface.editable.value.toString().equals("wifi"),
                "mixed paths share one authoritative editable, including queued snapshots");
        }
        reset(); deferred=true;
        surface.onKey(surface,30,key(0,'b')); connection.sendKeyEvent(key(1,'b'));
        connection.sendKeyEvent(key(0,'o')); surface.onKey(surface,30,key(1,'o'));
        surface.onKey(surface,30,key(0,'o')); connection.sendKeyEvent(key(1,'o'));
        flush(); check(text.toString().equals("boo"), "both handoffs retain real repeated text");
        reset();
        surface.onKey(surface,0,new KeyEvent(2,0,'中',0,0,"中文"));
        connection.sendKeyEvent(key(0,'!'));
        check(text.toString().equals("中文!"), "legacy multi-character commit shares editable");
        reset();
        surface.onKey(surface,KeyEvent.KEYCODE_DEL,new KeyEvent(0,KeyEvent.KEYCODE_DEL,0,0,0,null));
        connection.sendKeyEvent(new KeyEvent(1,KeyEvent.KEYCODE_DEL,0,0,0,null));
        check(deletes==1 && down==0, "surface deletion must not also emit Rust delete");
        reset(); connection.closed=true;
        surface.onKey(surface,30,key(0,'r'));
        check(text.toString().equals("r") && surface.mInputConnection!=connection, "hardware input creates a provisional editor after connection retirement");
        reset(); surface.mImeTextActive=false;
        surface.onKey(surface,30,key(0,'x'));
        check(text.toString().equals("x") && surface.editable.value.length()==0,
            "no editor retains the native character route");
        System.out.println("IME routing handoffs and repeat/modifier/navigation regressions passed");
    }
}
