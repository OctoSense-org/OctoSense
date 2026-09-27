package dev.makepad.android;
import java.util.ArrayList;
class MakepadNative {
    static long sequence;
    static final ArrayList<Integer> operations=new ArrayList<>();
    static long beginInput(){return ++sequence;}
    static void onImeOperation(long sequence,long session,boolean hardware,int kind,String text,int start,int end,int cursor){operations.add(kind);}
    static void onImeEditorAction(int action){}
    static void surfaceOnKeyDown(int code,int meta,boolean repeat){}
    static void surfaceOnKeyUp(int code,int meta){}
}
