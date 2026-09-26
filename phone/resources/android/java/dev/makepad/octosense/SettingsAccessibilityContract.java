package dev.makepad.octosense;

import java.util.ArrayList;
import java.util.Collections;
import java.util.EnumSet;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Finite presentation contract. Accessibility actions never name a system setting. */
final class SettingsAccessibilityContract {
    static final int MAX_NODES=256, MAX_TEXT=2048;
    private SettingsAccessibilityContract() {}

    enum Role {
        BUTTON("button"), TEXT("text"), EDIT("edit");
        final String wire;
        Role(String wire) {this.wire=wire;}
        static Role parse(String value) {
            for(Role role:values()) if(role.wire.equals(value)) return role;
            throw invalid();
        }
    }
    enum Action {
        CLICK("click"), FOCUS("focus"), SET_TEXT("set_text"),
        SCROLL_FORWARD("scroll_forward"), SCROLL_BACKWARD("scroll_backward");
        final String wire;
        Action(String wire) {this.wire=wire;}
        static Action parse(String value) {
            for(Action action:values()) if(action.wire.equals(value)) return action;
            throw invalid();
        }
    }
    static IllegalArgumentException invalid() {return new IllegalArgumentException("Invalid Settings accessibility presentation");}
    static String text(String value,int limit,boolean empty) {
        if(value==null || (!empty&&value.isEmpty()) || value.codePointCount(0,value.length())>limit) throw invalid();
        for(int i=0;i<value.length();i++) {
            char c=value.charAt(i);
            if(Character.isHighSurrogate(c)) {
                if(++i>=value.length()||!Character.isLowSurrogate(value.charAt(i))) throw invalid();
            } else if(Character.isLowSurrogate(c) || (Character.isISOControl(c)&&c!='\n'&&c!='\t')) throw invalid();
        }
        return value;
    }
    static String token(String value) {
        text(value,256,false);
        for(int i=0;i<value.length();i++) if(value.charAt(i)<33||value.charAt(i)>126) throw invalid();
        return value;
    }
    static String target(String value) {
        if(value==null||!value.matches("0|[1-9][0-9]{0,19}")) throw invalid();
        return value;
    }
    static int id(int value) {if(value<=0)throw invalid();return value;}

    static final class Bounds {
        final int x,y,width,height;
        Bounds(int x,int y,int width,int height) {
            if(x < -32768 || x>32768 || y < -32768 || y>32768
                    || width<0 || width>32768 || height<0 || height>32768) throw invalid();
            this.x=x;this.y=y;this.width=width;this.height=height;
        }
        boolean visible() {return width>0&&height>0;}
        boolean contains(Bounds other) {
            return other.x>=x&&other.y>=y&&other.x+other.width<=x+width&&other.y+other.height<=y+height;
        }
    }
    static final class Node {
        final int id,parent;
        final String target,label,value;
        final Role role;
        final boolean enabled,focused;
        final Set<Action> actions;
        final Bounds bounds;
        Node(int id,String target,int parent,Role role,String label,String value,
             boolean enabled,boolean focused,Set<Action> actions,Bounds bounds) {
            this.id=id(id);this.target=target(target);this.parent=parent;
            this.role=role;this.label=text(label,MAX_TEXT,false);this.value=text(value,MAX_TEXT,true);
            this.enabled=enabled;this.focused=focused;this.bounds=bounds;
            if(parent<0||role==null||actions==null||bounds==null||!bounds.visible()||(!enabled&&!actions.isEmpty())) throw invalid();
            EnumSet<Action> copy=EnumSet.noneOf(Action.class);copy.addAll(actions);
            for(Action action:copy) {
                if(role==Role.BUTTON&&action==Action.CLICK) continue;
                if(role==Role.EDIT&&(action==Action.FOCUS||action==Action.SET_TEXT)) continue;
                throw invalid();
            }
            this.actions=Collections.unmodifiableSet(copy);
        }
        boolean permits(Action action) {return enabled&&actions.contains(action);}
    }
    static final class Scroll {
        final int id;
        final String target;
        final Bounds bounds;
        final boolean forward,backward;
        Scroll(int id,String target,Bounds bounds,boolean forward,boolean backward) {
            this.id=id(id);this.target=target(target);this.bounds=bounds;
            this.forward=forward;this.backward=backward;
            if(bounds==null||!bounds.visible()) throw invalid();
        }
        boolean permits(Action action) {
            return action==Action.SCROLL_FORWARD?forward:action==Action.SCROLL_BACKWARD&&backward;
        }
    }
    static final class Layout {
        final String token,pane;
        final Bounds bounds;
        final Scroll scroll;
        final List<Node> nodes;
        Layout(String token,String pane,Bounds bounds,Scroll scroll,List<Node> nodes) {
            this.token=token(token);this.pane=text(pane,MAX_TEXT,false);this.bounds=bounds;this.scroll=scroll;
            if(bounds==null||!bounds.visible()||nodes==null||nodes.size()>MAX_NODES) throw invalid();
            HashSet<Integer> ids=new HashSet<>();
            if(scroll!=null) {
                if(!bounds.contains(scroll.bounds)) throw invalid();
                ids.add(scroll.id);
            }
            ArrayList<Node> copy=new ArrayList<>();
            for(Node node:nodes) {
                if(node==null||!ids.add(node.id)||!bounds.contains(node.bounds)
                        ||(node.parent!=0&&(scroll==null||node.parent!=scroll.id||!scroll.bounds.contains(node.bounds)))) throw invalid();
                copy.add(node);
            }
            this.nodes=Collections.unmodifiableList(copy);
        }
        Node node(int id) {for(Node node:nodes)if(node.id==id)return node;return null;}
    }
}
