package android.content.pm;
public class PackageInstaller {
 public static final String EXTRA_SESSION_ID="session_id",EXTRA_STATUS="status";
 public static final int STATUS_SUCCESS=0,STATUS_PENDING_USER_ACTION=-1,STATUS_FAILURE=1,STATUS_FAILURE_ABORTED=3;
 public final java.util.Set<Integer> sessions=new java.util.HashSet<>();public int commits,abandoned;
 public Object getSessionInfo(int id){return sessions.contains(id)?new Object():null;}
 public void abandonSession(int id){sessions.remove(id);abandoned++;}
 public Session openSession(int id){if(!sessions.contains(id))throw new IllegalArgumentException();return new Session();}
 public class Session implements AutoCloseable {public void commit(android.content.IntentSender sender){commits++;}public void close(){}}
}
