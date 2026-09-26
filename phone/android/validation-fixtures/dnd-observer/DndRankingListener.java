package dev.makepad.octosense.dndfixture;

import android.service.notification.NotificationListenerService;

/** Only DndFixture reads this service, filtering to its own synthetic notices. */
public final class DndRankingListener extends NotificationListenerService {
    static volatile DndRankingListener connected;
    @Override public void onListenerConnected(){connected=this;}
    @Override public void onListenerDisconnected(){if(connected==this)connected=null;}
    @Override public void onDestroy(){if(connected==this)connected=null;super.onDestroy();}
}
