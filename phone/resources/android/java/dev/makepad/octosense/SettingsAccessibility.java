package dev.makepad.octosense;

import android.content.Context;
import android.graphics.Rect;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.View;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityManager;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import java.util.ArrayList;
import java.util.EnumSet;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicLong;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import dev.makepad.octosense.SettingsAccessibilityContract.Action;
import dev.makepad.octosense.SettingsAccessibilityContract.Bounds;
import dev.makepad.octosense.SettingsAccessibilityContract.Layout;
import dev.makepad.octosense.SettingsAccessibilityContract.Node;
import dev.makepad.octosense.SettingsAccessibilityContract.Role;
import dev.makepad.octosense.SettingsAccessibilityContract.Scroll;

/** Android virtual tree for the compiled Settings view. All methods run on the UI thread. */
final class SettingsAccessibility extends View {
    interface Dispatch {void send(JSONObject request);}
    interface Visibility {void changed(boolean active);}
    interface Enabled {void changed(boolean enabled);}
    private final Dispatch dispatch;
    private final Visibility visibility;
    private final Enabled enabled;
    private final AccessibilityManager manager;
    private volatile boolean resumed,closed;
    private final AccessibilityManager.AccessibilityStateChangeListener stateListener=value -> publishEnabled();
    private final Handler handler=new Handler(Looper.getMainLooper());
    private Layout current;
    private int focused=-1,hovered=-1;
    private static final AtomicLong NEXT_REQUEST=new AtomicLong(1);
    private final HashMap<Long,Pending> pending=new HashMap<>();
    private static final class Pending {
        long id,expires;
        int node;
        String token,target,text,before;
        Action action;
    }
    SettingsAccessibility(Context context,Dispatch dispatch,Visibility visibility,Enabled enabled) {
        super(context);this.dispatch=dispatch;this.visibility=visibility;this.enabled=enabled;
        manager=context.getSystemService(AccessibilityManager.class);
        setWillNotDraw(true);setClickable(false);setFocusable(false);
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS);
        setVisibility(GONE);
        if(manager!=null)manager.addAccessibilityStateChangeListener(stateListener,handler);
        publishEnabled();
    }
    private boolean windowFocused;
    boolean enabledNow() {return resumed&&windowFocused&&!closed&&manager!=null&&manager.isEnabled();}
    void setWindowFocused(boolean focused) {windowFocused=focused;publishEnabled();}
    private void publishEnabled() {boolean value=enabledNow();if(!value)clear();enabled.changed(value);}
    void onResume() {resumed=true;publishEnabled();}
    void onPause() {resumed=false;publishEnabled();}
    void destroy() {closed=true;resumed=false;if(manager!=null)manager.removeAccessibilityStateChangeListener(stateListener);publishEnabled();}
    private static Object field(JSONObject object,String name) throws JSONException {return object.get(name);}
    private static String string(JSONObject object,String name) throws JSONException {
        Object value=field(object,name);if(!(value instanceof String))throw SettingsAccessibilityContract.invalid();return (String)value;
    }
    private static boolean bool(JSONObject object,String name) throws JSONException {
        Object value=field(object,name);if(!(value instanceof Boolean))throw SettingsAccessibilityContract.invalid();return (Boolean)value;
    }
    private static long number(Object value) {
        if(!(value instanceof Integer||value instanceof Long))throw SettingsAccessibilityContract.invalid();
        return ((Number)value).longValue();
    }
    private static int integer(Object value) {
        long result=number(value);if(result<Integer.MIN_VALUE||result>Integer.MAX_VALUE)throw SettingsAccessibilityContract.invalid();return (int)result;
    }
    private static Bounds bounds(Object value) throws JSONException {
        if(!(value instanceof JSONArray))throw SettingsAccessibilityContract.invalid();JSONArray array=(JSONArray)value;
        if(array.length()!=4)throw SettingsAccessibilityContract.invalid();
        return new Bounds(integer(array.get(0)),integer(array.get(1)),integer(array.get(2)),integer(array.get(3)));
    }
    void layout(String payload) {
        if(!enabledNow()){clear();return;}
        try {
            JSONObject object=new JSONObject(payload);
            if(integer(field(object,"schema"))!=1)throw SettingsAccessibilityContract.invalid();
            if(!bool(object,"active")){clear();return;}
            Scroll scroll=null;Object rawScroll=field(object,"scroll");
            if(rawScroll!=JSONObject.NULL) {
                if(!(rawScroll instanceof JSONObject))throw SettingsAccessibilityContract.invalid();JSONObject value=(JSONObject)rawScroll;
                scroll=new Scroll(integer(field(value,"id")),string(value,"target"),bounds(field(value,"bounds")),bool(value,"forward"),bool(value,"backward"));
            }
            Object rawNodes=field(object,"nodes");if(!(rawNodes instanceof JSONArray))throw SettingsAccessibilityContract.invalid();JSONArray array=(JSONArray)rawNodes;
            if(array.length()>SettingsAccessibilityContract.MAX_NODES)throw SettingsAccessibilityContract.invalid();
            ArrayList<Node> nodes=new ArrayList<>();
            for(int i=0;i<array.length();i++) {
                Object raw=array.get(i);if(!(raw instanceof JSONObject))throw SettingsAccessibilityContract.invalid();JSONObject node=(JSONObject)raw;
                Object parent=field(node,"parent"),rawActions=field(node,"actions");
                if(!(rawActions instanceof JSONArray))throw SettingsAccessibilityContract.invalid();JSONArray actions=(JSONArray)rawActions;
                if(actions.length()>3)throw SettingsAccessibilityContract.invalid();EnumSet<Action> allowed=EnumSet.noneOf(Action.class);
                for(int n=0;n<actions.length();n++) {
                    Object action=actions.get(n);if(!(action instanceof String)||!allowed.add(Action.parse((String)action)))throw SettingsAccessibilityContract.invalid();
                }
                nodes.add(new Node(integer(field(node,"id")),string(node,"target"),parent==JSONObject.NULL?0:SettingsAccessibilityContract.id(integer(parent)),
                        Role.parse(string(node,"role")),string(node,"label"),string(node,"value"),bool(node,"enabled"),bool(node,"focused"),allowed,bounds(field(node,"bounds"))));
            }
            Layout next=new Layout(string(object,"token"),string(object,"pane"),bounds(field(object,"bounds")),scroll,nodes);
            // Rust owns the process-monotonic semantic ID registry. Reject a
            // visible ID rebound as well, before exposing the replacement.
            if(current!=null) for(Node node:next.nodes) {
                Node old=current.node(node.id);
                if(old!=null&&(!old.target.equals(node.target)||!current.token.equals(next.token)))throw SettingsAccessibilityContract.invalid();
            }
            boolean pageChanged=current==null||!current.token.equals(next.token);
            boolean paneChanged=current==null||!current.pane.equals(next.pane);
            current=next;
            if(pageChanged||next.node(focused)==null)focused=-1;
            if(pageChanged||next.node(hovered)==null)hovered=-1;
            prunePending();
            setVisibility(VISIBLE);setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);visibility.changed(true);
            if(Build.VERSION.SDK_INT>=28&&paneChanged)setAccessibilityPaneTitle(next.pane);
            // setAccessibilityPaneTitle already announces a new pane on modern
            // Android. A distinct visit with the same title still needs one.
            if(pageChanged&&(Build.VERSION.SDK_INT<28||!paneChanged))sendAccessibilityEvent(AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED);
            treeChanged();
        } catch(JSONException|IllegalArgumentException malformed) {clear();}
    }
    void clear() {
        boolean active=current!=null;current=null;focused=-1;hovered=-1;pending.clear();handler.removeCallbacks(expire);
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS);setVisibility(GONE);
        if(Build.VERSION.SDK_INT>=28)setAccessibilityPaneTitle(null);
        visibility.changed(false);
        if(active)treeChanged();
    }
    private void treeChanged() {
        // Virtual children can be added/retired without an Android View layout.
        // Pane announcements alone do not invalidate a service's subtree cache.
        AccessibilityEvent event=AccessibilityEvent.obtain(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
        event.setPackageName(getContext().getPackageName());event.setSource(this);event.setClassName("android.view.ViewGroup");
        event.setContentChangeTypes(AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE);send(event);
    }
    private final Runnable expire=() -> prunePending();
    private boolean matches(Pending request) {
        if(current==null||!current.token.equals(request.token))return false;
        if(current.scroll!=null&&current.scroll.id==request.node)return current.scroll.target.equals(request.target);
        Node node=current.node(request.node);return node!=null&&node.target.equals(request.target);
    }
    private void prunePending() {
        long now=SystemClock.uptimeMillis();Iterator<Pending> iterator=pending.values().iterator();
        while(iterator.hasNext()) {Pending request=iterator.next();if(request.expires<=now||!matches(request))iterator.remove();}
        handler.removeCallbacks(expire);if(!pending.isEmpty())handler.postDelayed(expire,5000);
    }
    private boolean request(int id,String target,Action action,String text) {
        prunePending();if(current==null||pending.size()>=32)return false;
        long requestId=NEXT_REQUEST.getAndUpdate(value -> value==Long.MAX_VALUE?value:value+1);
        if(requestId<=0||requestId==Long.MAX_VALUE)return false;
        Pending request=new Pending();request.id=requestId;request.node=id;request.target=target;request.token=current.token;
        request.action=action;request.text=text;request.expires=SystemClock.uptimeMillis()+5000;
        Node node=current.node(id);request.before=node==null?"":node.value;
        try {
            JSONObject packet=new JSONObject().put("schema",1).put("request_id",request.id).put("token",request.token)
                    .put("id",id).put("target",target).put("action",action.wire);
            if(text!=null)packet.put("text",SettingsAccessibilityContract.text(text,SettingsAccessibilityContract.MAX_TEXT,true));
            pending.put(request.id,request);handler.removeCallbacks(expire);handler.postDelayed(expire,5000);
            dispatch.send(packet);return true;
        } catch(JSONException|IllegalArgumentException invalid) {pending.remove(request.id);return false;}
    }
    void result(String payload) {
        try {
            JSONObject packet=new JSONObject(payload);if(integer(field(packet,"schema"))!=1)return;
            long id=number(field(packet,"request_id"));boolean accepted=bool(packet,"accepted");Pending request=pending.remove(id);
            if(request==null||!accepted||request.expires<=SystemClock.uptimeMillis()||!matches(request))return;
            int type=request.action==Action.SET_TEXT?AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED
                    :request.action==Action.FOCUS?AccessibilityEvent.TYPE_VIEW_FOCUSED
                    :request.action==Action.CLICK?AccessibilityEvent.TYPE_VIEW_CLICKED:AccessibilityEvent.TYPE_VIEW_SCROLLED;
            AccessibilityEvent event=event(request.node,type);
            if(request.action==Action.SET_TEXT) {
                event.setBeforeText(request.before);event.setFromIndex(0);event.setRemovedCount(request.before.length());event.setAddedCount(request.text.length());
                event.getText().clear();event.getText().add(request.text);
            }
            send(event);
        } catch(JSONException|IllegalArgumentException malformed) { /* A missing result expires; never replay an action. */ }
    }
    @Override public AccessibilityNodeProvider getAccessibilityNodeProvider() {return current==null?null:provider;}
    @Override public boolean onTouchEvent(MotionEvent event) {return false;}
    @Override public boolean dispatchHoverEvent(MotionEvent event) {
        AccessibilityManager manager=getContext().getSystemService(AccessibilityManager.class);
        if(current==null||manager==null||!manager.isEnabled()||!manager.isTouchExplorationEnabled())return false;
        int under=-1;
        if(event.getAction()!=MotionEvent.ACTION_HOVER_EXIT) {
            int[] location=new int[2];getLocationInWindow(location);int x=(int)event.getX()+location[0],y=(int)event.getY()+location[1];
            for(Node node:current.nodes)if(rect(node.bounds).contains(x,y)){under=node.id;break;}
        }
        if(under!=hovered) {
            if(hovered!=-1)send(event(hovered,AccessibilityEvent.TYPE_VIEW_HOVER_EXIT));hovered=under;
            if(hovered!=-1)send(event(hovered,AccessibilityEvent.TYPE_VIEW_HOVER_ENTER));
        }
        return true;
    }
    private static Rect rect(Bounds bounds) {return new Rect(bounds.x,bounds.y,bounds.x+bounds.width,bounds.y+bounds.height);}
    private void place(AccessibilityNodeInfo info,Bounds bounds,Bounds parent) {
        int[] window=new int[2],screen=new int[2];getLocationInWindow(window);getLocationOnScreen(screen);
        Rect local=rect(bounds);local.offset(parent==null?-window[0]:-parent.x,parent==null?-window[1]:-parent.y);info.setBoundsInParent(local);
        Rect global=rect(bounds);global.offset(screen[0]-window[0],screen[1]-window[1]);info.setBoundsInScreen(global);
        Rect visible=new Rect();boolean shown=getGlobalVisibleRect(visible)&&Rect.intersects(global,visible);info.setVisibleToUser(shown);
    }
    private AccessibilityNodeInfo base(int id,String className) {
        AccessibilityNodeInfo info=AccessibilityNodeInfo.obtain();info.setPackageName(getContext().getPackageName());info.setClassName(className);
        info.setSource(this,id);return info;
    }
    private AccessibilityEvent event(int id,int type) {
        AccessibilityEvent event=AccessibilityEvent.obtain(type);event.setPackageName(getContext().getPackageName());event.setSource(this,id);
        Node node=current==null?null:current.node(id);
        event.setClassName(node==null?"android.widget.ScrollView":className(node));
        if(node!=null){if(node.role==Role.EDIT)event.getText().add(node.value);else event.setContentDescription(node.label);}
        return event;
    }
    private void send(AccessibilityEvent event) {
        AccessibilityManager manager=getContext().getSystemService(AccessibilityManager.class);
        if(manager!=null&&manager.isEnabled()&&getParent()!=null)getParent().requestSendAccessibilityEvent(this,event);
        else event.recycle();
    }
    private static String className(Node node) {
        return node.role==Role.EDIT?"android.widget.EditText":node.role==Role.BUTTON?"android.widget.Button":"android.widget.TextView";
    }
    private final AccessibilityNodeProvider provider=new AccessibilityNodeProvider() {
        @Override public AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
            Layout state=current;if(state==null)return null;
            if(id==HOST_VIEW_ID) {
                AccessibilityNodeInfo info=AccessibilityNodeInfo.obtain(SettingsAccessibility.this);
                SettingsAccessibility.super.onInitializeAccessibilityNodeInfo(info);info.setClassName("android.view.ViewGroup");info.setContentDescription(null);
                for(Node node:state.nodes)if(node.parent==0)info.addChild(SettingsAccessibility.this,node.id);
                if(state.scroll!=null)info.addChild(SettingsAccessibility.this,state.scroll.id);
                return info;
            }
            if(state.scroll!=null&&state.scroll.id==id) {
                Scroll scroll=state.scroll;AccessibilityNodeInfo info=base(id,"android.widget.ScrollView");info.setParent(SettingsAccessibility.this);
                place(info,scroll.bounds,null);info.setEnabled(true);info.setScrollable(scroll.forward||scroll.backward);
                if(scroll.forward)info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
                if(scroll.backward)info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
                for(Node node:state.nodes)if(node.parent==id)info.addChild(SettingsAccessibility.this,node.id);
                return info;
            }
            Node node=state.node(id);if(node==null)return null;
            AccessibilityNodeInfo info=base(id,className(node));
            if(node.parent==0)info.setParent(SettingsAccessibility.this);else info.setParent(SettingsAccessibility.this,node.parent);
            place(info,node.bounds,node.parent==0?null:state.scroll.bounds);info.setEnabled(node.enabled);info.setFocusable(true);info.setFocused(node.focused);
            info.setAccessibilityFocused(focused==id);
            if(node.role==Role.EDIT) {
                info.setText(node.value);info.setHintText(node.label);info.setEditable(node.permits(Action.SET_TEXT));info.setInputType(android.text.InputType.TYPE_CLASS_TEXT);
            } else if(node.role==Role.BUTTON) {
                info.setContentDescription(node.label);if(Build.VERSION.SDK_INT>=30&&!node.value.isEmpty())info.setStateDescription(node.value);
            } else info.setText(node.value.isEmpty()?node.label:node.label+": "+node.value);
            if(node.permits(Action.CLICK)||node.permits(Action.FOCUS)){info.setClickable(true);info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK);}
            if(node.permits(Action.FOCUS))info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_FOCUS);
            if(node.permits(Action.SET_TEXT))info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_TEXT);
            info.addAction(focused==id?AccessibilityNodeInfo.AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS:AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
            return info;
        }
        @Override public AccessibilityNodeInfo findFocus(int focus) {
            if(current==null)return null;
            if(focus==AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)return focused<0?null:createAccessibilityNodeInfo(focused);
            if(focus==AccessibilityNodeInfo.FOCUS_INPUT)for(Node node:current.nodes)if(node.focused)return createAccessibilityNodeInfo(node.id);
            return null;
        }
        @Override public List<AccessibilityNodeInfo> findAccessibilityNodeInfosByText(String text,int rootId) {
            ArrayList<AccessibilityNodeInfo> found=new ArrayList<>();if(current==null||text==null||text.length()>4096)return found;
            String query=text.toLowerCase(Locale.ROOT);
            for(Node node:current.nodes)if((rootId==HOST_VIEW_ID||rootId==node.id||rootId==node.parent)
                    &&(node.label.toLowerCase(Locale.ROOT).contains(query)||node.value.toLowerCase(Locale.ROOT).contains(query)))found.add(createAccessibilityNodeInfo(node.id));
            return found;
        }
        @Override public boolean performAction(int id,int action,Bundle arguments) {
            Layout state=current;if(state==null||!isShown())return false;
            if((id==HOST_VIEW_ID||state.scroll!=null&&state.scroll.id==id)
                    &&(action==AccessibilityNodeInfo.ACTION_SCROLL_FORWARD||action==AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD)) {
                Action request=action==AccessibilityNodeInfo.ACTION_SCROLL_FORWARD?Action.SCROLL_FORWARD:Action.SCROLL_BACKWARD;
                return state.scroll!=null&&state.scroll.permits(request)&&request(state.scroll.id,state.scroll.target,request,null);
            }
            Node node=state.node(id);if(node==null)return false;
            switch(action) {
                case AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS:
                    if(focused==id)return false;
                    if(focused!=-1)send(event(focused,AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED));
                    focused=id;send(event(id,AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED));return true;
                case AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS:
                    if(focused!=id)return false;focused=-1;send(event(id,AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED));return true;
                case AccessibilityNodeInfo.ACTION_CLICK:
                    Action click=node.role==Role.EDIT?Action.FOCUS:Action.CLICK;
                    return node.permits(click)&&request(id,node.target,click,null);
                case AccessibilityNodeInfo.ACTION_FOCUS:
                    return node.permits(Action.FOCUS)&&request(id,node.target,Action.FOCUS,null);
                case AccessibilityNodeInfo.ACTION_SET_TEXT:
                    if(!node.permits(Action.SET_TEXT)||arguments==null)return false;
                    CharSequence value=arguments.getCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE);
                    return value!=null&&request(id,node.target,Action.SET_TEXT,value.toString());
                default:return false;
            }
        }
    };
}
