package dev.makepad.octosense.a11yprobe;

import android.app.Instrumentation;
import android.app.UiAutomation;
import android.os.Bundle;
import android.view.accessibility.AccessibilityNodeInfo;

/** Focuses and activates a shell node by its spoken label through UiAutomation: the calls a screen reader makes. */
public class Probe extends Instrumentation {
    private String label="Sheets";
    private boolean click=true;
    @Override public void onCreate(Bundle args) {
        super.onCreate(args);
        if(args!=null) { if(args.getString("label")!=null) label=args.getString("label"); click=!"false".equals(args.getString("click")); }
        start();
    }
    @Override public void onStart() {
        Bundle out=new Bundle();
        try {
            UiAutomation ua=getUiAutomation();
            AccessibilityNodeInfo root=ua.getRootInActiveWindow();
            out.putString("window_package", root==null?"none":String.valueOf(root.getPackageName()));
            StringBuilder before=new StringBuilder(); collect(root,before,0); out.putString("before",before.toString());
            AccessibilityNodeInfo node=find(root,label);
            out.putBoolean("found",node!=null);
            if(node!=null) {
                out.putString("class",String.valueOf(node.getClassName()));
                out.putBoolean("clickable",node.isClickable());
                out.putString("bounds",boundsOf(node));
                out.putBoolean("focus_action",node.performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS));
                Thread.sleep(400);
                AccessibilityNodeInfo again=find(ua.getRootInActiveWindow(),label);
                out.putBoolean("is_focused",again!=null&&again.isAccessibilityFocused());
                if(click) {
                    out.putBoolean("click_action",again!=null&&again.performAction(AccessibilityNodeInfo.ACTION_CLICK));
                    Thread.sleep(1800);
                    StringBuilder after=new StringBuilder(); collect(ua.getRootInActiveWindow(),after,0); out.putString("after",after.toString());
                }
            }
        } catch(Throwable t) { out.putString("error",t.toString()); }
        finish(0,out);
    }
    private static String boundsOf(AccessibilityNodeInfo n) { android.graphics.Rect r=new android.graphics.Rect(); n.getBoundsInScreen(r); return r.toShortString(); }
    private static AccessibilityNodeInfo find(AccessibilityNodeInfo n,String label) {
        if(n==null) return null;
        if(n.getContentDescription()!=null && label.contentEquals(n.getContentDescription())) return n;
        for(int i=0;i<n.getChildCount();i++) { AccessibilityNodeInfo f=find(n.getChild(i),label); if(f!=null) return f; }
        return null;
    }
    private static void collect(AccessibilityNodeInfo n,StringBuilder sb,int depth) {
        if(n==null||depth>5) return;
        CharSequence d=n.getContentDescription();
        if(d!=null&&d.length()>0&&sb.length()<600) sb.append(d).append(" | ");
        for(int i=0;i<n.getChildCount();i++) collect(n.getChild(i),sb,depth+1);
    }
}
