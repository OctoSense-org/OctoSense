package dev.makepad.octosense.contracts;
import android.os.Bundle;
import dev.makepad.octosense.contracts.IHomeIntegrationCallback;

interface IHomeIntegration {
    Bundle getProtocolInfo();
    oneway void subscribe(String session, IHomeIntegrationCallback callback);
    oneway void unsubscribe(String session);
    oneway void publishHomeLayout(String session, long layoutRevision, in Bundle layout);
    oneway void setHomeReady(String session, long layoutRevision, boolean ready);
}
