package dev.makepad.octosense;

import java.util.ArrayList;
import java.util.Collections;
import java.util.EnumSet;
import java.util.List;
import static dev.makepad.octosense.SettingsAccessibilityContract.*;

/** Runs without Android: malformed virtual trees cannot retain actionable nodes. */
public final class SettingsAccessibilityContractTest {
    private static int checks;
    private static void rejects(Runnable action) {
        try {action.run();}catch(IllegalArgumentException expected){checks++;return;}
        throw new AssertionError("Malformed accessibility contract accepted");
    }
    private static Node button(int id,int parent) {
        return new Node(id,Integer.toString(id),parent,Role.BUTTON,"Increase media volume","33%",true,false,
                EnumSet.of(Action.CLICK),new Bounds(10,180,180,40));
    }
    public static void main(String[] args) {
        Bounds page=new Bounds(0,0,400,700);Scroll scroll=new Scroll(42,"42",new Bounds(0,150,400,550),true,false);
        Node heading=new Node(1,"1",0,Role.TEXT,"Sound","",true,false,Collections.emptySet(),new Bounds(0,0,400,60));
        Node edit=new Node(2,"2",0,Role.EDIT,"Search settings","中文 📨",true,true,
                EnumSet.of(Action.FOCUS,Action.SET_TEXT),new Bounds(0,70,400,60));
        Layout valid=new Layout("root:1/page:2","Sound",page,scroll,List.of(heading,edit,button(3,42)));
        if(valid.node(3)==null||valid.node(8)!=null||!edit.permits(Action.SET_TEXT)||edit.permits(Action.CLICK))throw new AssertionError("Role authority");
        if(!scroll.permits(Action.SCROLL_FORWARD)||scroll.permits(Action.SCROLL_BACKWARD)||scroll.permits(Action.CLICK))throw new AssertionError("Scroll authority");
        for(String value:new String[]{"","set_setting","execute","scroll","SET_TEXT"})rejects(()->Action.parse(value));
        for(String value:new String[]{"","password","shell","Button"})rejects(()->Role.parse(value));
        for(String value:new String[]{"","01","-1","1.0","1e2","0x12","123456789012345678901"})rejects(()->target(value));
        target("0");target("18446744073709551615");
        for(String value:new String[]{"","contains space","line\nfeed","中","x".repeat(257)})rejects(()->token(value));
        for(String value:new String[]{"bad\u0000text","\ud800","\udc00","\ud800x","x".repeat(2049)})rejects(()->text(value,2048,true));
        text("📨".repeat(2048),2048,true);text("中文\nsecond line",2048,true);
        rejects(()->new Bounds(Integer.MAX_VALUE,0,20,20));rejects(()->new Bounds(0,0,-1,20));
        rejects(()->new Layout("root","Sound",new Bounds(0,0,0,20),null,List.of()));
        rejects(()->new Layout("root","Sound",page,scroll,List.of(button(3,999))));
        rejects(()->new Layout("root","Sound",page,scroll,List.of(button(3,42),button(3,42))));
        rejects(()->new Layout("root","Sound",page,scroll,List.of(button(42,42))));
        rejects(()->new Layout("root","Sound",page,scroll,List.of(new Node(8,"8",42,Role.TEXT,"Outside scroll","",true,false,
                Collections.emptySet(),new Bounds(0,0,100,50)))));
        rejects(()->new Node(0,"0",0,Role.TEXT,"Text","",true,false,Collections.emptySet(),new Bounds(0,0,30,30)));
        rejects(()->new Node(1,"1",0,Role.BUTTON,"Disabled","",false,false,EnumSet.of(Action.CLICK),new Bounds(0,0,30,30)));
        rejects(()->new Node(1,"1",0,Role.TEXT,"Text","",true,false,EnumSet.of(Action.FOCUS),new Bounds(0,0,30,30)));
        rejects(()->new Node(1,"1",0,Role.EDIT,"Input","",true,false,EnumSet.of(Action.CLICK),new Bounds(0,0,30,30)));
        rejects(()->new Node(1,"1",0,Role.BUTTON,"Button","",true,false,EnumSet.of(Action.SET_TEXT),new Bounds(0,0,30,30)));
        ArrayList<Node> nodes=new ArrayList<>();for(int i=0;i<MAX_NODES;i++)nodes.add(button(1000+i,42));
        new Layout("root","Sound",page,scroll,nodes);nodes.add(button(2000,42));
        rejects(()->new Layout("root","Sound",page,scroll,nodes));
        System.out.println(checks+" invalid accessibility presentations rejected; bounded roles, identity, Unicode and scroll graph passed");
    }
}
