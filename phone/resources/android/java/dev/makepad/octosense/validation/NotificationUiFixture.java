package dev.makepad.octosense.validation;

import android.os.Bundle;
import java.util.ArrayList;
import java.util.function.Consumer;

/** Synthetic notification content only. Uses production PackageManager
 * presentation and rendering, without notification access or real actions. */
public final class NotificationUiFixture {
    private static volatile boolean active;
    private static Consumer<Bundle> publish;
    public static void start() {active=true;}
    public static boolean active() {return active;}
    public static void attach(Consumer<Bundle> sink) {if(active) publish=sink;}
    public static void finish() {active=false;publish=null;}
    public static void command(android.net.Uri route) {
        if(!active||publish==null) throw new IllegalStateException("Notification fixture is not running");
        String operation=route.getPath();
        if(!operation.equals("/notification/known")&&!operation.equals("/notification/update")&&
                !operation.equals("/notification/missing")&&!operation.equals("/notification/clear"))
            throw new IllegalArgumentException("Unknown notification fixture route");
        Bundle state=new Bundle();ArrayList<Bundle> notices=new ArrayList<>();
        if(!operation.equals("/notification/clear")) {
            Bundle notice=new Bundle();notice.putString("handle","notification-ui-fixture");
            notice.putString("package",operation.equals("/notification/missing")?"dev.makepad.octosense.fixture.missing":"dev.makepad.octosense.bridge");
            notice.putString("title",operation.equals("/notification/update")?"Updated notification":"Notification identity check");
            notice.putString("text",operation.equals("/notification/update")?"The same notification now has updated content.":"This sample uses the installed app label and icon.");
            notice.putString("app_label","Untrusted label must be replaced");notice.putString("app_icon","/untrusted/fixture.png");
            notice.putParcelableArrayList("actions",new ArrayList<Bundle>());notices.add(notice);
        }
        state.putParcelableArrayList("notifications",notices);publish.accept(state);
    }
}
