package dev.makepad.octosense;

import android.content.Context;
import android.graphics.Rect;
import android.os.Bundle;
import android.view.MotionEvent;
import android.view.View;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityManager;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import java.util.ArrayList;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * The shell's tappable regions as virtual accessibility nodes, so TalkBack
 * and UI automation can read and activate what the Rust surface draws. The
 * view covers the surface, consumes no touch and, under explore-by-touch,
 * maps hover to the node under the finger. The shell publishes the nodes
 * (`a11y.layout`) whenever they change; an activation goes back as
 * `a11y.activate` with the node's index.
 */
final class ShellAccessibility extends View {
    interface Activate { void activate(int index); }
    private static final class Node { int index; String label; final Rect bounds=new Rect(); }
    private final ArrayList<Node> nodes=new ArrayList<>();
    private final Activate activate;
    private int focused=-1;
    private int hovered=-1;
    private final AccessibilityNodeProvider provider=new AccessibilityNodeProvider() {
        @Override public AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
            if(id==View.NO_ID || id==AccessibilityNodeProvider.HOST_VIEW_ID) {
                AccessibilityNodeInfo host=AccessibilityNodeInfo.obtain(ShellAccessibility.this);
                ShellAccessibility.this.onInitializeAccessibilityNodeInfo(host);
                for(Node node:nodes) host.addChild(ShellAccessibility.this,node.index);
                return host;
            }
            Node node=find(id);
            if(node==null) return null;
            AccessibilityNodeInfo info=AccessibilityNodeInfo.obtain();
            info.setPackageName(getContext().getPackageName());
            info.setClassName("android.widget.Button");
            info.setSource(ShellAccessibility.this,id);
            info.setParent(ShellAccessibility.this);
            info.setContentDescription(node.label);
            info.setBoundsInParent(node.bounds);
            int[] location=new int[2];
            getLocationOnScreen(location);
            Rect screen=new Rect(node.bounds);
            screen.offset(location[0],location[1]);
            info.setBoundsInScreen(screen);
            info.setVisibleToUser(true);
            info.setEnabled(true);
            info.setClickable(true);
            info.setFocusable(true);
            info.setAccessibilityFocused(focused==id);
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK);
            info.addAction(focused==id?AccessibilityNodeInfo.AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS
                    :AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
            return info;
        }
        @Override public boolean performAction(int id,int action,Bundle arguments) {
            if(id==View.NO_ID || id==AccessibilityNodeProvider.HOST_VIEW_ID) return ShellAccessibility.super.performAccessibilityAction(action,arguments);
            Node node=find(id);
            if(node==null) return false;
            switch(action) {
                case AccessibilityNodeInfo.ACTION_CLICK:
                    activate.activate(node.index);
                    send(id,AccessibilityEvent.TYPE_VIEW_CLICKED);
                    return true;
                case AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS:
                    focused=id;
                    send(id,AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED);
                    return true;
                case AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS:
                    if(focused==id) focused=-1;
                    send(id,AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED);
                    return true;
                default: return false;
            }
        }
    };
    ShellAccessibility(Context context,Activate activate) {
        super(context);
        this.activate=activate;
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
        setClickable(false);
        setFocusable(false);
        setContentDescription("OctoSense");
    }
    @Override public AccessibilityNodeProvider getAccessibilityNodeProvider() { return provider; }
    /** Touches are the surface's. */
    @Override public boolean onTouchEvent(MotionEvent event) { return false; }
    /** Explore by touch: the node under the finger gets hover enter/exit, which TalkBack reads and focuses. */
    @Override public boolean dispatchHoverEvent(MotionEvent event) {
        AccessibilityManager manager=(AccessibilityManager)getContext().getSystemService(Context.ACCESSIBILITY_SERVICE);
        if(manager==null || !manager.isEnabled() || !manager.isTouchExplorationEnabled()) return false;
        int under=-1;
        if(event.getAction()!=MotionEvent.ACTION_HOVER_EXIT) {
            int x=(int)event.getX(),y=(int)event.getY();
            for(Node node:nodes) if(node.bounds.contains(x,y)) {under=node.index;break;}
        }
        if(under!=hovered) {
            if(hovered!=-1) send(hovered,AccessibilityEvent.TYPE_VIEW_HOVER_EXIT);
            hovered=under;
            if(hovered!=-1) send(hovered,AccessibilityEvent.TYPE_VIEW_HOVER_ENTER);
        }
        return true;
    }
    /** The shell's nodes for this frame: `{"nodes":[{"i":index,"l":label,"b":[x,y,w,h]}]}` in window pixels. */
    void layout(String payload) {
        ArrayList<Node> next=new ArrayList<>();
        try {
            JSONArray items=new JSONObject(payload).getJSONArray("nodes");
            for(int index=0;index<items.length() && index<256;index++) {
                JSONObject item=items.getJSONObject(index);
                JSONArray bounds=item.getJSONArray("b");
                Node node=new Node();
                node.index=item.getInt("i");
                node.label=item.optString("l","");
                node.bounds.set(bounds.getInt(0),bounds.getInt(1),bounds.getInt(0)+bounds.getInt(2),bounds.getInt(1)+bounds.getInt(3));
                if(node.index<0 || node.label.isEmpty()) continue;
                next.add(node);
            }
        } catch(JSONException e) { return; }
        nodes.clear();
        nodes.addAll(next);
        if(find(focused)==null) focused=-1;
        if(find(hovered)==null) hovered=-1;
        sendAccessibilityEvent(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
    }
    private Node find(int id) {
        for(Node node:nodes) if(node.index==id) return node;
        return null;
    }
    private void send(int id,int type) {
        AccessibilityManager manager=(AccessibilityManager)getContext().getSystemService(Context.ACCESSIBILITY_SERVICE);
        if(manager==null || !manager.isEnabled() || getParent()==null) return;
        Node node=find(id);
        AccessibilityEvent event=AccessibilityEvent.obtain(type);
        event.setPackageName(getContext().getPackageName());
        event.setClassName("android.widget.Button");
        event.setSource(this,id);
        if(node!=null) event.setContentDescription(node.label);
        getParent().requestSendAccessibilityEvent(this,event);
    }
}
