package dev.makepad.octosense.bridge;

import android.service.notification.NotificationListenerService;
import android.service.notification.StatusBarNotification;

public final class NotificationAccessService extends NotificationListenerService {
    private BridgeState state() { return ((BridgeApplication)getApplication()).state(); }
    @Override public void onListenerConnected() {
        state().listenerConnected(this);
    }
    @Override public void onListenerDisconnected() { state().listenerDisconnected(this); }
    @Override public void onNotificationPosted(StatusBarNotification notification) {
        state().notificationPosted(this,notification);
    }
    @Override public void onNotificationRemoved(StatusBarNotification notification) {
        state().notificationRemoved(this,notification);
    }
}
